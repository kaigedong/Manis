use std::path::Path;
#[cfg(unix)]
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::{env, error::Error, fmt};

use relay_core::{EmptyPolicyCatalog, PolicyCatalog};
use relay_engine::{
    ControllerEndpoint, EngineError, EngineManager, ManagedEngineConfig, ProbeStatus,
    ReadinessPolicy, ReadinessProbe,
};
#[cfg(unix)]
use relay_mihomo::UnixSocketTransport;
use relay_mihomo::{
    ControllerConfig, MihomoClient, MihomoError, MihomoSnapshot, ObservedRouteEvidence,
    StdHttpTransport, VersionInfo, to_policy_catalog,
};

const CONTROLLER_ENV: &str = "RELAY_MIHOMO_CONTROLLER";
const SECRET_ENV: &str = "RELAY_MIHOMO_SECRET";
const BINARY_ENV: &str = "RELAY_MIHOMO_BINARY";
const CONFIG_ENV: &str = "RELAY_MIHOMO_CONFIG";
const DATA_DIR_ENV: &str = "RELAY_MIHOMO_DATA_DIR";

#[derive(Clone, Debug)]
pub(crate) enum ControllerState {
    Demo,
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
    pub(crate) fn compact_label(&self) -> String {
        match self {
            Self::Demo => "Mihomo 未连接 · 演示".to_owned(),
            Self::Connecting { endpoint } => format!("正在连接 {endpoint}"),
            Self::Connected {
                version,
                active_connections,
                ..
            } => format!("Mihomo {version} · {active_connections} 条连接"),
            Self::Failed { message, .. } => format!("连接失败 · {message}"),
        }
    }

    pub(crate) fn endpoint(&self) -> Option<&str> {
        match self {
            Self::Demo => None,
            Self::Connecting { endpoint }
            | Self::Connected { endpoint, .. }
            | Self::Failed { endpoint, .. } => Some(endpoint),
        }
    }
}

#[derive(Clone)]
pub(crate) enum ControllerRuntime {
    External {
        endpoint: String,
    },
    Managed {
        endpoint: String,
        manager: Arc<Mutex<EngineManager>>,
    },
    Invalid {
        message: String,
    },
}

impl ControllerRuntime {
    pub(crate) fn endpoint_label(&self) -> String {
        match self {
            Self::External { endpoint } => endpoint.clone(),
            Self::Managed { endpoint, .. } => format!("Relay 托管 · {endpoint}"),
            Self::Invalid { .. } => "Relay 托管配置".to_owned(),
        }
    }

    pub(crate) fn initial_status(&self) -> String {
        match self {
            Self::External { .. } => "演示数据 · 尚未连接 Mihomo".to_owned(),
            Self::Managed { .. } => "托管内核已配置 · 点击启动 Mihomo".to_owned(),
            Self::Invalid { message } => format!("托管内核配置无效：{message}"),
        }
    }

    pub(crate) fn button_label(&self, state: &ControllerState) -> &'static str {
        match (self, state) {
            (_, ControllerState::Connecting { .. }) => "正在连接…",
            (Self::Managed { .. }, ControllerState::Demo | ControllerState::Failed { .. }) => {
                "启动 Mihomo"
            }
            (_, ControllerState::Connected { .. }) => "刷新数据",
            _ => "连接 Mihomo",
        }
    }

    pub(crate) fn connect(&self) -> Result<RuntimeSnapshot, LoadError> {
        match self {
            Self::External { endpoint } => Ok(RuntimeSnapshot {
                endpoint: endpoint.clone(),
                snapshot: load(endpoint)?,
            }),
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
                let endpoint = endpoint.uri();
                Ok(RuntimeSnapshot {
                    endpoint: format!("Relay 托管 · {endpoint}"),
                    snapshot: load(&endpoint)?,
                })
            }
            Self::Invalid { message } => Err(LoadError::Runtime(message.clone())),
        }
    }
}

pub(crate) struct RuntimeSnapshot {
    pub endpoint: String,
    pub snapshot: LoadedSnapshot,
}

pub(crate) struct LoadedSnapshot {
    pub catalog: PolicyCatalog,
    pub version: String,
    pub active_connections: usize,
    pub download_total: u64,
    pub upload_total: u64,
    pub observed_routes: Vec<ObservedRouteEvidence>,
}

#[derive(Debug)]
pub(crate) enum LoadError {
    Mihomo(MihomoError),
    EmptyCatalog(EmptyPolicyCatalog),
    Engine(EngineError),
    Runtime(String),
}

impl fmt::Display for LoadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Mihomo(error) => error.fmt(formatter),
            Self::EmptyCatalog(error) => error.fmt(formatter),
            Self::Engine(error) => error.fmt(formatter),
            Self::Runtime(message) => formatter.write_str(message),
        }
    }
}

impl Error for LoadError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Mihomo(error) => Some(error),
            Self::EmptyCatalog(error) => Some(error),
            Self::Engine(error) => Some(error),
            Self::Runtime(_) => None,
        }
    }
}

impl From<MihomoError> for LoadError {
    fn from(error: MihomoError) -> Self {
        Self::Mihomo(error)
    }
}

impl From<EmptyPolicyCatalog> for LoadError {
    fn from(error: EmptyPolicyCatalog) -> Self {
        Self::EmptyCatalog(error)
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

pub(crate) fn configured_runtime() -> ControllerRuntime {
    let binary = env::var_os(BINARY_ENV);
    let config_file = env::var_os(CONFIG_ENV);
    match (binary, config_file) {
        (None, None) => ControllerRuntime::External {
            endpoint: configured_endpoint(),
        },
        (Some(binary), Some(config_file)) => {
            build_managed_runtime(PathBuf::from(binary), PathBuf::from(config_file))
                .unwrap_or_else(|message| ControllerRuntime::Invalid { message })
        }
        _ => ControllerRuntime::Invalid {
            message: format!("{BINARY_ENV} 与 {CONFIG_ENV} 必须同时设置"),
        },
    }
}

fn build_managed_runtime(
    binary: PathBuf,
    config_file: PathBuf,
) -> Result<ControllerRuntime, String> {
    let data_dir = env::var_os(DATA_DIR_ENV)
        .map(PathBuf::from)
        .or_else(default_data_dir)
        .ok_or_else(|| format!("无法确定数据目录，请设置 {DATA_DIR_ENV}"))?;
    let controller = match env::var(CONTROLLER_ENV) {
        Ok(endpoint) => parse_managed_endpoint(&endpoint)?,
        Err(env::VarError::NotPresent) => default_managed_endpoint(&data_dir)?,
        Err(env::VarError::NotUnicode(_value)) => {
            return Err(format!("{CONTROLLER_ENV} 必须是有效 Unicode"));
        }
    };
    let config = ManagedEngineConfig::new(binary, config_file, data_dir, controller.clone());
    let endpoint = controller.uri();
    let manager = EngineManager::new(
        config,
        ReadinessPolicy::default(),
        Box::new(MihomoReadinessProbe),
    );
    Ok(ControllerRuntime::Managed {
        endpoint,
        manager: Arc::new(Mutex::new(manager)),
    })
}

fn parse_managed_endpoint(endpoint: &str) -> Result<ControllerEndpoint, String> {
    if let Some(path) = endpoint.strip_prefix("unix://") {
        return Ok(ControllerEndpoint::UnixSocket(PathBuf::from(path)));
    }
    if endpoint.starts_with("pipe://") {
        return Err("托管 Windows pipe 尚未开放；请先使用外部 loopback controller".to_owned());
    }
    ControllerConfig::new(endpoint).map_err(|error| error.to_string())?;
    Err("托管 TCP 暂未开放；请使用 Unix socket 或外部 loopback controller".to_owned())
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
    Err("托管 Windows pipe transport 尚未完成；当前请使用外部 loopback controller".to_owned())
}

#[cfg(not(any(unix, windows)))]
fn default_managed_endpoint(_data_dir: &Path) -> Result<ControllerEndpoint, String> {
    Err("当前平台没有默认的 Mihomo controller transport".to_owned())
}

#[cfg(target_os = "macos")]
fn default_data_dir() -> Option<PathBuf> {
    env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join("Library/Application Support/Relay/mihomo"))
}

#[cfg(windows)]
fn default_data_dir() -> Option<PathBuf> {
    env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .map(|root| root.join("Relay/mihomo"))
}

#[cfg(all(unix, not(target_os = "macos")))]
fn default_data_dir() -> Option<PathBuf> {
    env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .map(|root| root.join("relay/mihomo"))
        .or_else(|| {
            env::var_os("HOME")
                .map(PathBuf::from)
                .map(|home| home.join(".local/share/relay/mihomo"))
        })
}

pub(crate) fn configured_endpoint() -> String {
    env::var(CONTROLLER_ENV).unwrap_or_else(|_| ControllerConfig::default().base_url().to_owned())
}

pub(crate) fn load(endpoint: &str) -> Result<LoadedSnapshot, LoadError> {
    let snapshot = fetch_snapshot(endpoint)?;
    let catalog = to_policy_catalog(&snapshot)?;
    let version = snapshot
        .version
        .version
        .clone()
        .unwrap_or_else(|| "版本未知".to_owned());
    let active_connections = snapshot.connections.connections.len();
    let download_total = snapshot.connections.download_total;
    let upload_total = snapshot.connections.upload_total;
    let observed_routes = snapshot.observed_routes();

    Ok(LoadedSnapshot {
        catalog,
        version,
        active_connections,
        download_total,
        upload_total,
        observed_routes,
    })
}

fn fetch_snapshot(endpoint: &str) -> Result<MihomoSnapshot, MihomoError> {
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

    let config = with_configured_secret(ControllerConfig::new(endpoint)?);
    MihomoClient::new(config, StdHttpTransport::default()).fetch_snapshot()
}

fn fetch_version(endpoint: &str) -> Result<VersionInfo, MihomoError> {
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

    let config = ControllerConfig::new(endpoint)?;
    MihomoClient::new(config, StdHttpTransport::default()).fetch_version()
}

fn with_configured_secret(mut config: ControllerConfig) -> ControllerConfig {
    if let Ok(secret) = env::var(SECRET_ENV) {
        config = config.with_secret(secret);
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
    use std::path::Path;

    use relay_engine::ControllerEndpoint;

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
    fn parses_only_private_managed_controller_endpoints() -> Result<(), String> {
        assert_eq!(
            super::parse_managed_endpoint("unix:///tmp/relay/controller.sock")?,
            ControllerEndpoint::UnixSocket("/tmp/relay/controller.sock".into())
        );
        assert!(super::parse_managed_endpoint("http://127.0.0.1:19090").is_err());
        assert!(super::parse_managed_endpoint("http://[::1]:19090").is_err());
        assert!(super::parse_managed_endpoint("http://localhost:19090").is_err());
        assert!(super::parse_managed_endpoint("http://192.0.2.10:19090").is_err());
        assert!(super::parse_managed_endpoint(r"pipe://\\.\pipe\relay-mihomo").is_err());
        Ok(())
    }

    #[test]
    #[ignore = "requires RELAY_MIHOMO_CONTROLLER and a running controller"]
    fn reads_a_live_controller_snapshot() -> Result<(), Box<dyn std::error::Error>> {
        let endpoint = std::env::var(super::CONTROLLER_ENV)?;
        let snapshot = super::load(&endpoint)?;
        assert!(snapshot.catalog.iter().count() > 0);
        assert!(!snapshot.version.is_empty());
        Ok(())
    }
}
