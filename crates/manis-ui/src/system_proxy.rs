use std::fmt;

use crate::localization::{Language, copy};

mod recovery;
mod session;

pub(crate) use session::{SystemProxySession, TunDnsSession};

#[cfg(any(
    target_os = "linux",
    target_os = "windows",
    all(target_os = "macos", not(test))
))]
use recovery::read_recovery_snapshot;
#[cfg(all(any(target_os = "macos", target_os = "linux"), not(test)))]
use recovery::read_tun_dns_recovery_snapshot;
#[cfg(test)]
use recovery::{LEGACY_RELAY_RECOVERY_VERSION, read_recovery_snapshot_at};
use recovery::{
    RECOVERY_VERSION, TUN_DNS_RECOVERY_VERSION, decode_string, delete_recovery_snapshot,
    delete_recovery_snapshot_at, delete_tun_dns_recovery_snapshot, encode_string,
    recovery_version_supported, rollback_failed_message, write_recovery_snapshot,
    write_recovery_snapshot_at, write_tun_dns_recovery_snapshot,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ProxyPorts {
    pub http: Option<u16>,
    pub socks: Option<u16>,
}

impl ProxyPorts {
    pub(crate) fn usable_with_language(self, language: Language) -> Result<Self, SystemProxyError> {
        if self.http.is_none() && self.socks.is_none() {
            Err(SystemProxyError::Unavailable(
                language
                    .localized(copy::system_proxy::MIHOMO_HAS_NO_OPEN_HTTP_MIXED_OR_SOCKS_LISTENER)
                    .to_owned(),
            ))
        } else {
            Ok(self)
        }
    }
}

#[derive(Debug)]
pub(crate) enum SystemProxyError {
    Unavailable(String),
    CommandFailed(String),
}

impl fmt::Display for SystemProxyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable(message) | Self::CommandFailed(message) => {
                formatter.write_str(message)
            }
        }
    }
}

#[cfg(target_os = "macos")]
mod macos;

#[cfg(target_os = "linux")]
mod linux;

#[cfg(target_os = "windows")]
mod windows;

#[cfg(test)]
mod tests {
    use crate::localization::Language;

    use super::{ProxyPorts, SystemProxyError, read_recovery_snapshot_at};

    #[test]
    fn legacy_relay_recovery_version_remains_readable() {
        assert!(super::recovery_version_supported(
            super::LEGACY_RELAY_RECOVERY_VERSION
        ));
        assert!(super::recovery_version_supported(super::RECOVERY_VERSION));
    }

    #[test]
    fn proxy_ports_require_at_least_one_listener() {
        let error = ProxyPorts {
            http: None,
            socks: None,
        }
        .usable_with_language(Language::SimplifiedChinese)
        .expect_err("empty ports must fail closed");

        assert!(matches!(error, SystemProxyError::Unavailable(_)));
    }

    #[cfg(unix)]
    #[test]
    fn recovery_reader_rejects_symbolic_links() {
        use std::os::unix::fs::symlink;
        use std::time::{SystemTime, UNIX_EPOCH};

        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should follow Unix epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "manis-system-proxy-symlink-{}-{nonce}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).expect("create fixture directory");
        let target = root.join("target");
        let link = root.join("system-proxy.recovery");
        std::fs::write(&target, "manis-system-proxy-v1\nplatform=macos\n")
            .expect("write fixture target");
        symlink(&target, &link).expect("create fixture symlink");

        assert!(read_recovery_snapshot_at(&link, Language::English).is_err());

        std::fs::remove_dir_all(root).expect("remove fixture directory");
    }
}
