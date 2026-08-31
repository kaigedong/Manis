use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::fs;
use std::io::Read;
use std::net::TcpListener;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
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
    Connection, ConnectionsState, ControllerConfig, ControllerTransport, LiveController,
    MihomoClient, MihomoError, MihomoLogEntry, MihomoSnapshot, ObservedRouteEvidence,
    RuntimeConfig, StdHttpTransport, VersionInfo, to_policy_catalog,
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

mod managed_apply;
mod policy_store;
mod preview;
mod profile_compiler;
mod routing_order;
mod runtime;
mod store_snapshot;
mod workspace;

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
    load_routing_rule_group_order_in, move_routing_rule_group, normalized_routing_rule_group_order,
    save_routing_rule_group_order_in,
};
pub(crate) use store_snapshot::SubscriptionStoreSnapshot;
pub(crate) use workspace::*;
use workspace::{
    apply_qx_rule_sources, decode_hex, next_stored_source_id, profile_mode, valid_stored_id,
};
#[cfg(test)]
use workspace::{current_unix_nanos, storage_version_supported};
#[cfg(not(windows))]
use workspace::{
    private_store_entries, read_private_source_allow_empty, read_private_source_allow_empty_max,
    remove_private_source, require_clean_absolute_store,
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PolicyGroupBenchmarkSnapshot {
    pub delays: BTreeMap<String, u16>,
    pub current: Option<String>,
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct KernelLogEntry {
    pub sequence: u64,
    pub level: String,
    pub payload: String,
    pub timestamp_ms: u128,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LiveStreamStatus {
    pub activity: LiveStreamPhase,
    pub logs: LiveStreamPhase,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum LiveStreamPhase {
    Waiting,
    Connecting,
    Live,
    Unavailable,
    Reconnecting(usize),
    InterruptedHttp(u16),
    InvalidData,
    ControllerUnavailable,
    Retrying,
    StartFailed(String),
}

impl Default for LiveStreamStatus {
    fn default() -> Self {
        Self {
            activity: LiveStreamPhase::Waiting,
            logs: LiveStreamPhase::Waiting,
        }
    }
}

#[derive(Default)]
struct LiveMailbox {
    latest_connections: Option<ConnectionsState>,
    logs: VecDeque<KernelLogEntry>,
    dropped_logs: u64,
    status: LiveStreamStatus,
}

pub(crate) struct LiveRuntimeUpdate {
    pub connections: Option<ConnectionsState>,
    pub logs: Vec<KernelLogEntry>,
    pub dropped_logs: u64,
    pub status: LiveStreamStatus,
}

pub(crate) struct LiveRuntimeSession {
    cancelled: Arc<AtomicBool>,
    mailbox: Arc<Mutex<LiveMailbox>>,
}

impl LiveRuntimeSession {
    pub(crate) fn start(
        endpoint: &str,
        controller_secret: Option<&str>,
    ) -> Result<Self, LoadError> {
        let controller = live_controller(endpoint, controller_secret)?;
        let cancelled = Arc::new(AtomicBool::new(false));
        let mailbox = Arc::new(Mutex::new(LiveMailbox::default()));
        spawn_connection_stream(controller.clone(), cancelled.clone(), mailbox.clone());
        spawn_log_stream(controller, cancelled.clone(), mailbox.clone());
        Ok(Self { cancelled, mailbox })
    }

    pub(crate) fn drain(&self) -> LiveRuntimeUpdate {
        let Ok(mut mailbox) = self.mailbox.lock() else {
            return LiveRuntimeUpdate {
                connections: None,
                logs: Vec::new(),
                dropped_logs: 0,
                status: LiveStreamStatus {
                    activity: LiveStreamPhase::Unavailable,
                    logs: LiveStreamPhase::Unavailable,
                },
            };
        };
        LiveRuntimeUpdate {
            connections: mailbox.latest_connections.take(),
            logs: mailbox.logs.drain(..).collect(),
            dropped_logs: std::mem::take(&mut mailbox.dropped_logs),
            status: mailbox.status.clone(),
        }
    }
}

impl Drop for LiveRuntimeSession {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
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

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct ProxyDelayTarget {
    name: String,
    provider: Option<String>,
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

    pub(crate) fn from_policy_node(node: &manis_core::PolicyNode) -> Self {
        if node.kind == manis_core::PolicyCandidateKind::Node
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

pub(crate) fn configured_runtime(store_dir: Option<&Path>) -> ControllerRuntime {
    if let Some(variable) = first_unsupported_runtime_override(|name| env::var_os(name).is_some()) {
        return ControllerRuntime::Invalid {
            message: format!(
                "{variable} is no longer supported; Mihomo configuration and controller settings are managed only by Manis"
            ),
        };
    }
    let Some(store_dir) = store_dir else {
        return ControllerRuntime::Invalid {
            message: "Manis source directory could not be determined".to_owned(),
        };
    };
    #[cfg(debug_assertions)]
    let binary = brand::env_var_os(BINARY_ENV, LEGACY_RELAY_BINARY_ENV).map_or_else(
        discover_mihomo_binary,
        |binary| {
            canonical_binary(Path::new(&binary))
                .map_err(|_error| format!("{BINARY_ENV} does not point to an executable file"))
        },
    );
    #[cfg(not(debug_assertions))]
    let binary = discover_mihomo_binary();
    binary
        .and_then(|binary| build_saved_sources_mihomo_runtime_with_binary(store_dir, &binary))
        .unwrap_or_else(|message| ControllerRuntime::Invalid { message })
}

fn first_unsupported_runtime_override(is_set: impl Fn(&str) -> bool) -> Option<&'static str> {
    UNSUPPORTED_MIHOMO_RUNTIME_ENV
        .into_iter()
        .find(|name| is_set(name))
}

pub(crate) fn configured_sing_box_runtime(store_dir: &Path) -> Result<ControllerRuntime, String> {
    build_sing_box_runtime(store_dir)
}

fn build_saved_sources_mihomo_runtime_with_binary(
    store_dir: &Path,
    binary: &Path,
) -> Result<ControllerRuntime, String> {
    let data_dir = configured_data_dir()?;
    #[cfg(unix)]
    let controller = configured_managed_controller(&data_dir);
    #[cfg(not(unix))]
    let controller = configured_managed_controller(&data_dir)?;
    build_saved_sources_mihomo_runtime_in(store_dir, binary, &data_dir, &controller)
}

fn build_saved_sources_mihomo_runtime_in(
    store_dir: &Path,
    binary: &Path,
    data_dir: &Path,
    controller: &ControllerEndpoint,
) -> Result<ControllerRuntime, String> {
    sync_single_node_provider_files(store_dir, data_dir).map_err(|error| error.to_string())?;
    let profile = compile_saved_profile(store_dir, None, KernelKind::Mihomo)
        .map_err(|error| error.to_string())?;
    let spec = ManagedGeneratedProfile {
        kernel: KernelKind::Mihomo,
        binary: binary.to_path_buf(),
        data_dir: data_dir.to_path_buf(),
        controller: controller.clone(),
        expected_mixed_port: Some(profile.mixed_port),
        profile_store_dir: Some(store_dir.to_path_buf()),
        controller_secret: None,
    };
    let rendered = render_generated_profile(&spec, &profile).map_err(|error| error.to_string())?;
    let config_file =
        write_private_atomic(data_dir, GENERATED_PROFILE_FILE, rendered.as_bytes())
            .map_err(|_error| "private Mihomo configuration could not be written".to_owned())?;
    let config = managed_engine_config(&spec, config_file);
    validate_managed_config(&config).map_err(|error| error.to_string())?;
    let manager = EngineManager::new(config, ReadinessPolicy::default(), readiness_probe(&spec));
    Ok(ControllerRuntime::Managed {
        manager: Arc::new(Mutex::new(manager)),
        apply_lock: Arc::new(Mutex::new(())),
        profile_source: RuntimeProfileSource::SavedSources,
        generated_profile: Some(spec),
        privileged: Arc::new(AtomicBool::new(false)),
    })
}

fn build_sing_box_runtime(store_dir: &Path) -> Result<ControllerRuntime, String> {
    let binary = discover_sing_box_binary()?;
    let data_dir = brand::data_dir()
        .map(|directory| directory.join("sing-box"))
        .ok_or_else(|| "sing-box data directory could not be determined".to_owned())?;
    let port = TcpListener::bind("127.0.0.1:0")
        .and_then(|listener| listener.local_addr())
        .map(|address| address.port())
        .map_err(|_error| {
            "a loopback controller port could not be reserved for sing-box".to_owned()
        })?;
    let controller = ControllerEndpoint::Tcp(
        format!("127.0.0.1:{port}")
            .parse()
            .map_err(|_error| "sing-box controller address could not be created".to_owned())?,
    );
    let controller_secret = generate_controller_secret()?;
    let profile = compile_saved_profile(store_dir, None, KernelKind::SingBox)
        .map_err(|error| error.to_string())?;
    let spec = ManagedGeneratedProfile {
        kernel: KernelKind::SingBox,
        binary: binary.clone(),
        data_dir: data_dir.clone(),
        controller: controller.clone(),
        expected_mixed_port: None,
        profile_store_dir: Some(store_dir.to_path_buf()),
        controller_secret: Some(controller_secret.clone()),
    };
    let rendered = render_generated_profile(&spec, &profile).map_err(|error| error.to_string())?;
    let config_file =
        write_private_atomic(&data_dir, SING_BOX_PROFILE_FILE, rendered.as_bytes())
            .map_err(|_error| "private sing-box configuration could not be written".to_owned())?;
    let config = managed_engine_config(&spec, config_file);
    validate_managed_config(&config).map_err(|error| error.to_string())?;
    let manager = EngineManager::new(config, ReadinessPolicy::default(), readiness_probe(&spec));
    Ok(ControllerRuntime::Managed {
        manager: Arc::new(Mutex::new(manager)),
        apply_lock: Arc::new(Mutex::new(())),
        profile_source: RuntimeProfileSource::SavedSources,
        generated_profile: Some(spec),
        privileged: Arc::new(AtomicBool::new(false)),
    })
}

fn discover_sing_box_binary() -> Result<PathBuf, String> {
    if let Some(explicit) = brand::env_var_os(SING_BOX_BINARY_ENV, LEGACY_RELAY_SING_BOX_BINARY_ENV)
    {
        return canonical_binary(Path::new(&explicit)).map_err(|_error| {
            format!("{SING_BOX_BINARY_ENV} does not point to an executable file")
        });
    }
    let executable_name = if cfg!(windows) {
        "sing-box.exe"
    } else {
        "sing-box"
    };
    let mut candidates = Vec::new();
    if let Ok(current_exe) = env::current_exe()
        && let Some(directory) = current_exe.parent()
    {
        candidates.push(directory.join(executable_name));
    }
    if let Some(path) = env::var_os("PATH") {
        candidates.extend(env::split_paths(&path).map(|directory| directory.join(executable_name)));
    }
    #[cfg(target_os = "macos")]
    candidates.extend([
        PathBuf::from("/opt/homebrew/bin/sing-box"),
        PathBuf::from("/usr/local/bin/sing-box"),
    ]);
    candidates
        .into_iter()
        .find_map(|candidate| canonical_binary(&candidate).ok())
        .ok_or_else(|| format!("sing-box was not found; install it or set {SING_BOX_BINARY_ENV}"))
}

pub(crate) fn sing_box_binary_available() -> bool {
    discover_sing_box_binary().is_ok()
}

fn discover_mihomo_binary() -> Result<PathBuf, String> {
    core_update::managed_core_binary_path()
        .map_err(|error| error.to_string())
        .and_then(|path| {
            canonical_binary(&path).map_err(|_error| {
                "Manis-managed Mihomo is not installed; download the stable core in Runtime settings"
                    .to_owned()
            })
        })
}

#[cfg(unix)]
fn generate_controller_secret() -> Result<String, String> {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut random = [0_u8; 32];
    fs::File::open("/dev/urandom")
        .and_then(|mut file| file.read_exact(&mut random))
        .map_err(|_error| "sing-box controller secret could not be generated".to_owned())?;
    let mut secret = String::with_capacity(random.len() * 2);
    for byte in random {
        secret.push(char::from(HEX[usize::from(byte >> 4)]));
        secret.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    Ok(secret)
}

#[cfg(windows)]
fn generate_controller_secret() -> Result<String, String> {
    let output = std::process::Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "[guid]::NewGuid().ToString('N') + [guid]::NewGuid().ToString('N')",
        ])
        .output()
        .map_err(|_error| "sing-box controller secret could not be generated".to_owned())?;
    let secret = String::from_utf8(output.stdout)
        .map_err(|_error| "sing-box controller secret could not be generated".to_owned())?;
    let secret = secret.trim();
    if !output.status.success()
        || secret.len() != 64
        || !secret.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err("sing-box controller secret could not be generated".to_owned());
    }
    Ok(secret.to_owned())
}

fn has_only_clean_components(path: &Path) -> bool {
    path.components().all(|component| {
        matches!(
            component,
            Component::Prefix(_) | Component::RootDir | Component::Normal(_)
        )
    })
}

fn configured_mixed_port() -> Result<u16, String> {
    match brand::env_var_os(MIXED_PORT_ENV, LEGACY_RELAY_MIXED_PORT_ENV) {
        Some(value) => value
            .to_str()
            .ok_or_else(|| format!("{MIXED_PORT_ENV} must be valid Unicode"))?
            .parse::<u16>()
            .ok()
            .filter(|port| *port != 0)
            .ok_or_else(|| format!("{MIXED_PORT_ENV} must be a port from 1 to 65535")),
        None => Ok(DEFAULT_MANAGED_MIXED_PORT),
    }
}

fn configured_data_dir() -> Result<PathBuf, String> {
    brand::env_var_os(DATA_DIR_ENV, LEGACY_RELAY_DATA_DIR_ENV)
        .map(PathBuf::from)
        .or_else(default_data_dir)
        .ok_or_else(|| format!("data directory could not be determined; set {DATA_DIR_ENV}"))
}

#[cfg(unix)]
fn configured_managed_controller(data_dir: &Path) -> ControllerEndpoint {
    default_managed_endpoint(data_dir)
}

#[cfg(not(unix))]
fn configured_managed_controller(data_dir: &Path) -> Result<ControllerEndpoint, String> {
    default_managed_endpoint(data_dir)
}

#[cfg(unix)]
fn default_managed_endpoint(data_dir: &Path) -> ControllerEndpoint {
    ControllerEndpoint::UnixSocket(data_dir.join("controller.sock"))
}

#[cfg(windows)]
fn default_managed_endpoint(_data_dir: &Path) -> Result<ControllerEndpoint, String> {
    Err("managed Mihomo controller transport is not implemented on Windows".to_owned())
}

#[cfg(not(any(unix, windows)))]
fn default_managed_endpoint(_data_dir: &Path) -> Result<ControllerEndpoint, String> {
    Err("this platform has no default Mihomo controller transport".to_owned())
}

fn default_data_dir() -> Option<PathBuf> {
    brand::data_dir().map(|directory| directory.join("mihomo"))
}

pub(crate) fn load(
    endpoint: &str,
    controller_secret: Option<&str>,
) -> Result<LoadedSnapshot, LoadError> {
    Ok(loaded_snapshot(&fetch_snapshot(
        endpoint,
        controller_secret,
    )?))
}

pub(crate) fn load_sing_box(
    endpoint: &str,
    controller_secret: Option<&str>,
) -> Result<LoadedSnapshot, LoadError> {
    Ok(loaded_snapshot(&fetch_sing_box_snapshot(
        endpoint,
        controller_secret,
    )?))
}

fn loaded_snapshot(snapshot: &MihomoSnapshot) -> LoadedSnapshot {
    let catalog = to_policy_catalog(snapshot).ok();
    let providers = load_providers(&snapshot.providers);
    let version = snapshot
        .version
        .version
        .clone()
        .unwrap_or_else(|| "unknown version".to_owned());
    let active_connections = snapshot.connections.connections.len();
    let download_total = snapshot.connections.download_total;
    let upload_total = snapshot.connections.upload_total;
    let observed_routes = snapshot.observed_routes();
    let connections = snapshot.connections.connections.clone();
    let runtime = snapshot.runtime.clone();

    LoadedSnapshot {
        catalog,
        providers,
        version,
        active_connections,
        download_total,
        upload_total,
        observed_routes,
        connections,
        runtime,
    }
}

fn validate_managed_runtime(
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

fn live_controller(
    endpoint: &str,
    controller_secret: Option<&str>,
) -> Result<LiveController, MihomoError> {
    #[cfg(unix)]
    if let Some(socket_path) = unix_socket_path(endpoint)? {
        return Ok(LiveController::unix_socket(
            ControllerConfig::default(),
            socket_path,
        ));
    }

    #[cfg(not(unix))]
    if endpoint.starts_with("unix://") {
        return Err(MihomoError::InvalidConfig(
            "Unix controller sockets are not supported on this platform".to_owned(),
        ));
    }

    Ok(LiveController::loopback(with_controller_secret(
        ControllerConfig::new(endpoint)?,
        controller_secret,
    )))
}

fn spawn_connection_stream(
    controller: LiveController,
    cancelled: Arc<AtomicBool>,
    mailbox: Arc<Mutex<LiveMailbox>>,
) {
    thread::spawn(move || {
        let mut first_request = true;
        // Without a WebSocket upgrade Mihomo returns one HTTP snapshot. Poll at
        // the requested interval and keep a healthy snapshot live between polls.
        reconnect_live_stream(&cancelled, LIVE_CONNECTION_INTERVAL, |attempt| {
            if first_request || attempt > 0 {
                set_live_status(&mailbox, true, stream_phase(attempt));
            }
            first_request = false;
            let result = controller.stream_connections(
                LIVE_CONNECTION_INTERVAL,
                &cancelled,
                |connections| {
                    if let Ok(mut mailbox) = mailbox.lock() {
                        mailbox.latest_connections = Some(connections);
                        mailbox.status.activity = LiveStreamPhase::Live;
                    }
                },
            );
            if let Err(error) = &result {
                set_live_status(&mailbox, true, safe_stream_error(error));
            }
            result
        });
    });
}

fn spawn_log_stream(
    controller: LiveController,
    cancelled: Arc<AtomicBool>,
    mailbox: Arc<Mutex<LiveMailbox>>,
) {
    thread::spawn(move || {
        let mut sequence = 0_u64;
        reconnect_live_stream(&cancelled, Duration::from_millis(250), |attempt| {
            set_live_status(&mailbox, false, stream_phase(attempt));
            let result = controller.stream_logs("info", &cancelled, |entry| {
                sequence = sequence.wrapping_add(1);
                push_kernel_log(&mailbox, sequence, &entry);
            });
            if let Err(error) = &result {
                set_live_status(&mailbox, false, safe_stream_error(error));
            }
            result
        });
    });
}

fn reconnect_live_stream(
    cancelled: &AtomicBool,
    success_delay: Duration,
    mut connect: impl FnMut(usize) -> Result<(), MihomoError>,
) {
    let mut attempt = 0_usize;
    while !cancelled.load(Ordering::Relaxed) {
        let result = connect(attempt);
        if cancelled.load(Ordering::Relaxed) {
            break;
        }
        if result.is_ok() {
            attempt = 0;
        } else {
            attempt = attempt.saturating_add(1);
        }
        let shift = u32::try_from(attempt.min(5)).unwrap_or(5);
        let delay = if result.is_ok() {
            success_delay
        } else {
            Duration::from_millis(250_u64.saturating_mul(1_u64 << shift)).min(LIVE_RETRY_MAX)
        };
        let started = std::time::Instant::now();
        while started.elapsed() < delay && !cancelled.load(Ordering::Relaxed) {
            thread::sleep(Duration::from_millis(50));
        }
    }
}

fn stream_phase(attempt: usize) -> LiveStreamPhase {
    if attempt == 0 {
        LiveStreamPhase::Connecting
    } else {
        LiveStreamPhase::Reconnecting(attempt)
    }
}

fn safe_stream_error(error: &MihomoError) -> LiveStreamPhase {
    match error {
        MihomoError::HttpStatus { status_code, .. } => {
            LiveStreamPhase::InterruptedHttp(*status_code)
        }
        MihomoError::Json { .. } => LiveStreamPhase::InvalidData,
        MihomoError::Io(_) => LiveStreamPhase::ControllerUnavailable,
        _ => LiveStreamPhase::Retrying,
    }
}

fn set_live_status(mailbox: &Mutex<LiveMailbox>, activity: bool, status: LiveStreamPhase) {
    if let Ok(mut mailbox) = mailbox.lock() {
        if activity {
            mailbox.status.activity = status;
        } else {
            mailbox.status.logs = status;
        }
    }
}

fn push_kernel_log(mailbox: &Mutex<LiveMailbox>, sequence: u64, entry: &MihomoLogEntry) {
    let Ok(mut mailbox) = mailbox.lock() else {
        return;
    };
    if mailbox.logs.len() == LIVE_LOG_MAILBOX_CAPACITY {
        mailbox.logs.pop_front();
        mailbox.dropped_logs = mailbox.dropped_logs.saturating_add(1);
    }
    mailbox.logs.push_back(KernelLogEntry {
        sequence,
        level: sanitize_log_field(&entry.level, 16),
        payload: sanitize_kernel_log(&entry.payload),
        timestamp_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis(),
    });
    mailbox.status.logs = LiveStreamPhase::Live;
}

fn sanitize_log_field(value: &str, limit: usize) -> String {
    value
        .chars()
        .filter(|character| !character.is_control())
        .take(limit)
        .collect()
}

fn sanitize_kernel_log(value: &str) -> String {
    let bounded: String = value
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .take(2_048)
        .collect();
    let mut output = String::with_capacity(bounded.len());
    let mut remainder = bounded.as_str();
    while !remainder.is_empty() {
        let lowercase = remainder.to_ascii_lowercase();
        let next_secret = ["https://", "http://", "vless://"]
            .into_iter()
            .filter_map(|prefix| lowercase.find(prefix).map(|index| (index, prefix.len())))
            .min_by_key(|(index, _prefix)| *index);
        let Some((index, prefix_len)) = next_secret else {
            output.push_str(remainder);
            break;
        };
        output.push_str(&remainder[..index]);
        output.push_str("<redacted-url>");
        let secret = &remainder[index + prefix_len..];
        let end = secret
            .find(|character: char| character.is_whitespace() || matches!(character, '"' | '\''))
            .unwrap_or(secret.len());
        remainder = &secret[end..];
    }
    output
}

fn load_providers(providers: &[manis_mihomo::ProxyProvider]) -> Vec<LoadedProvider> {
    providers.iter().map(load_provider).collect()
}

fn load_provider(provider: &manis_mihomo::ProxyProvider) -> LoadedProvider {
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

fn fetch_snapshot(
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

fn fetch_sing_box_snapshot(
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

fn fetch_group_delay(
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

fn fetch_policy_group(
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

fn put_policy_group_selection(
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

fn policy_group_runtime_snapshot(
    group: manis_mihomo::MihomoPolicyGroup,
) -> ManagedPolicyRuntimeSnapshot {
    ManagedPolicyRuntimeSnapshot {
        current: group.current,
        candidates: group.all.into_iter().collect(),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum GlobalSelectionRoute {
    Direct,
    ViaGlobalExit,
}

fn global_selection_route(
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

fn select_global_node_at_endpoint(
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

fn select_policy_group_candidate(
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

fn is_selector_proxy_type(proxy_type: &str) -> bool {
    proxy_type.eq_ignore_ascii_case("Selector")
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

fn delay_controller_config(config: ControllerConfig) -> ControllerConfig {
    let connect_timeout = config.connect_timeout();
    config.with_timeouts(connect_timeout, GROUP_DELAY_CONTROLLER_READ_TIMEOUT)
}

fn fetch_proxy_delay_targets_bounded_with_progress(
    endpoint: &str,
    targets: &[ProxyDelayTarget],
    controller_secret: Option<&str>,
    mut on_result: impl FnMut(&str, Option<u16>),
) -> Result<BTreeMap<String, u16>, LoadError> {
    if targets.is_empty() {
        return Err(LoadError::Runtime(
            "the current group has no nodes that can be benchmarked".to_owned(),
        ));
    }
    let worker_count = targets.len().min(GROUP_DELAY_WORKERS);
    let chunk_size = targets.len().div_ceil(worker_count);
    let (delays, first_error) = thread::scope(|scope| {
        let (sender, receiver) = mpsc::channel();
        let handles = targets
            .chunks(chunk_size)
            .map(|chunk| {
                let sender = sender.clone();
                scope.spawn(move || {
                    for target in chunk {
                        let result = fetch_proxy_delay_target(endpoint, target, controller_secret);
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
        for handle in handles {
            drop(handle.join());
        }
        (delays, first_error)
    });
    if delays.is_empty() {
        return Err(first_error.map_or(LoadError::NoLatencyResults, LoadError::from));
    }
    Ok(delays)
}

fn running_managed_endpoint(manager: &Arc<Mutex<EngineManager>>) -> Result<String, LoadError> {
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

fn reload_mihomo_config(
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

fn fetch_runtime_config(
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

fn set_routing_mode(
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

fn fetch_version(endpoint: &str) -> Result<VersionInfo, MihomoError> {
    fetch_version_with_secret(endpoint, None)
}

fn fetch_version_with_secret(
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

fn with_controller_secret(
    mut config: ControllerConfig,
    controller_secret: Option<&str>,
) -> ControllerConfig {
    if let Some(secret) = controller_secret {
        config = config.with_secret(secret.to_owned());
    }
    config
}

#[cfg(unix)]
fn unix_socket_path(endpoint: &str) -> Result<Option<PathBuf>, MihomoError> {
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

#[cfg(all(test, unix))]
mod tests {
    use std::collections::BTreeSet;
    use std::fs;
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Condvar, Mutex};
    use std::time::Duration;

    use manis_engine::ControllerEndpoint;

    #[test]
    fn managed_mihomo_rejects_a_controller_with_a_failed_mixed_listener() {
        let spec = super::ManagedGeneratedProfile {
            kernel: manis_core::KernelKind::Mihomo,
            binary: PathBuf::from("/Applications/Manis.app/Contents/Resources/mihomo/mihomo"),
            data_dir: PathBuf::from("/tmp/manis-runtime"),
            controller: ControllerEndpoint::UnixSocket(PathBuf::from(
                "/tmp/manis-runtime/controller.sock",
            )),
            expected_mixed_port: Some(17_890),
            profile_store_dir: None,
            controller_secret: None,
        };
        let failed = manis_mihomo::RuntimeConfig {
            mixed_port: Some(0),
            ..manis_mihomo::RuntimeConfig::default()
        };
        let ready = manis_mihomo::RuntimeConfig {
            mixed_port: Some(17_890),
            ..manis_mihomo::RuntimeConfig::default()
        };

        assert!(super::validate_managed_runtime(&spec, &failed).is_err());
        assert!(super::validate_managed_runtime(&spec, &ready).is_ok());
    }

    #[test]
    fn legacy_relay_storage_versions_remain_readable() {
        assert!(super::storage_version_supported(
            Some(super::LEGACY_RELAY_STORED_SUBSCRIPTION_VERSION),
            super::STORED_SUBSCRIPTION_VERSION,
            super::LEGACY_RELAY_STORED_SUBSCRIPTION_VERSION,
        ));
        assert!(super::storage_version_supported(
            Some(super::LEGACY_RELAY_QX_RULE_SOURCE_VERSION),
            super::QX_RULE_SOURCE_VERSION,
            super::LEGACY_RELAY_QX_RULE_SOURCE_VERSION,
        ));
        assert!(super::storage_version_supported(
            Some(super::LEGACY_RELAY_NODE_SELECTION_PREFERENCES_VERSION),
            super::NODE_SELECTION_PREFERENCES_VERSION,
            super::LEGACY_RELAY_NODE_SELECTION_PREFERENCES_VERSION,
        ));
    }

    #[test]
    #[ignore = "requires a locally installed sing-box executable"]
    fn managed_sing_box_clash_api_loads_a_manis_snapshot() -> Result<(), Box<dyn std::error::Error>>
    {
        let binary = super::discover_sing_box_binary()?;
        let root = test_temp_dir("manis-sing-box-runtime");
        let data_dir = root.join("runtime");
        let vless = manis_profile::VlessProxy::parse_share_link(
            "vless://00000000-0000-4000-8000-000000000000@198.51.100.7:443?security=reality&encryption=none&pbk=Qs24XU-ibEZ3LWDjGBITKdQvualLy0pi_PI0qoF79A8&fp=chrome&type=tcp&flow=xtls-rprx-vision&sni=cdn.example.invalid#Reality%20TCP",
        )?;
        let mut profile = manis_profile::Profile::qx_sources(Vec::new(), vec![vless], 17_890)?;
        profile.rules = vec![manis_profile::Rule::Match {
            policy: manis_profile::PolicyRef::Group(manis_profile::Name::parse("Proxy")?),
        }];
        let address = TcpListener::bind("127.0.0.1:0")?.local_addr()?;
        let controller = ControllerEndpoint::Tcp(address);
        let secret = "fixture-controller-secret".to_owned();
        let spec = super::ManagedGeneratedProfile {
            kernel: manis_core::KernelKind::SingBox,
            binary,
            data_dir: data_dir.clone(),
            controller: controller.clone(),
            expected_mixed_port: None,
            profile_store_dir: None,
            controller_secret: Some(secret.clone()),
        };
        let rendered = super::render_generated_profile(&spec, &profile)?;
        let config_file = manis_profile::write_private_atomic(
            &data_dir,
            super::SING_BOX_PROFILE_FILE,
            rendered.as_bytes(),
        )?;
        let config = super::managed_engine_config(&spec, config_file);
        let mut manager = manis_engine::EngineManager::new(
            config,
            manis_engine::ReadinessPolicy::default(),
            super::readiness_probe(&spec),
        );
        let endpoint = manager.start()?.uri();

        super::set_routing_mode(&endpoint, manis_core::RoutingMode::Direct, Some(&secret))?;
        let runtime = super::fetch_sing_box_snapshot(&endpoint, Some(&secret))?;
        assert_eq!(runtime.runtime.mode, manis_core::RoutingMode::Direct);
        super::put_policy_group_selection(&endpoint, "GLOBAL", "Reality TCP", Some(&secret))?;
        let global = super::fetch_policy_group(&endpoint, "GLOBAL", Some(&secret))?;
        assert_eq!(global.current.as_deref(), Some("Reality TCP"));
        super::put_policy_group_selection(&endpoint, "Proxy", "Auto", Some(&secret))?;
        let selected = super::fetch_policy_group(&endpoint, "Proxy", Some(&secret))?;
        assert_eq!(selected.current.as_deref(), Some("Auto"));
        let snapshot = super::load_sing_box(&endpoint, Some(&secret));
        manager.stop()?;
        fs::remove_dir_all(root)?;
        snapshot?;
        Ok(())
    }

    #[test]
    fn remote_source_refresh_intervals_cycle_and_respect_last_success() {
        use super::RemoteSourceRefreshInterval as Interval;

        assert!(!Interval::Manual.is_due(0, u64::MAX));
        assert!(Interval::Hourly.is_due(0, 1));
        assert!(!Interval::Hourly.is_due(10_000, 13_599));
        assert!(Interval::Hourly.is_due(10_000, 13_600));
        assert!(!Interval::Daily.is_due(100_000, 99_000));
    }

    #[test]
    fn kernel_log_sanitizer_redacts_urls_and_bounds_dynamic_payloads() {
        let input = format!(
            "provider https://example.invalid/client?token=fixture-secret failed; node vless://uuid@host:443 {}",
            "x".repeat(3_000)
        );
        let sanitized = super::sanitize_kernel_log(&input);

        assert!(sanitized.contains("<redacted-url>"));
        assert!(!sanitized.contains("fixture-secret"));
        assert!(!sanitized.contains("vless://"));
        assert!(sanitized.chars().count() <= 2_048);

        let uppercase = super::sanitize_kernel_log(
            "provider HTTPS://example.invalid/path?token=uppercase-secret failed",
        );
        assert_eq!(
            uppercase, "provider <redacted-url> failed",
            "URI schemes are case-insensitive"
        );
    }

    #[test]
    fn preview_workspace_is_private_and_removed_on_drop() -> Result<(), Box<dyn std::error::Error>>
    {
        let workspace = super::PreviewWorkspace::create()?;
        let path = workspace.path().to_owned();
        assert!(path.is_dir());

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&path)?.permissions().mode() & 0o077, 0);
        }

        drop(workspace);
        assert!(!path.exists());
        Ok(())
    }

    #[test]
    fn preview_errors_never_expose_subscription_input() {
        let input = "https://subscription.example.invalid/private-token";
        let error = super::preview_subscription_with_binary(input, Path::new("/missing/mihomo"))
            .expect_err("missing preview binary should fail safely");

        assert!(!error.to_string().contains("private-token"));
        assert!(!format!("{error:?}").contains("private-token"));
    }

    #[cfg(not(windows))]
    #[test]
    fn imported_subscription_round_trips_privately_and_replaces_atomically()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = test_temp_dir("manis-import-store");
        let store = root.join("subscriptions");
        let first = "https://first.example.invalid/client?token=fixture-one";
        let second = "https://second.example.invalid/client?token=fixture-two";

        let first_secret = super::save_imported_subscription_in(&store, first)?;
        assert_eq!(
            super::load_imported_subscription_in(&store)?,
            Some(first_secret)
        );
        let second_secret = super::save_imported_subscription_in(&store, second)?;
        assert_eq!(
            super::load_imported_subscription_in(&store)?,
            Some(second_secret)
        );

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(fs::metadata(&store)?.permissions().mode() & 0o077, 0);
            assert_eq!(
                fs::metadata(store.join("subscription.url"))?
                    .permissions()
                    .mode()
                    & 0o077,
                0
            );
        }

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn routing_mode_round_trips_in_the_private_workspace_store()
    -> Result<(), Box<dyn std::error::Error>> {
        use manis_core::RoutingMode;

        let root = test_temp_dir("manis-routing-mode-store");
        let store = root.join("subscriptions");
        assert_eq!(super::load_routing_mode_in(&store)?, RoutingMode::Rule);

        super::save_routing_mode_in(&store, RoutingMode::Global)?;
        assert_eq!(super::load_routing_mode_in(&store)?, RoutingMode::Global);

        super::save_routing_mode_in(&store, RoutingMode::Direct)?;
        assert_eq!(super::load_routing_mode_in(&store)?, RoutingMode::Direct);
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[cfg(not(windows))]
    #[test]
    fn node_selection_preferences_missing_file_defaults() -> Result<(), Box<dyn std::error::Error>>
    {
        let root = test_temp_dir("manis-node-selection-missing");
        let store = root.join("subscriptions");

        assert_eq!(
            super::load_node_selection_preferences_in(&store)?,
            super::NodeSelectionPreferences::default()
        );

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[cfg(not(windows))]
    #[test]
    fn node_selection_preferences_round_trip_privately() -> Result<(), Box<dyn std::error::Error>> {
        use manis_core::NodeIdentity;

        let root = test_temp_dir("manis-node-selection-store");
        let store = root.join("subscriptions");
        let global = NodeIdentity::new("subscription:source-1", "Hong Kong Edge")?;
        let mut preferences = super::NodeSelectionPreferences::default();
        preferences.set_global(global.clone());
        preferences.set_policy_target("视频服务", "Tokyo Manual")?;

        super::save_node_selection_preferences_in(&store, &preferences)?;
        let loaded = super::load_node_selection_preferences_in(&store)?;

        assert_eq!(loaded.global(), Some(&global));
        assert_eq!(loaded.policy_target("视频服务"), Some("Tokyo Manual"));
        assert_eq!(
            loaded.iter_policy_targets().collect::<Vec<_>>(),
            vec![("视频服务", "Tokyo Manual")]
        );

        #[cfg(unix)]
        {
            assert_eq!(fs::metadata(&store)?.permissions().mode() & 0o077, 0);
            let path = store.join(super::NODE_SELECTION_PREFERENCES_FILE);
            assert_eq!(fs::metadata(&path)?.permissions().mode() & 0o077, 0);
            let stored_text = fs::read_to_string(path)?;
            assert!(!stored_text.contains("Hong Kong Edge"));
            assert!(!stored_text.contains("Tokyo Manual"));
        }

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[cfg(not(windows))]
    #[test]
    fn node_selection_preferences_reject_malformed_and_duplicate_records()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = test_temp_dir("manis-node-selection-invalid");
        let store = root.join("subscriptions");
        fs::create_dir(&store)?;
        fs::set_permissions(&store, fs::Permissions::from_mode(0o700))?;
        let path = store.join(super::NODE_SELECTION_PREFERENCES_FILE);

        let duplicate = [
            super::NODE_SELECTION_PREFERENCES_VERSION.to_owned(),
            format!(
                "policy\t{}\t{}",
                super::encode_hex("Proxy"),
                super::encode_hex("Hong Kong Edge")
            ),
            format!(
                "policy\t{}\t{}",
                super::encode_hex("Proxy"),
                super::encode_hex("Tokyo Edge")
            ),
        ]
        .join("\n");
        fs::write(&path, duplicate)?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
        assert!(super::load_node_selection_preferences_in(&store).is_err());

        let malformed = [
            super::NODE_SELECTION_PREFERENCES_VERSION.to_owned(),
            "global\tnot-hex\talso-not-hex".to_owned(),
        ]
        .join("\n");
        fs::write(&path, malformed)?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
        assert!(super::load_node_selection_preferences_in(&store).is_err());

        let invalid_target = [
            super::NODE_SELECTION_PREFERENCES_VERSION.to_owned(),
            format!(
                "policy\t{}\t{}",
                super::encode_hex("Proxy"),
                super::encode_hex(" bad target ")
            ),
        ]
        .join("\n");
        fs::write(&path, invalid_target)?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
        assert!(super::load_node_selection_preferences_in(&store).is_err());

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn node_selection_preferences_reject_group_readable_files()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = test_temp_dir("manis-node-selection-permission");
        let store = root.join("subscriptions");
        fs::create_dir(&store)?;
        fs::set_permissions(&store, fs::Permissions::from_mode(0o700))?;
        let path = store.join(super::NODE_SELECTION_PREFERENCES_FILE);
        fs::write(&path, super::NODE_SELECTION_PREFERENCES_VERSION)?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644))?;

        assert!(super::load_node_selection_preferences_in(&store).is_err());

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[cfg(not(windows))]
    #[test]
    fn source_store_keeps_multiple_subscriptions_saved_nodes_and_fold_state()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = test_temp_dir("manis-multi-source-store");
        let store = root.join("subscriptions");
        let imported_before = super::current_unix_secs();
        let first = super::save_subscription_source_in(
            &store,
            "https://first.example.invalid/client?token=fixture-one&name=First",
        )?;
        let second = super::save_subscription_source_in(
            &store,
            "https://second.example.invalid/client?token=fixture-two&name=Second",
        )?;
        let duplicate = super::save_subscription_source_in(
            &store,
            "https://first.example.invalid/client?token=fixture-one&name=First",
        )?;
        let saved = super::save_single_node_source_in(
            &store,
            "vless://00000000-0000-4000-8000-000000000000@edge.example.invalid:443?security=tls#Saved",
        )?;
        super::save_collapsed_groups_in(&store, [first.id.as_str(), "saved", "../../unsafe"])?;

        let subscriptions = super::load_subscription_sources_in(&store)?;
        let nodes = super::load_single_node_sources_in(&store)?;
        assert_eq!(subscriptions.len(), 2);
        assert_ne!(first.id, second.id);
        assert_eq!(duplicate.id, first.id);
        assert_eq!(
            subscriptions[0].refresh_interval,
            super::RemoteSourceRefreshInterval::Manual
        );
        assert!(subscriptions[0].last_successful_update_unix_secs >= imported_before);
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].source.preview().name, "Saved");
        assert!(!format!("{first:?}").contains("fixture-one"));
        assert!(!format!("{saved:?}").contains("00000000"));
        assert_eq!(
            super::load_collapsed_groups_in(&store)?,
            vec!["saved".to_owned(), first.id.clone()]
        );

        super::remove_subscription_source_in(&store, &first.id)?;
        super::remove_single_node_source_in(&store, &saved.id)?;
        assert_eq!(super::load_subscription_sources_in(&store)?.len(), 1);
        assert!(super::load_single_node_sources_in(&store)?.is_empty());

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[cfg(not(windows))]
    #[test]
    fn subscription_name_and_enabled_state_round_trip_and_control_compilation()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = test_temp_dir("manis-subscription-enabled-state");
        let store = root.join("subscriptions");
        let stored = super::save_subscription_source_with_options_in(
            &store,
            "https://disabled.example.invalid/client?token=private",
            "备用订阅",
            super::RemoteSourceRefreshInterval::SixHours,
            false,
        )?;

        let loaded = super::load_subscription_sources_in(&store)?;
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].name, "备用订阅");
        assert!(!loaded[0].enabled);
        assert_eq!(
            loaded[0].refresh_interval,
            super::RemoteSourceRefreshInterval::SixHours
        );
        let disabled_profile =
            super::compile_saved_profile(&store, None, manis_core::KernelKind::Mihomo)?;
        assert!(disabled_profile.providers.is_empty());

        super::update_subscription_source_enabled_in(&store, &stored.id, true)?;
        let enabled_profile =
            super::compile_saved_profile(&store, None, manis_core::KernelKind::Mihomo)?;
        assert_eq!(enabled_profile.providers.len(), 1);

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[cfg(not(windows))]
    #[test]
    fn single_node_sources_are_protocol_agnostic_editable_and_disableable()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = test_temp_dir("manis-single-node-source");
        let store = root.join("subscriptions");
        let stored = super::save_single_node_source_with_options_in(
            &store,
            "trojan://fixture-password@example.invalid:443?security=tls#Original",
            "家庭节点",
            false,
        )?;

        let loaded = super::load_single_node_sources_in(&store)?;
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].name, "家庭节点");
        assert!(!loaded[0].enabled);
        assert!(
            super::compile_saved_profile(&store, None, manis_core::KernelKind::Mihomo,)?
                .providers
                .is_empty()
        );

        let updated = super::update_single_node_source_in(
            &store,
            &stored.id,
            "ss://fixture@example.invalid:8388#Edited",
            "办公节点",
            true,
        )?;
        assert_eq!(updated.name, "办公节点");
        assert!(updated.enabled);
        let profile = super::compile_saved_profile(&store, None, manis_core::KernelKind::Mihomo)?;
        assert_eq!(profile.providers.len(), 1);
        assert!(matches!(
            profile.providers[0].source,
            manis_profile::ProxyProviderSource::File
        ));

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[cfg(not(windows))]
    #[test]
    fn subscription_sources_support_refresh_metadata_and_legacy_url_files()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = test_temp_dir("manis-source-refresh-metadata");
        let store = root.join("subscriptions");
        fs::create_dir(&store)?;
        fs::set_permissions(&store, fs::Permissions::from_mode(0o700))?;
        let legacy_path = store.join("source-feed.url");
        fs::write(
            &legacy_path,
            "https://legacy.example.invalid/client?token=fixture-legacy",
        )?;
        fs::set_permissions(&legacy_path, fs::Permissions::from_mode(0o600))?;

        let legacy = super::load_subscription_sources_in(&store)?
            .into_iter()
            .next()
            .ok_or("legacy source")?;
        assert_eq!(legacy.id, "source-feed");
        assert_eq!(
            legacy.refresh_interval,
            super::RemoteSourceRefreshInterval::Manual
        );
        assert_eq!(legacy.last_successful_update_unix_secs, 0);

        let updated = super::update_subscription_source_refresh_interval_in(
            &store,
            &legacy.id,
            super::RemoteSourceRefreshInterval::SixHours,
        )?;
        assert_eq!(
            updated.refresh_interval,
            super::RemoteSourceRefreshInterval::SixHours
        );
        assert_eq!(updated.last_successful_update_unix_secs, 0);

        let refreshed = super::mark_subscription_source_update_success_in(&store, &legacy.id, 42)?;
        assert_eq!(
            refreshed.refresh_interval,
            super::RemoteSourceRefreshInterval::SixHours
        );
        assert_eq!(refreshed.last_successful_update_unix_secs, 42);
        let reloaded = super::load_subscription_sources_in(&store)?;
        assert_eq!(reloaded, vec![refreshed]);

        let long_url = format!("https://long.example.invalid/{}", "a".repeat(9 * 1024));
        let long_source = super::save_subscription_source_in(&store, &long_url)?;
        assert!(
            super::load_subscription_sources_in(&store)?
                .iter()
                .any(|source| source.id == long_source.id)
        );

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[cfg(not(windows))]
    #[test]
    fn subscription_proxy_dns_is_extracted_and_persisted_across_v1_upgrade()
    -> Result<(), Box<dyn std::error::Error>> {
        let document = r#"
mixed-port: 7890
dns:
  enable: true
  proxy-server-nameserver:
    - 'https://192.0.2.10:8443/dns-query/clash?site=fixture'
    - "https://198.51.100.20/dns-query"
    - http://192.0.2.30/unsafe
proxies: []
"#;
        let nameservers = super::extract_subscription_proxy_nameservers(document);
        assert_eq!(nameservers.len(), 2);
        assert_eq!(
            super::extract_subscription_proxy_nameservers(
                "dns:\n  proxy-server-nameserver: [https://203.0.113.1/dns-query]\n"
            )
            .len(),
            1
        );

        let root = test_temp_dir("manis-subscription-proxy-dns");
        let store = root.join("subscriptions");
        fs::create_dir(&store)?;
        fs::set_permissions(&store, fs::Permissions::from_mode(0o700))?;
        let path = store.join("source-feed.url");
        let legacy_v1 = [
            super::LEGACY_RELAY_STORED_SUBSCRIPTION_VERSION.to_owned(),
            "id\tsource-feed".to_owned(),
            format!(
                "url\t{}",
                super::encode_hex("https://legacy.example.invalid/client?token=fixture")
            ),
            "refresh\tmanual".to_owned(),
            "last-success\t42".to_owned(),
        ]
        .join("\n");
        fs::write(&path, legacy_v1)?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;

        let upgraded = super::update_subscription_source_proxy_nameservers_in(
            &store,
            "source-feed",
            &nameservers,
        )?;
        assert_eq!(upgraded.proxy_server_nameservers, nameservers);
        assert_eq!(upgraded.last_successful_update_unix_secs, 42);

        let contents = fs::read_to_string(&path)?;
        assert!(contents.starts_with(super::STORED_SUBSCRIPTION_VERSION));
        let reloaded = super::load_subscription_sources_in(&store)?;
        assert_eq!(reloaded, vec![upgraded]);
        assert!(!format!("{:?}", reloaded[0]).contains("192.0.2.10"));
        let profile = super::compile_saved_profile(&store, None, manis_core::KernelKind::Mihomo)?;
        let yaml = manis_profile::render_mihomo_yaml(&profile)?;
        assert!(yaml.contains("https://192.0.2.10:8443/dns-query/clash?site=fixture"));

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[cfg(not(windows))]
    #[test]
    fn qx_rule_sources_round_trip_privately_with_counts() -> Result<(), Box<dyn std::error::Error>>
    {
        let root = test_temp_dir("manis-qx-rule-store");
        let store = root.join("subscriptions");
        let url = "https://rules.example.invalid/airports.list?token=fixture-secret";
        let content = r"
# QX rule source fixture
HOST-KEYWORD,google,PROXY
HOST-SUFFIX,githubusercontent.com,PROXY
IP-CIDR,192.0.2.0/24,DIRECT
";

        let stored = super::save_qx_rule_source_in(&store, url, "Proxy", content)?.into_source();
        let loaded = super::load_qx_rule_sources_in(&store)?;

        assert_eq!(loaded, vec![stored.clone()]);
        assert_eq!(stored.name, None);
        assert_eq!(stored.target_policy.as_str(), "Proxy");
        assert_eq!(stored.content, content);
        assert_eq!(stored.rule_count, 2);
        assert_eq!(stored.diagnostic_count, 1);
        assert_eq!(
            stored.refresh_interval,
            super::RemoteSourceRefreshInterval::Manual
        );
        assert!(stored.last_successful_update_unix_secs > 0);
        assert!(!format!("{stored:?}").contains("fixture-secret"));

        let duplicate = super::save_qx_rule_source_in(
            &store,
            url,
            "DIRECT",
            "DOMAIN-SUFFIX,duplicate.example,DIRECT",
        )?;
        let super::SaveQxRuleSourceOutcome::Existing(existing) = duplicate else {
            return Err("duplicate QX rule URL was stored twice".into());
        };
        assert_eq!(existing.id, stored.id);
        assert_eq!(existing.name, None);
        assert_eq!(existing.target_policy.as_str(), "Proxy");
        assert_eq!(existing.content, content);
        assert_eq!(super::load_qx_rule_sources_in(&store)?.len(), 1);

        #[cfg(unix)]
        {
            assert_eq!(fs::metadata(&store)?.permissions().mode() & 0o077, 0);
            let entry = fs::read_dir(&store)?.next().ok_or("stored QX file")??;
            assert_eq!(entry.metadata()?.permissions().mode() & 0o077, 0);
            let stored_bytes = fs::read(entry.path())?;
            let stored_text = String::from_utf8(stored_bytes)?;
            assert!(!stored_text.contains("fixture-secret"));
        }

        super::remove_qx_rule_source_in(&store, &stored.id)?;
        assert!(super::load_qx_rule_sources_in(&store)?.is_empty());
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[cfg(not(windows))]
    #[test]
    fn qx_rule_source_custom_name_round_trips_and_can_reset()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = test_temp_dir("manis-qx-rule-name");
        let store = root.join("subscriptions");
        let stored = super::save_named_qx_rule_source_in(
            &store,
            "https://rules.example.invalid/airports.list?token=fixture-secret",
            "  机场规则  ",
            "Proxy",
            "DOMAIN-SUFFIX,example.com,PROXY\n",
        )?
        .into_source();

        assert_eq!(stored.name.as_deref(), Some("机场规则"));
        let loaded = super::load_qx_rule_sources_in(&store)?;
        assert_eq!(loaded, vec![stored.clone()]);
        let entry = fs::read_dir(&store)?.next().ok_or("stored QX file")??;
        let stored_text = fs::read_to_string(entry.path())?;
        assert!(stored_text.starts_with(super::QX_RULE_SOURCE_VERSION));
        assert!(stored_text.contains(&format!("name\t{}", super::encode_hex("机场规则"))));

        let reset = super::update_qx_rule_source_name_in(&store, &stored.id, "   ")?;
        assert_eq!(reset.name, None);
        assert_eq!(
            super::load_qx_rule_sources_in(&store)?
                .into_iter()
                .next()
                .and_then(|source| source.name),
            None
        );

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[cfg(not(windows))]
    #[test]
    fn qx_rule_source_name_survives_existing_mutations() -> Result<(), Box<dyn std::error::Error>> {
        let root = test_temp_dir("manis-qx-rule-name-preserve");
        let store = root.join("subscriptions");
        let stored = super::save_named_qx_rule_source_in(
            &store,
            "https://rules.example.invalid/old.list?token=fixture-secret",
            "初始规则",
            "Proxy",
            "DOMAIN-SUFFIX,old.example,PROXY\n",
        )?
        .into_source();
        let interval = super::update_qx_rule_source_refresh_interval_in(
            &store,
            &stored.id,
            super::RemoteSourceRefreshInterval::Hourly,
        )?;
        let target = super::update_qx_rule_source_target_in(&store, &stored.id, "DIRECT")?;
        let disabled = super::update_qx_rule_source_enabled_in(&store, &stored.id, false)?;
        let refreshed = super::replace_qx_rule_source_content_in(
            &store,
            &stored.id,
            "DOMAIN-SUFFIX,refresh.example,DIRECT\n",
            321,
        )?;
        let edited = super::replace_qx_rule_source_definition_in(
            &store,
            &stored.id,
            "编辑后的规则",
            "https://rules.example.invalid/new.list?token=fixture-secret",
            "Proxy",
            "DOMAIN-SUFFIX,new.example,PROXY\n",
            super::RemoteSourceRefreshInterval::Daily,
            456,
        )?;

        assert_eq!(interval.name.as_deref(), Some("初始规则"));
        assert_eq!(target.name.as_deref(), Some("初始规则"));
        assert_eq!(disabled.name.as_deref(), Some("初始规则"));
        assert_eq!(refreshed.name.as_deref(), Some("初始规则"));
        assert_eq!(edited.id, stored.id);
        assert_eq!(edited.name.as_deref(), Some("编辑后的规则"));
        assert!(!edited.enabled);
        assert_eq!(edited.target_policy.as_str(), "Proxy");
        assert_eq!(
            edited.source.expose_to(str::to_owned),
            "https://rules.example.invalid/new.list?token=fixture-secret"
        );
        assert_eq!(
            super::load_qx_rule_sources_in(&store)?
                .into_iter()
                .next()
                .and_then(|source| source.name),
            Some("编辑后的规则".to_owned())
        );

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[cfg(not(windows))]
    #[test]
    fn qx_rule_source_invalid_name_does_not_damage_existing_file()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = test_temp_dir("manis-qx-rule-invalid-name");
        let store = root.join("subscriptions");
        let stored = super::save_named_qx_rule_source_in(
            &store,
            "https://rules.example.invalid/list?token=fixture-secret",
            "Valid",
            "Proxy",
            "DOMAIN-SUFFIX,example.com,PROXY\n",
        )?
        .into_source();
        let path = store.join(format!("{}{}", stored.id, super::QX_RULE_SOURCE_SUFFIX));
        let before = fs::read_to_string(&path)?;

        assert!(super::update_qx_rule_source_name_in(&store, &stored.id, "bad\nname").is_err());
        assert_eq!(fs::read_to_string(&path)?, before);
        assert_eq!(
            super::load_qx_rule_sources_in(&store)?
                .into_iter()
                .next()
                .and_then(|source| source.name),
            Some("Valid".to_owned())
        );
        assert!(
            super::save_named_qx_rule_source_in(
                &store,
                "https://rules.example.invalid/other.list",
                &"长".repeat(49),
                "Proxy",
                "DOMAIN-SUFFIX,other.example,PROXY\n",
            )
            .is_err()
        );
        assert_eq!(super::load_qx_rule_sources_in(&store)?.len(), 1);

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[cfg(not(windows))]
    #[test]
    fn routing_rule_group_order_round_trips_and_appends_new_groups()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = test_temp_dir("manis-routing-rule-group-order");
        let store = root.join("subscriptions");
        let first = super::save_qx_rule_source_in(
            &store,
            "https://rules.example.invalid/first.list",
            "DIRECT",
            "DOMAIN-SUFFIX,first.example,DIRECT\n",
        )?
        .into_source();
        let second = super::save_qx_rule_source_in(
            &store,
            "https://rules.example.invalid/second.list",
            "DIRECT",
            "DOMAIN-SUFFIX,second.example,DIRECT\n",
        )?
        .into_source();
        let stored_order = vec![
            second.id.clone(),
            super::MANUAL_ROUTING_RULE_GROUP_ID.to_owned(),
            first.id.clone(),
        ];

        super::save_routing_rule_group_order_in(&store, &stored_order)?;
        assert_eq!(
            super::load_routing_rule_group_order_in(&store)?,
            stored_order
        );

        let sources = super::load_qx_rule_sources_in(&store)?;
        let normalized = super::normalized_routing_rule_group_order(
            &[second.id.clone(), "qx-rule-removed".to_owned()],
            true,
            &sources,
        );
        assert_eq!(normalized[0], super::MANUAL_ROUTING_RULE_GROUP_ID);
        assert_eq!(normalized[1], second.id);
        assert_eq!(normalized[2], first.id);

        let mut moved = normalized.clone();
        assert!(super::move_routing_rule_group(&mut moved, &second.id, -1));
        assert_eq!(moved[0], second.id);
        assert_eq!(moved[1], super::MANUAL_ROUTING_RULE_GROUP_ID);
        assert!(!super::move_routing_rule_group(&mut moved, &second.id, -1));
        assert!(!super::move_routing_rule_group(&mut moved, &first.id, 1));

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[cfg(not(windows))]
    #[test]
    fn saved_rule_group_order_controls_compiled_rule_priority()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = test_temp_dir("manis-compiled-rule-group-order");
        let store = root.join("subscriptions");
        super::save_subscription_source_in(
            &store,
            "https://subscription.example.invalid/client?token=fixture",
        )?;
        let first = super::save_qx_rule_source_in(
            &store,
            "https://rules.example.invalid/first.list",
            "DIRECT",
            "DOMAIN-SUFFIX,first.example,DIRECT\n",
        )?
        .into_source();
        let second = super::save_qx_rule_source_in(
            &store,
            "https://rules.example.invalid/second.list",
            "DIRECT",
            "DOMAIN-SUFFIX,second.example,DIRECT\n",
        )?
        .into_source();
        let manual = crate::manual_rule::ManualRule::parse(
            crate::manual_rule::ManualRuleKind::Host,
            "manual.example",
            "DIRECT",
        )?;
        let final_rule = crate::manual_rule::ManualRule::final_rule("DIRECT")?;
        crate::manual_rule::save_manual_rules_in(&store, &[final_rule, manual])?;
        super::save_routing_rule_group_order_in(
            &store,
            &[
                second.id,
                super::MANUAL_ROUTING_RULE_GROUP_ID.to_owned(),
                first.id,
            ],
        )?;

        let profile = super::compile_saved_profile(&store, None, manis_core::KernelKind::Mihomo)?;
        let yaml = manis_profile::render_mihomo_yaml(&profile)?;
        let second_index = yaml.find("DOMAIN-SUFFIX,second.example,DIRECT");
        let manual_index = yaml.find("DOMAIN,manual.example,DIRECT");
        let first_index = yaml.find("DOMAIN-SUFFIX,first.example,DIRECT");
        let final_index = yaml.find("MATCH,DIRECT");
        assert!(second_index < manual_index && manual_index < first_index);
        assert!(first_index < final_index);
        assert!(yaml.trim_end().ends_with("\"MATCH,DIRECT\""));
        assert!(!yaml.contains("GEOIP,CN,DIRECT"));
        assert!(!yaml.contains("MATCH,__MANIS_GLOBAL__"));

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[cfg(not(windows))]
    #[test]
    fn qx_rule_sources_update_interval_and_success_atomically()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = test_temp_dir("manis-qx-rule-refresh");
        let store = root.join("subscriptions");
        let url = "https://rules.example.invalid/airports.list?token=fixture-secret";
        let initial = "DOMAIN-KEYWORD,google,PROXY\n";
        let updated_content = "DOMAIN-SUFFIX,github.com,PROXY\nDOMAIN-KEYWORD,youtube,PROXY\n";

        let stored = super::save_qx_rule_source_in(&store, url, "Proxy", initial)?.into_source();
        let interval_updated = super::update_qx_rule_source_refresh_interval_in(
            &store,
            &stored.id,
            super::RemoteSourceRefreshInterval::Hourly,
        )?;
        assert_eq!(
            interval_updated.refresh_interval,
            super::RemoteSourceRefreshInterval::Hourly
        );
        assert_eq!(
            interval_updated.last_successful_update_unix_secs,
            stored.last_successful_update_unix_secs
        );

        let refreshed =
            super::replace_qx_rule_source_content_in(&store, &stored.id, updated_content, 123)?;
        assert_eq!(refreshed.content, updated_content);
        assert_eq!(refreshed.rule_count, 2);
        assert_eq!(
            refreshed.refresh_interval,
            super::RemoteSourceRefreshInterval::Hourly
        );
        assert_eq!(refreshed.last_successful_update_unix_secs, 123);

        assert!(
            super::replace_qx_rule_source_content_in(&store, &stored.id, "# empty\n", 456).is_err()
        );
        let after_failed_refresh = super::load_qx_rule_sources_in(&store)?;
        assert_eq!(after_failed_refresh, vec![refreshed]);

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[cfg(not(windows))]
    #[test]
    fn qx_rule_source_definition_can_be_edited_without_changing_its_identity()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = test_temp_dir("manis-qx-rule-definition-edit");
        let store = root.join("subscriptions");
        let stored = super::save_qx_rule_source_in(
            &store,
            "https://rules.example.invalid/old.list",
            "Proxy",
            "DOMAIN-SUFFIX,old.example,PROXY\n",
        )?
        .into_source();
        let disabled = super::update_qx_rule_source_enabled_in(&store, &stored.id, false)?;
        assert!(!disabled.enabled);

        let edited = super::replace_qx_rule_source_definition_in(
            &store,
            &stored.id,
            "",
            "https://rules.example.invalid/new.list",
            "DIRECT",
            "DOMAIN-SUFFIX,new.example,DIRECT\n",
            super::RemoteSourceRefreshInterval::SixHours,
            456,
        )?;

        assert_eq!(edited.id, stored.id);
        assert!(!edited.enabled);
        assert_eq!(
            edited.source.expose_to(str::to_owned),
            "https://rules.example.invalid/new.list"
        );
        assert_eq!(edited.target_policy.as_str(), "DIRECT");
        assert_eq!(edited.rule_count, 1);
        assert_eq!(
            edited.refresh_interval,
            super::RemoteSourceRefreshInterval::SixHours
        );
        assert_eq!(edited.last_successful_update_unix_secs, 456);
        assert_eq!(super::load_qx_rule_sources_in(&store)?, vec![edited]);

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[cfg(not(windows))]
    #[test]
    fn qx_rule_source_target_update_preserves_source_and_refresh_metadata()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = test_temp_dir("manis-qx-rule-target");
        let store = root.join("subscriptions");
        let url = "https://rules.example.invalid/airports.list?token=fixture-secret";
        let content = "DOMAIN-KEYWORD,google,PROXY\nDOMAIN-SUFFIX,youtube.com,PROXY\n";
        let stored =
            super::save_qx_rule_source_in(&store, url, "Old policy", content)?.into_source();
        let with_interval = super::update_qx_rule_source_refresh_interval_in(
            &store,
            &stored.id,
            super::RemoteSourceRefreshInterval::Daily,
        )?;

        let updated = super::update_qx_rule_source_target_in(&store, &stored.id, "Streaming")?;

        assert_eq!(updated.id, stored.id);
        assert_eq!(updated.source, stored.source);
        assert_eq!(updated.target_policy.as_str(), "Streaming");
        assert_eq!(updated.content, content);
        assert_eq!(updated.rule_count, stored.rule_count);
        assert_eq!(updated.diagnostic_count, stored.diagnostic_count);
        assert_eq!(updated.refresh_interval, with_interval.refresh_interval);
        assert_eq!(
            updated.last_successful_update_unix_secs,
            stored.last_successful_update_unix_secs
        );
        assert_eq!(super::load_qx_rule_sources_in(&store)?, vec![updated]);

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[cfg(not(windows))]
    #[test]
    fn qx_rule_sources_read_legacy_v1_without_refresh_metadata()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = test_temp_dir("manis-qx-rule-legacy-refresh");
        let store = root.join("subscriptions");
        fs::create_dir(&store)?;
        fs::set_permissions(&store, fs::Permissions::from_mode(0o700))?;
        let id = "qx-rule-feed";
        let content = "DOMAIN-KEYWORD,google,PROXY\n";
        let legacy = [
            super::LEGACY_MANIS_QX_RULE_SOURCE_VERSION.to_owned(),
            format!("id\t{id}"),
            format!(
                "url\t{}",
                super::encode_hex("https://rules.example.invalid/list?token=fixture-secret")
            ),
            format!("target\t{}", super::encode_hex("Proxy")),
            format!("content\t{}", super::encode_hex(content)),
        ]
        .join("\n");
        let path = store.join(format!("{id}{}", super::QX_RULE_SOURCE_SUFFIX));
        fs::write(&path, legacy)?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;

        let loaded = super::load_qx_rule_sources_in(&store)?
            .into_iter()
            .next()
            .ok_or("legacy qx source")?;
        assert_eq!(loaded.id, id);
        assert_eq!(loaded.name, None);
        assert_eq!(loaded.content, content);
        assert!(loaded.enabled);
        assert_eq!(
            loaded.refresh_interval,
            super::RemoteSourceRefreshInterval::Manual
        );
        assert_eq!(loaded.last_successful_update_unix_secs, 0);

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[cfg(not(windows))]
    #[test]
    fn qx_rule_sources_read_legacy_v2_without_custom_name() -> Result<(), Box<dyn std::error::Error>>
    {
        let root = test_temp_dir("manis-qx-rule-legacy-v2-name");
        let store = root.join("subscriptions");
        fs::create_dir(&store)?;
        fs::set_permissions(&store, fs::Permissions::from_mode(0o700))?;
        let id = "qx-rule-feed";
        let content = "DOMAIN-KEYWORD,google,PROXY\n";
        let legacy = [
            super::LEGACY_MANIS_QX_RULE_SOURCE_VERSION_V2.to_owned(),
            format!("id\t{id}"),
            format!(
                "url\t{}",
                super::encode_hex("https://rules.example.invalid/list?token=fixture-secret")
            ),
            format!("target\t{}", super::encode_hex("Proxy")),
            format!("content\t{}", super::encode_hex(content)),
            "enabled\t1".to_owned(),
            "refresh\t1h".to_owned(),
            "last-success\t123".to_owned(),
        ]
        .join("\n");
        let path = store.join(format!("{id}{}", super::QX_RULE_SOURCE_SUFFIX));
        fs::write(&path, legacy)?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;

        let loaded = super::load_qx_rule_sources_in(&store)?
            .into_iter()
            .next()
            .ok_or("legacy v2 qx source")?;
        assert_eq!(loaded.id, id);
        assert_eq!(loaded.name, None);
        assert_eq!(loaded.content, content);
        assert!(loaded.enabled);
        assert_eq!(
            loaded.refresh_interval,
            super::RemoteSourceRefreshInterval::Hourly
        );
        assert_eq!(loaded.last_successful_update_unix_secs, 123);

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[cfg(not(windows))]
    #[test]
    fn qx_rule_sources_reject_invalid_inputs() -> Result<(), Box<dyn std::error::Error>> {
        let root = test_temp_dir("manis-qx-rule-invalid");
        let store = root.join("subscriptions");
        let valid_content = "DOMAIN-KEYWORD,google,PROXY\n";

        assert!(
            super::save_qx_rule_source_in(
                &store,
                "http://rules.example.invalid/list?token=fixture-secret",
                "Proxy",
                valid_content,
            )
            .is_err()
        );
        assert!(
            super::save_qx_rule_source_in(
                &store,
                "https://rules.example.invalid/list?token=fixture-secret",
                "bad,name",
                valid_content,
            )
            .is_err()
        );
        assert!(
            super::save_qx_rule_source_in(
                &store,
                "https://rules.example.invalid/list?token=fixture-secret",
                "Proxy",
                "# comments only\n",
            )
            .is_err()
        );
        assert!(
            super::save_qx_rule_source_in(
                &store,
                "https://rules.example.invalid/list?token=fixture-secret",
                "Proxy",
                &"x".repeat(super::MAX_QX_RULE_SOURCE_CONTENT_BYTES + 1),
            )
            .is_err()
        );

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[cfg(not(windows))]
    #[test]
    fn qx_rule_source_errors_redact_secret_inputs() {
        let root = test_temp_dir("manis-qx-rule-redaction");
        let store = root.join("subscriptions");
        let error = super::save_qx_rule_source_in(
            &store,
            "http://rules.example.invalid/list?token=private-fixture",
            "Proxy",
            "DOMAIN-KEYWORD,google,PROXY\n",
        )
        .expect_err("plain HTTP QX rule source must fail");

        assert!(!error.to_string().contains("private-fixture"));
        assert!(!format!("{error:?}").contains("private-fixture"));
        fs::remove_dir_all(root).expect("remove temp root");
    }

    #[test]
    fn qx_rule_sources_compile_in_source_order_without_generated_fallbacks()
    -> Result<(), Box<dyn std::error::Error>> {
        let source = super::StoredQxRuleSource {
            id: "qx-rule-fixture-1".to_owned(),
            name: None,
            source: manis_profile::SecretUrl::parse_https(
                "https://rules.example.invalid/airports.list?token=fixture-secret",
            )?,
            enabled: true,
            target_policy: manis_profile::Name::parse("Proxy")?,
            content: "DOMAIN-KEYWORD,google,PROXY\nDOMAIN-SUFFIX,githubusercontent.com,proxy\n"
                .to_owned(),
            rule_count: 2,
            diagnostic_count: 0,
            refresh_interval: super::RemoteSourceRefreshInterval::Manual,
            last_successful_update_unix_secs: 0,
        };
        let mut profile =
            manis_profile::Profile::qx_default(manis_profile::SecretUrl::parse_https(
                "https://subscription.example.invalid/client?token=fixture-secret",
            )?)?;
        let mut disabled_source = source.clone();
        disabled_source.enabled = false;

        super::apply_qx_rule_sources(&mut profile, &[source])?;
        let yaml = manis_profile::render_mihomo_yaml(&profile)?;

        assert!(
            yaml.find("- \"DOMAIN-KEYWORD,google,__MANIS_GLOBAL__\"")
                < yaml.find("- \"DOMAIN-SUFFIX,githubusercontent.com,__MANIS_GLOBAL__\"")
        );
        assert!(!yaml.contains("GEOIP,CN,DIRECT"));
        assert!(!yaml.contains("MATCH,__MANIS_GLOBAL__"));

        let mut disabled_profile = manis_profile::Profile::qx_default(
            manis_profile::SecretUrl::parse_https("https://subscription.example.invalid/client")?,
        )?;
        super::apply_qx_rule_sources(&mut disabled_profile, &[disabled_source])?;
        let disabled_yaml = manis_profile::render_mihomo_yaml(&disabled_profile)?;
        assert!(!disabled_yaml.contains("DOMAIN-KEYWORD,google"));
        Ok(())
    }

    #[test]
    fn legacy_proxy_rule_targets_resolve_to_the_first_user_policy_group()
    -> Result<(), Box<dyn std::error::Error>> {
        let source = super::StoredQxRuleSource {
            id: "qx-rule-fixture-legacy-target".to_owned(),
            name: None,
            source: manis_profile::SecretUrl::parse_https(
                "https://rules.example.invalid/legacy.list",
            )?,
            enabled: true,
            target_policy: manis_profile::Name::parse("Proxy")?,
            content: "DOMAIN-SUFFIX,google.com,PROXY\n".to_owned(),
            rule_count: 1,
            diagnostic_count: 0,
            refresh_interval: super::RemoteSourceRefreshInterval::Manual,
            last_successful_update_unix_secs: 0,
        };
        let group = manis_profile::UserPolicyGroup {
            name: manis_profile::Name::parse("香港")?,
            icon: None,
            kind: manis_profile::UserPolicyGroupKind::UrlTest {
                tolerance: 50,
                interval_secs: 300,
            },
            provider_indexes: vec![0],
            direct_proxies: Vec::new(),
            direct_policies: Vec::new(),
            filter: None,
        };
        let mut profile = manis_profile::Profile::qx_sources_with_groups(
            vec![manis_profile::SecretUrl::parse_https(
                "https://subscription.example.invalid/client",
            )?],
            Vec::new(),
            vec![group],
            17_890,
        )?;

        super::apply_qx_rule_sources(&mut profile, &[source])?;
        let yaml = manis_profile::render_mihomo_yaml(&profile)?;

        assert!(yaml.contains("- \"DOMAIN-SUFFIX,google.com,香港\""));
        assert!(!yaml.contains("name: \"Proxy\""));
        Ok(())
    }

    #[cfg(not(windows))]
    #[test]
    fn source_store_round_trips_editable_managed_policy_groups()
    -> Result<(), Box<dyn std::error::Error>> {
        use manis_core::{
            ManagedPolicyGroup, ManagedPolicyIcon, ManagedPolicyStrategy, NodeIdentity,
            PolicyCandidateMatcher,
        };

        let root = test_temp_dir("manis-managed-policies");
        let store = root.join("subscriptions");
        let mut group = ManagedPolicyGroup::new("policy-a-1", "香港优选")?;
        group.icon = ManagedPolicyIcon::Globe;
        group.strategy = ManagedPolicyStrategy::LowestLatency;
        group.set_test_interval_secs(1_800)?;
        group.switch_tolerance_ms = 150;
        group.set_matcher(PolicyCandidateMatcher::name_contains("Hong Kong")?)?;
        super::save_managed_policy_in(&store, &group)?;

        let mut explicit = ManagedPolicyGroup::new("policy-b-2", "手动出口")?;
        explicit.icon = ManagedPolicyIcon::Shield;
        explicit.set_matcher(PolicyCandidateMatcher::Explicit(BTreeSet::default()))?;
        explicit.toggle_member(NodeIdentity::new("subscription:source-1", "Tokyo Edge")?);
        explicit.toggle_member(NodeIdentity::new("saved", "Private Edge")?);
        explicit.toggle_member(NodeIdentity::new("builtin", "PROXY")?);
        super::save_managed_policy_in(&store, &explicit)?;

        let groups = super::load_managed_policy_groups_in(&store)?;
        assert_eq!(groups, vec![group.clone(), explicit.clone()]);

        group.rename("香港 · 自动")?;
        group.icon = ManagedPolicyIcon::Compass;
        super::save_managed_policy_in(&store, &group)?;
        let updated = super::load_managed_policy_groups_in(&store)?;
        assert_eq!(updated.len(), 2);
        assert_eq!(updated[0], group);

        super::remove_managed_policy_in(&store, &explicit.id)?;
        assert_eq!(super::load_managed_policy_groups_in(&store)?.len(), 1);
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[cfg(not(windows))]
    #[test]
    fn legacy_node_group_file_migrates_to_managed_policy_file()
    -> Result<(), Box<dyn std::error::Error>> {
        use std::os::unix::fs::PermissionsExt;

        let root = test_temp_dir("manis-managed-policy-migration");
        let store = root.join("subscriptions");
        fs::create_dir_all(&store)?;
        fs::set_permissions(&store, fs::Permissions::from_mode(0o700))?;
        let legacy = store.join("group-deadbeef-1.group");
        fs::write(
            &legacy,
            concat!(
                "manis-node-group-v1\n",
                "id\tgroup-deadbeef-1\n",
                "name\t4c656761637920506f6c696379\n",
                "icon\tbolt\n",
                "strategy\tmanual\n",
                "interval\t600\n",
                "matcher\tall\n",
                "filter\t"
            ),
        )?;
        fs::set_permissions(&legacy, fs::Permissions::from_mode(0o600))?;

        let policies = super::load_managed_policy_groups_in(&store)?;
        assert_eq!(policies.len(), 1);
        assert_eq!(policies[0].name, "Legacy Policy");
        assert!(!legacy.exists());
        let migrated = store.join("group-deadbeef-1.policy");
        assert!(migrated.exists());
        assert!(fs::read_to_string(&migrated)?.starts_with("manis-policy-group-v1\n"));

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn managed_policy_groups_compile_matchers_into_mihomo_candidates()
    -> Result<(), Box<dyn std::error::Error>> {
        use std::collections::HashMap;

        use manis_core::{
            ManagedPolicyGroup, ManagedPolicyStrategy, NodeIdentity, PolicyCandidateMatcher,
        };
        use manis_profile::{Name, PolicyRef, UserPolicyGroupKind, VlessProxy};

        let saved = VlessProxy::parse_share_link(
            "vless://00000000-0000-4000-8000-000000000000@edge.example.invalid:443?security=tls#Private%20Edge",
        )?;
        let indexes = HashMap::from([("source-a".to_owned(), 1_usize)]);

        let mut latency = ManagedPolicyGroup::new("group-a-1", "香港优选")?;
        latency.strategy = ManagedPolicyStrategy::LowestLatency;
        latency.set_test_interval_secs(300)?;
        latency.switch_tolerance_ms = 200;
        latency.set_matcher(PolicyCandidateMatcher::name_contains("Hong Kong")?)?;

        let mut explicit = ManagedPolicyGroup::new("group-b-2", "手动出口")?;
        explicit.set_matcher(PolicyCandidateMatcher::Explicit(BTreeSet::default()))?;
        explicit.toggle_member(NodeIdentity::new("subscription:source-a", "Tokyo (Fast)")?);
        explicit.toggle_member(NodeIdentity::new("saved", "Private Edge")?);
        explicit.toggle_member(NodeIdentity::new("policy:group-a-1", "香港优选")?);
        explicit.toggle_member(NodeIdentity::new("builtin", "DIRECT")?);
        explicit.toggle_member(NodeIdentity::new("builtin", "REJECT")?);
        explicit.toggle_member(NodeIdentity::new("builtin", "PROXY")?);

        let compiled =
            super::compile_managed_policy_groups(&[latency, explicit], &indexes, &[], &[saved], 2)?;

        assert_eq!(
            compiled[0].kind,
            UserPolicyGroupKind::UrlTest {
                tolerance: 200,
                interval_secs: 300,
            }
        );
        assert_eq!(compiled[0].provider_indexes, vec![0, 1]);
        assert_eq!(compiled[0].filter.as_deref(), Some("(?i)Hong Kong"));
        assert_eq!(compiled[1].provider_indexes, vec![1]);
        assert_eq!(
            compiled[1].filter.as_deref(),
            Some("^(?:Tokyo \\(Fast\\))$")
        );
        assert_eq!(compiled[1].direct_proxies.len(), 1);
        assert!(compiled[1].direct_policies.contains(&PolicyRef::Direct));
        assert!(compiled[1].direct_policies.contains(&PolicyRef::Reject));
        assert!(
            compiled[1]
                .direct_policies
                .contains(&PolicyRef::Group(Name::parse("香港优选")?))
        );
        assert!(
            compiled[1]
                .direct_policies
                .contains(&PolicyRef::Group(Name::parse(
                    super::MANIS_GLOBAL_GROUP_NAME
                )?))
        );
        Ok(())
    }

    #[test]
    fn imported_subscription_load_rejects_symlinks_and_redacts_corruption()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = test_temp_dir("manis-import-corrupt");
        let store = root.join("subscriptions");
        fs::create_dir(&store)?;
        let secret_path = store.join("subscription.url");
        fs::write(
            &secret_path,
            "https://example.invalid/?token=private-fixture\nsecond-line",
        )?;

        let error = super::load_imported_subscription_in(&store)
            .expect_err("multi-line stored input must fail closed");
        assert!(!error.to_string().contains("private-fixture"));
        assert!(!format!("{error:?}").contains("private-fixture"));

        #[cfg(unix)]
        {
            fs::remove_file(&secret_path)?;
            let outside = root.join("outside.url");
            fs::write(&outside, "https://example.invalid/subscription")?;
            std::os::unix::fs::symlink(&outside, &secret_path)?;
            assert!(super::load_imported_subscription_in(&store).is_err());
        }

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[cfg(not(windows))]
    #[test]
    fn removing_an_imported_subscription_is_idempotent() -> Result<(), Box<dyn std::error::Error>> {
        let root = test_temp_dir("manis-import-remove");
        let store = root.join("subscriptions");
        super::save_imported_subscription_in(
            &store,
            "https://example.invalid/client?token=fixture",
        )?;

        super::remove_imported_subscription_in(&store)?;
        super::remove_imported_subscription_in(&store)?;
        assert_eq!(super::load_imported_subscription_in(&store)?, None);

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    #[ignore = "requires MANIS_MIHOMO_TEST_BINARY pointing to a local Mihomo executable"]
    fn real_mihomo_previews_all_nodes_from_a_subscription() -> Result<(), Box<dyn std::error::Error>>
    {
        let binary = std::env::var_os("MANIS_MIHOMO_TEST_BINARY")
            .ok_or("MANIS_MIHOMO_TEST_BINARY is required")?;
        let listener = TcpListener::bind("127.0.0.1:0")?;
        listener.set_nonblocking(true)?;
        let subscription_url = format!("http://{}/subscription", listener.local_addr()?);
        let stop = Arc::new(AtomicBool::new(false));
        let server_stop = stop.clone();
        let server = std::thread::spawn(move || -> std::io::Result<()> {
            let body = r#"proxies:
  - name: "Fixture Alpha"
    type: ss
    server: 127.0.0.1
    port: 443
    cipher: aes-128-gcm
    password: fixture-alpha
  - name: "Fixture Beta"
    type: ss
    server: 127.0.0.1
    port: 8443
    cipher: aes-128-gcm
    password: fixture-beta
"#;
            while !server_stop.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let mut request_line = String::new();
                        BufReader::new(stream.try_clone()?).read_line(&mut request_line)?;
                        let response = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: text/yaml\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                            body.len()
                        );
                        stream.write_all(response.as_bytes())?;
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    Err(error) => return Err(error),
                }
            }
            Ok(())
        });

        let result = super::preview_subscription_with_binary(&subscription_url, Path::new(&binary));
        let import_root = test_temp_dir("manis-real-import");
        let store = import_root.join("subscriptions");
        super::save_imported_subscription_in(&store, &subscription_url)?;
        let restored_secret = super::load_imported_subscription_in(&store)?
            .ok_or("imported subscription should exist")?;
        let restored =
            super::preview_secret_subscription_with_binary(restored_secret, Path::new(&binary));
        stop.store(true, Ordering::Relaxed);
        server.join().map_err(|_| "fixture server panicked")??;
        let providers = result?;
        let restored_providers = restored?;

        assert_eq!(providers.len(), 1);
        assert_eq!(providers[0].nodes.len(), 2);
        assert_eq!(providers[0].nodes[0].name, "Fixture Alpha");
        assert_eq!(providers[0].nodes[1].name, "Fixture Beta");
        assert_eq!(restored_providers, providers);
        fs::remove_dir_all(import_root)?;
        Ok(())
    }

    #[test]
    fn external_controller_and_custom_config_overrides_are_not_runtime_inputs() {
        assert_eq!(
            super::first_unsupported_runtime_override(|name| name == super::CONTROLLER_ENV),
            Some(super::CONTROLLER_ENV)
        );
        assert_eq!(
            super::first_unsupported_runtime_override(|name| name == super::CONFIG_ENV),
            Some(super::CONFIG_ENV)
        );
        assert_eq!(
            super::first_unsupported_runtime_override(|name| name == super::CONTROLLER_SECRET_ENV),
            Some(super::CONTROLLER_SECRET_ENV)
        );
        assert_eq!(
            super::first_unsupported_runtime_override(|name| name == super::SUBSCRIPTION_FILE_ENV),
            Some(super::SUBSCRIPTION_FILE_ENV)
        );
        assert_eq!(
            super::first_unsupported_runtime_override(|name| name == super::BINARY_ENV),
            None
        );
    }

    #[test]
    fn saved_sources_build_a_managed_mihomo_runtime_without_starting_kernel()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = test_temp_dir("manis-managed-mihomo-saved-sources");
        let store = root.join("subscriptions");
        let data_dir = root.join("runtime");
        let binary = root.join("mihomo");
        fs::write(&binary, "#!/bin/sh\nexit 0\n")?;
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o700))?;
        let saved = super::save_single_node_source_in(
            &store,
            "vless://00000000-0000-4000-8000-000000000000@edge.example.invalid:443?security=tls#Private%20Edge",
        )?;

        let runtime = super::build_saved_sources_mihomo_runtime_in(
            &store,
            &binary.canonicalize()?,
            &data_dir,
            &ControllerEndpoint::UnixSocket(data_dir.join("controller.sock")),
        )?;
        let cloned_runtime = runtime.clone();

        match (&runtime, &cloned_runtime) {
            (
                super::ControllerRuntime::Managed {
                    apply_lock: left, ..
                },
                super::ControllerRuntime::Managed {
                    apply_lock: right, ..
                },
            ) => assert!(std::sync::Arc::ptr_eq(left, right)),
            _ => panic!("saved sources should share a managed apply lock"),
        }

        assert_eq!(
            runtime.managed_health()?,
            super::ManagedRuntimeHealth::Stopped
        );

        match runtime {
            super::ControllerRuntime::Managed {
                profile_source,
                generated_profile,
                ..
            } => {
                assert_eq!(profile_source, super::RuntimeProfileSource::SavedSources);
                assert_eq!(
                    generated_profile.expect("generated profile").kernel,
                    manis_core::KernelKind::Mihomo
                );
            }
            _ => panic!("saved sources should build a managed runtime"),
        }
        let generated = fs::read_to_string(data_dir.join(super::GENERATED_PROFILE_FILE))?;
        assert!(generated.contains("type: \"file\""));
        assert!(generated.contains(&format!("single_nodes/{}.txt", saved.id)));
        assert!(
            fs::read_to_string(
                data_dir
                    .join("single_nodes")
                    .join(format!("{}.txt", saved.id))
            )?
            .contains("Private%20Edge")
        );
        assert!(generated.contains("mixed-port: 17890"));

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn empty_workspace_builds_a_managed_direct_only_mihomo_runtime()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = test_temp_dir("manis-managed-mihomo-empty-workspace");
        let store = root.join("subscriptions");
        let data_dir = root.join("runtime");
        let binary = root.join("mihomo");
        fs::create_dir_all(&store)?;
        fs::set_permissions(&store, fs::Permissions::from_mode(0o700))?;
        fs::write(&binary, "#!/bin/sh\nexit 0\n")?;
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o700))?;

        let runtime = super::build_saved_sources_mihomo_runtime_in(
            &store,
            &binary.canonicalize()?,
            &data_dir,
            &ControllerEndpoint::UnixSocket(data_dir.join("controller.sock")),
        )?;

        assert!(matches!(runtime, super::ControllerRuntime::Managed { .. }));
        let generated = fs::read_to_string(data_dir.join(super::GENERATED_PROFILE_FILE))?;
        assert!(generated.contains("rules:\n"));
        assert!(generated.ends_with("  - \"MATCH,DIRECT\"\n"));
        assert!(!generated.contains("__MANIS_GLOBAL__"));

        fs::remove_dir_all(root)?;
        Ok(())
    }

    fn test_temp_dir(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("{name}-{}", std::process::id()));
        if path.exists() {
            fs::remove_dir_all(&path).expect("remove stale test directory");
        }
        fs::create_dir(&path).expect("create test directory");
        path
    }

    #[test]
    fn parses_absolute_unix_controller_endpoint() -> Result<(), Box<dyn std::error::Error>> {
        let path = super::unix_socket_path("unix:///tmp/verge/mihomo.sock")?
            .ok_or("expected a Unix socket path")?;
        assert_eq!(path, Path::new("/tmp/verge/mihomo.sock"));
        assert!(super::unix_socket_path("http://127.0.0.1:9090")?.is_none());
        Ok(())
    }

    #[test]
    fn rejects_relative_unix_controller_endpoint() {
        assert!(super::unix_socket_path("unix://relative.sock").is_err());
        assert!(super::unix_socket_path("unix://").is_err());
    }

    #[test]
    fn successful_activity_snapshots_use_the_poll_interval_without_reconnecting() {
        use std::sync::{Mutex, mpsc};
        use std::time::Instant;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let config =
            super::ControllerConfig::new(format!("http://{}", listener.local_addr().unwrap()))
                .unwrap();
        let (times_tx, times_rx) = mpsc::channel();
        let server = std::thread::spawn(move || {
            for _ in 0..2 {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap() == 0 || line == "\r\n" {
                        break;
                    }
                }
                let body = r#"{"downloadTotal":0,"uploadTotal":0,"connections":[]}"#;
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .unwrap();
                times_tx.send(Instant::now()).unwrap();
            }
        });
        let cancelled = Arc::new(AtomicBool::new(false));
        let mailbox = Arc::new(Mutex::new(super::LiveMailbox::default()));
        super::spawn_connection_stream(
            super::LiveController::loopback(config),
            cancelled.clone(),
            mailbox.clone(),
        );
        let first = times_rx.recv_timeout(Duration::from_secs(3));
        std::thread::sleep(Duration::from_millis(500));
        let phase = mailbox.lock().unwrap().status.activity.clone();
        let second = times_rx.recv_timeout(Duration::from_secs(3));
        cancelled.store(true, Ordering::Relaxed);
        server.join().unwrap();
        assert_eq!(phase, super::LiveStreamPhase::Live);
        assert!(
            second.unwrap().duration_since(first.unwrap()) >= super::LIVE_CONNECTION_INTERVAL,
            "successful finite snapshots must not trigger the fast reconnect loop"
        );
    }

    #[test]
    fn delay_controller_timeout_exceeds_the_kernel_test_timeout() {
        let config = super::delay_controller_config(manis_mihomo::ControllerConfig::default());

        assert!(
            config.read_timeout() > Duration::from_millis(u64::from(super::GROUP_DELAY_TIMEOUT_MS))
        );
    }

    #[test]
    fn fixture_group_benchmark_keeps_partial_proxy_results()
    -> Result<(), Box<dyn std::error::Error>> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let endpoint = format!("http://{}", listener.local_addr()?);
        let server = std::thread::spawn(move || -> std::io::Result<()> {
            for _ in 0..2 {
                let (mut stream, _) = listener.accept()?;
                let mut request_line = String::new();
                BufReader::new(stream.try_clone()?).read_line(&mut request_line)?;
                let response = if request_line.contains("Working%20Node") {
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 12\r\nConnection: close\r\n\r\n{\"delay\":64}"
                } else {
                    "HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                };
                stream.write_all(response.as_bytes())?;
            }
            Ok(())
        });
        let runtime = super::ControllerRuntime::Fixture { endpoint };
        let delays = runtime.test_proxy_delay_targets_with_progress(
            &[
                super::ProxyDelayTarget::direct("Working Node"),
                super::ProxyDelayTarget::direct("Offline Node"),
            ],
            |_name, _delay| {},
        )?;
        server.join().map_err(|_| "fixture server panicked")??;
        assert_eq!(delays.get("Working Node"), Some(&64));
        assert!(!delays.contains_key("Offline Node"));
        Ok(())
    }

    #[test]
    fn fixture_proxy_benchmark_reports_fast_nodes_before_slow_nodes_finish()
    -> Result<(), Box<dyn std::error::Error>> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let endpoint = format!("http://{}", listener.local_addr()?);
        let slow_gate = Arc::new((Mutex::new(false), Condvar::new()));
        let server_gate = slow_gate.clone();
        let server = std::thread::spawn(move || -> std::io::Result<()> {
            let handlers = (0..2)
                .map(|_| {
                    let (stream, _) = listener.accept()?;
                    let gate = server_gate.clone();
                    Ok(std::thread::spawn(move || -> std::io::Result<()> {
                        let mut stream = stream;
                        let mut request_line = String::new();
                        BufReader::new(stream.try_clone()?).read_line(&mut request_line)?;
                        let delay = if request_line.contains("Slow%20Node") {
                            let (lock, ready) = &*gate;
                            let mut released = lock.lock().map_err(|_| {
                                std::io::Error::other("slow fixture gate poisoned")
                            })?;
                            while !*released {
                                released = ready.wait(released).map_err(|_| {
                                    std::io::Error::other("slow fixture gate poisoned")
                                })?;
                            }
                            70
                        } else {
                            30
                        };
                        let body = format!(r#"{{"delay":{delay}}}"#);
                        let response = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                            body.len()
                        );
                        stream.write_all(response.as_bytes())?;
                        Ok(())
                    }))
                })
                .collect::<std::io::Result<Vec<_>>>()?;
            for handler in handlers {
                handler
                    .join()
                    .map_err(|_| std::io::Error::other("fixture handler panicked"))??;
            }
            Ok(())
        });

        let runtime = super::ControllerRuntime::Fixture { endpoint };
        let mut updates = Vec::new();
        let callback_gate = slow_gate.clone();
        let delays = runtime.test_proxy_delay_targets_with_progress(
            &[
                super::ProxyDelayTarget::direct("Slow Node"),
                super::ProxyDelayTarget::direct("Fast Node"),
            ],
            |name, delay| {
                updates.push((name.to_owned(), delay));
                if name == "Fast Node" {
                    let (lock, ready) = &*callback_gate;
                    let mut released = lock.lock().expect("fixture callback gate poisoned");
                    *released = true;
                    ready.notify_all();
                }
            },
        )?;
        server.join().map_err(|_| "fixture server panicked")??;

        assert_eq!(
            updates,
            vec![
                ("Fast Node".to_owned(), Some(30)),
                ("Slow Node".to_owned(), Some(70)),
            ]
        );
        assert_eq!(delays.get("Fast Node"), Some(&30));
        assert_eq!(delays.get("Slow Node"), Some(&70));
        Ok(())
    }

    #[test]
    fn provider_proxy_benchmark_uses_provider_healthcheck_endpoint()
    -> Result<(), Box<dyn std::error::Error>> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let endpoint = format!("http://{}", listener.local_addr()?);
        let server = std::thread::spawn(move || -> std::io::Result<String> {
            let (mut stream, _) = listener.accept()?;
            let mut request_line = String::new();
            BufReader::new(stream.try_clone()?).read_line(&mut request_line)?;
            let body = r#"{"delay":42}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes())?;
            Ok(request_line)
        });

        let runtime = super::ControllerRuntime::Fixture { endpoint };
        let delays = runtime.test_proxy_delay_targets_with_progress(
            &[super::ProxyDelayTarget::provider("Subscription 1", "HK 01")],
            |_name, _delay| {},
        )?;
        let request_line = server.join().map_err(|_| "fixture server panicked")??;

        assert!(
            request_line
                .starts_with("GET /providers/proxies/Subscription%201/HK%2001/healthcheck?url=")
        );
        assert_eq!(delays.get("HK 01"), Some(&42));
        Ok(())
    }

    #[test]
    fn runtime_policy_benchmark_uses_group_delay_then_reads_automatic_winner()
    -> Result<(), Box<dyn std::error::Error>> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let endpoint = format!("http://{}", listener.local_addr()?);
        let server = std::thread::spawn(move || -> std::io::Result<Vec<String>> {
            let mut requests = Vec::new();
            for _ in 0..2 {
                let (mut stream, _) = listener.accept()?;
                let mut request_line = String::new();
                BufReader::new(stream.try_clone()?).read_line(&mut request_line)?;
                let body = if request_line.contains("/group/Auto%20HK/delay?") {
                    r#"{"HK-01":68,"HK-02":29,"HK-03":0,"unrelated":42}"#
                } else {
                    r#"{"name":"Auto HK","type":"URLTest","now":"HK-02","all":["HK-01","HK-02"]}"#
                };
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(response.as_bytes())?;
                requests.push(request_line);
            }
            Ok(requests)
        });
        let runtime = super::ControllerRuntime::Fixture { endpoint };

        let result = runtime.test_policy_group_delay(
            "Auto HK",
            &[
                super::ProxyDelayTarget::direct("HK-01"),
                super::ProxyDelayTarget::direct("HK-02"),
            ],
        )?;
        let requests = server.join().map_err(|_| "fixture server panicked")??;

        assert!(requests[0].contains("GET /group/Auto%20HK/delay?"));
        assert!(requests[1].contains("GET /proxies/Auto%20HK HTTP/1.1"));
        assert_eq!(result.current.as_deref(), Some("HK-02"));
        assert_eq!(result.delays.get("HK-02"), Some(&29));
        assert_eq!(result.delays.len(), 2);
        Ok(())
    }

    #[test]
    fn runtime_policy_benchmark_falls_back_to_partial_node_results()
    -> Result<(), Box<dyn std::error::Error>> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let endpoint = format!("http://{}", listener.local_addr()?);
        let server = std::thread::spawn(move || -> std::io::Result<()> {
            for _ in 0..4 {
                let (mut stream, _) = listener.accept()?;
                let mut request_line = String::new();
                BufReader::new(stream.try_clone()?).read_line(&mut request_line)?;
                let response = if request_line.contains("/group/Auto%20HK/delay?") {
                    "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                        .to_owned()
                } else if request_line.contains("/proxies/HK-01/delay?") {
                    "HTTP/1.1 200 OK\r\nContent-Length: 12\r\nConnection: close\r\n\r\n{\"delay\":42}"
                        .to_owned()
                } else if request_line.contains("/proxies/HK-02/delay?") {
                    "HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                        .to_owned()
                } else {
                    let body = r#"{"name":"Auto HK","type":"URLTest","now":"HK-01","all":["HK-01","HK-02"]}"#;
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                };
                stream.write_all(response.as_bytes())?;
            }
            Ok(())
        });
        let runtime = super::ControllerRuntime::Fixture { endpoint };

        let result = runtime.test_policy_group_delay(
            "Auto HK",
            &[
                super::ProxyDelayTarget::direct("HK-01"),
                super::ProxyDelayTarget::direct("HK-02"),
            ],
        )?;
        server.join().map_err(|_| "fixture server panicked")??;

        assert_eq!(result.current.as_deref(), Some("HK-01"));
        assert_eq!(result.delays.get("HK-01"), Some(&42));
        assert!(!result.delays.contains_key("HK-02"));
        Ok(())
    }

    #[test]
    fn policy_benchmark_fallback_keeps_subscription_provider_identity()
    -> Result<(), Box<dyn std::error::Error>> {
        for (group_status, group_body) in [
            (504, r#"{"message":"test timed out"}"#),
            (200, "{}"),
            (200, r#"{"HK 01":0,"unrelated":42}"#),
        ] {
            let listener = TcpListener::bind("127.0.0.1:0")?;
            listener.set_nonblocking(true)?;
            let endpoint = format!("http://{}", listener.local_addr()?);
            let server = std::thread::spawn(move || -> std::io::Result<Vec<String>> {
                let deadline = std::time::Instant::now() + Duration::from_secs(3);
                let mut requests = Vec::new();
                while requests.len() < 3 && std::time::Instant::now() < deadline {
                    let (mut stream, _) = match listener.accept() {
                        Ok(connection) => connection,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            std::thread::sleep(Duration::from_millis(5));
                            continue;
                        }
                        Err(error) => return Err(error),
                    };
                    // Accepted sockets can inherit nonblocking mode on macOS.
                    stream.set_nonblocking(false)?;
                    stream.set_read_timeout(Some(Duration::from_secs(1)))?;
                    let mut request = String::new();
                    BufReader::new(stream.try_clone()?).read_line(&mut request)?;
                    let (status, body) = if request.contains("/group/Auto%20HK/delay?") {
                        (group_status, group_body)
                    } else if request
                        .contains("/providers/proxies/Subscription%201/HK%2001/healthcheck?")
                    {
                        (200, r#"{"delay":42}"#)
                    } else if request.starts_with("GET /proxies/Auto%20HK HTTP/") {
                        (200, r#"{"type":"URLTest","now":"HK 01","all":["HK 01"]}"#)
                    } else {
                        (404, r#"{"message":"Resource not found"}"#)
                    };
                    write!(
                        stream,
                        "HTTP/1.1 {status} Result\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )?;
                    requests.push(request);
                }
                Ok(requests)
            });
            let runtime = super::ControllerRuntime::Fixture { endpoint };
            let result = runtime.test_policy_group_delay(
                "Auto HK",
                &[super::ProxyDelayTarget::provider("Subscription 1", "HK 01")],
            );
            let requests = server.join().map_err(|_| "fixture server panicked")??;
            assert!(
                requests.iter().any(|path| path
                    .contains("/providers/proxies/Subscription%201/HK%2001/healthcheck?")),
                "provider-owned fallback must not call /proxies/HK%2001/delay: {requests:?}"
            );
            assert_eq!(result?.delays.get("HK 01"), Some(&42));
        }
        Ok(())
    }

    #[test]
    fn policy_benchmark_reports_fallback_error_and_rejects_zero_delay()
    -> Result<(), Box<dyn std::error::Error>> {
        for (status, body) in [(503, ""), (200, r#"{"delay":0}"#)] {
            let listener = TcpListener::bind("127.0.0.1:0")?;
            let endpoint = format!("http://{}", listener.local_addr()?);
            let server = std::thread::spawn(move || -> std::io::Result<()> {
                for (status, body) in [(404, ""), (status, body)] {
                    let (mut stream, _) = listener.accept()?;
                    let mut line = String::new();
                    BufReader::new(stream.try_clone()?).read_line(&mut line)?;
                    write!(
                        stream,
                        "HTTP/1.1 {status} Result\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )?;
                }
                Ok(())
            });
            let runtime = super::ControllerRuntime::Fixture { endpoint };
            let result = runtime.test_policy_group_delay(
                "Auto HK",
                &[super::ProxyDelayTarget::provider("Subscription 1", "HK 01")],
            );
            server.join().map_err(|_| "fixture server panicked")??;
            if status == 503 {
                assert!(matches!(
                    result,
                    Err(super::LoadError::Mihomo(
                        manis_mihomo::MihomoError::HttpStatus {
                            status_code: 503,
                            ..
                        }
                    ))
                ));
            } else {
                assert!(matches!(result, Err(super::LoadError::NoLatencyResults)));
            }
        }
        Ok(())
    }

    #[test]
    fn policy_benchmark_targets_distinguish_provider_nodes_from_nested_groups() {
        use super::ProxyDelayTarget;
        use manis_core::{PolicyCandidateKind, PolicyNode, ProxyId};
        let mut candidate = PolicyNode {
            id: ProxyId::new("node"),
            name: "HK 01".to_owned(),
            kind: PolicyCandidateKind::Node,
            provider: Some("Subscription 1".to_owned()),
            detail: "Trojan".to_owned(),
            latency_ms: None,
            alive: None,
        };
        assert_eq!(
            ProxyDelayTarget::from_policy_node(&candidate),
            ProxyDelayTarget::provider("Subscription 1", "HK 01")
        );
        candidate.kind = PolicyCandidateKind::PolicyGroup;
        assert_eq!(
            ProxyDelayTarget::from_policy_node(&candidate),
            ProxyDelayTarget::direct("HK 01")
        );
        candidate.kind = PolicyCandidateKind::Node;
        candidate.provider = None;
        assert_eq!(
            ProxyDelayTarget::from_policy_node(&candidate),
            ProxyDelayTarget::direct("HK 01")
        );
    }

    #[test]
    fn fixture_runtime_rejects_managed_policy_changes() {
        use manis_core::RoutingMode;

        let runtime = super::ControllerRuntime::Fixture {
            endpoint: "http://127.0.0.1:9".to_owned(),
        };

        assert!(
            runtime
                .select_policy_candidate("Manis Group", "Candidate")
                .is_err()
        );
        assert!(runtime.set_routing_mode(RoutingMode::Global).is_err());
        assert!(runtime.select_global_node("Candidate").is_err());
    }

    #[test]
    fn policy_group_snapshot_deduplicates_runtime_candidates() {
        let snapshot = super::policy_group_runtime_snapshot(manis_mihomo::MihomoPolicyGroup {
            name: Some("Manis Group".to_owned()),
            proxy_type: Some("Selector".to_owned()),
            current: Some("Tokyo".to_owned()),
            all: vec!["Tokyo".to_owned(), "Tokyo".to_owned(), "Osaka".to_owned()],
        });

        assert_eq!(snapshot.current.as_deref(), Some("Tokyo"));
        assert_eq!(
            snapshot.candidates,
            ["Osaka".to_owned(), "Tokyo".to_owned()]
                .into_iter()
                .collect()
        );
        assert!(super::is_selector_proxy_type("SELECTOR"));
        assert!(!super::is_selector_proxy_type("select-or"));
        assert!(!super::is_selector_proxy_type("URLTest"));
    }

    #[test]
    fn provider_node_global_selection_uses_the_internal_global_exit_group() {
        let global = manis_mihomo::MihomoPolicyGroup {
            name: Some("GLOBAL".to_owned()),
            proxy_type: Some("Selector".to_owned()),
            current: Some("DIRECT".to_owned()),
            all: vec!["DIRECT".to_owned(), "__MANIS_GLOBAL__".to_owned()],
        };
        let global_exit = manis_mihomo::MihomoPolicyGroup {
            name: Some("__MANIS_GLOBAL__".to_owned()),
            proxy_type: Some("Selector".to_owned()),
            current: Some("HK 01".to_owned()),
            all: vec!["HK 01".to_owned(), "HK 03".to_owned()],
        };

        assert_eq!(
            super::global_selection_route(&global, &global_exit, "HK 03"),
            Some(super::GlobalSelectionRoute::ViaGlobalExit)
        );
        assert_eq!(
            super::global_selection_route(&global, &global_exit, "DIRECT"),
            Some(super::GlobalSelectionRoute::Direct)
        );
        assert_eq!(
            super::global_selection_route(&global, &global_exit, "Missing"),
            None
        );
    }

    #[test]
    fn global_node_selection_applies_leaf_then_internal_group()
    -> Result<(), Box<dyn std::error::Error>> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let endpoint = format!("http://{}", listener.local_addr()?);
        let server = std::thread::spawn(move || -> std::io::Result<Vec<String>> {
            let mut requests = Vec::new();
            for index in 0..8 {
                let (mut stream, _) = listener.accept()?;
                let mut request_line = String::new();
                BufReader::new(stream.try_clone()?).read_line(&mut request_line)?;
                requests.push(request_line.trim().to_owned());
                let body = match index {
                    0 | 5 => {
                        r#"{"name":"GLOBAL","type":"Selector","now":"DIRECT","all":["DIRECT","__MANIS_GLOBAL__"]}"#
                    }
                    1 | 2 => {
                        r#"{"name":"__MANIS_GLOBAL__","type":"Selector","now":"HK 01","all":["HK 01","HK 03"]}"#
                    }
                    4 => {
                        r#"{"name":"__MANIS_GLOBAL__","type":"Selector","now":"HK 03","all":["HK 01","HK 03"]}"#
                    }
                    7 => {
                        r#"{"name":"GLOBAL","type":"Selector","now":"__MANIS_GLOBAL__","all":["DIRECT","__MANIS_GLOBAL__"]}"#
                    }
                    3 | 6 => "",
                    _ => unreachable!(),
                };
                let response = if body.is_empty() {
                    "HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                        .to_owned()
                } else {
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                };
                stream.write_all(response.as_bytes())?;
            }
            Ok(requests)
        });

        let snapshot = super::select_global_node_at_endpoint(&endpoint, "HK 03", None)?;
        let requests = server.join().map_err(|_| "fixture server panicked")??;

        assert_eq!(snapshot.current.as_deref(), Some("HK 03"));
        assert_eq!(
            requests,
            [
                "GET /proxies/GLOBAL HTTP/1.1",
                "GET /proxies/__MANIS_GLOBAL__ HTTP/1.1",
                "GET /proxies/__MANIS_GLOBAL__ HTTP/1.1",
                "PUT /proxies/__MANIS_GLOBAL__ HTTP/1.1",
                "GET /proxies/__MANIS_GLOBAL__ HTTP/1.1",
                "GET /proxies/GLOBAL HTTP/1.1",
                "PUT /proxies/GLOBAL HTTP/1.1",
                "GET /proxies/GLOBAL HTTP/1.1",
            ]
        );
        Ok(())
    }
}
