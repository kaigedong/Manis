#[cfg(unix)]
use std::path::PathBuf;
use std::{env, error::Error, fmt};

use relay_core::{EmptyPolicyCatalog, PolicyCatalog};
#[cfg(unix)]
use relay_mihomo::UnixSocketTransport;
use relay_mihomo::{
    ControllerConfig, MihomoClient, MihomoError, MihomoSnapshot, ObservedRouteEvidence,
    StdHttpTransport, to_policy_catalog,
};

const CONTROLLER_ENV: &str = "RELAY_MIHOMO_CONTROLLER";
const SECRET_ENV: &str = "RELAY_MIHOMO_SECRET";

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
    pub(crate) fn button_label(&self) -> &'static str {
        match self {
            Self::Demo | Self::Failed { .. } => "连接 Mihomo",
            Self::Connecting { .. } => "正在连接…",
            Self::Connected { .. } => "刷新数据",
        }
    }

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
}

impl fmt::Display for LoadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Mihomo(error) => error.fmt(formatter),
            Self::EmptyCatalog(error) => error.fmt(formatter),
        }
    }
}

impl Error for LoadError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Mihomo(error) => Some(error),
            Self::EmptyCatalog(error) => Some(error),
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
    #[ignore = "requires RELAY_MIHOMO_CONTROLLER and a running controller"]
    fn reads_a_live_controller_snapshot() -> Result<(), Box<dyn std::error::Error>> {
        let endpoint = std::env::var(super::CONTROLLER_ENV)?;
        let snapshot = super::load(&endpoint)?;
        assert!(snapshot.catalog.iter().count() > 0);
        assert!(!snapshot.version.is_empty());
        Ok(())
    }
}
