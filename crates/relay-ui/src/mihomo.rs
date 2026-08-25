use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
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
use relay_profile::{Profile, SecretUrl, render_mihomo_yaml, write_private_atomic};

const CONTROLLER_ENV: &str = "RELAY_MIHOMO_CONTROLLER";
const SECRET_ENV: &str = "RELAY_MIHOMO_SECRET";
const BINARY_ENV: &str = "RELAY_MIHOMO_BINARY";
const CONFIG_ENV: &str = "RELAY_MIHOMO_CONFIG";
const DATA_DIR_ENV: &str = "RELAY_MIHOMO_DATA_DIR";
const SUBSCRIPTION_FILE_ENV: &str = "RELAY_MIHOMO_SUBSCRIPTION_FILE";
const MIXED_PORT_ENV: &str = "RELAY_MIHOMO_MIXED_PORT";
const DEFAULT_MANAGED_MIXED_PORT: u16 = 17_890;
const MAX_SUBSCRIPTION_FILE_BYTES: u64 = 16 * 1024;

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
        profile_source: RuntimeProfileSource,
    },
    Invalid {
        message: String,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RuntimeProfileSource {
    ExternalController,
    ExistingConfig,
    PrivateSubscription,
    Invalid,
}

impl RuntimeProfileSource {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::ExternalController => "外部控制器",
            Self::ExistingConfig => "已有 Mihomo 配置",
            Self::PrivateSubscription => "私有 HTTPS 订阅",
            Self::Invalid => "配置不可用",
        }
    }

    pub(crate) fn detail(self) -> &'static str {
        match self {
            Self::ExternalController => "Relay 只读取控制器；未连接时使用示例预览",
            Self::ExistingConfig => "由 Relay 启动，但不解析或展示配置文件内容",
            Self::PrivateSubscription => "链接已隐藏；只向私有 Mihomo 配置写入",
            Self::Invalid => "请检查本机启动参数；敏感输入不会显示在这里",
        }
    }
}

impl ControllerRuntime {
    pub(crate) fn profile_source(&self) -> RuntimeProfileSource {
        match self {
            Self::External { .. } => RuntimeProfileSource::ExternalController,
            Self::Managed { profile_source, .. } => *profile_source,
            Self::Invalid { .. } => RuntimeProfileSource::Invalid,
        }
    }

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
    let subscription_file = env::var_os(SUBSCRIPTION_FILE_ENV);
    match select_runtime_input(binary, config_file, subscription_file) {
        Ok(RuntimeInput::External) => ControllerRuntime::External {
            endpoint: configured_endpoint(),
        },
        Ok(RuntimeInput::ExistingConfig {
            binary,
            config_file,
        }) => build_managed_runtime(PathBuf::from(binary), PathBuf::from(config_file))
            .unwrap_or_else(|message| ControllerRuntime::Invalid { message }),
        Ok(RuntimeInput::SubscriptionFile {
            binary,
            subscription_file,
        }) => build_subscription_runtime(PathBuf::from(binary), &PathBuf::from(subscription_file))
            .unwrap_or_else(|message| ControllerRuntime::Invalid { message }),
        Err(message) => ControllerRuntime::Invalid { message },
    }
}

#[derive(Debug, PartialEq, Eq)]
enum RuntimeInput {
    External,
    ExistingConfig {
        binary: std::ffi::OsString,
        config_file: std::ffi::OsString,
    },
    SubscriptionFile {
        binary: std::ffi::OsString,
        subscription_file: std::ffi::OsString,
    },
}

fn select_runtime_input(
    binary: Option<std::ffi::OsString>,
    config_file: Option<std::ffi::OsString>,
    subscription_file: Option<std::ffi::OsString>,
) -> Result<RuntimeInput, String> {
    match (binary, config_file, subscription_file) {
        (None, None, None) => Ok(RuntimeInput::External),
        (Some(binary), Some(config_file), None) => Ok(RuntimeInput::ExistingConfig {
            binary,
            config_file,
        }),
        (Some(binary), None, Some(subscription_file)) => Ok(RuntimeInput::SubscriptionFile {
            binary,
            subscription_file,
        }),
        (_, Some(_), Some(_)) => Err(format!(
            "{CONFIG_ENV} 与 {SUBSCRIPTION_FILE_ENV} 不能同时设置"
        )),
        _ => Err(format!(
            "{BINARY_ENV} 必须与 {CONFIG_ENV} 或 {SUBSCRIPTION_FILE_ENV} 之一同时设置"
        )),
    }
}

fn build_subscription_runtime(
    binary: PathBuf,
    subscription_file: &Path,
) -> Result<ControllerRuntime, String> {
    let data_dir = configured_data_dir()?;
    let controller = configured_managed_controller(&data_dir)?;
    let subscription = read_private_subscription(subscription_file)?;
    let mut profile = Profile::qx_default(subscription).map_err(|error| error.to_string())?;
    profile.mixed_port = configured_mixed_port()?;
    let yaml = render_mihomo_yaml(&profile).map_err(|error| error.to_string())?;
    let config_file = write_private_atomic(&data_dir, "relay-generated.yaml", yaml.as_bytes())
        .map_err(|error| error.to_string())?;
    Ok(build_managed_runtime_with_controller(
        binary,
        config_file,
        data_dir,
        controller,
        RuntimeProfileSource::PrivateSubscription,
    ))
}

fn read_private_subscription(path: &Path) -> Result<SecretUrl, String> {
    if !path.is_absolute() || !has_only_clean_components(path) {
        return Err(format!("{SUBSCRIPTION_FILE_ENV} 必须是无跳转段的绝对路径"));
    }
    let metadata =
        fs::symlink_metadata(path).map_err(|_source| "无法读取私有订阅文件元数据".to_owned())?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("私有订阅来源必须是普通文件，不能是符号链接".to_owned());
    }
    let file = fs::File::open(path).map_err(|_source| "无法打开私有订阅文件".to_owned())?;
    let opened_metadata = file
        .metadata()
        .map_err(|_source| "无法读取已打开订阅文件的元数据".to_owned())?;
    if opened_metadata.len() > MAX_SUBSCRIPTION_FILE_BYTES {
        return Err("私有订阅文件超过 16 KiB 限制".to_owned());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};

        if metadata.dev() != opened_metadata.dev() || metadata.ino() != opened_metadata.ino() {
            return Err("私有订阅文件在读取期间发生变化".to_owned());
        }
        if opened_metadata.permissions().mode() & 0o077 != 0 {
            return Err("私有订阅文件权限必须为 0600 或更严格".to_owned());
        }
    }
    let mut contents = String::new();
    file.take(MAX_SUBSCRIPTION_FILE_BYTES + 1)
        .read_to_string(&mut contents)
        .map_err(|_source| "私有订阅文件必须是有效 UTF-8 文本".to_owned())?;
    if contents.len() as u64 > MAX_SUBSCRIPTION_FILE_BYTES {
        return Err("私有订阅文件超过 16 KiB 限制".to_owned());
    }
    let value = contents.trim_end_matches(['\r', '\n']);
    if value.is_empty() || value.lines().count() != 1 || value.trim() != value {
        return Err("私有订阅文件必须只包含一行 HTTPS URL".to_owned());
    }
    SecretUrl::parse_https(value)
        .map_err(|_error| "私有订阅文件必须只包含一个有效 HTTPS URL".to_owned())
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
    match env::var(MIXED_PORT_ENV) {
        Ok(value) => value
            .parse::<u16>()
            .ok()
            .filter(|port| *port != 0)
            .ok_or_else(|| format!("{MIXED_PORT_ENV} 必须是 1 到 65535 的端口")),
        Err(env::VarError::NotPresent) => Ok(DEFAULT_MANAGED_MIXED_PORT),
        Err(env::VarError::NotUnicode(_value)) => {
            Err(format!("{MIXED_PORT_ENV} 必须是有效 Unicode"))
        }
    }
}

fn build_managed_runtime(
    binary: PathBuf,
    config_file: PathBuf,
) -> Result<ControllerRuntime, String> {
    let data_dir = configured_data_dir()?;
    build_managed_runtime_in(binary, config_file, data_dir)
}

fn configured_data_dir() -> Result<PathBuf, String> {
    env::var_os(DATA_DIR_ENV)
        .map(PathBuf::from)
        .or_else(default_data_dir)
        .ok_or_else(|| format!("无法确定数据目录，请设置 {DATA_DIR_ENV}"))
}

fn build_managed_runtime_in(
    binary: PathBuf,
    config_file: PathBuf,
    data_dir: PathBuf,
) -> Result<ControllerRuntime, String> {
    let controller = configured_managed_controller(&data_dir)?;
    Ok(build_managed_runtime_with_controller(
        binary,
        config_file,
        data_dir,
        controller,
        RuntimeProfileSource::ExistingConfig,
    ))
}

fn configured_managed_controller(data_dir: &Path) -> Result<ControllerEndpoint, String> {
    let controller = match env::var(CONTROLLER_ENV) {
        Ok(endpoint) => parse_managed_endpoint(&endpoint)?,
        Err(env::VarError::NotPresent) => default_managed_endpoint(data_dir)?,
        Err(env::VarError::NotUnicode(_value)) => {
            return Err(format!("{CONTROLLER_ENV} 必须是有效 Unicode"));
        }
    };
    Ok(controller)
}

fn build_managed_runtime_with_controller(
    binary: PathBuf,
    config_file: PathBuf,
    data_dir: PathBuf,
    controller: ControllerEndpoint,
    profile_source: RuntimeProfileSource,
) -> ControllerRuntime {
    let endpoint = controller.uri();
    let config = ManagedEngineConfig::new(binary, config_file, data_dir, controller);
    let manager = EngineManager::new(
        config,
        ReadinessPolicy::default(),
        Box::new(MihomoReadinessProbe),
    );
    ControllerRuntime::Managed {
        endpoint,
        manager: Arc::new(Mutex::new(manager)),
        profile_source,
    }
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
    use std::ffi::OsString;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};

    use relay_engine::ControllerEndpoint;

    #[test]
    fn runtime_profile_source_exposes_only_safe_copy() {
        use super::RuntimeProfileSource;

        assert_eq!(
            RuntimeProfileSource::PrivateSubscription.label(),
            "私有 HTTPS 订阅"
        );
        assert!(
            RuntimeProfileSource::PrivateSubscription
                .detail()
                .contains("链接已隐藏")
        );
        assert!(
            !RuntimeProfileSource::PrivateSubscription
                .detail()
                .contains("token")
        );
    }

    #[test]
    fn selects_external_existing_and_subscription_runtime_inputs() -> Result<(), String> {
        assert_eq!(
            super::select_runtime_input(None, None, None)?,
            super::RuntimeInput::External
        );
        assert_eq!(
            super::select_runtime_input(
                Some(OsString::from("mihomo")),
                Some(OsString::from("config.yaml")),
                None,
            )?,
            super::RuntimeInput::ExistingConfig {
                binary: OsString::from("mihomo"),
                config_file: OsString::from("config.yaml"),
            }
        );
        assert_eq!(
            super::select_runtime_input(
                Some(OsString::from("mihomo")),
                None,
                Some(OsString::from("relay.subscription.secret")),
            )?,
            super::RuntimeInput::SubscriptionFile {
                binary: OsString::from("mihomo"),
                subscription_file: OsString::from("relay.subscription.secret"),
            }
        );
        assert!(
            super::select_runtime_input(
                Some(OsString::from("mihomo")),
                Some(OsString::from("config.yaml")),
                Some(OsString::from("relay.subscription.secret")),
            )
            .is_err()
        );
        assert!(super::select_runtime_input(Some(OsString::from("mihomo")), None, None).is_err());
        assert!(
            super::select_runtime_input(
                None,
                None,
                Some(OsString::from("relay.subscription.secret")),
            )
            .is_err()
        );
        Ok(())
    }

    #[test]
    fn reads_only_private_single_line_https_subscription_files()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = test_temp_dir("relay-ui-subscription");
        let source = root.join("relay.subscription.secret");
        fs::write(
            &source,
            "https://subscription.example.invalid/client?token=fixture-secret\n",
        )?;
        fs::set_permissions(&source, fs::Permissions::from_mode(0o600))?;

        let secret = super::read_private_subscription(&source)?;
        assert_eq!(format!("{secret:?}"), "SecretUrl(<redacted>)");
        assert!(!format!("{secret:?}").contains("fixture-secret"));

        fs::write(
            &source,
            "http://subscription.example.invalid/fixture-secret",
        )?;
        let message = super::read_private_subscription(&source).expect_err("http must fail");
        assert!(!message.contains("fixture-secret"));
        assert!(!message.contains("subscription.example.invalid"));

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn rejects_public_or_symlink_subscription_files() -> Result<(), Box<dyn std::error::Error>> {
        let root = test_temp_dir("relay-ui-subscription-safety");
        let source = root.join("relay.subscription.secret");
        fs::write(
            &source,
            "https://subscription.example.invalid/fixture-secret",
        )?;
        fs::set_permissions(&source, fs::Permissions::from_mode(0o644))?;
        assert!(super::read_private_subscription(&source).is_err());

        fs::set_permissions(&source, fs::Permissions::from_mode(0o600))?;
        let link = root.join("link.subscription.secret");
        std::os::unix::fs::symlink(&source, &link)?;
        assert!(super::read_private_subscription(&link).is_err());

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
