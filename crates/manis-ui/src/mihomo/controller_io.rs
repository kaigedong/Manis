use super::{
    Arc, CONFIG_RELOAD_CONFIRM_INTERVAL, CONFIG_RELOAD_CONFIRM_READS, ControllerConfig,
    ControllerTransport, EngineManager, GROUP_DELAY_TEST_URL, GROUP_DELAY_TIMEOUT_MS, LoadError,
    LoadedProvider, LoadedProviderNode, LoadedSnapshot, LogLevel, MANIS_GLOBAL_GROUP_NAME,
    ManagedGeneratedProfile, ManagedPolicyRuntimeSnapshot, MihomoClient, MihomoError,
    MihomoSnapshot, Mutex, RoutingMode, RuntimeConfig, StdHttpTransport, VersionInfo,
    delay_controller_config, record_event, thread, to_policy_catalog,
};
#[cfg(unix)]
use super::{PathBuf, UnixSocketTransport};

pub(crate) fn load(
    endpoint: &str,
    controller_secret: Option<&str>,
) -> Result<LoadedSnapshot, LoadError> {
    Ok(loaded_snapshot(fetch_snapshot(
        endpoint,
        controller_secret,
    )?))
}

pub(crate) fn load_sing_box(
    endpoint: &str,
    controller_secret: Option<&str>,
) -> Result<LoadedSnapshot, LoadError> {
    Ok(loaded_snapshot(fetch_sing_box_snapshot(
        endpoint,
        controller_secret,
    )?))
}

fn loaded_snapshot(snapshot: MihomoSnapshot) -> LoadedSnapshot {
    let catalog = to_policy_catalog(&snapshot).ok();
    let providers = load_providers(&snapshot.providers);
    let observed_routes = snapshot.observed_routes();
    let MihomoSnapshot {
        version,
        connections,
        runtime,
        ..
    } = snapshot;
    let version = version
        .version
        .unwrap_or_else(|| "unknown version".to_owned());
    let active_connections = connections.connections.len();
    let download_total = connections.download_total;
    let upload_total = connections.upload_total;

    LoadedSnapshot {
        catalog,
        providers,
        version,
        active_connections,
        download_total,
        upload_total,
        observed_routes,
        connections: connections.connections,
        runtime,
    }
}

pub(super) fn validate_managed_runtime(
    spec: &ManagedGeneratedProfile,
    runtime: &RuntimeConfig,
) -> Result<(), LoadError> {
    let Some(expected) = spec.expected_mixed_port else {
        return Ok(());
    };
    if runtime.mixed_port == Some(expected) {
        return Ok(());
    }
    Err(LoadError::Runtime(format!(
        "Mihomo did not listen on Manis proxy port {expected}; a stale kernel process may remain from an earlier abnormal exit"
    )))
}

pub(super) fn load_providers(providers: &[manis_mihomo::ProxyProvider]) -> Vec<LoadedProvider> {
    providers.iter().map(load_provider).collect()
}

pub(super) fn load_provider(provider: &manis_mihomo::ProxyProvider) -> LoadedProvider {
    LoadedProvider {
        name: provider.name.clone(),
        vehicle_type: provider.vehicle_type.clone(),
        nodes: provider
            .proxies
            .iter()
            .map(|proxy| LoadedProviderNode {
                name: proxy.name.clone(),
                protocol: proxy.proxy_type.clone(),
                latency_label: proxy
                    .latest_latency_ms()
                    .map(|delay| format!("{delay:.0} ms")),
                alive: proxy.alive,
            })
            .collect(),
    }
}

pub(super) fn fetch_snapshot(
    endpoint: &str,
    controller_secret: Option<&str>,
) -> Result<MihomoSnapshot, MihomoError> {
    #[cfg(unix)]
    if let Some(socket_path) = unix_socket_path(endpoint)? {
        let config = ControllerConfig::default();
        return MihomoClient::new(config, UnixSocketTransport::new(socket_path)).fetch_snapshot();
    }

    #[cfg(not(unix))]
    if endpoint.starts_with("unix://") {
        return Err(MihomoError::InvalidConfig(
            "Unix controller sockets are not supported on this platform".to_owned(),
        ));
    }

    let config = with_controller_secret(ControllerConfig::new(endpoint)?, controller_secret);
    MihomoClient::new(config, StdHttpTransport::default()).fetch_snapshot()
}

pub(super) fn fetch_sing_box_snapshot(
    endpoint: &str,
    controller_secret: Option<&str>,
) -> Result<MihomoSnapshot, MihomoError> {
    if endpoint.starts_with("unix://") || endpoint.starts_with("pipe://") {
        return Err(MihomoError::InvalidConfig(
            "sing-box Clash API requires loopback HTTP".to_owned(),
        ));
    }
    let config = with_controller_secret(ControllerConfig::new(endpoint)?, controller_secret);
    MihomoClient::new(config, StdHttpTransport::default()).fetch_sing_box_snapshot()
}

pub(super) fn fetch_group_delay(
    endpoint: &str,
    group_name: &str,
    controller_secret: Option<&str>,
) -> Result<std::collections::BTreeMap<String, u16>, MihomoError> {
    #[cfg(unix)]
    if let Some(socket_path) = unix_socket_path(endpoint)? {
        return MihomoClient::new(
            delay_controller_config(ControllerConfig::default()),
            UnixSocketTransport::new(socket_path),
        )
        .fetch_group_delay(group_name, GROUP_DELAY_TEST_URL, GROUP_DELAY_TIMEOUT_MS);
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
    MihomoClient::new(config, StdHttpTransport::default()).fetch_group_delay(
        group_name,
        GROUP_DELAY_TEST_URL,
        GROUP_DELAY_TIMEOUT_MS,
    )
}

pub(super) fn fetch_policy_group(
    endpoint: &str,
    group_name: &str,
    controller_secret: Option<&str>,
) -> Result<manis_mihomo::MihomoPolicyGroup, MihomoError> {
    #[cfg(unix)]
    if let Some(socket_path) = unix_socket_path(endpoint)? {
        return MihomoClient::new(
            ControllerConfig::default(),
            UnixSocketTransport::new(socket_path),
        )
        .fetch_policy_group(group_name);
    }

    #[cfg(not(unix))]
    if endpoint.starts_with("unix://") {
        return Err(MihomoError::InvalidConfig(
            "Unix controller sockets are not supported on this platform".to_owned(),
        ));
    }

    let config = with_controller_secret(ControllerConfig::new(endpoint)?, controller_secret);
    MihomoClient::new(config, StdHttpTransport::default()).fetch_policy_group(group_name)
}

pub(super) fn put_policy_group_selection(
    endpoint: &str,
    group_name: &str,
    selected_name: &str,
    controller_secret: Option<&str>,
) -> Result<(), MihomoError> {
    #[cfg(unix)]
    if let Some(socket_path) = unix_socket_path(endpoint)? {
        return MihomoClient::new(
            ControllerConfig::default(),
            UnixSocketTransport::new(socket_path),
        )
        .select_policy_group_node(group_name, selected_name);
    }

    #[cfg(not(unix))]
    if endpoint.starts_with("unix://") {
        return Err(MihomoError::InvalidConfig(
            "Unix controller sockets are not supported on this platform".to_owned(),
        ));
    }

    let config = with_controller_secret(ControllerConfig::new(endpoint)?, controller_secret);
    MihomoClient::new(config, StdHttpTransport::default())
        .select_policy_group_node(group_name, selected_name)
}

pub(super) fn policy_group_runtime_snapshot(
    group: manis_mihomo::MihomoPolicyGroup,
) -> ManagedPolicyRuntimeSnapshot {
    ManagedPolicyRuntimeSnapshot {
        current: group.current,
        candidates: group.all.into_iter().collect(),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum GlobalSelectionRoute {
    Direct,
    ViaGlobalExit,
}

pub(super) fn global_selection_route(
    global: &manis_mihomo::MihomoPolicyGroup,
    global_exit: &manis_mihomo::MihomoPolicyGroup,
    selected_name: &str,
) -> Option<GlobalSelectionRoute> {
    if !global
        .proxy_type
        .as_deref()
        .is_some_and(is_selector_proxy_type)
    {
        return None;
    }
    if global
        .all
        .iter()
        .any(|candidate| candidate == MANIS_GLOBAL_GROUP_NAME)
        && global_exit
            .proxy_type
            .as_deref()
            .is_some_and(is_selector_proxy_type)
        && global_exit
            .all
            .iter()
            .any(|candidate| candidate == selected_name)
    {
        Some(GlobalSelectionRoute::ViaGlobalExit)
    } else if global
        .all
        .iter()
        .any(|candidate| candidate == selected_name)
    {
        Some(GlobalSelectionRoute::Direct)
    } else {
        None
    }
}

pub(super) fn select_global_node_at_endpoint(
    endpoint: &str,
    selected_name: &str,
    controller_secret: Option<&str>,
) -> Result<ManagedPolicyRuntimeSnapshot, LoadError> {
    let global = fetch_policy_group(endpoint, "GLOBAL", controller_secret)?;
    let global_exit = fetch_policy_group(endpoint, MANIS_GLOBAL_GROUP_NAME, controller_secret)?;
    match global_selection_route(&global, &global_exit, selected_name) {
        Some(GlobalSelectionRoute::Direct) => {
            select_policy_group_candidate(endpoint, "GLOBAL", selected_name, controller_secret)
        }
        Some(GlobalSelectionRoute::ViaGlobalExit) => {
            let previous = global_exit.current.clone();
            select_policy_group_candidate(
                endpoint,
                MANIS_GLOBAL_GROUP_NAME,
                selected_name,
                controller_secret,
            )?;
            if let Err(error) = select_policy_group_candidate(
                endpoint,
                "GLOBAL",
                MANIS_GLOBAL_GROUP_NAME,
                controller_secret,
            ) {
                if let Some(previous) = previous
                    && previous != selected_name
                    && let Err(rollback_error) = select_policy_group_candidate(
                        endpoint,
                        MANIS_GLOBAL_GROUP_NAME,
                        &previous,
                        controller_secret,
                    )
                {
                    record_event(
                        LogLevel::Warn,
                        "global.node.rollback_failed",
                        format!("group={MANIS_GLOBAL_GROUP_NAME} error={rollback_error}"),
                    );
                }
                return Err(error);
            }
            Ok(ManagedPolicyRuntimeSnapshot {
                current: Some(selected_name.to_owned()),
                candidates: global_exit.all.into_iter().collect(),
            })
        }
        None => Err(LoadError::Runtime(
            "selected node is not in the active Mihomo global exit chain".to_owned(),
        )),
    }
}

pub(super) fn select_policy_group_candidate(
    endpoint: &str,
    group_name: &str,
    selected_name: &str,
    controller_secret: Option<&str>,
) -> Result<ManagedPolicyRuntimeSnapshot, LoadError> {
    let group = fetch_policy_group(endpoint, group_name, controller_secret)?;
    if !group
        .proxy_type
        .as_deref()
        .is_some_and(is_selector_proxy_type)
    {
        return Err(LoadError::Runtime(
            "only manual-selection policy groups can switch nodes".to_owned(),
        ));
    }
    if !group.all.iter().any(|candidate| candidate == selected_name) {
        return Err(LoadError::Runtime(
            "selected node is not in the active Mihomo policy group".to_owned(),
        ));
    }
    put_policy_group_selection(endpoint, group_name, selected_name, controller_secret)?;
    let selected = fetch_policy_group(endpoint, group_name, controller_secret)?;
    if selected.current.as_deref() != Some(selected_name) {
        return Err(LoadError::Runtime(format!(
            "Mihomo did not confirm the node switch for policy group '{group_name}'"
        )));
    }
    Ok(policy_group_runtime_snapshot(selected))
}

pub(super) fn is_selector_proxy_type(proxy_type: &str) -> bool {
    proxy_type.eq_ignore_ascii_case("Selector")
}

pub(super) fn running_managed_endpoint(
    manager: &Arc<Mutex<EngineManager>>,
) -> Result<String, LoadError> {
    let mut manager = manager.lock().map_err(|_poisoned| {
        LoadError::Runtime("managed kernel state lock is poisoned".to_owned())
    })?;
    manager
        .running_endpoint()?
        .ok_or_else(|| {
            LoadError::Runtime("Mihomo is not running; connect the kernel first".to_owned())
        })
        .map(|endpoint| endpoint.uri())
}

pub(super) fn reload_mihomo_config(
    endpoint: &str,
    payload: &str,
    expected_tun: bool,
    controller_secret: Option<&str>,
) -> Result<(), MihomoError> {
    #[cfg(unix)]
    if let Some(socket_path) = unix_socket_path(endpoint)? {
        let client = MihomoClient::new(
            ControllerConfig::default(),
            UnixSocketTransport::new(socket_path),
        );
        return reload_and_confirm_tun(&client, payload, expected_tun);
    }

    #[cfg(not(unix))]
    if endpoint.starts_with("unix://") {
        return Err(MihomoError::InvalidConfig(
            "Unix controller sockets are not supported on this platform".to_owned(),
        ));
    }

    let config = with_controller_secret(ControllerConfig::new(endpoint)?, controller_secret);
    let client = MihomoClient::new(config, StdHttpTransport::default());
    reload_and_confirm_tun(&client, payload, expected_tun)
}

fn reload_and_confirm_tun<T: ControllerTransport>(
    client: &MihomoClient<T>,
    payload: &str,
    expected_tun: bool,
) -> Result<(), MihomoError> {
    client.reload_config_payload(payload)?;
    let mut last_observation = "runtime config was not readable".to_owned();
    for _ in 0..CONFIG_RELOAD_CONFIRM_READS {
        thread::sleep(CONFIG_RELOAD_CONFIRM_INTERVAL);
        match client.fetch_runtime_config() {
            Ok(runtime) if runtime.tun.enable == expected_tun => return Ok(()),
            Ok(runtime) => {
                last_observation = format!("observed tun.enable={}", runtime.tun.enable);
            }
            Err(error) => {
                last_observation = format!("runtime config read failed: {error}");
            }
        }
    }
    let rollback = client.set_tun_enabled(false);
    Err(MihomoError::InvalidResponse(match rollback {
        Ok(()) => format!(
            "Mihomo full reload did not retain tun.enable={expected_tun} ({last_observation}); TUN was disabled and the kernel remains available"
        ),
        Err(rollback) => format!(
            "Mihomo full reload did not retain tun.enable={expected_tun} ({last_observation}), and disabling TUN also failed: {rollback}"
        ),
    }))
}

pub(super) fn fetch_runtime_config(
    endpoint: &str,
    controller_secret: Option<&str>,
) -> Result<RuntimeConfig, MihomoError> {
    #[cfg(unix)]
    if let Some(socket_path) = unix_socket_path(endpoint)? {
        return MihomoClient::new(
            ControllerConfig::default(),
            UnixSocketTransport::new(socket_path),
        )
        .fetch_runtime_config();
    }

    #[cfg(not(unix))]
    if endpoint.starts_with("unix://") {
        return Err(MihomoError::InvalidConfig(
            "Unix controller sockets are not supported on this platform".to_owned(),
        ));
    }

    let config = with_controller_secret(ControllerConfig::new(endpoint)?, controller_secret);
    MihomoClient::new(config, StdHttpTransport::default()).fetch_runtime_config()
}

pub(super) fn set_routing_mode(
    endpoint: &str,
    mode: RoutingMode,
    controller_secret: Option<&str>,
) -> Result<(), MihomoError> {
    #[cfg(unix)]
    if let Some(socket_path) = unix_socket_path(endpoint)? {
        return MihomoClient::new(
            ControllerConfig::default(),
            UnixSocketTransport::new(socket_path),
        )
        .set_routing_mode(mode);
    }

    #[cfg(not(unix))]
    if endpoint.starts_with("unix://") {
        return Err(MihomoError::InvalidConfig(
            "Unix controller sockets are not supported on this platform".to_owned(),
        ));
    }

    let config = with_controller_secret(ControllerConfig::new(endpoint)?, controller_secret);
    MihomoClient::new(config, StdHttpTransport::default()).set_routing_mode(mode)
}

pub(super) fn fetch_version(endpoint: &str) -> Result<VersionInfo, MihomoError> {
    fetch_version_with_secret(endpoint, None)
}

pub(super) fn fetch_version_with_secret(
    endpoint: &str,
    secret: Option<&str>,
) -> Result<VersionInfo, MihomoError> {
    #[cfg(unix)]
    if let Some(socket_path) = unix_socket_path(endpoint)? {
        return MihomoClient::new(
            ControllerConfig::default(),
            UnixSocketTransport::new(socket_path),
        )
        .fetch_version();
    }

    #[cfg(not(unix))]
    if endpoint.starts_with("unix://") {
        return Err(MihomoError::InvalidConfig(
            "Unix controller sockets are not supported on this platform".to_owned(),
        ));
    }

    if endpoint.starts_with("pipe://") {
        return Err(MihomoError::InvalidConfig(
            "Windows controller pipe transport is not implemented yet".to_owned(),
        ));
    }

    let mut config = ControllerConfig::new(endpoint)?;
    if let Some(secret) = secret {
        config = config.with_secret(secret);
    }
    MihomoClient::new(config, StdHttpTransport::default()).fetch_version()
}

pub(super) fn with_controller_secret(
    mut config: ControllerConfig,
    controller_secret: Option<&str>,
) -> ControllerConfig {
    if let Some(secret) = controller_secret {
        config = config.with_secret(secret.to_owned());
    }
    config
}

#[cfg(unix)]
pub(super) fn unix_socket_path(endpoint: &str) -> Result<Option<PathBuf>, MihomoError> {
    let Some(path) = endpoint.strip_prefix("unix://") else {
        return Ok(None);
    };
    let path = PathBuf::from(path);
    if !path.is_absolute() {
        return Err(MihomoError::InvalidConfig(
            "Unix controller socket path must be absolute".to_owned(),
        ));
    }

    Ok(Some(path))
}
