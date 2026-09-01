use std::sync::mpsc;
use std::thread;

use manis_core::{PolicyCandidateKind, PolicyNode};

#[cfg(unix)]
use super::UnixSocketTransport;
#[cfg(unix)]
use super::unix_socket_path;
use super::{
    BTreeMap, ControllerConfig, GROUP_DELAY_CONTROLLER_READ_TIMEOUT, GROUP_DELAY_TEST_URL,
    GROUP_DELAY_TIMEOUT_MS, GROUP_DELAY_WORKERS, LoadError, LogLevel, MihomoClient, MihomoError,
    StdHttpTransport, record_event, with_controller_secret,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PolicyGroupBenchmarkSnapshot {
    pub delays: BTreeMap<String, u16>,
    pub current: Option<String>,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct ProxyDelayTarget {
    pub(super) name: String,
    pub(super) provider: Option<String>,
}

impl ProxyDelayTarget {
    pub(crate) fn direct(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            provider: None,
        }
    }

    pub(crate) fn provider(provider: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            provider: Some(provider.into()),
        }
    }

    pub(crate) fn from_policy_node(node: &PolicyNode) -> Self {
        if node.kind == PolicyCandidateKind::Node
            && let Some(provider) = node.provider.as_deref().filter(|name| !name.is_empty())
        {
            Self::provider(provider, node.name.clone())
        } else {
            Self::direct(node.name.clone())
        }
    }

    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    fn source_label(&self) -> &str {
        self.provider.as_deref().unwrap_or("direct")
    }
}

fn fetch_proxy_delay_target(
    endpoint: &str,
    target: &ProxyDelayTarget,
    controller_secret: Option<&str>,
) -> Result<u16, MihomoError> {
    #[cfg(unix)]
    if let Some(socket_path) = unix_socket_path(endpoint)? {
        let client = MihomoClient::new(
            delay_controller_config(ControllerConfig::default()),
            UnixSocketTransport::new(socket_path),
        );
        return match target.provider.as_deref() {
            Some(provider) => client.fetch_provider_proxy_delay(
                provider,
                &target.name,
                GROUP_DELAY_TEST_URL,
                GROUP_DELAY_TIMEOUT_MS,
            ),
            None => {
                client.fetch_proxy_delay(&target.name, GROUP_DELAY_TEST_URL, GROUP_DELAY_TIMEOUT_MS)
            }
        };
    }

    #[cfg(not(unix))]
    if endpoint.starts_with("unix://") {
        return Err(MihomoError::InvalidConfig(
            "Unix controller sockets are not supported on this platform".to_owned(),
        ));
    }

    let config = delay_controller_config(with_controller_secret(
        ControllerConfig::new(endpoint)?,
        controller_secret,
    ));
    let client = MihomoClient::new(config, StdHttpTransport::default());
    match target.provider.as_deref() {
        Some(provider) => client.fetch_provider_proxy_delay(
            provider,
            &target.name,
            GROUP_DELAY_TEST_URL,
            GROUP_DELAY_TIMEOUT_MS,
        ),
        None => {
            client.fetch_proxy_delay(&target.name, GROUP_DELAY_TEST_URL, GROUP_DELAY_TIMEOUT_MS)
        }
    }
}

pub(crate) fn delay_controller_config(config: ControllerConfig) -> ControllerConfig {
    let connect_timeout = config.connect_timeout();
    config.with_timeouts(connect_timeout, GROUP_DELAY_CONTROLLER_READ_TIMEOUT)
}

pub(crate) fn fetch_proxy_delay_targets_bounded_with_progress(
    endpoint: &str,
    targets: &[ProxyDelayTarget],
    controller_secret: Option<&str>,
    mut on_result: impl FnMut(&str, Option<u16>),
) -> Result<BTreeMap<String, u16>, LoadError> {
    fetch_proxy_delay_targets_bounded_with_progress_by(targets, &mut on_result, |target| {
        fetch_proxy_delay_target(endpoint, target, controller_secret)
    })
}

pub(crate) fn fetch_proxy_delay_targets_bounded_with_progress_by(
    targets: &[ProxyDelayTarget],
    on_result: &mut impl FnMut(&str, Option<u16>),
    fetch_target: impl Fn(&ProxyDelayTarget) -> Result<u16, MihomoError> + Sync,
) -> Result<BTreeMap<String, u16>, LoadError> {
    if targets.is_empty() {
        return Err(LoadError::Runtime(
            "the current group has no nodes that can be benchmarked".to_owned(),
        ));
    }
    let worker_count = targets.len().min(GROUP_DELAY_WORKERS);
    let chunk_size = targets.len().div_ceil(worker_count);
    let (delays, first_error, worker_panicked) = thread::scope(|scope| {
        let (sender, receiver) = mpsc::channel();
        let handles = targets
            .chunks(chunk_size)
            .map(|chunk| {
                let sender = sender.clone();
                let fetch_target = &fetch_target;
                scope.spawn(move || {
                    for target in chunk {
                        let result = fetch_target(target);
                        if sender.send((target.clone(), result)).is_err() {
                            break;
                        }
                    }
                })
            })
            .collect::<Vec<_>>();
        drop(sender);
        let mut delays = BTreeMap::new();
        let mut first_error = None;
        for (target, result) in receiver {
            match result {
                Ok(delay) if delay > 0 => {
                    on_result(target.name(), Some(delay));
                    delays.insert(target.name, delay);
                }
                Ok(_) => on_result(target.name(), None),
                Err(error) => {
                    on_result(target.name(), None);
                    record_event(
                        LogLevel::Warn,
                        "node.delay.failed",
                        format!(
                            "source={} node={} error={error}",
                            target.source_label(),
                            target.name()
                        ),
                    );
                    first_error.get_or_insert(error);
                }
            }
        }
        let mut worker_panicked = false;
        for handle in handles {
            if handle.join().is_err() {
                worker_panicked = true;
            }
        }
        (delays, first_error, worker_panicked)
    });
    if worker_panicked {
        return Err(LoadError::Runtime(
            "node delay benchmark worker panicked".to_owned(),
        ));
    }
    if delays.is_empty() {
        return Err(first_error.map_or(LoadError::NoLatencyResults, LoadError::from));
    }
    Ok(delays)
}
