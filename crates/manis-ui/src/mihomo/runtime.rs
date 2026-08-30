use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    sync::{Arc, Mutex, atomic::Ordering},
};

use manis_core::{KernelKind, RoutingMode};
use manis_engine::{EngineManager, ReadinessPolicy, validate_managed_config};
use manis_mihomo::MihomoError;

use crate::diagnostics::{LogLevel, record_event};

use super::{
    ControllerRuntime, GENERATED_PROFILE_FILE, GeneratedProfileApply, LoadError,
    ManagedGeneratedProfile, ManagedPolicyRuntimeSnapshot, ManagedRuntimeHealth,
    PolicyGroupBenchmarkSnapshot, ProxyDelayTarget, RuntimeProfileSource, RuntimeSnapshot,
    compile_managed_generated_profile, fetch_group_delay, fetch_policy_group,
    fetch_proxy_delay_targets_bounded_with_progress, fetch_proxy_delays_bounded, load,
    load_sing_box, managed_apply, managed_engine_config, readiness_probe, reload_mihomo_config,
    render_generated_profile_with_tun, running_managed_endpoint, select_global_node_at_endpoint,
    select_policy_group_candidate, set_routing_mode, validate_managed_runtime,
};

const MANAGED_KERNEL_LOCK_POISONED: &str = "managed kernel state lock is poisoned";

impl ControllerRuntime {
    pub(crate) fn is_fixture(&self) -> bool {
        match self {
            #[cfg(any(test, feature = "snapshot-fixtures"))]
            Self::Fixture { .. } => true,
            _ => false,
        }
    }

    fn controller_secret(&self) -> Option<String> {
        match self {
            Self::Managed {
                generated_profile: Some(spec),
                ..
            } => spec.controller_secret.clone(),
            #[cfg(any(test, feature = "snapshot-fixtures"))]
            Self::Fixture { .. } => None,
            Self::Managed { .. } | Self::Invalid { .. } => None,
        }
    }

    pub(crate) fn stop_managed(&self) -> Result<(), LoadError> {
        if let Self::Managed { manager, .. } = self {
            manager
                .lock()
                .map_err(|_poisoned| LoadError::Runtime(MANAGED_KERNEL_LOCK_POISONED.to_owned()))?
                .stop()?;
        }
        Ok(())
    }

    pub(crate) fn managed_health(&self) -> Result<ManagedRuntimeHealth, LoadError> {
        match self {
            #[cfg(any(test, feature = "snapshot-fixtures"))]
            Self::Fixture { .. } => Ok(ManagedRuntimeHealth::NotManaged),
            Self::Invalid { message } => Err(LoadError::Runtime(message.clone())),
            Self::Managed { manager, .. } => manager
                .lock()
                .map_err(|_poisoned| LoadError::Runtime(MANAGED_KERNEL_LOCK_POISONED.to_owned()))?
                .running_endpoint()
                .map(|endpoint| {
                    if endpoint.is_some() {
                        ManagedRuntimeHealth::Running
                    } else {
                        ManagedRuntimeHealth::Stopped
                    }
                })
                .map_err(LoadError::from),
        }
    }

    pub(crate) fn profile_source(&self) -> RuntimeProfileSource {
        match self {
            #[cfg(any(test, feature = "snapshot-fixtures"))]
            Self::Fixture { .. } => RuntimeProfileSource::FixtureController,
            Self::Managed { profile_source, .. } => *profile_source,
            Self::Invalid { .. } => RuntimeProfileSource::Invalid,
        }
    }

    pub(crate) fn endpoint_label(&self) -> String {
        match self {
            #[cfg(any(test, feature = "snapshot-fixtures"))]
            Self::Fixture { endpoint } => endpoint.clone(),
            Self::Managed { .. } => "Manis managed".to_owned(),
            Self::Invalid { .. } => "not connected".to_owned(),
        }
    }

    pub(crate) fn connect(&self) -> Result<RuntimeSnapshot, LoadError> {
        match self {
            #[cfg(any(test, feature = "snapshot-fixtures"))]
            Self::Fixture { endpoint } => Ok(RuntimeSnapshot {
                endpoint: endpoint.clone(),
                controller_endpoint: endpoint.clone(),
                controller_secret: None,
                snapshot: load(endpoint, None)?,
            }),
            Self::Managed {
                manager,
                generated_profile,
                privileged,
                ..
            } => {
                let secret = generated_profile
                    .as_ref()
                    .and_then(|spec| spec.controller_secret.clone());
                let endpoint = {
                    let mut manager = manager.lock().map_err(|_poisoned| {
                        LoadError::Runtime(MANAGED_KERNEL_LOCK_POISONED.to_owned())
                    })?;
                    let running = manager.running_endpoint()?;
                    #[cfg(target_os = "macos")]
                    if running.is_none()
                        && !privileged.load(Ordering::Acquire)
                        && let Some(spec) = generated_profile
                        && spec.kernel == KernelKind::Mihomo
                    {
                        let config =
                            managed_engine_config(spec, spec.data_dir.join(GENERATED_PROFILE_FILE));
                        validate_managed_config(&config)?;
                        if let Some(spawner) = crate::macos_privileged::MacosPrivilegedProcessSpawner::recover_if_available()
                            .map_err(|error| {
                                LoadError::Runtime(format!(
                                    "macOS TUN helper could not be recovered: {error}"
                                ))
                            })?
                        {
                            crate::macos_privileged::MacosPrivilegedProcessSpawner::reclaim_stale_ordinary(
                                &config.launch_command(),
                            )
                                .map_err(|error| {
                                    LoadError::Runtime(format!(
                                        "stale unprivileged Mihomo process could not be reclaimed: {error}"
                                    ))
                                })?;
                            *manager = EngineManager::with_adapters(
                                config,
                                ReadinessPolicy::default(),
                                Box::new(spawner),
                                readiness_probe(spec),
                            );
                            privileged.store(true, Ordering::Release);
                        }
                    }
                    match running {
                        Some(endpoint) => endpoint,
                        None => manager.start()?,
                    }
                };
                let endpoint = endpoint.uri();
                let snapshot = load(&endpoint, secret.as_deref())?;
                if let Some(spec) = generated_profile
                    && let Err(error) = validate_managed_runtime(spec, &snapshot.runtime)
                {
                    let _ = manager
                        .lock()
                        .map_err(|_poisoned| {
                            LoadError::Runtime(MANAGED_KERNEL_LOCK_POISONED.to_owned())
                        })?
                        .stop();
                    return Err(error);
                }
                Ok(RuntimeSnapshot {
                    endpoint: "Manis managed".to_owned(),
                    controller_endpoint: endpoint.clone(),
                    controller_secret: secret.clone(),
                    snapshot,
                })
            }
            Self::Invalid { message } => Err(LoadError::Runtime(message.clone())),
        }
    }

    pub(crate) fn connect_sing_box(&self) -> Result<RuntimeSnapshot, LoadError> {
        match self {
            #[cfg(any(test, feature = "snapshot-fixtures"))]
            Self::Fixture { endpoint } => Ok(RuntimeSnapshot {
                endpoint: endpoint.clone(),
                controller_endpoint: endpoint.clone(),
                controller_secret: None,
                snapshot: load_sing_box(endpoint, None)?,
            }),
            Self::Managed {
                manager,
                generated_profile,
                ..
            } => {
                let secret = generated_profile
                    .as_ref()
                    .and_then(|spec| spec.controller_secret.clone());
                let endpoint = {
                    let mut manager = manager.lock().map_err(|_poisoned| {
                        LoadError::Runtime(MANAGED_KERNEL_LOCK_POISONED.to_owned())
                    })?;
                    match manager.running_endpoint()? {
                        Some(endpoint) => endpoint,
                        None => manager.start()?,
                    }
                };
                let endpoint = endpoint.uri();
                Ok(RuntimeSnapshot {
                    endpoint: "Manis managed".to_owned(),
                    controller_endpoint: endpoint.clone(),
                    controller_secret: secret.clone(),
                    snapshot: load_sing_box(&endpoint, secret.as_deref())?,
                })
            }
            Self::Invalid { message } => Err(LoadError::Runtime(message.clone())),
        }
    }

    pub(crate) fn set_tun_enabled(&self, enabled: bool) -> Result<(), LoadError> {
        record_event(
            LogLevel::Info,
            "controller.tun.requested",
            format!(
                "enabled={enabled} ownership={}",
                if matches!(self, Self::Managed { .. }) {
                    "managed"
                } else if self.is_fixture() {
                    "fixture"
                } else {
                    "invalid"
                }
            ),
        );
        let (manager, spec) = self.managed_mihomo_tun_parts()?;
        let profile = compile_managed_generated_profile(spec)?;
        let payload = render_generated_profile_with_tun(spec, &profile, enabled)?;
        if enabled {
            self.prepare_tun_activation()?;
        }
        let controller_secret = self.controller_secret();
        let endpoint = running_managed_endpoint(manager)?;
        record_event(
            LogLevel::Info,
            "controller.tun.config_reload.requested",
            format!(
                "enabled={enabled} method=PUT endpoint=/configs?force=true bytes={}",
                payload.len()
            ),
        );
        let result =
            reload_mihomo_config(&endpoint, &payload, enabled, controller_secret.as_deref())
                .map_err(LoadError::from);
        if result.is_ok() {
            record_event(
                LogLevel::Info,
                "controller.tun.config_reload.succeeded",
                format!("enabled={enabled} rebuild=general,dns,listeners,tun,providers"),
            );
        }
        #[cfg(target_os = "macos")]
        let result = match result {
            Ok(()) if !enabled => self.release_macos_tun_route(),
            result => result,
        };
        #[cfg(target_os = "macos")]
        if enabled && result.is_ok() {
            match crate::macos_privileged::existing_tun_route() {
                Ok(Some(route)) => {
                    record_event(LogLevel::Info, "controller.tun.route_confirmed", route);
                }
                Ok(None) => record_event(
                    LogLevel::Warn,
                    "controller.tun.route_missing",
                    "Mihomo accepted TUN enable but the expected split-default route was not found",
                ),
                Err(error) => record_event(
                    LogLevel::Warn,
                    "controller.tun.route_probe_failed",
                    error.to_string(),
                ),
            }
        }
        match &result {
            Ok(()) => record_event(
                LogLevel::Info,
                "controller.tun.succeeded",
                format!("enabled={enabled} endpoint={endpoint}"),
            ),
            Err(error) => record_event(
                LogLevel::Error,
                "controller.tun.failed",
                format!("enabled={enabled} error={error}"),
            ),
        }
        result
    }

    fn prepare_tun_activation(&self) -> Result<(), LoadError> {
        #[cfg(target_os = "macos")]
        {
            if let Some(conflict) =
                crate::macos_privileged::existing_tun_route().map_err(|error| {
                    LoadError::Runtime(format!("macOS TUN routes could not be inspected: {error}"))
                })?
            {
                record_event(LogLevel::Error, "controller.tun.conflict", conflict.clone());
                return Err(LoadError::Runtime(format!(
                    "another TUN is using the system proxy route ({conflict}); turn off TUN mode in other proxy applications first"
                )));
            }
            if let Err(error) = self.ensure_privileged_mihomo() {
                record_event(
                    LogLevel::Error,
                    "controller.tun.failed",
                    format!("enabled=true phase=privilege_promotion error={error}"),
                );
                return Err(error);
            }
            record_event(
                LogLevel::Info,
                "controller.tun.interface_selection",
                "strategy=auto-detect-interface",
            );
        }
        #[cfg(target_os = "linux")]
        if let Err(error) = self.ensure_linux_tun_capabilities() {
            record_event(
                LogLevel::Error,
                "controller.tun.failed",
                format!("enabled=true phase=capability_promotion error={error}"),
            );
            return Err(error);
        }
        Ok(())
    }

    fn managed_mihomo_tun_parts(
        &self,
    ) -> Result<(&Arc<Mutex<EngineManager>>, &ManagedGeneratedProfile), LoadError> {
        let Self::Managed {
            manager,
            generated_profile: Some(spec),
            ..
        } = self
        else {
            return Err(LoadError::Runtime(match self {
                #[cfg(any(test, feature = "snapshot-fixtures"))]
                Self::Fixture { .. } => "test snapshots cannot enable TUN mode".to_owned(),
                Self::Invalid { message } => message.clone(),
                Self::Managed { .. } => {
                    "TUN mode requires a Mihomo configuration generated from saved Manis sources"
                        .to_owned()
                }
            }));
        };
        if spec.kernel != KernelKind::Mihomo {
            return Err(LoadError::Runtime(
                "the TUN configuration reload path supports only Mihomo".to_owned(),
            ));
        }
        Ok((manager, spec))
    }

    #[cfg(target_os = "macos")]
    fn release_macos_tun_route(&self) -> Result<(), LoadError> {
        if crate::macos_privileged::wait_for_tun_route_release().map_err(|error| {
            LoadError::Runtime(format!(
                "macOS TUN route release could not be confirmed: {error}"
            ))
        })? {
            return Ok(());
        }

        record_event(
            LogLevel::Warn,
            "controller.tun.route_release_restart",
            "Mihomo kept the macOS TUN route after disable; restarting managed core",
        );
        let Self::Managed { manager, .. } = self else {
            return Err(LoadError::Runtime(
                "the TUN route remained after disable, and Manis cannot restart the active kernel"
                    .to_owned(),
            ));
        };
        let mut manager = manager
            .lock()
            .map_err(|_poisoned| LoadError::Runtime(MANAGED_KERNEL_LOCK_POISONED.to_owned()))?;
        manager.stop()?;
        manager.start()?;
        if !crate::macos_privileged::wait_for_tun_route_release().map_err(|error| {
            LoadError::Runtime(format!(
                "macOS TUN route release could not be confirmed after restart: {error}"
            ))
        })? {
            return Err(LoadError::Runtime(
                "the macOS TUN route remained after Mihomo restarted".to_owned(),
            ));
        }
        record_event(
            LogLevel::Info,
            "controller.tun.route_release_succeeded",
            "managed core restarted and macOS TUN route was released",
        );
        Ok(())
    }

    #[cfg(target_os = "macos")]
    fn ensure_privileged_mihomo(&self) -> Result<(), LoadError> {
        use crate::macos_privileged::MacosPrivilegedProcessSpawner;

        let Self::Managed {
            manager,
            generated_profile: Some(spec),
            privileged,
            ..
        } = self
        else {
            return Err(LoadError::Runtime(
                "macOS TUN requires a Mihomo configuration generated from saved Manis sources"
                    .to_owned(),
            ));
        };
        if spec.kernel != KernelKind::Mihomo {
            return Err(LoadError::Runtime(
                "the macOS privileged helper currently supports only Mihomo".to_owned(),
            ));
        }
        if privileged.load(Ordering::Acquire) {
            record_event(
                LogLevel::Debug,
                "helper.promotion.skipped",
                "reason=already_privileged",
            );
            return Ok(());
        }

        // Registration and approval are checked before touching the healthy unprivileged core.
        record_event(
            LogLevel::Info,
            "helper.promotion.requested",
            "kernel=mihomo",
        );
        let spawner = MacosPrivilegedProcessSpawner::prepare().map_err(|error| {
            LoadError::Runtime(format!("macOS TUN helper could not be prepared: {error}"))
        })?;
        let final_path = spec.data_dir.join(GENERATED_PROFILE_FILE);
        let config = managed_engine_config(spec, final_path.clone());
        validate_managed_config(&config)?;

        let mut manager = manager
            .lock()
            .map_err(|_poisoned| LoadError::Runtime(MANAGED_KERNEL_LOCK_POISONED.to_owned()))?;
        let was_running = manager.running_endpoint()?.is_some();
        record_event(
            LogLevel::Info,
            "helper.promotion.prepared",
            format!("ordinary_core_running={was_running}"),
        );
        if was_running {
            manager.stop()?;
            record_event(
                LogLevel::Info,
                "helper.promotion.ordinary_core_stopped",
                "ordinary Mihomo stopped before privileged launch",
            );
        } else {
            MacosPrivilegedProcessSpawner::reclaim_stale_ordinary(&config.launch_command())
                .map_err(|error| {
                    LoadError::Runtime(format!(
                        "stale unprivileged Mihomo process could not be reclaimed: {error}"
                    ))
                })?;
        }
        *manager = EngineManager::with_adapters(
            config,
            ReadinessPolicy::default(),
            Box::new(spawner),
            readiness_probe(spec),
        );
        start_promoted_mihomo(&mut manager, spec, final_path, was_running, privileged)
    }

    #[cfg(target_os = "linux")]
    fn ensure_linux_tun_capabilities(&self) -> Result<(), LoadError> {
        let Self::Managed {
            manager,
            generated_profile: Some(spec),
            privileged,
            ..
        } = self
        else {
            return Err(LoadError::Runtime(
                "Linux TUN requires a Mihomo configuration generated from saved Manis sources"
                    .to_owned(),
            ));
        };
        if spec.kernel != KernelKind::Mihomo {
            return Err(LoadError::Runtime(
                "Linux TUN capability authorization supports only Mihomo".to_owned(),
            ));
        }
        if privileged.load(Ordering::Acquire) {
            record_event(
                LogLevel::Debug,
                "helper.promotion.skipped",
                "platform=linux reason=already_capable",
            );
            return Ok(());
        }

        record_event(
            LogLevel::Info,
            "helper.promotion.requested",
            "platform=linux kernel=mihomo capabilities=cap_net_admin,cap_net_raw",
        );
        let (state, privileged_binary) = crate::linux_privileged::ensure_tun_capabilities()
            .map_err(|error| LoadError::Runtime(error.to_string()))?;

        let final_path = spec.data_dir.join(GENERATED_PROFILE_FILE);
        let mut privileged_spec = spec.clone();
        privileged_spec.binary = privileged_binary;
        let config = managed_engine_config(&privileged_spec, final_path.clone());
        validate_managed_config(&config)?;

        let mut manager = manager
            .lock()
            .map_err(|_poisoned| LoadError::Runtime(MANAGED_KERNEL_LOCK_POISONED.to_owned()))?;
        let was_running = manager.running_endpoint()?.is_some();
        if was_running {
            manager.stop()?;
        }
        *manager = EngineManager::new(config, ReadinessPolicy::default(), readiness_probe(spec));
        match manager.start() {
            Ok(_endpoint) => {
                privileged.store(true, Ordering::Release);
                record_event(
                    LogLevel::Info,
                    "helper.promotion.succeeded",
                    format!(
                        "platform=linux capability_state={state:?} ordinary_core_restarted={was_running}"
                    ),
                );
                Ok(())
            }
            Err(error) => {
                record_event(
                    LogLevel::Error,
                    "helper.promotion.failed",
                    format!("platform=linux error={error}"),
                );
                let fallback_config = managed_engine_config(spec, final_path);
                *manager = EngineManager::new(
                    fallback_config,
                    ReadinessPolicy::default(),
                    readiness_probe(spec),
                );
                let fallback = was_running.then(|| manager.start().err()).flatten();
                let message = fallback.map_or_else(
                    || format!("Mihomo could not start after Linux TUN authorization: {error}"),
                    |fallback| {
                        format!(
                            "Mihomo could not start after Linux TUN authorization: {error}; restoring ordinary Mihomo also failed: {fallback}"
                        )
                    },
                );
                Err(LoadError::Runtime(message))
            }
        }
    }

    pub(crate) fn set_routing_mode(&self, mode: RoutingMode) -> Result<(), LoadError> {
        record_event(
            LogLevel::Info,
            "controller.routing.requested",
            format!("mode={mode:?}"),
        );
        let controller_secret = self.controller_secret();
        let endpoint = match self {
            Self::Managed { manager, .. } => {
                let mut manager = manager.lock().map_err(|_poisoned| {
                    LoadError::Runtime(MANAGED_KERNEL_LOCK_POISONED.to_owned())
                })?;
                manager
                    .running_endpoint()?
                    .ok_or_else(|| {
                        LoadError::Runtime(
                            "Mihomo is not running; connect the kernel first".to_owned(),
                        )
                    })?
                    .uri()
            }
            #[cfg(any(test, feature = "snapshot-fixtures"))]
            Self::Fixture { .. } => {
                return Err(LoadError::Runtime(
                    "test snapshots cannot change routing mode".to_owned(),
                ));
            }
            Self::Invalid { message } => return Err(LoadError::Runtime(message.clone())),
        };
        let result = set_routing_mode(&endpoint, mode, controller_secret.as_deref());
        match &result {
            Ok(()) => record_event(
                LogLevel::Info,
                "controller.routing.succeeded",
                format!("mode={mode:?} endpoint={endpoint}"),
            ),
            Err(error) => record_event(
                LogLevel::Error,
                "controller.routing.failed",
                format!("mode={mode:?} error={error}"),
            ),
        }
        result.map_err(LoadError::from)
    }

    pub(crate) fn select_global_node(
        &self,
        selected_name: &str,
    ) -> Result<ManagedPolicyRuntimeSnapshot, LoadError> {
        let controller_secret = self.controller_secret();
        let Self::Managed {
            manager,
            generated_profile: Some(_),
            ..
        } = self
        else {
            return Err(LoadError::Runtime(
                "the active controller is not managed by Manis; its global exit cannot be changed"
                    .to_owned(),
            ));
        };
        let endpoint = {
            let mut manager = manager
                .lock()
                .map_err(|_poisoned| LoadError::Runtime(MANAGED_KERNEL_LOCK_POISONED.to_owned()))?;
            manager
                .running_endpoint()?
                .ok_or_else(|| {
                    LoadError::Runtime(
                        "Mihomo is not running; start the managed kernel first".to_owned(),
                    )
                })?
                .uri()
        };
        select_global_node_at_endpoint(&endpoint, selected_name, controller_secret.as_deref())
    }

    pub(crate) fn test_proxy_candidates_delay(
        &self,
        group_name: &str,
        candidate_names: &[String],
    ) -> Result<std::collections::BTreeMap<String, u16>, LoadError> {
        if candidate_names.is_empty() {
            return Err(LoadError::Runtime(
                "the current group has no nodes that can be benchmarked".to_owned(),
            ));
        }
        let controller_secret = self.controller_secret();
        let (endpoint, managed) = match self {
            #[cfg(any(test, feature = "snapshot-fixtures"))]
            Self::Fixture { endpoint } => (endpoint.clone(), false),
            Self::Managed { manager, .. } => {
                let endpoint = {
                    let mut manager = manager.lock().map_err(|_poisoned| {
                        LoadError::Runtime(MANAGED_KERNEL_LOCK_POISONED.to_owned())
                    })?;
                    match manager.running_endpoint()? {
                        Some(endpoint) => endpoint,
                        None => manager.start()?,
                    }
                };
                (endpoint.uri(), true)
            }
            Self::Invalid { message } => return Err(LoadError::Runtime(message.clone())),
        };

        if managed {
            match fetch_group_delay(&endpoint, group_name, controller_secret.as_deref()) {
                Ok(delays) => {
                    let candidates = candidate_names.iter().collect::<BTreeSet<_>>();
                    return Ok(delays
                        .into_iter()
                        .filter(|(name, _delay)| candidates.contains(name))
                        .collect());
                }
                Err(MihomoError::HttpStatus {
                    status_code: 404, ..
                }) => {}
                Err(error) => return Err(error.into()),
            }
        }

        fetch_proxy_delays_bounded(&endpoint, candidate_names, controller_secret.as_deref())
    }

    pub(crate) fn test_proxy_delay_targets_with_progress(
        &self,
        targets: &[ProxyDelayTarget],
        on_result: impl FnMut(&str, Option<u16>),
    ) -> Result<BTreeMap<String, u16>, LoadError> {
        if targets.is_empty() {
            return Err(LoadError::Runtime(
                "the current group has no nodes that can be benchmarked".to_owned(),
            ));
        }
        let controller_secret = self.controller_secret();
        let endpoint = match self {
            #[cfg(any(test, feature = "snapshot-fixtures"))]
            Self::Fixture { endpoint } => endpoint.clone(),
            Self::Managed { manager, .. } => {
                let endpoint = {
                    let mut manager = manager.lock().map_err(|_poisoned| {
                        LoadError::Runtime(MANAGED_KERNEL_LOCK_POISONED.to_owned())
                    })?;
                    match manager.running_endpoint()? {
                        Some(endpoint) => endpoint,
                        None => manager.start()?,
                    }
                };
                endpoint.uri()
            }
            Self::Invalid { message } => return Err(LoadError::Runtime(message.clone())),
        };

        fetch_proxy_delay_targets_bounded_with_progress(
            &endpoint,
            targets,
            controller_secret.as_deref(),
            on_result,
        )
    }

    pub(crate) fn test_policy_group_delay(
        &self,
        group_name: &str,
        candidate_names: &[String],
    ) -> Result<PolicyGroupBenchmarkSnapshot, LoadError> {
        let controller_secret = self.controller_secret();
        let endpoint = match self {
            #[cfg(any(test, feature = "snapshot-fixtures"))]
            Self::Fixture { endpoint } => endpoint.clone(),
            Self::Managed { manager, .. } => {
                let endpoint = {
                    let mut manager = manager.lock().map_err(|_poisoned| {
                        LoadError::Runtime(MANAGED_KERNEL_LOCK_POISONED.to_owned())
                    })?;
                    match manager.running_endpoint()? {
                        Some(endpoint) => endpoint,
                        None => manager.start()?,
                    }
                };
                endpoint.uri()
            }
            Self::Invalid { message } => return Err(LoadError::Runtime(message.clone())),
        };
        let candidates = candidate_names.iter().collect::<BTreeSet<_>>();
        let group_delays = fetch_group_delay(&endpoint, group_name, controller_secret.as_deref());
        let delays = match group_delays {
            Ok(delays) => delays
                .into_iter()
                .filter(|(name, _delay)| candidates.contains(name))
                .collect::<BTreeMap<_, _>>(),
            Err(group_error) => {
                match fetch_proxy_delays_bounded(
                    &endpoint,
                    candidate_names,
                    controller_secret.as_deref(),
                ) {
                    Ok(delays) => delays,
                    Err(_fallback_error) => return Err(group_error.into()),
                }
            }
        };
        if delays.is_empty() {
            return Err(LoadError::Runtime(
                "Mihomo returned no valid node latency measurements".to_owned(),
            ));
        }
        let current = fetch_policy_group(&endpoint, group_name, controller_secret.as_deref())
            .ok()
            .and_then(|group| group.current);
        Ok(PolicyGroupBenchmarkSnapshot { delays, current })
    }

    pub(crate) fn select_policy_candidate(
        &self,
        group_name: &str,
        selected_name: &str,
    ) -> Result<ManagedPolicyRuntimeSnapshot, LoadError> {
        let controller_secret = self.controller_secret();
        let Self::Managed {
            manager,
            generated_profile: Some(_),
            ..
        } = self
        else {
            return Err(LoadError::Runtime(
                "the active controller is not managed by Manis; its policy groups cannot be changed"
                    .to_owned(),
            ));
        };
        let endpoint = {
            let mut manager = manager
                .lock()
                .map_err(|_poisoned| LoadError::Runtime(MANAGED_KERNEL_LOCK_POISONED.to_owned()))?;
            manager
                .running_endpoint()?
                .ok_or_else(|| {
                    LoadError::Runtime(
                        "Mihomo is not running; start the managed kernel first".to_owned(),
                    )
                })?
                .uri()
        };
        select_policy_group_candidate(
            &endpoint,
            group_name,
            selected_name,
            controller_secret.as_deref(),
        )
    }

    pub(crate) fn apply_saved_sources(
        &self,
        store_dir: &Path,
    ) -> Result<GeneratedProfileApply, LoadError> {
        managed_apply::apply_saved_sources(self, store_dir)
    }
}

#[cfg(target_os = "macos")]
fn start_promoted_mihomo(
    manager: &mut EngineManager,
    spec: &ManagedGeneratedProfile,
    final_path: std::path::PathBuf,
    was_running: bool,
    privileged: &std::sync::atomic::AtomicBool,
) -> Result<(), LoadError> {
    match manager.start() {
        Ok(_endpoint) => {
            privileged.store(true, Ordering::Release);
            record_event(
                LogLevel::Info,
                "helper.promotion.succeeded",
                "privileged Mihomo controller became ready",
            );
            Ok(())
        }
        Err(privileged_error) => {
            record_event(
                LogLevel::Error,
                "helper.promotion.failed",
                privileged_error.to_string(),
            );
            let fallback_config = managed_engine_config(spec, final_path);
            *manager = EngineManager::new(
                fallback_config,
                ReadinessPolicy::default(),
                readiness_probe(spec),
            );
            let fallback = was_running.then(|| manager.start().err()).flatten();
            let message = fallback.map_or_else(
                || format!("privileged Mihomo failed to start: {privileged_error}"),
                |fallback| {
                    format!(
                        "privileged Mihomo failed to start: {privileged_error}; restoring unprivileged Mihomo also failed: {fallback}"
                    )
                },
            );
            Err(LoadError::Runtime(message))
        }
    }
}
