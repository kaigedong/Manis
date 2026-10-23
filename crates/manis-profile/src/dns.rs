use std::fmt;
use std::net::IpAddr;

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

    /// Parses a plain resolver endpoint: an IPv4 or IPv6 literal with an optional port.
    ///
    /// Android exposes the active network's resolver as a bare address instead of `DoH`, so the
    /// mobile client resolves proxy server hostnames exactly the way the phone does.
    ///
    /// # Errors
    /// Returns a redacted validation error when the input is not an IP literal.
    pub fn parse_plain(input: &str) -> Result<Self, ProfileError> {
        let value = input.trim();
        if value.is_empty() || value.len() > MAX_PROXY_DNS_SERVER_BYTES {
            return Err(ProfileError::InvalidValue("proxy DNS server"));
        }
        let ip = value
            .parse::<IpAddr>()
            .or_else(|_error| {
                value
                    .parse::<std::net::SocketAddr>()
                    .map(|address| address.ip())
            })
            .or_else(|_error| {
                value
                    .strip_prefix('[')
                    .and_then(|inner| inner.strip_suffix(']'))
                    .ok_or(())
                    .and_then(|inner| inner.parse::<IpAddr>().map_err(|_error| ()))
            })
            .map_err(|_error| ProfileError::InvalidValue("proxy DNS server"))?;
        Ok(Self(ip.to_string()))
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

#[cfg(test)]
mod tests {
    use super::ProxyDnsServer;

    #[test]
    fn accepts_plain_resolvers() {
        for value in [
            "192.168.3.1",
            "1.1.1.1:53",
            "::1",
            "[2606:4700:4700::1111]:53",
        ] {
            assert!(ProxyDnsServer::parse_plain(value).is_ok(), "{value}");
        }
    }

    #[test]
    fn normalizes_to_a_bare_address() {
        assert_eq!(
            ProxyDnsServer::parse_plain("1.1.1.1:53")
                .expect("parses")
                .as_str(),
            "1.1.1.1"
        );
    }

    #[test]
    fn rejects_hostnames_and_schemes() {
        for value in [
            "",
            "router.local",
            "https://223.5.5.5/dns-query",
            "not an ip",
        ] {
            assert!(ProxyDnsServer::parse_plain(value).is_err(), "{value}");
        }
    }
}
