use std::fs;
use std::io::Read;
use std::net::TcpListener;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::{env, error::Error, fmt};

use relay_core::{EmptyPolicyCatalog, PolicyCatalog};
use relay_engine::{
    ControllerEndpoint, EngineError, EngineManager, ManagedEngineConfig, ProbeStatus,
    ReadinessPolicy, ReadinessProbe,
};
#[cfg(unix)]
use relay_mihomo::UnixSocketTransport;
use relay_mihomo::{
    Connection, ControllerConfig, MihomoClient, MihomoError, MihomoSnapshot, ObservedRouteEvidence,
    RuntimeConfig, StdHttpTransport, VersionInfo, to_policy_catalog,
};
use relay_profile::{Profile, SecretUrl, render_mihomo_yaml, write_private_atomic};

use crate::subscription::VlessSource;

const CONTROLLER_ENV: &str = "RELAY_MIHOMO_CONTROLLER";
const SECRET_ENV: &str = "RELAY_MIHOMO_SECRET";
const BINARY_ENV: &str = "RELAY_MIHOMO_BINARY";
const CONFIG_ENV: &str = "RELAY_MIHOMO_CONFIG";
const DATA_DIR_ENV: &str = "RELAY_MIHOMO_DATA_DIR";
const SUBSCRIPTION_FILE_ENV: &str = "RELAY_MIHOMO_SUBSCRIPTION_FILE";
const MIXED_PORT_ENV: &str = "RELAY_MIHOMO_MIXED_PORT";
const PREVIEW_BINARY_ENV: &str = "RELAY_MIHOMO_PREVIEW_BINARY";
const DEFAULT_MANAGED_MIXED_PORT: u16 = 17_890;
const MAX_SUBSCRIPTION_FILE_BYTES: u64 = 16 * 1024;
const IMPORTED_SUBSCRIPTION_FILE: &str = "subscription.url";
const STORED_SUBSCRIPTION_PREFIX: &str = "source-";
const STORED_SUBSCRIPTION_SUFFIX: &str = ".url";
const SAVED_VLESS_PREFIX: &str = "saved-";
const SAVED_VLESS_SUFFIX: &str = ".vless";
const WORKSPACE_STATE_FILE: &str = "workspace.state";
const PREVIEW_PROVIDER_ATTEMPTS: usize = 80;
const PREVIEW_PROVIDER_DELAY: Duration = Duration::from_millis(250);
static NEXT_PREVIEW_WORKSPACE: AtomicU64 = AtomicU64::new(0);
static NEXT_STORED_SOURCE: AtomicU64 = AtomicU64::new(0);

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

    pub(crate) fn set_tun_enabled(&self, enabled: bool) -> Result<(), LoadError> {
        let endpoint = match self {
            Self::External { endpoint } => endpoint.clone(),
            Self::Managed { manager, .. } => {
                let mut manager = manager
                    .lock()
                    .map_err(|_poisoned| LoadError::Runtime("托管内核状态锁已损坏".to_owned()))?;
                manager
                    .running_endpoint()?
                    .ok_or_else(|| LoadError::Runtime("Mihomo 尚未运行，请先连接内核".to_owned()))?
                    .uri()
            }
            Self::Invalid { message } => return Err(LoadError::Runtime(message.clone())),
        };
        set_tun_enabled(&endpoint, enabled).map_err(LoadError::from)
    }
}

pub(crate) struct RuntimeSnapshot {
    pub endpoint: String,
    pub snapshot: LoadedSnapshot,
}

pub(crate) struct LoadedSnapshot {
    pub catalog: PolicyCatalog,
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct StoredSubscription {
    pub id: String,
    pub source: SecretUrl,
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
            Self::DataDirectoryUnavailable => "无法确定 Relay 的用户数据目录",
            Self::InvalidSource => "订阅地址无效，未执行导入",
            Self::StoreUnavailable => "无法安全保存订阅，请检查用户数据目录权限",
            Self::StoredSourceUnavailable => "已保存的订阅无法安全读取，需要重新导入",
        })
    }
}

impl Error for SubscriptionStoreError {}

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
                "relay-p-{:x}-{nonce:x}-{sequence:x}",
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
    default_relay_data_dir()
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
    write_private_atomic(directory, &file_name, input.as_bytes())
        .map_err(|_error| SubscriptionStoreError::StoreUnavailable)?;
    Ok(StoredSubscription { id, source })
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
        let contents = read_private_source(&path)?;
        let source = SecretUrl::parse_subscription(&contents)
            .map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)?;
        if !sources
            .iter()
            .any(|stored: &StoredSubscription| stored.source == source)
        {
            sources.push(StoredSubscription { id, source });
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
                }]
            })
            .unwrap_or_default()
    })
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

fn next_stored_source_id(prefix: &str) -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let sequence = NEXT_STORED_SOURCE.fetch_add(1, Ordering::Relaxed);
    format!("{prefix}{timestamp:x}-{sequence:x}")
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
    if contents.len() as u64 > MAX_SUBSCRIPTION_FILE_BYTES
        || contents.is_empty()
        || contents.lines().count() != 1
        || contents.trim() != contents
    {
        return Err(SubscriptionStoreError::StoredSourceUnavailable);
    }
    SecretUrl::parse_subscription(&contents)
        .map(Some)
        .map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)
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
    if let Some(explicit) = env::var_os(PREVIEW_BINARY_ENV) {
        return canonical_binary(Path::new(&explicit));
    }

    let executable_name = if cfg!(windows) {
        "mihomo.exe"
    } else {
        "mihomo"
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
    candidates.push(PathBuf::from(
        "/Applications/Clash Verge.app/Contents/MacOS/verge-mihomo",
    ));

    candidates
        .into_iter()
        .find_map(|candidate| canonical_binary(&candidate).ok())
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
fn load_subscription_provider(providers: &[relay_mihomo::ProxyProvider]) -> Vec<LoadedProvider> {
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
fn default_relay_data_dir() -> Option<PathBuf> {
    env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join("Library/Application Support/Relay"))
}

#[cfg(windows)]
fn default_relay_data_dir() -> Option<PathBuf> {
    env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .map(|root| root.join("Relay"))
}

#[cfg(all(unix, not(target_os = "macos")))]
fn default_relay_data_dir() -> Option<PathBuf> {
    env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .map(|root| root.join("relay"))
        .or_else(|| {
            env::var_os("HOME")
                .map(PathBuf::from)
                .map(|home| home.join(".local/share/relay"))
        })
}

#[cfg(not(any(unix, windows)))]
fn default_relay_data_dir() -> Option<PathBuf> {
    None
}

fn default_data_dir() -> Option<PathBuf> {
    default_relay_data_dir().map(|directory| directory.join("mihomo"))
}

pub(crate) fn configured_endpoint() -> String {
    env::var(CONTROLLER_ENV).unwrap_or_else(|_| ControllerConfig::default().base_url().to_owned())
}

pub(crate) fn load(endpoint: &str) -> Result<LoadedSnapshot, LoadError> {
    let snapshot = fetch_snapshot(endpoint)?;
    let catalog = to_policy_catalog(&snapshot)?;
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

    Ok(LoadedSnapshot {
        catalog,
        providers,
        version,
        active_connections,
        download_total,
        upload_total,
        observed_routes,
        connections,
        runtime,
    })
}

fn load_providers(providers: &[relay_mihomo::ProxyProvider]) -> Vec<LoadedProvider> {
    providers.iter().map(load_provider).collect()
}

fn load_provider(provider: &relay_mihomo::ProxyProvider) -> LoadedProvider {
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

fn set_tun_enabled(endpoint: &str, enabled: bool) -> Result<(), MihomoError> {
    #[cfg(unix)]
    if let Some(socket_path) = unix_socket_path(endpoint)? {
        return MihomoClient::new(
            ControllerConfig::default(),
            UnixSocketTransport::new(socket_path),
        )
        .set_tun_enabled(enabled);
    }

    #[cfg(not(unix))]
    if endpoint.starts_with("unix://") {
        return Err(MihomoError::InvalidConfig(
            "Unix controller sockets are not supported on this platform".to_owned(),
        ));
    }

    let config = with_configured_secret(ControllerConfig::new(endpoint)?);
    MihomoClient::new(config, StdHttpTransport::default()).set_tun_enabled(enabled)
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
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

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
        let root = test_temp_dir("relay-import-store");
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

    #[cfg(not(windows))]
    #[test]
    fn source_store_keeps_multiple_subscriptions_saved_nodes_and_fold_state()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = test_temp_dir("relay-multi-source-store");
        let store = root.join("subscriptions");
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
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].source.preview().name, "Saved");
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

    #[test]
    fn imported_subscription_load_rejects_symlinks_and_redacts_corruption()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = test_temp_dir("relay-import-corrupt");
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
        let root = test_temp_dir("relay-import-remove");
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
    #[ignore = "requires RELAY_MIHOMO_TEST_BINARY pointing to a local Mihomo executable"]
    fn real_mihomo_previews_all_nodes_from_a_subscription() -> Result<(), Box<dyn std::error::Error>>
    {
        let binary = std::env::var_os("RELAY_MIHOMO_TEST_BINARY")
            .ok_or("RELAY_MIHOMO_TEST_BINARY is required")?;
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
        let import_root = test_temp_dir("relay-real-import");
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
        assert!(
            snapshot
                .providers
                .iter()
                .any(|provider| !provider.nodes.is_empty())
        );
        assert!(!snapshot.version.is_empty());
        Ok(())
    }
}
