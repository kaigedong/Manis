use std::collections::{BTreeMap, BTreeSet, HashMap};
#[cfg(not(windows))]
use std::fs;
#[cfg(unix)]
use std::io::Read;
use std::net::TcpListener;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::{env, error::Error, fmt};

use manis_core::{KernelKind, NodeIdentity, PolicyCandidateMatcher, PolicyCatalog, RoutingMode};
use manis_engine::{
    ControllerEndpoint, EngineError, EngineManager, ManagedEngineConfig, ProbeStatus,
    ReadinessPolicy, ReadinessProbe, validate_managed_config,
};
#[cfg(unix)]
use manis_mihomo::UnixSocketTransport;
use manis_mihomo::{
    Connection, ControllerConfig, ControllerTransport, MihomoClient, MihomoError, MihomoSnapshot,
    ObservedRouteEvidence, RuntimeConfig, StdHttpTransport, VersionInfo, to_policy_catalog,
};
use manis_profile::{
    MANIS_GLOBAL_GROUP_NAME, Name, PolicyRef, Profile, ProfileMode, ProxyDnsServer, QxRuleList,
    Rule, SecretUrl, SingBoxOptions, UserPolicyGroup, UserPolicyGroupKind, VlessProxy,
    render_mihomo_yaml, render_mihomo_yaml_with_tun, render_sing_box_json, write_private_atomic,
};
use ureq::{Agent, ResponseExt as _};

use crate::diagnostics::{LogLevel, record_event};
use crate::subscription::SingleNodeSource;
use crate::{brand, core_update};

mod benchmark;
mod controller_io;
mod live_runtime;
mod managed_apply;
mod policy_store;
mod preview;
mod profile_compiler;
mod routing_order;
mod runtime;
mod runtime_build;
mod store_snapshot;
mod workspace;

#[cfg(test)]
pub(super) use benchmark::fetch_proxy_delay_targets_bounded_with_progress_by;
pub(crate) use benchmark::{PolicyGroupBenchmarkSnapshot, ProxyDelayTarget};
pub(super) use benchmark::{
    delay_controller_config, fetch_proxy_delay_targets_bounded_with_progress,
};
pub(crate) use live_runtime::{
    KernelLogEntry, LiveRuntimeSession, LiveStreamPhase, LiveStreamStatus,
};
#[cfg(test)]
use live_runtime::{LiveMailbox, sanitize_kernel_log, spawn_connection_stream};
#[cfg(test)]
use manis_mihomo::LiveController;
use policy_store::compile_managed_policy_groups;
pub(crate) use policy_store::*;
use preview::canonical_binary;
pub(crate) use preview::*;
#[cfg(test)]
use preview::{
    PreviewWorkspace, extract_subscription_proxy_nameservers,
    preview_secret_subscription_with_binary, preview_subscription_with_binary,
};
pub(crate) use routing_order::{
    MoveDirection, load_routing_rule_group_order_in, move_routing_rule_group,
    normalized_routing_rule_group_order, save_routing_rule_group_order_in,
};
pub(crate) use store_snapshot::SourceStoreTransaction;
pub(crate) use workspace::*;
use workspace::{
    apply_qx_rule_sources, decode_hex, next_stored_source_id, profile_mode, valid_stored_id,
};
#[cfg(test)]
use workspace::{current_unix_nanos, storage_version_supported};
#[cfg(not(windows))]
use workspace::{
    private_store_entries, read_private_source_allow_empty, remove_private_source,
    require_clean_absolute_store,
};

const CONTROLLER_ENV: &str = "MANIS_MIHOMO_CONTROLLER";
const LEGACY_RELAY_CONTROLLER_ENV: &str = "RELAY_MIHOMO_CONTROLLER";
const CONTROLLER_SECRET_ENV: &str = "MANIS_MIHOMO_SECRET";
const LEGACY_RELAY_CONTROLLER_SECRET_ENV: &str = "RELAY_MIHOMO_SECRET";
#[cfg(debug_assertions)]
const BINARY_ENV: &str = "MANIS_MIHOMO_BINARY";
#[cfg(debug_assertions)]
const LEGACY_RELAY_BINARY_ENV: &str = "RELAY_MIHOMO_BINARY";
const CONFIG_ENV: &str = "MANIS_MIHOMO_CONFIG";
const LEGACY_RELAY_CONFIG_ENV: &str = "RELAY_MIHOMO_CONFIG";
const DATA_DIR_ENV: &str = "MANIS_MIHOMO_DATA_DIR";
const LEGACY_RELAY_DATA_DIR_ENV: &str = "RELAY_MIHOMO_DATA_DIR";
const SUBSCRIPTION_FILE_ENV: &str = "MANIS_MIHOMO_SUBSCRIPTION_FILE";
const LEGACY_RELAY_SUBSCRIPTION_FILE_ENV: &str = "RELAY_MIHOMO_SUBSCRIPTION_FILE";
const MIXED_PORT_ENV: &str = "MANIS_MIHOMO_MIXED_PORT";
const LEGACY_RELAY_MIXED_PORT_ENV: &str = "RELAY_MIHOMO_MIXED_PORT";
const SING_BOX_BINARY_ENV: &str = "MANIS_SING_BOX_BINARY";
const LEGACY_RELAY_SING_BOX_BINARY_ENV: &str = "RELAY_SING_BOX_BINARY";
const DEFAULT_MANAGED_MIXED_PORT: u16 = 17_890;
const MAX_SUBSCRIPTION_FILE_BYTES: u64 = 16 * 1024;
const MAX_STORED_SUBSCRIPTION_FILE_BYTES: u64 = 2 * 16 * 1024 + 1024;
const IMPORTED_SUBSCRIPTION_FILE: &str = "subscription.url";
const STORED_SUBSCRIPTION_PREFIX: &str = "source-";
const STORED_SUBSCRIPTION_SUFFIX: &str = ".url";
const STORED_SUBSCRIPTION_VERSION: &str = "manis-subscription-source-v3";
const LEGACY_MANIS_STORED_SUBSCRIPTION_VERSION_V2: &str = "manis-subscription-source-v2";
const LEGACY_MANIS_STORED_SUBSCRIPTION_VERSION: &str = "manis-subscription-source-v1";
const LEGACY_RELAY_STORED_SUBSCRIPTION_VERSION: &str = "relay-subscription-source-v1";
const MAX_SUBSCRIPTION_DOCUMENT_BYTES: u64 = 4 * 1024 * 1024;
const MAX_SUBSCRIPTION_PROXY_DNS_SERVERS: usize = 8;
const MAX_SUBSCRIPTION_SOURCE_NAME_BYTES: usize = 96;
const LEGACY_GENERATED_PROXY_GROUP_NAME: &str = "Proxy";
const SUBSCRIPTION_DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(15);
const SUBSCRIPTION_MAX_REDIRECTS: u32 = 5;
const SAVED_SINGLE_NODE_PREFIX: &str = "saved-";
const SAVED_SINGLE_NODE_SUFFIX: &str = ".vless";
const SAVED_SINGLE_NODE_VERSION: &str = "manis-single-node-source-v1";
const LEGACY_SAVED_SINGLE_NODE_VERSION: &str = "manis-vless-source-v1";
const QX_RULE_SOURCE_PREFIX: &str = "qx-rule-";
const QX_RULE_SOURCE_SUFFIX: &str = ".qxrules";
const QX_RULE_SOURCE_VERSION: &str = "manis-qx-rule-source-v3";
const LEGACY_MANIS_QX_RULE_SOURCE_VERSION_V2: &str = "manis-qx-rule-source-v2";
const LEGACY_MANIS_QX_RULE_SOURCE_VERSION: &str = "manis-qx-rule-source-v1";
const LEGACY_RELAY_QX_RULE_SOURCE_VERSION: &str = "relay-qx-rule-source-v1";
const MAX_QX_RULE_SOURCE_CONTENT_BYTES: usize = 1024 * 1024;
const MAX_QX_RULE_SOURCE_FILE_BYTES: u64 = 2 * 1024 * 1024 + 64 * 1024;
const ROUTING_RULE_GROUP_ORDER_FILE: &str = "routing-rule-group-order.state";
const ROUTING_RULE_GROUP_ORDER_VERSION: &str = "manis-routing-rule-group-order-v1";
const MAX_ROUTING_RULE_GROUPS: usize = 257;
const MAX_ROUTING_RULE_GROUP_ORDER_FILE_BYTES: u64 = 64 * 1024;
pub(crate) const MANUAL_ROUTING_RULE_GROUP_ID: &str = "manual";
const WORKSPACE_STATE_FILE: &str = "workspace.state";
const ROUTING_MODE_FILE: &str = "routing.mode";
const NODE_SELECTION_PREFERENCES_FILE: &str = "node-selection.state";
const NODE_SELECTION_PREFERENCES_VERSION: &str = "manis-node-selection-v1";
const LEGACY_RELAY_NODE_SELECTION_PREFERENCES_VERSION: &str = "relay-node-selection-v1";
const MAX_NODE_SELECTION_POLICY_TARGETS: usize = 256;
const MAX_NODE_SELECTION_FILE_BYTES: u64 = 64 * 1024;
const MANAGED_POLICY_PREFIX: &str = "policy-";
const MANAGED_POLICY_SUFFIX: &str = ".policy";
const MANAGED_POLICY_VERSION: &str = "manis-policy-group-v1";
const LEGACY_MANAGED_POLICY_PREFIX: &str = "group-";
const LEGACY_MANAGED_POLICY_SUFFIX: &str = ".group";
const LEGACY_MANIS_MANAGED_POLICY_VERSION: &str = "manis-node-group-v1";
const LEGACY_RELAY_MANAGED_POLICY_VERSION: &str = "relay-node-group-v1";
const MAX_MANAGED_POLICIES: usize = 32;
const GENERATED_PROFILE_FILE: &str = "manis-generated.yaml";
const CANDIDATE_PROFILE_FILE: &str = "manis-generated.candidate.yaml";
const SING_BOX_PROFILE_FILE: &str = "manis-generated.json";
const SING_BOX_CANDIDATE_FILE: &str = "manis-generated.candidate.json";
const PREVIEW_PROVIDER_ATTEMPTS: usize = 80;
const PREVIEW_PROVIDER_DELAY: Duration = Duration::from_millis(250);
const PREVIEW_ENGINE_START_ATTEMPTS: usize = 4;
const LIVE_CONNECTION_INTERVAL: Duration = Duration::from_millis(750);
const LIVE_LOG_MAILBOX_CAPACITY: usize = 256;
const LIVE_RETRY_MAX: Duration = Duration::from_secs(5);
const GROUP_DELAY_TEST_URL: &str = "https://www.gstatic.com/generate_204";
const GROUP_DELAY_TIMEOUT_MS: u16 = 5_000;
const GROUP_DELAY_CONTROLLER_READ_TIMEOUT: Duration = Duration::from_secs(9);
const GROUP_DELAY_WORKERS: usize = 8;
const CONFIG_RELOAD_CONFIRM_INTERVAL: Duration = Duration::from_millis(250);
const CONFIG_RELOAD_CONFIRM_READS: usize = 3;
static NEXT_PREVIEW_WORKSPACE: AtomicU64 = AtomicU64::new(0);
static NEXT_STORED_SOURCE: AtomicU64 = AtomicU64::new(0);

const UNSUPPORTED_MIHOMO_RUNTIME_ENV: [&str; 8] = [
    CONTROLLER_ENV,
    LEGACY_RELAY_CONTROLLER_ENV,
    CONTROLLER_SECRET_ENV,
    LEGACY_RELAY_CONTROLLER_SECRET_ENV,
    CONFIG_ENV,
    LEGACY_RELAY_CONFIG_ENV,
    SUBSCRIPTION_FILE_ENV,
    LEGACY_RELAY_SUBSCRIPTION_FILE_ENV,
];

#[derive(Clone, Debug)]
pub(crate) enum ControllerState {
    Disconnected,
    Connecting {
        endpoint: String,
    },
    Connected {
        endpoint: String,
        version: String,
        active_connections: usize,
        download_total: u64,
        upload_total: u64,
    },
    Failed {
        endpoint: String,
        message: String,
    },
}

impl ControllerState {
    pub(crate) fn endpoint(&self) -> Option<&str> {
        match self {
            Self::Disconnected => None,
            Self::Connecting { endpoint }
            | Self::Connected { endpoint, .. }
            | Self::Failed { endpoint, .. } => Some(endpoint),
        }
    }
}

#[derive(Clone)]
pub(crate) enum ControllerRuntime {
    #[cfg(any(test, feature = "snapshot-fixtures"))]
    Fixture {
        endpoint: String,
    },
    Managed {
        manager: Arc<Mutex<EngineManager>>,
        apply_lock: Arc<Mutex<()>>,
        profile_source: RuntimeProfileSource,
        generated_profile: Option<ManagedGeneratedProfile>,
        privileged: Arc<AtomicBool>,
    },
    Invalid {
        message: String,
    },
}

#[derive(Clone, Debug)]
pub(crate) struct ManagedGeneratedProfile {
    kernel: KernelKind,
    binary: PathBuf,
    data_dir: PathBuf,
    controller: ControllerEndpoint,
    expected_mixed_port: Option<u16>,
    profile_store_dir: Option<PathBuf>,
    controller_secret: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ManagedPolicyRuntimeSnapshot {
    pub current: Option<String>,
    pub candidates: BTreeSet<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GeneratedProfileApply {
    Updated,
    Restarted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ManagedRuntimeHealth {
    #[cfg(any(test, feature = "snapshot-fixtures"))]
    NotManaged,
    Running,
    Stopped,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RuntimeProfileSource {
    #[cfg(any(test, feature = "snapshot-fixtures"))]
    FixtureController,
    SavedSources,
    Invalid,
}

impl RuntimeProfileSource {
    pub(crate) fn diagnostic_key(self) -> &'static str {
        match self {
            #[cfg(any(test, feature = "snapshot-fixtures"))]
            Self::FixtureController => "fixture-controller",
            Self::SavedSources => "saved-sources",
            Self::Invalid => "invalid",
        }
    }
}

fn generated_engine_manager(
    spec: &ManagedGeneratedProfile,
    config: ManagedEngineConfig,
    privileged: bool,
) -> Result<EngineManager, LoadError> {
    #[cfg(target_os = "macos")]
    if privileged {
        let spawner =
            crate::macos_privileged::MacosPrivilegedProcessSpawner::prepare().map_err(|error| {
                LoadError::Runtime(format!("macOS TUN helper could not be reached: {error}"))
            })?;
        return Ok(EngineManager::with_adapters(
            config,
            ReadinessPolicy::default(),
            Box::new(spawner),
            readiness_probe(spec),
        ));
    }

    #[cfg(not(target_os = "macos"))]
    let _ = privileged;
    Ok(EngineManager::new(
        config,
        ReadinessPolicy::default(),
        readiness_probe(spec),
    ))
}

fn generated_profile_names(kernel: KernelKind) -> (&'static str, &'static str) {
    match kernel {
        KernelKind::Mihomo => (CANDIDATE_PROFILE_FILE, GENERATED_PROFILE_FILE),
        KernelKind::SingBox => (SING_BOX_CANDIDATE_FILE, SING_BOX_PROFILE_FILE),
    }
}

fn compile_saved_profile(
    store_dir: &Path,
    base_subscription: Option<SecretUrl>,
    kernel: KernelKind,
) -> Result<Profile, LoadError> {
    profile_compiler::compile_saved_profile(store_dir, base_subscription, kernel)
}

fn sync_single_node_provider_files(store_dir: &Path, data_dir: &Path) -> Result<(), LoadError> {
    let provider_dir = data_dir.join("single_nodes");
    for stored in load_single_node_sources_in(store_dir)
        .map_err(|_error| {
            LoadError::Runtime("saved single-node sources could not be read".to_owned())
        })?
        .into_iter()
        .filter(|stored| stored.enabled)
    {
        let file_name = format!("{}.txt", stored.id);
        stored.source.expose_to(|value| {
            write_private_atomic(&provider_dir, &file_name, value.as_bytes()).map_err(|_error| {
                LoadError::Runtime("single-node runtime source could not be written".to_owned())
            })
        })?;
    }
    Ok(())
}

fn render_generated_profile(
    spec: &ManagedGeneratedProfile,
    profile: &Profile,
) -> Result<String, LoadError> {
    render_generated_profile_with_tun(spec, profile, false)
}

fn render_generated_profile_with_tun(
    spec: &ManagedGeneratedProfile,
    profile: &Profile,
    tun_enabled: bool,
) -> Result<String, LoadError> {
    match spec.kernel {
        KernelKind::Mihomo => render_mihomo_yaml_with_tun(profile, tun_enabled)
            .map_err(|error| LoadError::Runtime(error.to_string())),
        KernelKind::SingBox => {
            let ControllerEndpoint::Tcp(address) = spec.controller else {
                return Err(LoadError::Runtime(
                    "sing-box requires a private loopback Clash API".to_owned(),
                ));
            };
            let secret = spec.controller_secret.as_deref().ok_or_else(|| {
                LoadError::Runtime("sing-box controller has no authentication secret".to_owned())
            })?;
            render_sing_box_json(profile, &SingBoxOptions::new(address.to_string(), secret))
                .map_err(|error| LoadError::Runtime(error.to_string()))
        }
    }
}

fn compile_managed_generated_profile(spec: &ManagedGeneratedProfile) -> Result<Profile, LoadError> {
    let store_dir = spec.profile_store_dir.as_deref().ok_or_else(|| {
        LoadError::Runtime("managed kernel has no Manis source directory".to_owned())
    })?;
    compile_saved_profile(store_dir, None, spec.kernel)
}

fn managed_engine_config(
    spec: &ManagedGeneratedProfile,
    config_file: PathBuf,
) -> ManagedEngineConfig {
    match spec.kernel {
        KernelKind::Mihomo => ManagedEngineConfig::new(
            spec.binary.clone(),
            config_file,
            spec.data_dir.clone(),
            spec.controller.clone(),
        ),
        KernelKind::SingBox => ManagedEngineConfig::new_sing_box(
            spec.binary.clone(),
            config_file,
            spec.data_dir.clone(),
            spec.controller.clone(),
            spec.controller_secret.is_some(),
        ),
    }
}

fn managed_engine_config_for_privilege(
    spec: &ManagedGeneratedProfile,
    config_file: PathBuf,
    privileged: bool,
) -> Result<ManagedEngineConfig, LoadError> {
    if privileged && spec.kernel != KernelKind::Mihomo {
        return Err(LoadError::Runtime(
            "privileged managed runtime supports only Mihomo".to_owned(),
        ));
    }
    #[cfg(target_os = "linux")]
    if privileged {
        let mut privileged_spec = spec.clone();
        privileged_spec.binary = crate::linux_privileged::packaged_tun_core()
            .map_err(|error| LoadError::Runtime(error.to_string()))?;
        return Ok(managed_engine_config(&privileged_spec, config_file));
    }
    #[cfg(not(target_os = "linux"))]
    let _ = privileged;
    Ok(managed_engine_config(spec, config_file))
}

fn readiness_probe(spec: &ManagedGeneratedProfile) -> Box<dyn ReadinessProbe> {
    match spec.kernel {
        KernelKind::Mihomo => Box::new(MihomoReadinessProbe),
        KernelKind::SingBox => Box::new(SingBoxReadinessProbe {
            secret: spec.controller_secret.clone().unwrap_or_default(),
        }),
    }
}

pub(crate) struct RuntimeSnapshot {
    pub endpoint: String,
    pub controller_endpoint: String,
    pub controller_secret: Option<String>,
    pub snapshot: LoadedSnapshot,
}

pub(crate) struct LoadedSnapshot {
    pub catalog: Option<PolicyCatalog>,
    pub providers: Vec<LoadedProvider>,
    pub version: String,
    pub active_connections: usize,
    pub download_total: u64,
    pub upload_total: u64,
    pub observed_routes: Vec<ObservedRouteEvidence>,
    pub connections: Vec<Connection>,
    pub runtime: RuntimeConfig,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LoadedProvider {
    pub name: String,
    pub vehicle_type: Option<String>,
    pub nodes: Vec<LoadedProviderNode>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LoadedProviderNode {
    pub name: String,
    pub protocol: String,
    pub latency_label: Option<String>,
    pub alive: Option<bool>,
}

#[cfg(unix)]
fn load_subscription_provider(providers: &[manis_mihomo::ProxyProvider]) -> Vec<LoadedProvider> {
    providers
        .iter()
        .filter(|provider| provider.name == "subscription")
        .map(|provider| {
            let mut loaded = load_provider(provider);
            "Subscription preview".clone_into(&mut loaded.name);
            loaded
        })
        .collect()
}

#[derive(Debug)]
pub(crate) enum LoadError {
    Mihomo(MihomoError),
    Engine(EngineError),
    NoLatencyResults,
    Runtime(String),
    ProxyModeLost(String),
}

impl fmt::Display for LoadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Mihomo(error) => error.fmt(formatter),
            Self::Engine(error) => error.fmt(formatter),
            Self::NoLatencyResults => {
                formatter.write_str("Mihomo returned no positive node latency measurements")
            }
            Self::Runtime(message) | Self::ProxyModeLost(message) => formatter.write_str(message),
        }
    }
}

impl Error for LoadError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Mihomo(error) => Some(error),
            Self::Engine(error) => Some(error),
            Self::NoLatencyResults | Self::Runtime(_) | Self::ProxyModeLost(_) => None,
        }
    }
}

impl From<MihomoError> for LoadError {
    fn from(error: MihomoError) -> Self {
        Self::Mihomo(error)
    }
}

impl From<EngineError> for LoadError {
    fn from(error: EngineError) -> Self {
        Self::Engine(error)
    }
}

struct MihomoReadinessProbe;

impl ReadinessProbe for MihomoReadinessProbe {
    fn check(&mut self, endpoint: &ControllerEndpoint) -> ProbeStatus {
        fetch_version(&endpoint.uri()).map_or(ProbeStatus::Pending, |_version| ProbeStatus::Ready)
    }
}

struct SingBoxReadinessProbe {
    secret: String,
}

impl ReadinessProbe for SingBoxReadinessProbe {
    fn check(&mut self, endpoint: &ControllerEndpoint) -> ProbeStatus {
        fetch_version_with_secret(&endpoint.uri(), Some(&self.secret))
            .map_or(ProbeStatus::Pending, |_version| ProbeStatus::Ready)
    }
}

#[cfg(test)]
use runtime_build::{
    build_saved_sources_mihomo_runtime_in, discover_sing_box_binary,
    first_unsupported_runtime_override,
};
use runtime_build::{configured_mixed_port, has_only_clean_components};
pub(crate) use runtime_build::{
    configured_runtime, configured_sing_box_runtime, sing_box_binary_available,
};

#[cfg(unix)]
use controller_io::unix_socket_path;
#[cfg(test)]
use controller_io::{
    GlobalSelectionRoute, fetch_sing_box_snapshot, global_selection_route, is_selector_proxy_type,
    policy_group_runtime_snapshot, put_policy_group_selection,
};
use controller_io::{
    fetch_group_delay, fetch_policy_group, fetch_runtime_config, fetch_version,
    fetch_version_with_secret, load_provider, reload_mihomo_config, running_managed_endpoint,
    select_global_node_at_endpoint, select_policy_group_candidate, set_routing_mode,
    validate_managed_runtime, with_controller_secret,
};
pub(crate) use controller_io::{load, load_sing_box};

#[cfg(all(test, unix))]
#[path = "mihomo/tests.rs"]
mod tests;
