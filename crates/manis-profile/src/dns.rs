use std::fmt;

use crate::{ProfileError, is_https_url};

const MAX_PROXY_DNS_SERVER_BYTES: usize = 1024;

/// Stable Linux TUN interface name used by Mihomo and systemd-resolved integration.
#[cfg(target_os = "linux")]
pub const LINUX_TUN_DEVICE: &str = "Meta";
/// Synthetic TUN peer used as the systemd-resolved DNS endpoint on Linux.
#[cfg(target_os = "linux")]
pub const LINUX_TUN_DNS_SERVER: &str = "198.18.0.2";

/// An HTTPS DNS endpoint used only to resolve proxy server hostnames.
///
/// Subscription documents are remote input, so this type keeps validation at the boundary and
/// prevents arbitrary YAML values from reaching the generated Mihomo configuration.
#[derive(Clone, Eq, Hash, PartialEq)]
pub struct ProxyDnsServer(String);

impl ProxyDnsServer {
    /// Parses a bounded HTTPS DNS endpoint.
    ///
    /// # Errors
    /// Returns a redacted validation error for non-HTTPS, malformed, or unsafe input.
    pub fn parse_https(input: &str) -> Result<Self, ProfileError> {
        if input.len() > MAX_PROXY_DNS_SERVER_BYTES || !is_https_url(input) {
            return Err(ProfileError::InvalidValue("proxy DNS server"));
        }
        Ok(Self(input.to_owned()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for ProxyDnsServer {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ProxyDnsServer(<redacted>)")
    }
}
