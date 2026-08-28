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

use manis_core::{
    KernelKind, ManagedPolicyGroup, ManagedPolicyIcon, ManagedPolicyStrategy, NodeIdentity,
    PolicyCandidateMatcher, PolicyCatalog, RoutingMode,
};
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

use crate::brand;
use crate::diagnostics::{LogLevel, record_event};
use crate::subscription::VlessSource;

const CONTROLLER_ENV: &str = "MANIS_MIHOMO_CONTROLLER";
const LEGACY_RELAY_CONTROLLER_ENV: &str = "RELAY_MIHOMO_CONTROLLER";
const CONTROLLER_SECRET_ENV: &str = "MANIS_MIHOMO_SECRET";
const LEGACY_RELAY_CONTROLLER_SECRET_ENV: &str = "RELAY_MIHOMO_SECRET";
const BINARY_ENV: &str = "MANIS_MIHOMO_BINARY";
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
const STORED_SUBSCRIPTION_VERSION: &str = "manis-subscription-source-v2";
const LEGACY_MANIS_STORED_SUBSCRIPTION_VERSION: &str = "manis-subscription-source-v1";
const LEGACY_RELAY_STORED_SUBSCRIPTION_VERSION: &str = "relay-subscription-source-v1";
const MAX_SUBSCRIPTION_DOCUMENT_BYTES: u64 = 4 * 1024 * 1024;
const MAX_SUBSCRIPTION_PROXY_DNS_SERVERS: usize = 8;
const LEGACY_GENERATED_PROXY_GROUP_NAME: &str = "Proxy";
const SUBSCRIPTION_DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(15);
const SUBSCRIPTION_MAX_REDIRECTS: u32 = 5;
const SAVED_VLESS_PREFIX: &str = "saved-";
const SAVED_VLESS_SUFFIX: &str = ".vless";
const QX_RULE_SOURCE_PREFIX: &str = "qx-rule-";
const QX_RULE_SOURCE_SUFFIX: &str = ".qxrules";
const QX_RULE_SOURCE_VERSION: &str = "manis-qx-rule-source-v1";
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
        endpoint: String,
        manager: Arc<Mutex<EngineManager>>,
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
    pub(crate) fn label(self) -> &'static str {
        match self {
            #[cfg(any(test, feature = "snapshot-fixtures"))]
            Self::FixtureController => "测试快照",
            Self::SavedSources => "Manis 已保存来源",
            Self::Invalid => "配置不可用",
        }
    }

    pub(crate) fn detail(self) -> &'static str {
        match self {
            #[cfg(any(test, feature = "snapshot-fixtures"))]
            Self::FixtureController => "仅用于测试快照",
            Self::SavedSources => "从本机私有来源编译",
            Self::Invalid => "请检查本机启动参数",
        }
    }
}

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
                .map_err(|_poisoned| LoadError::Runtime("托管内核状态锁已损坏".to_owned()))?
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
                .map_err(|_poisoned| LoadError::Runtime("托管内核状态锁已损坏".to_owned()))?
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
            Self::Managed { endpoint, .. } => format!("Manis 托管 · {endpoint}"),
            Self::Invalid { .. } => "Manis 托管配置".to_owned(),
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
                        LoadError::Runtime("托管内核状态锁已损坏".to_owned())
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
                                    "无法恢复 macOS TUN 辅助服务：{error}"
                                ))
                            })?
                        {
                            crate::macos_privileged::MacosPrivilegedProcessSpawner::reclaim_stale_ordinary(
                                &config.launch_command(),
                            )
                                .map_err(|error| {
                                    LoadError::Runtime(format!(
                                        "无法回收 Manis 遗留的普通 Mihomo：{error}"
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
                        .map_err(|_poisoned| LoadError::Runtime("托管内核状态锁已损坏".to_owned()))?
                        .stop();
                    return Err(error);
                }
                Ok(RuntimeSnapshot {
                    endpoint: format!("Manis 托管 · {endpoint}"),
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
                        LoadError::Runtime("托管内核状态锁已损坏".to_owned())
                    })?;
                    match manager.running_endpoint()? {
                        Some(endpoint) => endpoint,
                        None => manager.start()?,
                    }
                };
                let endpoint = endpoint.uri();
                Ok(RuntimeSnapshot {
                    endpoint: format!("Manis 托管 · {endpoint}"),
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
        #[cfg(target_os = "macos")]
        if enabled {
            if let Some(conflict) = crate::macos_privileged::existing_tun_route()
                .map_err(|error| LoadError::Runtime(format!("无法检查 macOS TUN 路由：{error}")))?
            {
                record_event(LogLevel::Error, "controller.tun.conflict", conflict.clone());
                return Err(LoadError::Runtime(format!(
                    "检测到其他 TUN 正在占用系统代理路由（{conflict}）；请先关闭其他代理软件的 TUN 模式"
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
                Self::Fixture { .. } => "测试快照不能启用 TUN 模式".to_owned(),
                Self::Invalid { message } => message.clone(),
                Self::Managed { .. } => {
                    "TUN 模式仅支持由 Manis 已保存来源生成的 Mihomo 配置".to_owned()
                }
            }));
        };
        if spec.kernel != KernelKind::Mihomo {
            return Err(LoadError::Runtime(
                "当前 TUN 配置重载流程仅支持 Mihomo".to_owned(),
            ));
        }
        Ok((manager, spec))
    }

    #[cfg(target_os = "macos")]
    fn release_macos_tun_route(&self) -> Result<(), LoadError> {
        if crate::macos_privileged::wait_for_tun_route_release().map_err(|error| {
            LoadError::Runtime(format!("无法确认 macOS TUN 路由已释放：{error}"))
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
                "关闭 TUN 后路由仍然存在，且当前内核不能由 Manis 重启".to_owned(),
            ));
        };
        let mut manager = manager
            .lock()
            .map_err(|_poisoned| LoadError::Runtime("托管内核状态锁已损坏".to_owned()))?;
        manager.stop()?;
        manager.start()?;
        if !crate::macos_privileged::wait_for_tun_route_release().map_err(|error| {
            LoadError::Runtime(format!("重启后无法确认 macOS TUN 路由已释放：{error}"))
        })? {
            return Err(LoadError::Runtime(
                "Mihomo 重启后 macOS TUN 路由仍未释放".to_owned(),
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
                "macOS TUN 仅支持由 Manis 已保存来源生成的 Mihomo 配置".to_owned(),
            ));
        };
        if spec.kernel != KernelKind::Mihomo {
            return Err(LoadError::Runtime(
                "macOS 特权辅助服务当前仅支持 Mihomo".to_owned(),
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
        let spawner = MacosPrivilegedProcessSpawner::prepare()
            .map_err(|error| LoadError::Runtime(format!("无法准备 macOS TUN 辅助服务：{error}")))?;
        let final_path = spec.data_dir.join(GENERATED_PROFILE_FILE);
        let config = managed_engine_config(spec, final_path.clone());
        validate_managed_config(&config)?;

        let mut manager = manager
            .lock()
            .map_err(|_poisoned| LoadError::Runtime("托管内核状态锁已损坏".to_owned()))?;
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
                    LoadError::Runtime(format!("无法回收 Manis 遗留的普通 Mihomo：{error}"))
                })?;
        }
        *manager = EngineManager::with_adapters(
            config,
            ReadinessPolicy::default(),
            Box::new(spawner),
            readiness_probe(spec),
        );
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
                let fallback = if was_running {
                    manager.start().err()
                } else {
                    None
                };
                let message = match fallback {
                    Some(fallback) => format!(
                        "特权 Mihomo 启动失败：{privileged_error}；恢复普通 Mihomo 也失败：{fallback}"
                    ),
                    None => format!("特权 Mihomo 启动失败：{privileged_error}"),
                };
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
                let mut manager = manager
                    .lock()
                    .map_err(|_poisoned| LoadError::Runtime("托管内核状态锁已损坏".to_owned()))?;
                manager
                    .running_endpoint()?
                    .ok_or_else(|| LoadError::Runtime("Mihomo 尚未运行，请先连接内核".to_owned()))?
                    .uri()
            }
            #[cfg(any(test, feature = "snapshot-fixtures"))]
            Self::Fixture { .. } => {
                return Err(LoadError::Runtime("测试快照不能修改路由模式".to_owned()));
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
                "当前控制器不由 Manis 管理，不能修改全局出口".to_owned(),
            ));
        };
        let endpoint = {
            let mut manager = manager
                .lock()
                .map_err(|_poisoned| LoadError::Runtime("托管内核状态锁已损坏".to_owned()))?;
            manager
                .running_endpoint()?
                .ok_or_else(|| LoadError::Runtime("Mihomo 尚未运行，请先启动托管内核".to_owned()))?
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
            return Err(LoadError::Runtime("当前分组没有可测速节点".to_owned()));
        }
        let controller_secret = self.controller_secret();
        let (endpoint, managed) = match self {
            #[cfg(any(test, feature = "snapshot-fixtures"))]
            Self::Fixture { endpoint } => (endpoint.clone(), false),
            Self::Managed { manager, .. } => {
                let endpoint = {
                    let mut manager = manager.lock().map_err(|_poisoned| {
                        LoadError::Runtime("托管内核状态锁已损坏".to_owned())
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
            return Err(LoadError::Runtime("当前分组没有可测速节点".to_owned()));
        }
        let controller_secret = self.controller_secret();
        let endpoint = match self {
            #[cfg(any(test, feature = "snapshot-fixtures"))]
            Self::Fixture { endpoint } => endpoint.clone(),
            Self::Managed { manager, .. } => {
                let endpoint = {
                    let mut manager = manager.lock().map_err(|_poisoned| {
                        LoadError::Runtime("托管内核状态锁已损坏".to_owned())
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
                        LoadError::Runtime("托管内核状态锁已损坏".to_owned())
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
                "Mihomo 未返回任何有效节点延迟".to_owned(),
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
                "当前控制器不由 Manis 管理，不能修改其策略组".to_owned(),
            ));
        };
        let endpoint = {
            let mut manager = manager
                .lock()
                .map_err(|_poisoned| LoadError::Runtime("托管内核状态锁已损坏".to_owned()))?;
            manager
                .running_endpoint()?
                .ok_or_else(|| LoadError::Runtime("Mihomo 尚未运行，请先启动托管内核".to_owned()))?
                .uri()
        };
        select_policy_group_candidate(
            &endpoint,
            group_name,
            selected_name,
            controller_secret.as_deref(),
        )
    }

    #[allow(clippy::too_many_lines)]
    pub(crate) fn apply_saved_sources(
        &self,
        store_dir: &Path,
    ) -> Result<GeneratedProfileApply, LoadError> {
        let Self::Managed {
            manager,
            generated_profile: Some(spec),
            privileged,
            ..
        } = self
        else {
            return Err(LoadError::Runtime(match self {
                #[cfg(any(test, feature = "snapshot-fixtures"))]
                Self::Fixture { .. } => "测试快照不能写入运行配置".to_owned(),
                Self::Invalid { message } => message.clone(),
                Self::Managed { .. } => "托管内核缺少 Manis 生成配置".to_owned(),
            }));
        };
        let profile = compile_saved_profile(store_dir, None, spec.kernel)?;
        let rendered = render_generated_profile(spec, &profile)?;
        let (candidate_name, final_name) = generated_profile_names(spec.kernel);
        let candidate_path =
            write_private_atomic(&spec.data_dir, candidate_name, rendered.as_bytes())
                .map_err(|_error| LoadError::Runtime("无法写入候选托管配置".to_owned()))?;
        let candidate_config = managed_engine_config(spec, candidate_path.clone());
        let validation = validate_managed_config(&candidate_config);
        let _ = fs::remove_file(&candidate_path);
        validation?;

        let final_path = spec.data_dir.join(final_name);
        let previous_config = fs::read(&final_path).ok();
        write_private_atomic(&spec.data_dir, final_name, rendered.as_bytes())
            .map_err(|_error| LoadError::Runtime("无法替换托管配置".to_owned()))?;
        let final_config = managed_engine_config(spec, final_path);
        let mut manager = manager
            .lock()
            .map_err(|_poisoned| LoadError::Runtime("托管内核状态锁已损坏".to_owned()))?;
        let running_endpoint = manager.running_endpoint()?;
        let was_running = running_endpoint.is_some();
        let restore_tun = if spec.kernel == KernelKind::Mihomo {
            running_endpoint
                .as_ref()
                .map(|endpoint| {
                    fetch_runtime_config(&endpoint.uri(), spec.controller_secret.as_deref())
                        .map(|runtime| runtime.tun.enable)
                        .map_err(LoadError::from)
                })
                .transpose()?
                .unwrap_or(false)
        } else {
            false
        };
        let was_privileged = privileged.load(Ordering::Acquire);
        if was_running {
            manager.stop()?;
            #[cfg(target_os = "macos")]
            if restore_tun
                && !crate::macos_privileged::wait_for_tun_route_release().map_err(|error| {
                    LoadError::ProxyModeLost(format!(
                        "重载托管配置前无法确认 macOS TUN 路由已释放：{error}"
                    ))
                })?
            {
                return Err(LoadError::ProxyModeLost(
                    "重载托管配置前 macOS TUN 路由未释放".to_owned(),
                ));
            }
        }
        *manager = match generated_engine_manager(spec, final_config, was_privileged) {
            Ok(manager) => manager,
            Err(error) if restore_tun => {
                return Err(LoadError::ProxyModeLost(format!(
                    "TUN 已停止，但无法创建新托管内核：{error}"
                )));
            }
            Err(error) => return Err(error),
        };
        if was_running && let Err(error) = manager.start() {
            let restart_error = error.to_string();
            let mut rollback_running = false;
            if let Some(previous_config) = previous_config {
                let _ = write_private_atomic(&spec.data_dir, final_name, &previous_config);
                let rollback_config = managed_engine_config(spec, spec.data_dir.join(final_name));
                *manager = match generated_engine_manager(spec, rollback_config, was_privileged) {
                    Ok(manager) => manager,
                    Err(rollback_error) if restore_tun => {
                        return Err(LoadError::ProxyModeLost(format!(
                            "新配置启动失败（{restart_error}），旧配置管理器恢复也失败：{rollback_error}"
                        )));
                    }
                    Err(rollback_error) => return Err(rollback_error),
                };
                rollback_running = manager.start().is_ok();
            }
            drop(manager);
            if restore_tun && rollback_running {
                record_event(
                    LogLevel::Info,
                    "proxy.mode.restore.requested",
                    "mode=tun reason=managed_config_rollback",
                );
                if let Err(restore_error) = self.set_tun_enabled(true) {
                    record_event(
                        LogLevel::Error,
                        "proxy.mode.restore.failed",
                        format!("mode=tun reason=managed_config_rollback error={restore_error}"),
                    );
                    return Err(LoadError::ProxyModeLost(format!(
                        "新配置启动失败（{restart_error}），旧配置已恢复，但 TUN 重新启用失败：{restore_error}"
                    )));
                }
                record_event(
                    LogLevel::Info,
                    "proxy.mode.restore.succeeded",
                    "mode=tun reason=managed_config_rollback",
                );
            } else if restore_tun {
                return Err(LoadError::ProxyModeLost(format!(
                    "新配置启动失败（{restart_error}），且旧配置未能重新启动"
                )));
            }
            return Err(LoadError::Engine(error));
        }
        drop(manager);
        if restore_tun {
            record_event(
                LogLevel::Info,
                "proxy.mode.restore.requested",
                "mode=tun reason=managed_config_restart",
            );
            if let Err(error) = self.set_tun_enabled(true) {
                record_event(
                    LogLevel::Error,
                    "proxy.mode.restore.failed",
                    format!("mode=tun reason=managed_config_restart error={error}"),
                );
                return Err(LoadError::ProxyModeLost(error.to_string()));
            }
            record_event(
                LogLevel::Info,
                "proxy.mode.restore.succeeded",
                "mode=tun reason=managed_config_restart",
            );
        }
        Ok(if was_running {
            GeneratedProfileApply::Restarted
        } else {
            GeneratedProfileApply::Updated
        })
    }
}

fn generated_engine_manager(
    spec: &ManagedGeneratedProfile,
    config: ManagedEngineConfig,
    privileged: bool,
) -> Result<EngineManager, LoadError> {
    #[cfg(target_os = "macos")]
    if privileged {
        let spawner = crate::macos_privileged::MacosPrivilegedProcessSpawner::prepare()
            .map_err(|error| LoadError::Runtime(format!("无法连接 macOS TUN 辅助服务：{error}")))?;
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
    let mut subscriptions = base_subscription.into_iter().collect::<Vec<_>>();
    let stored_subscriptions = load_subscription_sources_in(store_dir)
        .map_err(|_error| LoadError::Runtime("无法读取已保存的订阅来源".to_owned()))?;
    if kernel == KernelKind::SingBox && !stored_subscriptions.is_empty() {
        return Err(LoadError::Runtime(
            "sing-box 暂不能直接读取 Clash 订阅；请先使用手动 VLESS 节点".to_owned(),
        ));
    }
    let mut stored_provider_indexes = HashMap::new();
    let mut proxy_server_nameservers = Vec::new();
    for stored in &stored_subscriptions {
        let provider_index = if let Some(index) = subscriptions
            .iter()
            .position(|subscription| subscription == &stored.source)
        {
            index
        } else {
            subscriptions.push(stored.source.clone());
            subscriptions.len() - 1
        };
        stored_provider_indexes.insert(stored.id.as_str(), provider_index);
        for nameserver in &stored.proxy_server_nameservers {
            if !proxy_server_nameservers.contains(nameserver)
                && proxy_server_nameservers.len() < MAX_SUBSCRIPTION_PROXY_DNS_SERVERS
            {
                proxy_server_nameservers.push(nameserver.clone());
            }
        }
    }
    let vless_nodes = load_vless_sources_in(store_dir)
        .map_err(|_error| LoadError::Runtime("无法读取已保存的 VLESS 节点".to_owned()))?
        .into_iter()
        .map(|stored| {
            stored
                .source
                .expose_to(VlessProxy::parse_share_link)
                .map_err(|error| LoadError::Runtime(error.to_string()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mixed_port = configured_mixed_port().map_err(LoadError::Runtime)?;
    let policy_groups = load_managed_policy_groups_in(store_dir)
        .map_err(|_error| LoadError::Runtime("无法读取策略组".to_owned()))?;
    let user_groups = compile_managed_policy_groups(
        &policy_groups,
        &stored_provider_indexes,
        &vless_nodes,
        subscriptions.len(),
    )?;
    let bootstrap = subscriptions.is_empty() && vless_nodes.is_empty();
    let mut profile = if bootstrap {
        if !user_groups.is_empty() {
            return Err(LoadError::Runtime(
                "没有节点来源时不能生成策略组".to_owned(),
            ));
        }
        Profile::managed_empty(mixed_port)
    } else {
        Profile::qx_sources_with_groups(subscriptions, vless_nodes, user_groups, mixed_port)
    }
    .map_err(|error| LoadError::Runtime(error.to_string()))?;
    let bootstrap_fallback = if bootstrap { profile.rules.pop() } else { None };
    if !proxy_server_nameservers.is_empty() {
        profile.set_proxy_server_nameservers(proxy_server_nameservers);
    }
    let routing_mode = load_routing_mode_in(store_dir)
        .map_err(|_error| LoadError::Runtime("无法读取已保存的路由模式".to_owned()))?;
    profile.set_mode(profile_mode(routing_mode));
    let qx_rule_sources = load_qx_rule_sources_in(store_dir)
        .map_err(|_error| LoadError::Runtime("无法读取 QX 规则来源".to_owned()))?;
    let manual_rules = crate::manual_rule::load_manual_rules_in(store_dir)
        .map_err(|error| LoadError::Runtime(error.to_string()))?;
    let stored_group_order = load_routing_rule_group_order_in(store_dir)
        .map_err(|_error| LoadError::Runtime("无法读取已保存的分流规则分组顺序".to_owned()))?;
    let group_order = normalized_routing_rule_group_order(
        &stored_group_order,
        !manual_rules.is_empty(),
        &qx_rule_sources,
    );
    for group_id in group_order {
        if group_id == MANUAL_ROUTING_RULE_GROUP_ID {
            crate::manual_rule::append_manual_rules(&mut profile, &manual_rules, kernel)
                .map_err(|error| LoadError::Runtime(error.to_string()))?;
        } else if let Some(source) = qx_rule_sources.iter().find(|source| source.id == group_id) {
            apply_qx_rule_sources(&mut profile, std::slice::from_ref(source))?;
        }
    }
    if let Some(fallback) = bootstrap_fallback {
        profile.rules.push(fallback);
    }
    Ok(profile)
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
                    "sing-box 需要私有 loopback Clash API".to_owned(),
                ));
            };
            let secret = spec
                .controller_secret
                .as_deref()
                .ok_or_else(|| LoadError::Runtime("sing-box controller 缺少认证密钥".to_owned()))?;
            render_sing_box_json(profile, &SingBoxOptions::new(address.to_string(), secret))
                .map_err(|error| LoadError::Runtime(error.to_string()))
        }
    }
}

fn compile_managed_generated_profile(spec: &ManagedGeneratedProfile) -> Result<Profile, LoadError> {
    let store_dir = spec
        .profile_store_dir
        .as_deref()
        .ok_or_else(|| LoadError::Runtime("托管内核缺少 Manis 来源目录".to_owned()))?;
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
    pub activity: String,
    pub logs: String,
}

impl Default for LiveStreamStatus {
    fn default() -> Self {
        Self {
            activity: "等待连接".to_owned(),
            logs: "等待连接".to_owned(),
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
                    activity: "实时状态不可用".to_owned(),
                    logs: "实时状态不可用".to_owned(),
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

    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    fn source_label(&self) -> &str {
        self.provider.as_deref().unwrap_or("direct")
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum RemoteSourceRefreshInterval {
    #[default]
    Manual,
    Hourly,
    SixHours,
    TwelveHours,
    Daily,
}

impl RemoteSourceRefreshInterval {
    fn key(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::Hourly => "1h",
            Self::SixHours => "6h",
            Self::TwelveHours => "12h",
            Self::Daily => "24h",
        }
    }

    fn parse_key(input: &str) -> Option<Self> {
        match input {
            "manual" => Some(Self::Manual),
            "1h" => Some(Self::Hourly),
            "6h" => Some(Self::SixHours),
            "12h" => Some(Self::TwelveHours),
            "24h" => Some(Self::Daily),
            _ => None,
        }
    }

    pub(crate) fn interval_secs(self) -> Option<u64> {
        match self {
            Self::Manual => None,
            Self::Hourly => Some(60 * 60),
            Self::SixHours => Some(6 * 60 * 60),
            Self::TwelveHours => Some(12 * 60 * 60),
            Self::Daily => Some(24 * 60 * 60),
        }
    }

    pub(crate) fn next(self) -> Self {
        match self {
            Self::Manual => Self::Hourly,
            Self::Hourly => Self::SixHours,
            Self::SixHours => Self::TwelveHours,
            Self::TwelveHours => Self::Daily,
            Self::Daily => Self::Manual,
        }
    }

    pub(crate) fn is_due(self, last_successful_update_unix_secs: u64, now_unix_secs: u64) -> bool {
        let Some(interval_secs) = self.interval_secs() else {
            return false;
        };
        last_successful_update_unix_secs == 0
            || now_unix_secs.saturating_sub(last_successful_update_unix_secs) >= interval_secs
    }
}

#[derive(Clone, Eq, PartialEq)]
pub(crate) struct StoredSubscription {
    pub id: String,
    pub source: SecretUrl,
    pub refresh_interval: RemoteSourceRefreshInterval,
    pub last_successful_update_unix_secs: u64,
    pub proxy_server_nameservers: Vec<ProxyDnsServer>,
}

impl fmt::Debug for StoredSubscription {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StoredSubscription")
            .field("id", &self.id)
            .field("source", &"<redacted>")
            .field("refresh_interval", &self.refresh_interval)
            .field(
                "last_successful_update_unix_secs",
                &self.last_successful_update_unix_secs,
            )
            .field(
                "proxy_server_nameservers",
                &format_args!("<{} redacted>", self.proxy_server_nameservers.len()),
            )
            .finish()
    }
}

#[derive(Clone, Eq, PartialEq)]
pub(crate) struct StoredVlessNode {
    pub id: String,
    pub source: VlessSource,
}

impl fmt::Debug for StoredVlessNode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StoredVlessNode")
            .field("id", &self.id)
            .field("source", &"<redacted>")
            .finish()
    }
}

#[derive(Clone, Eq, PartialEq)]
pub(crate) struct StoredQxRuleSource {
    pub id: String,
    pub source: SecretUrl,
    pub target_policy: Name,
    pub content: String,
    pub rule_count: usize,
    pub diagnostic_count: usize,
    pub refresh_interval: RemoteSourceRefreshInterval,
    pub last_successful_update_unix_secs: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum SaveQxRuleSourceOutcome {
    Created(StoredQxRuleSource),
    Existing(StoredQxRuleSource),
}

impl SaveQxRuleSourceOutcome {
    #[cfg(test)]
    pub(crate) fn into_source(self) -> StoredQxRuleSource {
        match self {
            Self::Created(source) | Self::Existing(source) => source,
        }
    }
}

impl fmt::Debug for StoredQxRuleSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StoredQxRuleSource")
            .field("id", &self.id)
            .field("source", &"<redacted>")
            .field("target_policy", &self.target_policy)
            .field("content", &"<redacted>")
            .field("rule_count", &self.rule_count)
            .field("diagnostic_count", &self.diagnostic_count)
            .field("refresh_interval", &self.refresh_interval)
            .field(
                "last_successful_update_unix_secs",
                &self.last_successful_update_unix_secs,
            )
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SubscriptionPreviewError {
    UnsupportedPlatform,
    BinaryUnavailable,
    InvalidSource,
    WorkspaceUnavailable,
    ProfileUnavailable,
    EngineUnavailable,
    ProviderUnavailable,
    EmptyProvider,
}

impl fmt::Display for SubscriptionPreviewError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::UnsupportedPlatform => "当前平台尚不能启动隔离的 Mihomo 预览进程",
            Self::BinaryUnavailable => "找不到 Mihomo 内核；请安装 Clash Verge Rev 或配置预览内核",
            Self::InvalidSource => "订阅地址无效，请检查后重试",
            Self::WorkspaceUnavailable => "无法创建私有预览空间，请检查临时目录权限",
            Self::ProfileUnavailable => "无法生成安全的订阅预览配置",
            Self::EngineUnavailable => "Mihomo 预览进程启动失败",
            Self::ProviderUnavailable => "Mihomo 无法下载或解析这份订阅，请检查网络和订阅状态",
            Self::EmptyProvider => "订阅可以访问，但没有解析出任何代理节点",
        })
    }
}

impl Error for SubscriptionPreviewError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SubscriptionStoreError {
    DataDirectoryUnavailable,
    InvalidSource,
    StoreUnavailable,
    StoredSourceUnavailable,
}

impl fmt::Display for SubscriptionStoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::DataDirectoryUnavailable => "无法确定 Manis 的用户数据目录",
            Self::InvalidSource => "订阅地址无效，未执行导入",
            Self::StoreUnavailable => "无法安全保存订阅，请检查用户数据目录权限",
            Self::StoredSourceUnavailable => "已保存的订阅无法安全读取，需要重新导入",
        })
    }
}

impl Error for SubscriptionStoreError {}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct NodeSelectionPreferences {
    global: Option<NodeIdentity>,
    policy_targets: BTreeMap<String, String>,
}

impl NodeSelectionPreferences {
    pub(crate) fn global(&self) -> Option<&NodeIdentity> {
        self.global.as_ref()
    }

    pub(crate) fn set_global(&mut self, global: NodeIdentity) {
        self.global = Some(global);
    }

    #[allow(dead_code)]
    pub(crate) fn clear_global(&mut self) {
        self.global = None;
    }

    pub(crate) fn policy_target(&self, policy: &str) -> Option<&str> {
        self.policy_targets.get(policy).map(String::as_str)
    }

    pub(crate) fn set_policy_target(
        &mut self,
        policy: impl AsRef<str>,
        target: impl AsRef<str>,
    ) -> Result<(), SubscriptionStoreError> {
        let policy = policy.as_ref();
        let target = target.as_ref();
        validate_node_selection_policy(policy)?;
        validate_node_selection_target(target)?;
        self.policy_targets
            .insert(policy.to_owned(), target.to_owned());
        Ok(())
    }

    #[allow(dead_code)]
    pub(crate) fn clear_policy_target(
        &mut self,
        policy: &str,
    ) -> Result<(), SubscriptionStoreError> {
        validate_node_selection_policy(policy)?;
        self.policy_targets.remove(policy);
        Ok(())
    }

    pub(crate) fn iter_policy_targets(&self) -> impl Iterator<Item = (&str, &str)> {
        self.policy_targets
            .iter()
            .map(|(policy, target)| (policy.as_str(), target.as_str()))
    }

    fn validate(&self) -> Result<(), SubscriptionStoreError> {
        if self.policy_targets.len() > MAX_NODE_SELECTION_POLICY_TARGETS {
            return Err(SubscriptionStoreError::InvalidSource);
        }
        for (policy, target) in &self.policy_targets {
            validate_node_selection_policy(policy)?;
            validate_node_selection_target(target)?;
        }
        Ok(())
    }
}

struct PreviewWorkspace {
    path: PathBuf,
}

impl PreviewWorkspace {
    fn create() -> Result<Self, std::io::Error> {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        #[cfg(unix)]
        let temp_root = PathBuf::from("/tmp");
        #[cfg(not(unix))]
        let temp_root = env::temp_dir();
        for _ in 0..16 {
            let sequence = NEXT_PREVIEW_WORKSPACE.fetch_add(1, Ordering::Relaxed);
            let path = temp_root.join(format!(
                "manis-p-{:x}-{nonce:x}-{sequence:x}",
                std::process::id()
            ));
            let mut builder = fs::DirBuilder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            match builder.create(&path) {
                Ok(()) => return Ok(Self { path }),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error),
            }
        }
        Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "could not reserve a unique preview workspace",
        ))
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for PreviewWorkspace {
    fn drop(&mut self) {
        if let Ok(metadata) = fs::symlink_metadata(&self.path)
            && metadata.is_dir()
            && !metadata.file_type().is_symlink()
        {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

pub(crate) fn discover_subscription_proxy_nameservers(source: &SecretUrl) -> Vec<ProxyDnsServer> {
    if let Ok(document) = download_subscription_document(source) {
        let nameservers = extract_subscription_proxy_nameservers(&document);
        record_event(
            LogLevel::Info,
            "subscription.metadata.loaded",
            format!("proxy_dns_count={}", nameservers.len()),
        );
        nameservers
    } else {
        record_event(
            LogLevel::Warn,
            "subscription.metadata.unavailable",
            "proxy_dns_count=0; using safe defaults",
        );
        Vec::new()
    }
}

fn download_subscription_document(source: &SecretUrl) -> Result<String, ()> {
    let config = Agent::config_builder()
        .https_only(source.is_https())
        .max_redirects(SUBSCRIPTION_MAX_REDIRECTS)
        .timeout_global(Some(SUBSCRIPTION_DOWNLOAD_TIMEOUT))
        .user_agent("clash.meta")
        .build();
    let agent: Agent = config.into();
    let mut response = source
        .expose_to(|url| agent.get(url).call())
        .map_err(|_error| ())?;
    if source.is_https() && response.get_uri().scheme_str() != Some("https") {
        return Err(());
    }
    let document = response
        .body_mut()
        .with_config()
        .limit(MAX_SUBSCRIPTION_DOCUMENT_BYTES + 1)
        .lossy_utf8(false)
        .read_to_string()
        .map_err(|_error| ())?;
    if document.len() as u64 > MAX_SUBSCRIPTION_DOCUMENT_BYTES {
        return Err(());
    }
    Ok(document)
}

fn extract_subscription_proxy_nameservers(document: &str) -> Vec<ProxyDnsServer> {
    let mut dns_indent = None;
    let mut list_indent = None;
    let mut nameservers = Vec::new();

    for line in document.lines() {
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        let indent = line
            .len()
            .saturating_sub(line.trim_start_matches(' ').len());
        let trimmed = line.trim();
        let Some(active_dns_indent) = dns_indent else {
            if indent == 0 && trimmed == "dns:" {
                dns_indent = Some(indent);
            }
            continue;
        };
        if indent <= active_dns_indent {
            break;
        }

        if let Some(active_list_indent) = list_indent {
            if indent <= active_list_indent {
                break;
            }
            let Some(value) = trimmed.strip_prefix('-') else {
                continue;
            };
            push_proxy_dns_scalar(value, &mut nameservers);
            if nameservers.len() == MAX_SUBSCRIPTION_PROXY_DNS_SERVERS {
                break;
            }
            continue;
        }

        let Some(value) = trimmed.strip_prefix("proxy-server-nameserver:") else {
            continue;
        };
        list_indent = Some(indent);
        let value = value.trim();
        if let Some(inline) = value
            .strip_prefix('[')
            .and_then(|value| value.strip_suffix(']'))
        {
            for scalar in split_inline_yaml_scalars(inline) {
                push_proxy_dns_scalar(scalar, &mut nameservers);
                if nameservers.len() == MAX_SUBSCRIPTION_PROXY_DNS_SERVERS {
                    break;
                }
            }
            break;
        }
    }
    nameservers
}

fn split_inline_yaml_scalars(value: &str) -> Vec<&str> {
    let mut values = Vec::new();
    let mut quote = None;
    let mut start = 0;
    for (index, character) in value.char_indices() {
        match (quote, character) {
            (None, '\'' | '"') => quote = Some(character),
            (Some(active), current) if active == current => quote = None,
            (None, ',') => {
                values.push(&value[start..index]);
                start = index + character.len_utf8();
            }
            _ => {}
        }
    }
    values.push(&value[start..]);
    values
}

fn push_proxy_dns_scalar(value: &str, nameservers: &mut Vec<ProxyDnsServer>) {
    let value = value.trim();
    let value = if value.len() >= 2
        && ((value.starts_with('\'') && value.ends_with('\''))
            || (value.starts_with('"') && value.ends_with('"')))
    {
        &value[1..value.len() - 1]
    } else {
        value
    };
    let Ok(nameserver) = ProxyDnsServer::parse_https(value) else {
        return;
    };
    if !nameservers.contains(&nameserver) {
        nameservers.push(nameserver);
    }
}

pub(crate) fn preview_subscription(
    input: &str,
) -> Result<Vec<LoadedProvider>, SubscriptionPreviewError> {
    let binary = discover_preview_binary()?;
    preview_subscription_with_binary(input, &binary)
}

pub(crate) fn preview_imported_subscription(
    subscription: SecretUrl,
) -> Result<Vec<LoadedProvider>, SubscriptionPreviewError> {
    let binary = discover_preview_binary()?;
    preview_secret_subscription_with_binary(subscription, &binary)
}

fn preview_subscription_with_binary(
    input: &str,
    binary: &Path,
) -> Result<Vec<LoadedProvider>, SubscriptionPreviewError> {
    let subscription = SecretUrl::parse_subscription(input)
        .map_err(|_error| SubscriptionPreviewError::InvalidSource)?;
    preview_secret_subscription_with_binary(subscription, binary)
}

fn preview_secret_subscription_with_binary(
    subscription: SecretUrl,
    binary: &Path,
) -> Result<Vec<LoadedProvider>, SubscriptionPreviewError> {
    #[cfg(not(unix))]
    {
        let _ = (subscription, binary);
        return Err(SubscriptionPreviewError::UnsupportedPlatform);
    }

    #[cfg(unix)]
    {
        let binary = canonical_binary(binary)?;
        let workspace = PreviewWorkspace::create()
            .map_err(|_error| SubscriptionPreviewError::WorkspaceUnavailable)?;
        let mixed_port = reserve_preview_port()?;
        let profile = Profile::subscription_preview(subscription, mixed_port)
            .map_err(|_error| SubscriptionPreviewError::ProfileUnavailable)?;
        let yaml = render_mihomo_yaml(&profile)
            .map_err(|_error| SubscriptionPreviewError::ProfileUnavailable)?;
        let config_file = write_private_atomic(workspace.path(), "preview.yaml", yaml.as_bytes())
            .map_err(|_error| SubscriptionPreviewError::WorkspaceUnavailable)?;
        let controller = ControllerEndpoint::UnixSocket(workspace.path().join("controller.sock"));
        let config =
            ManagedEngineConfig::new(binary, config_file, workspace.path().to_owned(), controller);
        let mut manager = EngineManager::new(
            config,
            ReadinessPolicy::default(),
            Box::new(MihomoReadinessProbe),
        );
        let endpoint = manager
            .start()
            .map_err(|_error| SubscriptionPreviewError::EngineUnavailable)?;
        let providers = wait_for_preview_providers(&endpoint);
        manager
            .stop()
            .map_err(|_error| SubscriptionPreviewError::EngineUnavailable)?;
        providers
    }
}

pub(crate) fn imported_subscription_store_dir() -> Result<PathBuf, SubscriptionStoreError> {
    brand::data_dir()
        .map(|directory| directory.join("subscriptions"))
        .ok_or(SubscriptionStoreError::DataDirectoryUnavailable)
}

#[cfg(not(windows))]
pub(crate) fn save_subscription_source_in(
    directory: &Path,
    input: &str,
) -> Result<StoredSubscription, SubscriptionStoreError> {
    let source = SecretUrl::parse_subscription(input)
        .map_err(|_error| SubscriptionStoreError::InvalidSource)?;
    if let Some(existing) = load_subscription_sources_in(directory)?
        .into_iter()
        .find(|stored| stored.source == source)
    {
        return Ok(existing);
    }
    let id = next_stored_source_id(STORED_SUBSCRIPTION_PREFIX);
    let file_name = format!("{id}{STORED_SUBSCRIPTION_SUFFIX}");
    let last_successful_update_unix_secs = current_unix_secs();
    let contents = encode_subscription_source(
        &id,
        input,
        RemoteSourceRefreshInterval::Manual,
        last_successful_update_unix_secs,
        &[],
    )?;
    write_private_atomic(directory, &file_name, contents.as_bytes())
        .map_err(|_error| SubscriptionStoreError::StoreUnavailable)?;
    Ok(StoredSubscription {
        id,
        source,
        refresh_interval: RemoteSourceRefreshInterval::Manual,
        last_successful_update_unix_secs,
        proxy_server_nameservers: Vec::new(),
    })
}

#[cfg(windows)]
pub(crate) fn save_subscription_source_in(
    _directory: &Path,
    _input: &str,
) -> Result<StoredSubscription, SubscriptionStoreError> {
    Err(SubscriptionStoreError::StoreUnavailable)
}

#[cfg(not(windows))]
pub(crate) fn load_subscription_sources_in(
    directory: &Path,
) -> Result<Vec<StoredSubscription>, SubscriptionStoreError> {
    let mut sources = Vec::new();
    let Some(entries) = private_store_entries(directory)? else {
        return Ok(sources);
    };
    for path in entries {
        let Some(file_name) = path.file_name().and_then(std::ffi::OsStr::to_str) else {
            continue;
        };
        let id = if file_name == IMPORTED_SUBSCRIPTION_FILE {
            "subscription:legacy".to_owned()
        } else if let Some(id) = file_name.strip_suffix(STORED_SUBSCRIPTION_SUFFIX)
            && valid_stored_id(id, STORED_SUBSCRIPTION_PREFIX)
        {
            id.to_owned()
        } else {
            continue;
        };
        let contents =
            read_private_source_allow_empty_max(&path, MAX_STORED_SUBSCRIPTION_FILE_BYTES)?;
        let decoded = decode_subscription_source(&contents, &id)?;
        if !sources
            .iter()
            .any(|stored: &StoredSubscription| stored.source == decoded.stored.source)
        {
            sources.push(decoded.stored);
        }
    }
    sources.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(sources)
}

#[cfg(windows)]
pub(crate) fn load_subscription_sources_in(
    directory: &Path,
) -> Result<Vec<StoredSubscription>, SubscriptionStoreError> {
    load_imported_subscription_in(directory).map(|source| {
        source
            .map(|source| {
                vec![StoredSubscription {
                    id: "subscription:legacy".to_owned(),
                    source,
                    refresh_interval: RemoteSourceRefreshInterval::Manual,
                    last_successful_update_unix_secs: 0,
                    proxy_server_nameservers: Vec::new(),
                }]
            })
            .unwrap_or_default()
    })
}

#[cfg(not(windows))]
#[allow(dead_code)]
pub(crate) fn update_subscription_source_refresh_interval_in(
    directory: &Path,
    id: &str,
    refresh_interval: RemoteSourceRefreshInterval,
) -> Result<StoredSubscription, SubscriptionStoreError> {
    let decoded = read_subscription_source_by_id_in(directory, id)?;
    write_subscription_source_in(
        directory,
        id,
        &decoded.url_input,
        refresh_interval,
        decoded.stored.last_successful_update_unix_secs,
        &decoded.stored.proxy_server_nameservers,
    )
}

#[cfg(windows)]
#[allow(dead_code)]
pub(crate) fn update_subscription_source_refresh_interval_in(
    _directory: &Path,
    _id: &str,
    _refresh_interval: RemoteSourceRefreshInterval,
) -> Result<StoredSubscription, SubscriptionStoreError> {
    Err(SubscriptionStoreError::StoreUnavailable)
}

#[cfg(not(windows))]
#[allow(dead_code)]
pub(crate) fn mark_subscription_source_update_success_in(
    directory: &Path,
    id: &str,
    last_successful_update_unix_secs: u64,
) -> Result<StoredSubscription, SubscriptionStoreError> {
    let decoded = read_subscription_source_by_id_in(directory, id)?;
    write_subscription_source_in(
        directory,
        id,
        &decoded.url_input,
        decoded.stored.refresh_interval,
        last_successful_update_unix_secs,
        &decoded.stored.proxy_server_nameservers,
    )
}

#[cfg(not(windows))]
pub(crate) fn update_subscription_source_proxy_nameservers_in(
    directory: &Path,
    id: &str,
    proxy_server_nameservers: &[ProxyDnsServer],
) -> Result<StoredSubscription, SubscriptionStoreError> {
    if proxy_server_nameservers.is_empty()
        || proxy_server_nameservers.len() > MAX_SUBSCRIPTION_PROXY_DNS_SERVERS
    {
        return Err(SubscriptionStoreError::InvalidSource);
    }
    let decoded = read_subscription_source_by_id_in(directory, id)?;
    write_subscription_source_in(
        directory,
        id,
        &decoded.url_input,
        decoded.stored.refresh_interval,
        decoded.stored.last_successful_update_unix_secs,
        proxy_server_nameservers,
    )
}

#[cfg(windows)]
pub(crate) fn update_subscription_source_proxy_nameservers_in(
    _directory: &Path,
    _id: &str,
    _proxy_server_nameservers: &[ProxyDnsServer],
) -> Result<StoredSubscription, SubscriptionStoreError> {
    Err(SubscriptionStoreError::StoreUnavailable)
}

#[cfg(windows)]
#[allow(dead_code)]
pub(crate) fn mark_subscription_source_update_success_in(
    _directory: &Path,
    _id: &str,
    _last_successful_update_unix_secs: u64,
) -> Result<StoredSubscription, SubscriptionStoreError> {
    Err(SubscriptionStoreError::StoreUnavailable)
}

#[cfg(not(windows))]
pub(crate) fn remove_subscription_source_in(
    directory: &Path,
    id: &str,
) -> Result<(), SubscriptionStoreError> {
    require_clean_absolute_store(directory)?;
    let file_name = if id == "subscription:legacy" {
        IMPORTED_SUBSCRIPTION_FILE.to_owned()
    } else if valid_stored_id(id, STORED_SUBSCRIPTION_PREFIX) {
        format!("{id}{STORED_SUBSCRIPTION_SUFFIX}")
    } else {
        return Err(SubscriptionStoreError::StoreUnavailable);
    };
    remove_private_source(&directory.join(file_name))
}

#[cfg(windows)]
pub(crate) fn remove_subscription_source_in(
    _directory: &Path,
    _id: &str,
) -> Result<(), SubscriptionStoreError> {
    Err(SubscriptionStoreError::StoreUnavailable)
}

#[cfg(not(windows))]
pub(crate) fn save_vless_source_in(
    directory: &Path,
    input: &str,
) -> Result<StoredVlessNode, SubscriptionStoreError> {
    let source =
        VlessSource::parse(input).map_err(|_error| SubscriptionStoreError::InvalidSource)?;
    if let Some(existing) = load_vless_sources_in(directory)?
        .into_iter()
        .find(|stored| stored.source == source)
    {
        return Ok(existing);
    }
    let id = next_stored_source_id(SAVED_VLESS_PREFIX);
    let file_name = format!("{id}{SAVED_VLESS_SUFFIX}");
    source.expose_to(|value| {
        write_private_atomic(directory, &file_name, value.as_bytes())
            .map_err(|_error| SubscriptionStoreError::StoreUnavailable)
    })?;
    Ok(StoredVlessNode { id, source })
}

#[cfg(windows)]
pub(crate) fn save_vless_source_in(
    _directory: &Path,
    _input: &str,
) -> Result<StoredVlessNode, SubscriptionStoreError> {
    Err(SubscriptionStoreError::StoreUnavailable)
}

#[cfg(not(windows))]
pub(crate) fn load_vless_sources_in(
    directory: &Path,
) -> Result<Vec<StoredVlessNode>, SubscriptionStoreError> {
    let mut nodes = Vec::new();
    let Some(entries) = private_store_entries(directory)? else {
        return Ok(nodes);
    };
    for path in entries {
        let Some(file_name) = path.file_name().and_then(std::ffi::OsStr::to_str) else {
            continue;
        };
        let Some(id) = file_name.strip_suffix(SAVED_VLESS_SUFFIX) else {
            continue;
        };
        if !valid_stored_id(id, SAVED_VLESS_PREFIX) {
            continue;
        }
        let contents = read_private_source(&path)?;
        let source = VlessSource::parse(&contents)
            .map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)?;
        nodes.push(StoredVlessNode {
            id: id.to_owned(),
            source,
        });
    }
    nodes.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(nodes)
}

#[cfg(windows)]
pub(crate) fn load_vless_sources_in(
    _directory: &Path,
) -> Result<Vec<StoredVlessNode>, SubscriptionStoreError> {
    Ok(Vec::new())
}

#[cfg(not(windows))]
pub(crate) fn remove_vless_source_in(
    directory: &Path,
    id: &str,
) -> Result<(), SubscriptionStoreError> {
    require_clean_absolute_store(directory)?;
    if !valid_stored_id(id, SAVED_VLESS_PREFIX) {
        return Err(SubscriptionStoreError::StoreUnavailable);
    }
    remove_private_source(&directory.join(format!("{id}{SAVED_VLESS_SUFFIX}")))
}

#[cfg(windows)]
pub(crate) fn remove_vless_source_in(
    _directory: &Path,
    _id: &str,
) -> Result<(), SubscriptionStoreError> {
    Err(SubscriptionStoreError::StoreUnavailable)
}

#[cfg(not(windows))]
#[allow(dead_code)]
pub(crate) fn save_qx_rule_source_in(
    directory: &Path,
    url_input: &str,
    target_policy: &str,
    content: &str,
) -> Result<SaveQxRuleSourceOutcome, SubscriptionStoreError> {
    let source = SecretUrl::parse_https(url_input)
        .map_err(|_error| SubscriptionStoreError::InvalidSource)?;
    let target_policy =
        Name::parse(target_policy).map_err(|_error| SubscriptionStoreError::InvalidSource)?;
    let (rule_count, diagnostic_count) = validate_qx_rule_source_content(content)?;
    if let Some(existing) = load_qx_rule_sources_in(directory)?
        .into_iter()
        .find(|stored| stored.source == source)
    {
        return Ok(SaveQxRuleSourceOutcome::Existing(existing));
    }
    let id = next_stored_source_id(QX_RULE_SOURCE_PREFIX);
    let file_name = format!("{id}{QX_RULE_SOURCE_SUFFIX}");
    let last_successful_update_unix_secs = current_unix_secs();
    let contents = encode_qx_rule_source(
        &id,
        url_input,
        &target_policy,
        content,
        RemoteSourceRefreshInterval::Manual,
        last_successful_update_unix_secs,
    )?;
    write_private_atomic(directory, &file_name, contents.as_bytes())
        .map_err(|_error| SubscriptionStoreError::StoreUnavailable)?;
    Ok(SaveQxRuleSourceOutcome::Created(StoredQxRuleSource {
        id,
        source,
        target_policy,
        content: content.to_owned(),
        rule_count,
        diagnostic_count,
        refresh_interval: RemoteSourceRefreshInterval::Manual,
        last_successful_update_unix_secs,
    }))
}

#[cfg(windows)]
pub(crate) fn save_qx_rule_source_in(
    _directory: &Path,
    _url_input: &str,
    _target_policy: &str,
    _content: &str,
) -> Result<SaveQxRuleSourceOutcome, SubscriptionStoreError> {
    Err(SubscriptionStoreError::StoreUnavailable)
}

#[cfg(not(windows))]
pub(crate) fn load_qx_rule_sources_in(
    directory: &Path,
) -> Result<Vec<StoredQxRuleSource>, SubscriptionStoreError> {
    let mut sources = Vec::new();
    let Some(entries) = private_store_entries(directory)? else {
        return Ok(sources);
    };
    for path in entries {
        let Some(file_name) = path.file_name().and_then(std::ffi::OsStr::to_str) else {
            continue;
        };
        let Some(id) = file_name.strip_suffix(QX_RULE_SOURCE_SUFFIX) else {
            continue;
        };
        if !valid_stored_id(id, QX_RULE_SOURCE_PREFIX) {
            continue;
        }
        let contents = read_private_source_allow_empty_max(&path, MAX_QX_RULE_SOURCE_FILE_BYTES)?;
        sources.push(decode_qx_rule_source(&contents, id)?);
    }
    sources.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(sources)
}

pub(crate) fn normalized_routing_rule_group_order(
    stored_order: &[String],
    has_manual_rules: bool,
    sources: &[StoredQxRuleSource],
) -> Vec<String> {
    let source_ids = sources
        .iter()
        .map(|source| source.id.as_str())
        .collect::<BTreeSet<_>>();
    let mut seen = BTreeSet::new();
    let mut order = stored_order
        .iter()
        .filter(|id| {
            (has_manual_rules && id.as_str() == MANUAL_ROUTING_RULE_GROUP_ID)
                || source_ids.contains(id.as_str())
        })
        .filter(|id| seen.insert((*id).clone()))
        .cloned()
        .collect::<Vec<_>>();
    if has_manual_rules && seen.insert(MANUAL_ROUTING_RULE_GROUP_ID.to_owned()) {
        order.insert(0, MANUAL_ROUTING_RULE_GROUP_ID.to_owned());
    }
    for source in sources {
        if seen.insert(source.id.clone()) {
            order.push(source.id.clone());
        }
    }
    order
}

pub(crate) fn move_routing_rule_group(order: &mut [String], group_id: &str, direction: i8) -> bool {
    if !matches!(direction, -1 | 1) {
        return false;
    }
    let Some(index) = order.iter().position(|id| id == group_id) else {
        return false;
    };
    let target = if direction < 0 {
        index.checked_sub(1)
    } else {
        index.checked_add(1).filter(|target| *target < order.len())
    };
    let Some(target) = target else {
        return false;
    };
    order.swap(index, target);
    true
}

fn valid_routing_rule_group_id(id: &str) -> bool {
    id == MANUAL_ROUTING_RULE_GROUP_ID || valid_stored_id(id, QX_RULE_SOURCE_PREFIX)
}

#[cfg(not(windows))]
pub(crate) fn save_routing_rule_group_order_in(
    directory: &Path,
    order: &[String],
) -> Result<(), SubscriptionStoreError> {
    if order.len() > MAX_ROUTING_RULE_GROUPS {
        return Err(SubscriptionStoreError::StoreUnavailable);
    }
    let mut seen = BTreeSet::new();
    if order
        .iter()
        .any(|id| !valid_routing_rule_group_id(id) || !seen.insert(id.as_str()))
    {
        return Err(SubscriptionStoreError::StoreUnavailable);
    }
    let mut contents = ROUTING_RULE_GROUP_ORDER_VERSION.to_owned();
    for id in order {
        contents.push('\n');
        contents.push_str(id);
    }
    write_private_atomic(
        directory,
        ROUTING_RULE_GROUP_ORDER_FILE,
        contents.as_bytes(),
    )
    .map(|_path| ())
    .map_err(|_error| SubscriptionStoreError::StoreUnavailable)
}

#[cfg(windows)]
pub(crate) fn save_routing_rule_group_order_in(
    _directory: &Path,
    _order: &[String],
) -> Result<(), SubscriptionStoreError> {
    Err(SubscriptionStoreError::StoreUnavailable)
}

#[cfg(not(windows))]
pub(crate) fn load_routing_rule_group_order_in(
    directory: &Path,
) -> Result<Vec<String>, SubscriptionStoreError> {
    require_clean_absolute_store(directory)?;
    let path = directory.join(ROUTING_RULE_GROUP_ORDER_FILE);
    let contents = match fs::symlink_metadata(&path) {
        Ok(_) => {
            read_private_source_allow_empty_max(&path, MAX_ROUTING_RULE_GROUP_ORDER_FILE_BYTES)?
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(_error) => return Err(SubscriptionStoreError::StoredSourceUnavailable),
    };
    let mut lines = contents.lines();
    if lines.next() != Some(ROUTING_RULE_GROUP_ORDER_VERSION) {
        return Err(SubscriptionStoreError::StoredSourceUnavailable);
    }
    let mut seen = BTreeSet::new();
    lines
        .map(str::to_owned)
        .map(|id| {
            if valid_routing_rule_group_id(&id) && seen.insert(id.clone()) {
                Ok(id)
            } else {
                Err(SubscriptionStoreError::StoredSourceUnavailable)
            }
        })
        .collect()
}

#[cfg(windows)]
pub(crate) fn load_routing_rule_group_order_in(
    _directory: &Path,
) -> Result<Vec<String>, SubscriptionStoreError> {
    Ok(Vec::new())
}

#[cfg(windows)]
pub(crate) fn load_qx_rule_sources_in(
    _directory: &Path,
) -> Result<Vec<StoredQxRuleSource>, SubscriptionStoreError> {
    Ok(Vec::new())
}

#[cfg(not(windows))]
#[allow(dead_code)]
pub(crate) fn remove_qx_rule_source_in(
    directory: &Path,
    id: &str,
) -> Result<(), SubscriptionStoreError> {
    require_clean_absolute_store(directory)?;
    if !valid_stored_id(id, QX_RULE_SOURCE_PREFIX) {
        return Err(SubscriptionStoreError::StoreUnavailable);
    }
    remove_private_source(&directory.join(format!("{id}{QX_RULE_SOURCE_SUFFIX}")))
}

#[cfg(windows)]
pub(crate) fn remove_qx_rule_source_in(
    _directory: &Path,
    _id: &str,
) -> Result<(), SubscriptionStoreError> {
    Err(SubscriptionStoreError::StoreUnavailable)
}

#[cfg(not(windows))]
#[allow(dead_code)]
pub(crate) fn update_qx_rule_source_refresh_interval_in(
    directory: &Path,
    id: &str,
    refresh_interval: RemoteSourceRefreshInterval,
) -> Result<StoredQxRuleSource, SubscriptionStoreError> {
    let decoded = read_qx_rule_source_by_id_in(directory, id)?;
    write_qx_rule_source_in(
        directory,
        id,
        &decoded.url_input,
        decoded.stored.target_policy.as_str(),
        &decoded.stored.content,
        refresh_interval,
        decoded.stored.last_successful_update_unix_secs,
    )
}

#[cfg(windows)]
#[allow(dead_code)]
pub(crate) fn update_qx_rule_source_refresh_interval_in(
    _directory: &Path,
    _id: &str,
    _refresh_interval: RemoteSourceRefreshInterval,
) -> Result<StoredQxRuleSource, SubscriptionStoreError> {
    Err(SubscriptionStoreError::StoreUnavailable)
}

#[cfg(not(windows))]
#[allow(dead_code)]
pub(crate) fn update_qx_rule_source_target_in(
    directory: &Path,
    id: &str,
    target_policy: &str,
) -> Result<StoredQxRuleSource, SubscriptionStoreError> {
    let decoded = read_qx_rule_source_by_id_in(directory, id)?;
    write_qx_rule_source_in(
        directory,
        id,
        &decoded.url_input,
        target_policy,
        &decoded.stored.content,
        decoded.stored.refresh_interval,
        decoded.stored.last_successful_update_unix_secs,
    )
}

#[cfg(windows)]
#[allow(dead_code)]
pub(crate) fn update_qx_rule_source_target_in(
    _directory: &Path,
    _id: &str,
    _target_policy: &str,
) -> Result<StoredQxRuleSource, SubscriptionStoreError> {
    Err(SubscriptionStoreError::StoreUnavailable)
}

#[cfg(not(windows))]
#[allow(dead_code)]
pub(crate) fn replace_qx_rule_source_content_in(
    directory: &Path,
    id: &str,
    content: &str,
    last_successful_update_unix_secs: u64,
) -> Result<StoredQxRuleSource, SubscriptionStoreError> {
    validate_qx_rule_source_content(content)?;
    let decoded = read_qx_rule_source_by_id_in(directory, id)?;
    write_qx_rule_source_in(
        directory,
        id,
        &decoded.url_input,
        decoded.stored.target_policy.as_str(),
        content,
        decoded.stored.refresh_interval,
        last_successful_update_unix_secs,
    )
}

#[cfg(windows)]
#[allow(dead_code)]
pub(crate) fn replace_qx_rule_source_content_in(
    _directory: &Path,
    _id: &str,
    _content: &str,
    _last_successful_update_unix_secs: u64,
) -> Result<StoredQxRuleSource, SubscriptionStoreError> {
    Err(SubscriptionStoreError::StoreUnavailable)
}

fn apply_qx_rule_sources(
    profile: &mut Profile,
    sources: &[StoredQxRuleSource],
) -> Result<(), LoadError> {
    let has_user_named_proxy = profile
        .groups
        .iter()
        .any(|group| group.name.as_str() == LEGACY_GENERATED_PROXY_GROUP_NAME);
    let legacy_proxy_target = (!has_user_named_proxy)
        .then(|| {
            profile
                .groups
                .iter()
                .find(|group| group.name.as_str() != MANIS_GLOBAL_GROUP_NAME)
                .or_else(|| profile.groups.first())
                .map(|group| PolicyRef::Group(group.name.clone()))
        })
        .flatten();
    let mut imported_rules = Vec::new();
    for source in sources {
        let target_policy =
            qx_rule_target_policy(&source.target_policy, legacy_proxy_target.as_ref());
        let parsed = QxRuleList::parse(&source.content);
        if parsed.rules.is_empty() {
            return Err(LoadError::Runtime(
                "已保存的 QX 规则源没有可导入规则".to_owned(),
            ));
        }
        let rules = parsed
            .to_profile_rules(|_source_policy| Some(target_policy.clone()))
            .map_err(|_error| LoadError::Runtime("无法映射 QX 规则策略".to_owned()))?;
        imported_rules.extend(rules);
    }
    if imported_rules.is_empty() {
        return Ok(());
    }
    let insert_at = profile
        .rules
        .iter()
        .position(|rule| matches!(rule, Rule::Match { .. }))
        .unwrap_or(profile.rules.len());
    profile.rules.splice(insert_at..insert_at, imported_rules);
    profile
        .validate()
        .map_err(|error| LoadError::Runtime(error.to_string()))
}

fn qx_rule_target_policy(
    target_policy: &Name,
    legacy_proxy_target: Option<&PolicyRef>,
) -> PolicyRef {
    match target_policy.as_str() {
        "DIRECT" => PolicyRef::Direct,
        "REJECT" => PolicyRef::Reject,
        LEGACY_GENERATED_PROXY_GROUP_NAME => legacy_proxy_target
            .cloned()
            .unwrap_or_else(|| PolicyRef::Group(target_policy.clone())),
        _ => PolicyRef::Group(target_policy.clone()),
    }
}

#[cfg(not(windows))]
pub(crate) fn save_collapsed_groups_in<'a>(
    directory: &Path,
    group_ids: impl IntoIterator<Item = &'a str>,
) -> Result<(), SubscriptionStoreError> {
    let mut ids: Vec<_> = group_ids
        .into_iter()
        .filter(|id| valid_workspace_group_id(id))
        .collect();
    ids.sort_unstable();
    ids.dedup();
    let contents = ids.join("\n");
    write_private_atomic(directory, WORKSPACE_STATE_FILE, contents.as_bytes())
        .map(|_path| ())
        .map_err(|_error| SubscriptionStoreError::StoreUnavailable)
}

#[cfg(windows)]
pub(crate) fn save_collapsed_groups_in<'a>(
    _directory: &Path,
    _group_ids: impl IntoIterator<Item = &'a str>,
) -> Result<(), SubscriptionStoreError> {
    Err(SubscriptionStoreError::StoreUnavailable)
}

#[cfg(not(windows))]
pub(crate) fn load_collapsed_groups_in(
    directory: &Path,
) -> Result<Vec<String>, SubscriptionStoreError> {
    require_clean_absolute_store(directory)?;
    let path = directory.join(WORKSPACE_STATE_FILE);
    let contents = match fs::symlink_metadata(&path) {
        Ok(_) => read_private_source_allow_empty(&path)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(_error) => return Err(SubscriptionStoreError::StoredSourceUnavailable),
    };
    contents
        .lines()
        .map(str::to_owned)
        .map(|id| {
            valid_workspace_group_id(&id)
                .then_some(id)
                .ok_or(SubscriptionStoreError::StoredSourceUnavailable)
        })
        .collect()
}

#[cfg(windows)]
pub(crate) fn load_collapsed_groups_in(
    _directory: &Path,
) -> Result<Vec<String>, SubscriptionStoreError> {
    Ok(Vec::new())
}

#[cfg(not(windows))]
pub(crate) fn save_routing_mode_in(
    directory: &Path,
    mode: RoutingMode,
) -> Result<(), SubscriptionStoreError> {
    write_private_atomic(directory, ROUTING_MODE_FILE, mode.wire_value().as_bytes())
        .map(|_path| ())
        .map_err(|_error| SubscriptionStoreError::StoreUnavailable)
}

#[cfg(windows)]
pub(crate) fn save_routing_mode_in(
    _directory: &Path,
    _mode: RoutingMode,
) -> Result<(), SubscriptionStoreError> {
    Err(SubscriptionStoreError::StoreUnavailable)
}

#[cfg(not(windows))]
pub(crate) fn load_routing_mode_in(
    directory: &Path,
) -> Result<RoutingMode, SubscriptionStoreError> {
    require_clean_absolute_store(directory)?;
    let path = directory.join(ROUTING_MODE_FILE);
    let contents = match fs::symlink_metadata(&path) {
        Ok(_) => read_private_source_allow_empty(&path)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(RoutingMode::Rule);
        }
        Err(_error) => return Err(SubscriptionStoreError::StoredSourceUnavailable),
    };
    RoutingMode::parse_wire_value(contents.trim())
        .ok_or(SubscriptionStoreError::StoredSourceUnavailable)
}

#[cfg(windows)]
pub(crate) fn load_routing_mode_in(
    _directory: &Path,
) -> Result<RoutingMode, SubscriptionStoreError> {
    Ok(RoutingMode::Rule)
}

#[cfg(not(windows))]
pub(crate) fn save_node_selection_preferences_in(
    directory: &Path,
    preferences: &NodeSelectionPreferences,
) -> Result<(), SubscriptionStoreError> {
    preferences.validate()?;
    let contents = encode_node_selection_preferences(preferences)?;
    write_private_atomic(
        directory,
        NODE_SELECTION_PREFERENCES_FILE,
        contents.as_bytes(),
    )
    .map(|_path| ())
    .map_err(|_error| SubscriptionStoreError::StoreUnavailable)
}

#[cfg(windows)]
pub(crate) fn save_node_selection_preferences_in(
    _directory: &Path,
    _preferences: &NodeSelectionPreferences,
) -> Result<(), SubscriptionStoreError> {
    Err(SubscriptionStoreError::StoreUnavailable)
}

#[cfg(not(windows))]
pub(crate) fn load_node_selection_preferences_in(
    directory: &Path,
) -> Result<NodeSelectionPreferences, SubscriptionStoreError> {
    require_clean_absolute_store(directory)?;
    let path = directory.join(NODE_SELECTION_PREFERENCES_FILE);
    let contents = match fs::symlink_metadata(&path) {
        Ok(_) => read_private_source_allow_empty_max(&path, MAX_NODE_SELECTION_FILE_BYTES)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(NodeSelectionPreferences::default());
        }
        Err(_error) => return Err(SubscriptionStoreError::StoredSourceUnavailable),
    };
    decode_node_selection_preferences(&contents)
}

#[cfg(windows)]
pub(crate) fn load_node_selection_preferences_in(
    _directory: &Path,
) -> Result<NodeSelectionPreferences, SubscriptionStoreError> {
    Ok(NodeSelectionPreferences::default())
}

fn profile_mode(mode: RoutingMode) -> ProfileMode {
    match mode {
        RoutingMode::Direct => ProfileMode::Direct,
        RoutingMode::Global => ProfileMode::Global,
        RoutingMode::Rule => ProfileMode::Rule,
    }
}

#[cfg(not(windows))]
#[allow(dead_code)]
struct DecodedSubscriptionSource {
    stored: StoredSubscription,
    url_input: String,
}

#[cfg(not(windows))]
#[allow(dead_code)]
fn read_subscription_source_by_id_in(
    directory: &Path,
    id: &str,
) -> Result<DecodedSubscriptionSource, SubscriptionStoreError> {
    require_clean_absolute_store(directory)?;
    let file_name = subscription_source_file_name(id)?;
    let contents = read_private_source_allow_empty_max(
        &directory.join(file_name),
        MAX_STORED_SUBSCRIPTION_FILE_BYTES,
    )?;
    decode_subscription_source(&contents, id)
}

#[cfg(not(windows))]
#[allow(dead_code)]
fn write_subscription_source_in(
    directory: &Path,
    id: &str,
    url_input: &str,
    refresh_interval: RemoteSourceRefreshInterval,
    last_successful_update_unix_secs: u64,
    proxy_server_nameservers: &[ProxyDnsServer],
) -> Result<StoredSubscription, SubscriptionStoreError> {
    let source = SecretUrl::parse_subscription(url_input)
        .map_err(|_error| SubscriptionStoreError::InvalidSource)?;
    let contents = encode_subscription_source(
        id,
        url_input,
        refresh_interval,
        last_successful_update_unix_secs,
        proxy_server_nameservers,
    )?;
    let file_name = subscription_source_file_name(id)?;
    write_private_atomic(directory, &file_name, contents.as_bytes())
        .map_err(|_error| SubscriptionStoreError::StoreUnavailable)?;
    Ok(StoredSubscription {
        id: id.to_owned(),
        source,
        refresh_interval,
        last_successful_update_unix_secs,
        proxy_server_nameservers: proxy_server_nameservers.to_vec(),
    })
}

#[cfg(not(windows))]
fn encode_subscription_source(
    id: &str,
    url_input: &str,
    refresh_interval: RemoteSourceRefreshInterval,
    last_successful_update_unix_secs: u64,
    proxy_server_nameservers: &[ProxyDnsServer],
) -> Result<String, SubscriptionStoreError> {
    if !valid_subscription_source_id(id) {
        return Err(SubscriptionStoreError::InvalidSource);
    }
    if url_input.len() > crate::subscription::MAX_SUBSCRIPTION_BYTES {
        return Err(SubscriptionStoreError::InvalidSource);
    }
    SecretUrl::parse_subscription(url_input)
        .map_err(|_error| SubscriptionStoreError::InvalidSource)?;
    if proxy_server_nameservers.len() > MAX_SUBSCRIPTION_PROXY_DNS_SERVERS {
        return Err(SubscriptionStoreError::InvalidSource);
    }
    let mut lines = vec![
        STORED_SUBSCRIPTION_VERSION.to_owned(),
        format!("id\t{id}"),
        format!("url\t{}", encode_hex(url_input)),
        format!("refresh\t{}", refresh_interval.key()),
        format!("last-success\t{last_successful_update_unix_secs}"),
    ];
    lines.extend(
        proxy_server_nameservers
            .iter()
            .map(|nameserver| format!("proxy-dns\t{}", encode_hex(nameserver.as_str()))),
    );
    Ok(lines.join("\n"))
}

#[cfg(not(windows))]
fn decode_subscription_source(
    contents: &str,
    expected_id: &str,
) -> Result<DecodedSubscriptionSource, SubscriptionStoreError> {
    if !valid_subscription_source_id(expected_id) {
        return Err(SubscriptionStoreError::StoredSourceUnavailable);
    }
    let version = contents.lines().next();
    let structured = version.is_some_and(|version| {
        matches!(
            version,
            STORED_SUBSCRIPTION_VERSION
                | LEGACY_MANIS_STORED_SUBSCRIPTION_VERSION
                | LEGACY_RELAY_STORED_SUBSCRIPTION_VERSION
        )
    });
    if !structured {
        if contents.is_empty() || contents.lines().count() != 1 || contents.trim() != contents {
            return Err(SubscriptionStoreError::StoredSourceUnavailable);
        }
        let source = SecretUrl::parse_subscription(contents)
            .map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)?;
        return Ok(DecodedSubscriptionSource {
            stored: StoredSubscription {
                id: expected_id.to_owned(),
                source,
                refresh_interval: RemoteSourceRefreshInterval::Manual,
                last_successful_update_unix_secs: 0,
                proxy_server_nameservers: Vec::new(),
            },
            url_input: contents.to_owned(),
        });
    }

    let mut id = None;
    let mut url = None;
    let mut refresh_interval = None;
    let mut last_successful_update_unix_secs = None;
    let mut proxy_server_nameservers = Vec::new();
    for line in contents.lines().skip(1) {
        let fields = line.split('\t').collect::<Vec<_>>();
        match fields.as_slice() {
            ["id", value] if id.is_none() => id = Some((*value).to_owned()),
            ["url", value] if url.is_none() => url = Some(decode_hex(value)?),
            ["refresh", value] if refresh_interval.is_none() => {
                refresh_interval = Some(
                    RemoteSourceRefreshInterval::parse_key(value)
                        .ok_or(SubscriptionStoreError::StoredSourceUnavailable)?,
                );
            }
            ["last-success", value] if last_successful_update_unix_secs.is_none() => {
                last_successful_update_unix_secs = Some(
                    value
                        .parse::<u64>()
                        .map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)?,
                );
            }
            ["proxy-dns", value]
                if version == Some(STORED_SUBSCRIPTION_VERSION)
                    && proxy_server_nameservers.len() < MAX_SUBSCRIPTION_PROXY_DNS_SERVERS =>
            {
                let decoded = decode_hex(value)?;
                let nameserver = ProxyDnsServer::parse_https(&decoded)
                    .map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)?;
                if !proxy_server_nameservers.contains(&nameserver) {
                    proxy_server_nameservers.push(nameserver);
                }
            }
            _ => return Err(SubscriptionStoreError::StoredSourceUnavailable),
        }
    }
    let id = id.ok_or(SubscriptionStoreError::StoredSourceUnavailable)?;
    if id != expected_id || !valid_subscription_source_id(&id) {
        return Err(SubscriptionStoreError::StoredSourceUnavailable);
    }
    let url_input = url.ok_or(SubscriptionStoreError::StoredSourceUnavailable)?;
    let source = SecretUrl::parse_subscription(&url_input)
        .map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)?;
    Ok(DecodedSubscriptionSource {
        stored: StoredSubscription {
            id,
            source,
            refresh_interval: refresh_interval.unwrap_or_default(),
            last_successful_update_unix_secs: last_successful_update_unix_secs.unwrap_or_default(),
            proxy_server_nameservers,
        },
        url_input,
    })
}

#[cfg(not(windows))]
#[allow(dead_code)]
fn subscription_source_file_name(id: &str) -> Result<String, SubscriptionStoreError> {
    if id == "subscription:legacy" {
        Ok(IMPORTED_SUBSCRIPTION_FILE.to_owned())
    } else if valid_stored_id(id, STORED_SUBSCRIPTION_PREFIX) {
        Ok(format!("{id}{STORED_SUBSCRIPTION_SUFFIX}"))
    } else {
        Err(SubscriptionStoreError::StoreUnavailable)
    }
}

#[cfg(not(windows))]
fn valid_subscription_source_id(id: &str) -> bool {
    id == "subscription:legacy" || valid_stored_id(id, STORED_SUBSCRIPTION_PREFIX)
}

pub(crate) fn new_managed_policy_id() -> String {
    next_stored_source_id(MANAGED_POLICY_PREFIX)
}

fn compile_managed_policy_groups(
    groups: &[ManagedPolicyGroup],
    stored_provider_indexes: &HashMap<&str, usize>,
    vless_nodes: &[VlessProxy],
    provider_count: usize,
) -> Result<Vec<UserPolicyGroup>, LoadError> {
    groups
        .iter()
        .map(|group| {
            let mut provider_indexes = Vec::new();
            let mut direct_proxies = Vec::new();
            let filter = match &group.matcher {
                PolicyCandidateMatcher::All => {
                    provider_indexes.extend(0..provider_count);
                    direct_proxies.extend(vless_nodes.iter().map(|proxy| proxy.name().clone()));
                    None
                }
                PolicyCandidateMatcher::NameContains(fragment) => {
                    provider_indexes.extend(0..provider_count);
                    let lowercase = fragment.to_lowercase();
                    direct_proxies.extend(
                        vless_nodes
                            .iter()
                            .filter(|proxy| {
                                proxy.name().as_str().to_lowercase().contains(&lowercase)
                            })
                            .map(|proxy| proxy.name().clone()),
                    );
                    Some(format!("(?i){}", escape_regex(fragment)))
                }
                PolicyCandidateMatcher::Explicit(members) => {
                    let mut provider_names = Vec::new();
                    for member in members {
                        if member.source_id == "saved" {
                            if let Some(proxy) = vless_nodes
                                .iter()
                                .find(|proxy| proxy.name().as_str() == member.node_name)
                            {
                                direct_proxies.push(proxy.name().clone());
                            }
                            continue;
                        }
                        let Some(stored_id) = member.source_id.strip_prefix("subscription:") else {
                            continue;
                        };
                        let Some(index) = stored_provider_indexes.get(stored_id).copied() else {
                            continue;
                        };
                        if !provider_indexes.contains(&index) {
                            provider_indexes.push(index);
                        }
                        provider_names.push(member.node_name.as_str());
                    }
                    (!provider_names.is_empty()).then(|| {
                        format!(
                            "^(?:{})$",
                            provider_names
                                .into_iter()
                                .map(escape_regex)
                                .collect::<Vec<_>>()
                                .join("|")
                        )
                    })
                }
            };
            if provider_indexes.is_empty() && direct_proxies.is_empty() {
                return Err(LoadError::Runtime(format!(
                    "策略组“{}”没有匹配到可用节点",
                    group.name
                )));
            }
            let kind = match group.strategy {
                ManagedPolicyStrategy::Manual => UserPolicyGroupKind::Select,
                ManagedPolicyStrategy::LowestLatency => UserPolicyGroupKind::UrlTest {
                    tolerance: 50,
                    interval_secs: group.test_interval_secs,
                },
            };
            Ok(UserPolicyGroup {
                name: Name::parse(&group.name)
                    .map_err(|error| LoadError::Runtime(error.to_string()))?,
                icon: None,
                kind,
                provider_indexes,
                direct_proxies,
                filter,
            })
        })
        .collect()
}

fn escape_regex(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        if matches!(
            character,
            '\\' | '.' | '+' | '*' | '?' | '(' | ')' | '|' | '[' | ']' | '{' | '}' | '^' | '$'
        ) {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    escaped
}

#[cfg(not(windows))]
pub(crate) fn save_managed_policy_in(
    directory: &Path,
    group: &ManagedPolicyGroup,
) -> Result<(), SubscriptionStoreError> {
    group
        .validate()
        .map_err(|_error| SubscriptionStoreError::InvalidSource)?;
    if !valid_managed_policy_id(&group.id) {
        return Err(SubscriptionStoreError::InvalidSource);
    }
    let contents = encode_managed_policy(group)?;
    let file_name = format!("{}{MANAGED_POLICY_SUFFIX}", group.id);
    write_private_atomic(directory, &file_name, contents.as_bytes())
        .map(|_path| ())
        .map_err(|_error| SubscriptionStoreError::StoreUnavailable)
}

#[cfg(windows)]
pub(crate) fn save_managed_policy_in(
    _directory: &Path,
    _group: &ManagedPolicyGroup,
) -> Result<(), SubscriptionStoreError> {
    Err(SubscriptionStoreError::StoreUnavailable)
}

#[cfg(not(windows))]
pub(crate) fn load_managed_policy_groups_in(
    directory: &Path,
) -> Result<Vec<ManagedPolicyGroup>, SubscriptionStoreError> {
    let mut groups = BTreeMap::new();
    let Some(entries) = private_store_entries(directory)? else {
        return Ok(Vec::new());
    };
    for path in &entries {
        let Some(file_name) = path.file_name().and_then(std::ffi::OsStr::to_str) else {
            continue;
        };
        let Some(id) = file_name.strip_suffix(MANAGED_POLICY_SUFFIX) else {
            continue;
        };
        if !valid_managed_policy_id(id) {
            continue;
        }
        let contents = read_private_source_allow_empty(path)?;
        let group = decode_managed_policy(&contents, id)?;
        groups.insert(group.id.clone(), group);
        if groups.len() > MAX_MANAGED_POLICIES {
            return Err(SubscriptionStoreError::StoredSourceUnavailable);
        }
    }
    for path in entries {
        let Some(file_name) = path.file_name().and_then(std::ffi::OsStr::to_str) else {
            continue;
        };
        let Some(id) = file_name.strip_suffix(LEGACY_MANAGED_POLICY_SUFFIX) else {
            continue;
        };
        if !valid_stored_id(id, LEGACY_MANAGED_POLICY_PREFIX) {
            continue;
        }
        if !groups.contains_key(id) {
            let contents = read_private_source_allow_empty(&path)?;
            let group = decode_managed_policy(&contents, id)?;
            save_managed_policy_in(directory, &group)?;
            groups.insert(group.id.clone(), group);
        }
        remove_private_source(&path)?;
        if groups.len() > MAX_MANAGED_POLICIES {
            return Err(SubscriptionStoreError::StoredSourceUnavailable);
        }
    }
    Ok(groups.into_values().collect())
}

#[cfg(windows)]
pub(crate) fn load_managed_policy_groups_in(
    _directory: &Path,
) -> Result<Vec<ManagedPolicyGroup>, SubscriptionStoreError> {
    Ok(Vec::new())
}

#[cfg(not(windows))]
pub(crate) fn remove_managed_policy_in(
    directory: &Path,
    id: &str,
) -> Result<(), SubscriptionStoreError> {
    require_clean_absolute_store(directory)?;
    if !valid_managed_policy_id(id) {
        return Err(SubscriptionStoreError::StoreUnavailable);
    }
    remove_private_source(&directory.join(format!("{id}{MANAGED_POLICY_SUFFIX}")))?;
    remove_private_source(&directory.join(format!("{id}{LEGACY_MANAGED_POLICY_SUFFIX}")))
}

#[cfg(windows)]
pub(crate) fn remove_managed_policy_in(
    _directory: &Path,
    _id: &str,
) -> Result<(), SubscriptionStoreError> {
    Err(SubscriptionStoreError::StoreUnavailable)
}

fn encode_managed_policy(group: &ManagedPolicyGroup) -> Result<String, SubscriptionStoreError> {
    group
        .validate()
        .map_err(|_error| SubscriptionStoreError::InvalidSource)?;
    let (matcher_key, filter, members): (&str, &str, Vec<&NodeIdentity>) = match &group.matcher {
        PolicyCandidateMatcher::All => ("all", "", Vec::new()),
        PolicyCandidateMatcher::NameContains(value) => ("name", value, Vec::new()),
        PolicyCandidateMatcher::Explicit(members) => ("explicit", "", members.iter().collect()),
    };
    let mut lines = vec![
        MANAGED_POLICY_VERSION.to_owned(),
        format!("id\t{}", group.id),
        format!("name\t{}", encode_hex(&group.name)),
        format!("icon\t{}", group.icon.key()),
        format!("strategy\t{}", group.strategy.key()),
        format!("interval\t{}", group.test_interval_secs),
        format!("matcher\t{matcher_key}"),
        format!("filter\t{}", encode_hex(filter)),
    ];
    lines.extend(members.into_iter().map(|member| {
        format!(
            "member\t{}\t{}",
            encode_hex(&member.source_id),
            encode_hex(&member.node_name)
        )
    }));
    let contents = lines.join("\n");
    if contents.len() as u64 > MAX_SUBSCRIPTION_FILE_BYTES {
        return Err(SubscriptionStoreError::InvalidSource);
    }
    Ok(contents)
}

fn decode_managed_policy(
    contents: &str,
    expected_id: &str,
) -> Result<ManagedPolicyGroup, SubscriptionStoreError> {
    let mut lines = contents.lines();
    if !lines.next().is_some_and(|version| {
        matches!(
            version,
            MANAGED_POLICY_VERSION
                | LEGACY_MANIS_MANAGED_POLICY_VERSION
                | LEGACY_RELAY_MANAGED_POLICY_VERSION
        )
    }) {
        return Err(SubscriptionStoreError::StoredSourceUnavailable);
    }
    let mut id = None;
    let mut name = None;
    let mut icon = None;
    let mut strategy = None;
    let mut interval = None;
    let mut matcher = None;
    let mut filter = None;
    let mut members = BTreeSet::new();
    for line in lines {
        let fields = line.split('\t').collect::<Vec<_>>();
        match fields.as_slice() {
            ["id", value] if id.is_none() => id = Some((*value).to_owned()),
            ["name", value] if name.is_none() => name = Some(decode_hex(value)?),
            ["icon", value] if icon.is_none() => {
                icon = Some(
                    ManagedPolicyIcon::parse_key(value)
                        .map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)?,
                );
            }
            ["strategy", value] if strategy.is_none() => {
                strategy = Some(
                    ManagedPolicyStrategy::parse_key(value)
                        .map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)?,
                );
            }
            ["interval", value] if interval.is_none() => {
                interval = Some(
                    value
                        .parse::<u32>()
                        .map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)?,
                );
            }
            ["matcher", value] if matcher.is_none() => matcher = Some((*value).to_owned()),
            ["filter", value] if filter.is_none() => filter = Some(decode_hex(value)?),
            ["member", source, node] => {
                members.insert(
                    NodeIdentity::new(&decode_hex(source)?, &decode_hex(node)?)
                        .map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)?,
                );
            }
            _ => return Err(SubscriptionStoreError::StoredSourceUnavailable),
        }
    }
    let id = id.ok_or(SubscriptionStoreError::StoredSourceUnavailable)?;
    if id != expected_id {
        return Err(SubscriptionStoreError::StoredSourceUnavailable);
    }
    let mut group = ManagedPolicyGroup::new(
        &id,
        &name.ok_or(SubscriptionStoreError::StoredSourceUnavailable)?,
    )
    .map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)?;
    group.icon = icon.ok_or(SubscriptionStoreError::StoredSourceUnavailable)?;
    group.strategy = strategy.ok_or(SubscriptionStoreError::StoredSourceUnavailable)?;
    group
        .set_test_interval_secs(interval.unwrap_or(600))
        .map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)?;
    let filter = filter.ok_or(SubscriptionStoreError::StoredSourceUnavailable)?;
    let parsed_matcher = match matcher.as_deref() {
        Some("all") if filter.is_empty() && members.is_empty() => PolicyCandidateMatcher::All,
        Some("name") if !filter.is_empty() && members.is_empty() => {
            PolicyCandidateMatcher::name_contains(&filter)
                .map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)?
        }
        Some("explicit") if filter.is_empty() => PolicyCandidateMatcher::Explicit(members),
        _ => return Err(SubscriptionStoreError::StoredSourceUnavailable),
    };
    group
        .set_matcher(parsed_matcher)
        .map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)?;
    group
        .validate()
        .map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)?;
    Ok(group)
}

fn valid_managed_policy_id(id: &str) -> bool {
    valid_stored_id(id, MANAGED_POLICY_PREFIX) || valid_stored_id(id, LEGACY_MANAGED_POLICY_PREFIX)
}

fn encode_node_selection_preferences(
    preferences: &NodeSelectionPreferences,
) -> Result<String, SubscriptionStoreError> {
    preferences.validate()?;
    let mut lines = vec![NODE_SELECTION_PREFERENCES_VERSION.to_owned()];
    if let Some(global) = preferences.global() {
        let checked = NodeIdentity::new(&global.source_id, &global.node_name)
            .map_err(|_error| SubscriptionStoreError::InvalidSource)?;
        lines.push(format!(
            "global\t{}\t{}",
            encode_hex(&checked.source_id),
            encode_hex(&checked.node_name)
        ));
    }
    lines.extend(
        preferences.iter_policy_targets().map(|(policy, target)| {
            format!("policy\t{}\t{}", encode_hex(policy), encode_hex(target))
        }),
    );
    let contents = lines.join("\n");
    if contents.len() as u64 > MAX_NODE_SELECTION_FILE_BYTES {
        return Err(SubscriptionStoreError::InvalidSource);
    }
    Ok(contents)
}

fn decode_node_selection_preferences(
    contents: &str,
) -> Result<NodeSelectionPreferences, SubscriptionStoreError> {
    let mut lines = contents.lines();
    if !storage_version_supported(
        lines.next(),
        NODE_SELECTION_PREFERENCES_VERSION,
        LEGACY_RELAY_NODE_SELECTION_PREFERENCES_VERSION,
    ) {
        return Err(SubscriptionStoreError::StoredSourceUnavailable);
    }
    let mut preferences = NodeSelectionPreferences::default();
    let mut seen_global = false;
    for line in lines {
        let fields = line.split('\t').collect::<Vec<_>>();
        match fields.as_slice() {
            ["global", source, node] if !seen_global => {
                seen_global = true;
                let source = decode_hex(source)?;
                let node = decode_hex(node)?;
                preferences.global = Some(
                    NodeIdentity::new(&source, &node)
                        .map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)?,
                );
            }
            ["policy", policy, target] => {
                let policy = decode_hex(policy)?;
                let target = decode_hex(target)?;
                validate_node_selection_policy(&policy)
                    .map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)?;
                validate_node_selection_target(&target)
                    .map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)?;
                if preferences.policy_targets.insert(policy, target).is_some()
                    || preferences.policy_targets.len() > MAX_NODE_SELECTION_POLICY_TARGETS
                {
                    return Err(SubscriptionStoreError::StoredSourceUnavailable);
                }
            }
            _ => return Err(SubscriptionStoreError::StoredSourceUnavailable),
        }
    }
    Ok(preferences)
}

fn validate_node_selection_policy(policy: &str) -> Result<(), SubscriptionStoreError> {
    Name::parse(policy)
        .map(|_name| ())
        .map_err(|_error| SubscriptionStoreError::InvalidSource)
}

fn validate_node_selection_target(target: &str) -> Result<(), SubscriptionStoreError> {
    if valid_node_selection_target(target) {
        Ok(())
    } else {
        Err(SubscriptionStoreError::InvalidSource)
    }
}

fn valid_node_selection_target(target: &str) -> bool {
    !target.is_empty()
        && target.len() <= 512
        && target.trim() == target
        && !target.chars().any(char::is_control)
}

#[allow(dead_code)]
struct DecodedQxRuleSource {
    stored: StoredQxRuleSource,
    url_input: String,
}

#[cfg(not(windows))]
#[allow(dead_code)]
fn read_qx_rule_source_by_id_in(
    directory: &Path,
    id: &str,
) -> Result<DecodedQxRuleSource, SubscriptionStoreError> {
    require_clean_absolute_store(directory)?;
    let file_name = qx_rule_source_file_name(id)?;
    let contents = read_private_source_allow_empty_max(
        &directory.join(file_name),
        MAX_QX_RULE_SOURCE_FILE_BYTES,
    )?;
    decode_qx_rule_source_with_url(&contents, id)
}

#[cfg(not(windows))]
#[allow(dead_code)]
fn write_qx_rule_source_in(
    directory: &Path,
    id: &str,
    url_input: &str,
    target_policy: &str,
    content: &str,
    refresh_interval: RemoteSourceRefreshInterval,
    last_successful_update_unix_secs: u64,
) -> Result<StoredQxRuleSource, SubscriptionStoreError> {
    let source = SecretUrl::parse_https(url_input)
        .map_err(|_error| SubscriptionStoreError::InvalidSource)?;
    let target_policy =
        Name::parse(target_policy).map_err(|_error| SubscriptionStoreError::InvalidSource)?;
    let (rule_count, diagnostic_count) = validate_qx_rule_source_content(content)?;
    let contents = encode_qx_rule_source(
        id,
        url_input,
        &target_policy,
        content,
        refresh_interval,
        last_successful_update_unix_secs,
    )?;
    let file_name = qx_rule_source_file_name(id)?;
    write_private_atomic(directory, &file_name, contents.as_bytes())
        .map_err(|_error| SubscriptionStoreError::StoreUnavailable)?;
    Ok(StoredQxRuleSource {
        id: id.to_owned(),
        source,
        target_policy,
        content: content.to_owned(),
        rule_count,
        diagnostic_count,
        refresh_interval,
        last_successful_update_unix_secs,
    })
}

#[cfg(not(windows))]
#[allow(dead_code)]
fn qx_rule_source_file_name(id: &str) -> Result<String, SubscriptionStoreError> {
    if valid_stored_id(id, QX_RULE_SOURCE_PREFIX) {
        Ok(format!("{id}{QX_RULE_SOURCE_SUFFIX}"))
    } else {
        Err(SubscriptionStoreError::StoreUnavailable)
    }
}

fn encode_qx_rule_source(
    id: &str,
    url_input: &str,
    target_policy: &Name,
    content: &str,
    refresh_interval: RemoteSourceRefreshInterval,
    last_successful_update_unix_secs: u64,
) -> Result<String, SubscriptionStoreError> {
    if !valid_stored_id(id, QX_RULE_SOURCE_PREFIX) {
        return Err(SubscriptionStoreError::InvalidSource);
    }
    SecretUrl::parse_https(url_input).map_err(|_error| SubscriptionStoreError::InvalidSource)?;
    validate_qx_rule_source_content(content)?;
    Ok([
        QX_RULE_SOURCE_VERSION.to_owned(),
        format!("id\t{id}"),
        format!("url\t{}", encode_hex(url_input)),
        format!("target\t{}", encode_hex(target_policy.as_str())),
        format!("content\t{}", encode_hex(content)),
        format!("refresh\t{}", refresh_interval.key()),
        format!("last-success\t{last_successful_update_unix_secs}"),
    ]
    .join("\n"))
}

fn decode_qx_rule_source(
    contents: &str,
    expected_id: &str,
) -> Result<StoredQxRuleSource, SubscriptionStoreError> {
    decode_qx_rule_source_with_url(contents, expected_id).map(|decoded| decoded.stored)
}

fn decode_qx_rule_source_with_url(
    contents: &str,
    expected_id: &str,
) -> Result<DecodedQxRuleSource, SubscriptionStoreError> {
    let mut lines = contents.lines();
    if !storage_version_supported(
        lines.next(),
        QX_RULE_SOURCE_VERSION,
        LEGACY_RELAY_QX_RULE_SOURCE_VERSION,
    ) {
        return Err(SubscriptionStoreError::StoredSourceUnavailable);
    }
    let mut id = None;
    let mut url = None;
    let mut target = None;
    let mut content = None;
    let mut refresh_interval = None;
    let mut last_successful_update_unix_secs = None;
    for line in lines {
        let fields = line.split('\t').collect::<Vec<_>>();
        match fields.as_slice() {
            ["id", value] if id.is_none() => id = Some((*value).to_owned()),
            ["url", value] if url.is_none() => url = Some(decode_hex(value)?),
            ["target", value] if target.is_none() => target = Some(decode_hex(value)?),
            ["content", value] if content.is_none() => content = Some(decode_hex(value)?),
            ["refresh", value] if refresh_interval.is_none() => {
                refresh_interval = Some(
                    RemoteSourceRefreshInterval::parse_key(value)
                        .ok_or(SubscriptionStoreError::StoredSourceUnavailable)?,
                );
            }
            ["last-success", value] if last_successful_update_unix_secs.is_none() => {
                last_successful_update_unix_secs = Some(
                    value
                        .parse::<u64>()
                        .map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)?,
                );
            }
            _ => return Err(SubscriptionStoreError::StoredSourceUnavailable),
        }
    }
    let id = id.ok_or(SubscriptionStoreError::StoredSourceUnavailable)?;
    if id != expected_id || !valid_stored_id(&id, QX_RULE_SOURCE_PREFIX) {
        return Err(SubscriptionStoreError::StoredSourceUnavailable);
    }
    let url_input = url.ok_or(SubscriptionStoreError::StoredSourceUnavailable)?;
    let source = SecretUrl::parse_https(&url_input)
        .map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)?;
    let target_policy =
        Name::parse(&target.ok_or(SubscriptionStoreError::StoredSourceUnavailable)?)
            .map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)?;
    let content = content.ok_or(SubscriptionStoreError::StoredSourceUnavailable)?;
    let (rule_count, diagnostic_count) = validate_qx_rule_source_content(&content)?;
    Ok(DecodedQxRuleSource {
        stored: StoredQxRuleSource {
            id,
            source,
            target_policy,
            content,
            rule_count,
            diagnostic_count,
            refresh_interval: refresh_interval.unwrap_or_default(),
            last_successful_update_unix_secs: last_successful_update_unix_secs.unwrap_or_default(),
        },
        url_input,
    })
}

fn validate_qx_rule_source_content(
    content: &str,
) -> Result<(usize, usize), SubscriptionStoreError> {
    if content.is_empty() || content.len() > MAX_QX_RULE_SOURCE_CONTENT_BYTES {
        return Err(SubscriptionStoreError::InvalidSource);
    }
    let parsed = QxRuleList::parse(content);
    if parsed.rules.is_empty() {
        return Err(SubscriptionStoreError::InvalidSource);
    }
    Ok((parsed.rules.len(), parsed.diagnostics.len()))
}

fn storage_version_supported(actual: Option<&str>, current: &str, legacy_relay: &str) -> bool {
    actual.is_some_and(|actual| actual == current || actual == legacy_relay)
}

fn encode_hex(value: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(value.len() * 2);
    for byte in value.bytes() {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

fn decode_hex(value: &str) -> Result<String, SubscriptionStoreError> {
    if !value.len().is_multiple_of(2) {
        return Err(SubscriptionStoreError::StoredSourceUnavailable);
    }
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len() / 2);
    for index in (0..bytes.len()).step_by(2) {
        let high = decode_hex_digit(bytes[index])?;
        let low = decode_hex_digit(bytes[index + 1])?;
        decoded.push((high << 4) | low);
    }
    String::from_utf8(decoded).map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)
}

fn decode_hex_digit(value: u8) -> Result<u8, SubscriptionStoreError> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        _ => Err(SubscriptionStoreError::StoredSourceUnavailable),
    }
}

fn next_stored_source_id(prefix: &str) -> String {
    let timestamp = current_unix_nanos();
    let sequence = NEXT_STORED_SOURCE.fetch_add(1, Ordering::Relaxed);
    format!("{prefix}{timestamp:x}-{sequence:x}")
}

pub(crate) fn current_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn current_unix_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

fn valid_stored_id(id: &str, prefix: &str) -> bool {
    id.strip_prefix(prefix).is_some_and(|suffix| {
        !suffix.is_empty()
            && suffix
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() || byte == b'-')
    })
}

fn valid_workspace_group_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 160
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b':' | b'-' | b'_'))
}

#[cfg(not(windows))]
fn private_store_entries(directory: &Path) -> Result<Option<Vec<PathBuf>>, SubscriptionStoreError> {
    require_clean_absolute_store(directory)?;
    let iterator = match fs::read_dir(directory) {
        Ok(iterator) => iterator,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_error) => return Err(SubscriptionStoreError::StoredSourceUnavailable),
    };
    let mut paths = Vec::new();
    for entry in iterator {
        paths.push(
            entry
                .map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)?
                .path(),
        );
    }
    Ok(Some(paths))
}

#[cfg(not(windows))]
fn read_private_source(path: &Path) -> Result<String, SubscriptionStoreError> {
    let contents = read_private_source_allow_empty(path)?;
    if contents.is_empty() || contents.lines().count() != 1 || contents.trim() != contents {
        return Err(SubscriptionStoreError::StoredSourceUnavailable);
    }
    Ok(contents)
}

#[cfg(not(windows))]
fn read_private_source_allow_empty(path: &Path) -> Result<String, SubscriptionStoreError> {
    read_private_source_allow_empty_max(path, MAX_SUBSCRIPTION_FILE_BYTES)
}

#[cfg(not(windows))]
fn read_private_source_allow_empty_max(
    path: &Path,
    max_bytes: u64,
) -> Result<String, SubscriptionStoreError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(SubscriptionStoreError::StoredSourceUnavailable);
    }
    let file =
        fs::File::open(path).map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)?;
    let opened_metadata = file
        .metadata()
        .map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)?;
    if opened_metadata.len() > max_bytes {
        return Err(SubscriptionStoreError::StoredSourceUnavailable);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};

        if metadata.dev() != opened_metadata.dev()
            || metadata.ino() != opened_metadata.ino()
            || opened_metadata.permissions().mode() & 0o077 != 0
        {
            return Err(SubscriptionStoreError::StoredSourceUnavailable);
        }
    }
    let mut contents = String::new();
    file.take(max_bytes + 1)
        .read_to_string(&mut contents)
        .map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)?;
    if contents.len() as u64 > max_bytes {
        return Err(SubscriptionStoreError::StoredSourceUnavailable);
    }
    Ok(contents)
}

#[cfg(not(windows))]
fn remove_private_source(path: &Path) -> Result<(), SubscriptionStoreError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            Err(SubscriptionStoreError::StoredSourceUnavailable)
        }
        Ok(_) => fs::remove_file(path).map_err(|_error| SubscriptionStoreError::StoreUnavailable),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_error) => Err(SubscriptionStoreError::StoreUnavailable),
    }
}

#[cfg(not(windows))]
#[allow(dead_code)]
pub(crate) fn save_imported_subscription_in(
    directory: &Path,
    input: &str,
) -> Result<SecretUrl, SubscriptionStoreError> {
    let subscription = SecretUrl::parse_subscription(input)
        .map_err(|_error| SubscriptionStoreError::InvalidSource)?;
    write_private_atomic(directory, IMPORTED_SUBSCRIPTION_FILE, input.as_bytes())
        .map_err(|_error| SubscriptionStoreError::StoreUnavailable)?;
    Ok(subscription)
}

#[cfg(windows)]
pub(crate) fn save_imported_subscription_in(
    _directory: &Path,
    _input: &str,
) -> Result<SecretUrl, SubscriptionStoreError> {
    Err(SubscriptionStoreError::StoreUnavailable)
}

#[cfg(not(windows))]
#[allow(dead_code)]
pub(crate) fn load_imported_subscription_in(
    directory: &Path,
) -> Result<Option<SecretUrl>, SubscriptionStoreError> {
    require_clean_absolute_store(directory)?;
    let path = directory.join(IMPORTED_SUBSCRIPTION_FILE);
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_error) => return Err(SubscriptionStoreError::StoredSourceUnavailable),
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(SubscriptionStoreError::StoredSourceUnavailable);
    }
    let file =
        fs::File::open(&path).map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)?;
    let opened_metadata = file
        .metadata()
        .map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)?;
    if opened_metadata.len() > MAX_SUBSCRIPTION_FILE_BYTES {
        return Err(SubscriptionStoreError::StoredSourceUnavailable);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};

        if metadata.dev() != opened_metadata.dev()
            || metadata.ino() != opened_metadata.ino()
            || opened_metadata.permissions().mode() & 0o077 != 0
        {
            return Err(SubscriptionStoreError::StoredSourceUnavailable);
        }
    }
    let mut contents = String::new();
    file.take(MAX_SUBSCRIPTION_FILE_BYTES + 1)
        .read_to_string(&mut contents)
        .map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)?;
    if contents.len() as u64 > MAX_SUBSCRIPTION_FILE_BYTES {
        return Err(SubscriptionStoreError::StoredSourceUnavailable);
    }
    decode_subscription_source(&contents, "subscription:legacy")
        .map(|decoded| Some(decoded.stored.source))
}

#[cfg(windows)]
pub(crate) fn load_imported_subscription_in(
    directory: &Path,
) -> Result<Option<SecretUrl>, SubscriptionStoreError> {
    if directory.join(IMPORTED_SUBSCRIPTION_FILE).exists() {
        Err(SubscriptionStoreError::StoredSourceUnavailable)
    } else {
        Ok(None)
    }
}

#[cfg(not(windows))]
#[allow(dead_code)]
pub(crate) fn remove_imported_subscription_in(
    directory: &Path,
) -> Result<(), SubscriptionStoreError> {
    require_clean_absolute_store(directory)?;
    let path = directory.join(IMPORTED_SUBSCRIPTION_FILE);
    match fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            Err(SubscriptionStoreError::StoredSourceUnavailable)
        }
        Ok(_) => fs::remove_file(path).map_err(|_error| SubscriptionStoreError::StoreUnavailable),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_error) => Err(SubscriptionStoreError::StoreUnavailable),
    }
}

#[cfg(windows)]
pub(crate) fn remove_imported_subscription_in(
    _directory: &Path,
) -> Result<(), SubscriptionStoreError> {
    Err(SubscriptionStoreError::StoreUnavailable)
}

#[cfg(not(windows))]
fn require_clean_absolute_store(directory: &Path) -> Result<(), SubscriptionStoreError> {
    if !directory.is_absolute() || !has_only_clean_components(directory) {
        return Err(SubscriptionStoreError::StoreUnavailable);
    }
    match fs::symlink_metadata(directory) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            Err(SubscriptionStoreError::StoreUnavailable)
        }
        Ok(metadata) => {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if metadata.permissions().mode() & 0o077 != 0 {
                    return Err(SubscriptionStoreError::StoredSourceUnavailable);
                }
            }
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_error) => Err(SubscriptionStoreError::StoreUnavailable),
    }
}

fn discover_preview_binary() -> Result<PathBuf, SubscriptionPreviewError> {
    if let Some(explicit) = brand::env_var_os(BINARY_ENV, LEGACY_RELAY_BINARY_ENV) {
        return canonical_binary(Path::new(&explicit));
    }

    let executable_name = if cfg!(windows) {
        "mihomo.exe"
    } else {
        "mihomo"
    };
    first_existing_binary(mihomo_binary_candidates(executable_name))
        .ok_or(SubscriptionPreviewError::BinaryUnavailable)
}

fn canonical_binary(path: &Path) -> Result<PathBuf, SubscriptionPreviewError> {
    let canonical = path
        .canonicalize()
        .map_err(|_error| SubscriptionPreviewError::BinaryUnavailable)?;
    canonical
        .is_file()
        .then_some(canonical)
        .ok_or(SubscriptionPreviewError::BinaryUnavailable)
}

fn reserve_preview_port() -> Result<u16, SubscriptionPreviewError> {
    TcpListener::bind("127.0.0.1:0")
        .and_then(|listener| listener.local_addr())
        .map(|address| address.port())
        .map_err(|_error| SubscriptionPreviewError::WorkspaceUnavailable)
}

#[cfg(unix)]
fn wait_for_preview_providers(
    endpoint: &ControllerEndpoint,
) -> Result<Vec<LoadedProvider>, SubscriptionPreviewError> {
    let ControllerEndpoint::UnixSocket(socket_path) = endpoint else {
        return Err(SubscriptionPreviewError::UnsupportedPlatform);
    };
    let client = MihomoClient::new(
        ControllerConfig::default(),
        UnixSocketTransport::new(socket_path),
    );
    for attempt in 0..PREVIEW_PROVIDER_ATTEMPTS {
        if let Ok(providers) = client.fetch_proxy_providers() {
            let providers = load_subscription_provider(&providers);
            if providers.iter().any(|provider| !provider.nodes.is_empty()) {
                return Ok(providers);
            }
        }
        if attempt + 1 < PREVIEW_PROVIDER_ATTEMPTS {
            thread::sleep(PREVIEW_PROVIDER_DELAY);
        }
    }
    match client.fetch_proxy_providers() {
        Ok(providers)
            if providers.iter().any(|provider| {
                provider.name == "subscription" && !provider.proxies.is_empty()
            }) =>
        {
            Ok(load_subscription_provider(&providers))
        }
        Ok(_) => Err(SubscriptionPreviewError::EmptyProvider),
        Err(_) => Err(SubscriptionPreviewError::ProviderUnavailable),
    }
}

#[cfg(unix)]
fn load_subscription_provider(providers: &[manis_mihomo::ProxyProvider]) -> Vec<LoadedProvider> {
    providers
        .iter()
        .filter(|provider| provider.name == "subscription")
        .map(|provider| {
            let mut loaded = load_provider(provider);
            "订阅预览".clone_into(&mut loaded.name);
            loaded
        })
        .collect()
}

#[derive(Debug)]
pub(crate) enum LoadError {
    Mihomo(MihomoError),
    Engine(EngineError),
    Runtime(String),
    ProxyModeLost(String),
}

impl fmt::Display for LoadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Mihomo(error) => error.fmt(formatter),
            Self::Engine(error) => error.fmt(formatter),
            Self::Runtime(message) | Self::ProxyModeLost(message) => formatter.write_str(message),
        }
    }
}

impl Error for LoadError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Mihomo(error) => Some(error),
            Self::Engine(error) => Some(error),
            Self::Runtime(_) | Self::ProxyModeLost(_) => None,
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
            message: format!("{variable} 已不再支持；Mihomo 配置和 controller 只能由 Manis 管理"),
        };
    }
    let Some(store_dir) = store_dir else {
        return ControllerRuntime::Invalid {
            message: "无法确定 Manis 来源目录".to_owned(),
        };
    };
    let binary = match brand::env_var_os(BINARY_ENV, LEGACY_RELAY_BINARY_ENV) {
        Some(binary) => canonical_binary(Path::new(&binary))
            .map_err(|_error| format!("{BINARY_ENV} 不是可执行文件")),
        None => discover_mihomo_binary(),
    };
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
    let controller = configured_managed_controller(&data_dir)?;
    build_saved_sources_mihomo_runtime_in(store_dir, binary, &data_dir, &controller)
}

fn build_saved_sources_mihomo_runtime_in(
    store_dir: &Path,
    binary: &Path,
    data_dir: &Path,
    controller: &ControllerEndpoint,
) -> Result<ControllerRuntime, String> {
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
    let config_file = write_private_atomic(data_dir, GENERATED_PROFILE_FILE, rendered.as_bytes())
        .map_err(|_error| "无法写入 Mihomo 私有配置".to_owned())?;
    let config = managed_engine_config(&spec, config_file);
    validate_managed_config(&config).map_err(|error| error.to_string())?;
    let endpoint = controller.uri();
    let manager = EngineManager::new(config, ReadinessPolicy::default(), readiness_probe(&spec));
    Ok(ControllerRuntime::Managed {
        endpoint,
        manager: Arc::new(Mutex::new(manager)),
        profile_source: RuntimeProfileSource::SavedSources,
        generated_profile: Some(spec),
        privileged: Arc::new(AtomicBool::new(false)),
    })
}

fn build_sing_box_runtime(store_dir: &Path) -> Result<ControllerRuntime, String> {
    let binary = discover_sing_box_binary()?;
    let data_dir = brand::data_dir()
        .map(|directory| directory.join("sing-box"))
        .ok_or_else(|| "无法确定 sing-box 数据目录".to_owned())?;
    let port = TcpListener::bind("127.0.0.1:0")
        .and_then(|listener| listener.local_addr())
        .map(|address| address.port())
        .map_err(|_error| "无法为 sing-box 分配本机 controller 端口".to_owned())?;
    let controller = ControllerEndpoint::Tcp(
        format!("127.0.0.1:{port}")
            .parse()
            .map_err(|_error| "无法生成 sing-box controller 地址".to_owned())?,
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
    let config_file = write_private_atomic(&data_dir, SING_BOX_PROFILE_FILE, rendered.as_bytes())
        .map_err(|_error| "无法写入 sing-box 私有配置".to_owned())?;
    let config = managed_engine_config(&spec, config_file);
    validate_managed_config(&config).map_err(|error| error.to_string())?;
    let endpoint = controller.uri();
    let manager = EngineManager::new(config, ReadinessPolicy::default(), readiness_probe(&spec));
    Ok(ControllerRuntime::Managed {
        endpoint,
        manager: Arc::new(Mutex::new(manager)),
        profile_source: RuntimeProfileSource::SavedSources,
        generated_profile: Some(spec),
        privileged: Arc::new(AtomicBool::new(false)),
    })
}

fn discover_sing_box_binary() -> Result<PathBuf, String> {
    if let Some(explicit) = brand::env_var_os(SING_BOX_BINARY_ENV, LEGACY_RELAY_SING_BOX_BINARY_ENV)
    {
        return canonical_binary(Path::new(&explicit))
            .map_err(|_error| format!("{SING_BOX_BINARY_ENV} 不是可执行文件"));
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
        .ok_or_else(|| format!("未找到 sing-box；请先安装内核或设置 {SING_BOX_BINARY_ENV}"))
}

pub(crate) fn sing_box_binary_available() -> bool {
    discover_sing_box_binary().is_ok()
}

fn discover_mihomo_binary() -> Result<PathBuf, String> {
    let executable_name = if cfg!(windows) {
        "mihomo.exe"
    } else {
        "mihomo"
    };
    first_existing_binary(mihomo_binary_candidates(executable_name))
        .ok_or_else(|| format!("未找到 Mihomo；请安装官方内核或设置 {BINARY_ENV}"))
}

fn mihomo_binary_candidates(executable_name: &str) -> Vec<PathBuf> {
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
        PathBuf::from("/opt/homebrew/bin/mihomo"),
        PathBuf::from("/usr/local/bin/mihomo"),
    ]);
    candidates
}

fn first_existing_binary(candidates: Vec<PathBuf>) -> Option<PathBuf> {
    candidates
        .into_iter()
        .find_map(|candidate| canonical_binary(&candidate).ok())
}

#[cfg(unix)]
fn generate_controller_secret() -> Result<String, String> {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut random = [0_u8; 32];
    fs::File::open("/dev/urandom")
        .and_then(|mut file| file.read_exact(&mut random))
        .map_err(|_error| "无法生成 sing-box controller 密钥".to_owned())?;
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
        .map_err(|_error| "无法生成 sing-box controller 密钥".to_owned())?;
    let secret = String::from_utf8(output.stdout)
        .map_err(|_error| "无法生成 sing-box controller 密钥".to_owned())?;
    let secret = secret.trim();
    if !output.status.success()
        || secret.len() != 64
        || !secret.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err("无法生成 sing-box controller 密钥".to_owned());
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
            .ok_or_else(|| format!("{MIXED_PORT_ENV} 必须是有效 Unicode"))?
            .parse::<u16>()
            .ok()
            .filter(|port| *port != 0)
            .ok_or_else(|| format!("{MIXED_PORT_ENV} 必须是 1 到 65535 的端口")),
        None => Ok(DEFAULT_MANAGED_MIXED_PORT),
    }
}

fn configured_data_dir() -> Result<PathBuf, String> {
    brand::env_var_os(DATA_DIR_ENV, LEGACY_RELAY_DATA_DIR_ENV)
        .map(PathBuf::from)
        .or_else(default_data_dir)
        .ok_or_else(|| format!("无法确定数据目录，请设置 {DATA_DIR_ENV}"))
}

fn configured_managed_controller(data_dir: &Path) -> Result<ControllerEndpoint, String> {
    default_managed_endpoint(data_dir)
}

#[cfg(unix)]
#[allow(clippy::unnecessary_wraps)]
fn default_managed_endpoint(data_dir: &Path) -> Result<ControllerEndpoint, String> {
    Ok(ControllerEndpoint::UnixSocket(
        data_dir.join("controller.sock"),
    ))
}

#[cfg(windows)]
fn default_managed_endpoint(_data_dir: &Path) -> Result<ControllerEndpoint, String> {
    Err("Windows 托管 Mihomo controller transport 尚未完成".to_owned())
}

#[cfg(not(any(unix, windows)))]
fn default_managed_endpoint(_data_dir: &Path) -> Result<ControllerEndpoint, String> {
    Err("当前平台没有默认的 Mihomo controller transport".to_owned())
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
        .unwrap_or_else(|| "版本未知".to_owned());
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
        "Mihomo 未能监听 Manis 代理端口 {expected}；可能存在上次异常退出后残留的内核进程"
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
        reconnect_live_stream(&cancelled, |attempt| {
            set_live_status(&mailbox, true, stream_phase(attempt));
            let result = controller.stream_connections(
                LIVE_CONNECTION_INTERVAL,
                &cancelled,
                |connections| {
                    if let Ok(mut mailbox) = mailbox.lock() {
                        mailbox.latest_connections = Some(connections);
                        "实时".clone_into(&mut mailbox.status.activity);
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
        reconnect_live_stream(&cancelled, |attempt| {
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
        let delay =
            Duration::from_millis(250_u64.saturating_mul(1_u64 << shift)).min(LIVE_RETRY_MAX);
        let started = std::time::Instant::now();
        while started.elapsed() < delay && !cancelled.load(Ordering::Relaxed) {
            thread::sleep(Duration::from_millis(50));
        }
    }
}

fn stream_phase(attempt: usize) -> String {
    if attempt == 0 {
        "正在建立实时流".to_owned()
    } else {
        format!("正在重连 · 第 {attempt} 次")
    }
}

fn safe_stream_error(error: &MihomoError) -> String {
    match error {
        MihomoError::HttpStatus { status_code, .. } => {
            format!("流中断 · HTTP {status_code}")
        }
        MihomoError::Json { .. } => "流数据无法解析 · 正在重试".to_owned(),
        MihomoError::Io(_) => "控制器暂时不可达 · 正在重试".to_owned(),
        _ => "实时流不可用 · 正在重试".to_owned(),
    }
}

fn set_live_status(mailbox: &Mutex<LiveMailbox>, activity: bool, status: String) {
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
    "实时".clone_into(&mut mailbox.status.logs);
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
            "所选节点不在当前 Mihomo 全局出口链路中".to_owned(),
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
            "只有手动选择策略组可以切换节点".to_owned(),
        ));
    }
    if !group.all.iter().any(|candidate| candidate == selected_name) {
        return Err(LoadError::Runtime(
            "所选节点不在当前 Mihomo 策略组中".to_owned(),
        ));
    }
    put_policy_group_selection(endpoint, group_name, selected_name, controller_secret)?;
    let selected = fetch_policy_group(endpoint, group_name, controller_secret)?;
    if selected.current.as_deref() != Some(selected_name) {
        return Err(LoadError::Runtime(format!(
            "Mihomo 未确认策略组“{group_name}”的节点切换"
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

fn fetch_proxy_delays_bounded(
    endpoint: &str,
    candidate_names: &[String],
    controller_secret: Option<&str>,
) -> Result<BTreeMap<String, u16>, LoadError> {
    fetch_proxy_delays_bounded_with_progress(
        endpoint,
        candidate_names,
        controller_secret,
        |_name, _delay| {},
    )
}

fn fetch_proxy_delays_bounded_with_progress(
    endpoint: &str,
    candidate_names: &[String],
    controller_secret: Option<&str>,
    on_result: impl FnMut(&str, Option<u16>),
) -> Result<BTreeMap<String, u16>, LoadError> {
    let targets = candidate_names
        .iter()
        .cloned()
        .map(ProxyDelayTarget::direct)
        .collect::<Vec<_>>();
    fetch_proxy_delay_targets_bounded_with_progress(
        endpoint,
        &targets,
        controller_secret,
        on_result,
    )
}

fn fetch_proxy_delay_targets_bounded_with_progress(
    endpoint: &str,
    targets: &[ProxyDelayTarget],
    controller_secret: Option<&str>,
    mut on_result: impl FnMut(&str, Option<u16>),
) -> Result<BTreeMap<String, u16>, LoadError> {
    if targets.is_empty() {
        return Err(LoadError::Runtime("当前分组没有可测速节点".to_owned()));
    }
    let worker_count = targets.len().min(GROUP_DELAY_WORKERS);
    let chunk_size = targets.len().div_ceil(worker_count);
    let delays = thread::scope(|scope| {
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
        for (target, result) in receiver {
            match result {
                Ok(delay) => {
                    on_result(target.name(), Some(delay));
                    delays.insert(target.name, delay);
                }
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
                }
            }
        }
        for handle in handles {
            drop(handle.join());
        }
        delays
    });
    if delays.is_empty() {
        return Err(LoadError::Runtime(
            "所有节点测速均失败，请检查 Mihomo 连接与网络后重试".to_owned(),
        ));
    }
    Ok(delays)
}

fn running_managed_endpoint(manager: &Arc<Mutex<EngineManager>>) -> Result<String, LoadError> {
    let mut manager = manager
        .lock()
        .map_err(|_poisoned| LoadError::Runtime("托管内核状态锁已损坏".to_owned()))?;
    manager
        .running_endpoint()?
        .ok_or_else(|| LoadError::Runtime("Mihomo 尚未运行，请先连接内核".to_owned()))
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

        assert_eq!(Interval::Manual.next(), Interval::Hourly);
        assert_eq!(Interval::Hourly.next(), Interval::SixHours);
        assert_eq!(Interval::SixHours.next(), Interval::TwelveHours);
        assert_eq!(Interval::TwelveHours.next(), Interval::Daily);
        assert_eq!(Interval::Daily.next(), Interval::Manual);

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
        preferences.set_policy_target("Proxy", "Hong Kong Edge")?;
        preferences.set_policy_target("视频服务", "Tokyo Manual")?;
        preferences.clear_policy_target("Proxy")?;

        super::save_node_selection_preferences_in(&store, &preferences)?;
        let loaded = super::load_node_selection_preferences_in(&store)?;

        assert_eq!(loaded.global(), Some(&global));
        assert_eq!(loaded.policy_target("Proxy"), None);
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
        let saved = super::save_vless_source_in(
            &store,
            "vless://00000000-0000-4000-8000-000000000000@edge.example.invalid:443?security=tls#Saved",
        )?;
        super::save_collapsed_groups_in(&store, [first.id.as_str(), "saved", "../../unsafe"])?;

        let subscriptions = super::load_subscription_sources_in(&store)?;
        let nodes = super::load_vless_sources_in(&store)?;
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
        super::remove_vless_source_in(&store, &saved.id)?;
        assert_eq!(super::load_subscription_sources_in(&store)?.len(), 1);
        assert!(super::load_vless_sources_in(&store)?.is_empty());

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
# airports.list excerpt
DOMAIN-KEYWORD,google,PROXY
DOMAIN-SUFFIX,githubusercontent.com,PROXY
IP-CIDR,192.0.2.0/24,DIRECT
";

        let stored = super::save_qx_rule_source_in(&store, url, "Proxy", content)?.into_source();
        let loaded = super::load_qx_rule_sources_in(&store)?;

        assert_eq!(loaded, vec![stored.clone()]);
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
        crate::manual_rule::save_manual_rules_in(&store, &[manual])?;
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
        assert!(second_index < manual_index && manual_index < first_index);
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
            super::QX_RULE_SOURCE_VERSION.to_owned(),
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
        assert_eq!(loaded.content, content);
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
            source: manis_profile::SecretUrl::parse_https(
                "https://rules.example.invalid/airports.list?token=fixture-secret",
            )?,
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

        super::apply_qx_rule_sources(&mut profile, &[source])?;
        let yaml = manis_profile::render_mihomo_yaml(&profile)?;

        assert!(
            yaml.find("- \"DOMAIN-KEYWORD,google,__MANIS_GLOBAL__\"")
                < yaml.find("- \"DOMAIN-SUFFIX,githubusercontent.com,__MANIS_GLOBAL__\"")
        );
        assert!(!yaml.contains("GEOIP,CN,DIRECT"));
        assert!(!yaml.contains("MATCH,__MANIS_GLOBAL__"));
        Ok(())
    }

    #[test]
    fn legacy_proxy_rule_targets_resolve_to_the_first_user_policy_group()
    -> Result<(), Box<dyn std::error::Error>> {
        let source = super::StoredQxRuleSource {
            id: "qx-rule-fixture-legacy-target".to_owned(),
            source: manis_profile::SecretUrl::parse_https(
                "https://rules.example.invalid/legacy.list",
            )?,
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
        group.set_matcher(PolicyCandidateMatcher::name_contains("Hong Kong")?)?;
        super::save_managed_policy_in(&store, &group)?;

        let mut explicit = ManagedPolicyGroup::new("policy-b-2", "手动出口")?;
        explicit.icon = ManagedPolicyIcon::Shield;
        explicit.set_matcher(PolicyCandidateMatcher::Explicit(BTreeSet::default()))?;
        explicit.toggle_member(NodeIdentity::new("subscription:source-1", "Tokyo Edge")?);
        explicit.toggle_member(NodeIdentity::new("saved", "Private Edge")?);
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
        use manis_profile::{UserPolicyGroupKind, VlessProxy};

        let saved = VlessProxy::parse_share_link(
            "vless://00000000-0000-4000-8000-000000000000@edge.example.invalid:443?security=tls#Private%20Edge",
        )?;
        let indexes = HashMap::from([("source-a", 1_usize)]);

        let mut latency = ManagedPolicyGroup::new("group-a-1", "香港优选")?;
        latency.strategy = ManagedPolicyStrategy::LowestLatency;
        latency.set_test_interval_secs(300)?;
        latency.set_matcher(PolicyCandidateMatcher::name_contains("Hong Kong")?)?;

        let mut explicit = ManagedPolicyGroup::new("group-b-2", "手动出口")?;
        explicit.set_matcher(PolicyCandidateMatcher::Explicit(BTreeSet::default()))?;
        explicit.toggle_member(NodeIdentity::new("subscription:source-a", "Tokyo (Fast)")?);
        explicit.toggle_member(NodeIdentity::new("saved", "Private Edge")?);

        let compiled =
            super::compile_managed_policy_groups(&[latency, explicit], &indexes, &[saved], 2)?;

        assert_eq!(
            compiled[0].kind,
            UserPolicyGroupKind::UrlTest {
                tolerance: 50,
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
    fn first_existing_binary_prefers_canonical_candidate() -> Result<(), Box<dyn std::error::Error>>
    {
        let root = test_temp_dir("manis-mihomo-discovery");
        let missing = root.join("missing-mihomo");
        let binary = root.join("mihomo");
        fs::write(&binary, "#!/bin/sh\nexit 0\n")?;
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o700))?;

        assert_eq!(
            super::first_existing_binary(vec![missing, binary.clone()]),
            Some(binary.canonicalize()?)
        );

        fs::remove_dir_all(root)?;
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
        super::save_vless_source_in(
            &store,
            "vless://00000000-0000-4000-8000-000000000000@edge.example.invalid:443?security=tls#Private%20Edge",
        )?;

        let runtime = super::build_saved_sources_mihomo_runtime_in(
            &store,
            &binary.canonicalize()?,
            &data_dir,
            &ControllerEndpoint::UnixSocket(data_dir.join("controller.sock")),
        )?;

        assert_eq!(
            runtime.managed_health()?,
            super::ManagedRuntimeHealth::Stopped
        );

        match runtime {
            super::ControllerRuntime::Managed {
                profile_source,
                generated_profile,
                endpoint,
                ..
            } => {
                assert_eq!(profile_source, super::RuntimeProfileSource::SavedSources);
                assert_eq!(
                    endpoint,
                    format!("unix://{}", data_dir.join("controller.sock").display())
                );
                assert_eq!(
                    generated_profile.expect("generated profile").kernel,
                    manis_core::KernelKind::Mihomo
                );
            }
            _ => panic!("saved sources should build a managed runtime"),
        }
        let generated = fs::read_to_string(data_dir.join(super::GENERATED_PROFILE_FILE))?;
        assert!(generated.contains("Private Edge"));
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
        let delays = runtime.test_proxy_candidates_delay(
            "Local Group",
            &["Working Node".to_owned(), "Offline Node".to_owned()],
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
                    r#"{"HK-01":68,"HK-02":29}"#
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

        let result = runtime
            .test_policy_group_delay("Auto HK", &["HK-01".to_owned(), "HK-02".to_owned()])?;
        let requests = server.join().map_err(|_| "fixture server panicked")??;

        assert!(requests[0].contains("GET /group/Auto%20HK/delay?"));
        assert!(requests[1].contains("GET /proxies/Auto%20HK HTTP/1.1"));
        assert_eq!(result.current.as_deref(), Some("HK-02"));
        assert_eq!(result.delays.get("HK-02"), Some(&29));
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

        let result = runtime
            .test_policy_group_delay("Auto HK", &["HK-01".to_owned(), "HK-02".to_owned()])?;
        server.join().map_err(|_| "fixture server panicked")??;

        assert_eq!(result.current.as_deref(), Some("HK-01"));
        assert_eq!(result.delays.get("HK-01"), Some(&42));
        assert!(!result.delays.contains_key("HK-02"));
        Ok(())
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
