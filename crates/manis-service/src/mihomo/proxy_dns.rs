//! Saved proxy-server DNS policy (`settings.dns.set`).
//!
//! The NSH settings page edits how Manis resolves proxy-server hostnames: the
//! encrypted `DoH` default, the system resolver, or the user's own endpoints.
//! The policy is stored next to the saved sources so the headless engine and
//! the NSH host read one copy.

use std::path::Path;

use serde::{Deserialize, Serialize};

use super::{MAX_SUBSCRIPTION_PROXY_DNS_SERVERS, ProxyDnsServer, SubscriptionStoreError};

const PROXY_DNS_FILE: &str = "proxy-dns.state";
const PROXY_DNS_VERSION: u32 = 1;
const MAX_PROXY_DNS_FILE_BYTES: u64 = 8 * 1024;

/// How Manis resolves proxy-server hostnames.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ProxyDnsMode {
    /// Encrypted `DoH` resolvers; the default.
    #[default]
    Doh,
    /// The system's own resolver.
    System,
}

impl ProxyDnsMode {
    #[must_use]
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Doh => "doh",
            Self::System => "system",
        }
    }

    #[must_use]
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Doh => "加密 DNS（DoH）",
            Self::System => "本机 DNS",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "doh" => Some(Self::Doh),
            "system" => Some(Self::System),
            _ => None,
        }
    }
}

/// The saved DNS policy: a mode plus the user's custom resolvers.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct ProxyDnsSettings {
    version: u32,
    pub(crate) mode: ProxyDnsMode,
    #[serde(default)]
    pub(crate) servers: Vec<String>,
}

impl Default for ProxyDnsSettings {
    fn default() -> Self {
        Self {
            version: PROXY_DNS_VERSION,
            mode: ProxyDnsMode::Doh,
            servers: Vec::new(),
        }
    }
}

/// Reads the saved policy; a missing or unreadable file is the default.
#[must_use]
pub(crate) fn load_proxy_dns_in(directory: &Path) -> ProxyDnsSettings {
    let Ok(Some(source)) =
        crate::config_toml::read_entry(directory, PROXY_DNS_FILE, MAX_PROXY_DNS_FILE_BYTES)
    else {
        return ProxyDnsSettings::default();
    };
    let Ok(stored) = serde_json::from_str::<ProxyDnsSettings>(&source) else {
        return ProxyDnsSettings::default();
    };
    if stored.version != PROXY_DNS_VERSION {
        return ProxyDnsSettings::default();
    }
    ProxyDnsSettings {
        version: PROXY_DNS_VERSION,
        mode: stored.mode,
        servers: validated_servers(&stored.servers).unwrap_or_default(),
    }
}

/// Validates and stores a policy edited by the settings page.
pub(crate) fn save_proxy_dns_in(
    directory: &Path,
    mode: &str,
    servers: &[String],
) -> Result<ProxyDnsSettings, SubscriptionStoreError> {
    let mode = ProxyDnsMode::parse(mode).ok_or(SubscriptionStoreError::InvalidSource)?;
    let servers = validated_servers(servers)?;
    let settings = ProxyDnsSettings {
        version: PROXY_DNS_VERSION,
        mode,
        servers,
    };
    let encoded =
        serde_json::to_string(&settings).map_err(|_error| SubscriptionStoreError::InvalidSource)?;
    crate::config_toml::write_entry(directory, PROXY_DNS_FILE, &encoded)
        .map_err(|_error| SubscriptionStoreError::StoreUnavailable)?;
    Ok(settings)
}

fn validated_servers(servers: &[String]) -> Result<Vec<String>, SubscriptionStoreError> {
    let mut validated = Vec::new();
    for server in servers {
        let parsed = ProxyDnsServer::parse_any(server.trim())
            .map_err(|_error| SubscriptionStoreError::InvalidSource)?;
        let value = parsed.as_str().to_owned();
        if !validated.contains(&value) {
            validated.push(value);
        }
        if validated.len() > MAX_SUBSCRIPTION_PROXY_DNS_SERVERS {
            return Err(SubscriptionStoreError::InvalidSource);
        }
    }
    Ok(validated)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use super::{ProxyDnsMode, load_proxy_dns_in, save_proxy_dns_in};

    fn temp_dir(name: &str) -> PathBuf {
        let base = fs::canonicalize(std::env::temp_dir()).expect("canonical temp directory");
        let root = base.join(format!("{name}-{}", std::process::id()));
        if root.exists() {
            fs::remove_dir_all(&root).expect("remove stale fixture");
        }
        fs::create_dir_all(&root).expect("create fixture");
        root
    }

    #[test]
    fn a_missing_policy_is_the_doh_default() {
        let root = temp_dir("manis-proxy-dns-default");
        let settings = load_proxy_dns_in(&root);
        assert_eq!(settings.mode, ProxyDnsMode::Doh);
        assert!(settings.servers.is_empty());
        fs::remove_dir_all(root).expect("remove fixture");
    }

    #[test]
    fn the_policy_round_trips_and_normalizes_servers() {
        let root = temp_dir("manis-proxy-dns-roundtrip");
        let saved = save_proxy_dns_in(
            &root,
            "doh",
            &[
                "https://1.1.1.1/dns-query".to_owned(),
                " 1.1.1.1 ".to_owned(),
                "https://1.1.1.1/dns-query".to_owned(),
            ],
        )
        .expect("save policy");
        assert_eq!(saved.servers, ["https://1.1.1.1/dns-query", "1.1.1.1"]);

        let loaded = load_proxy_dns_in(&root);
        assert_eq!(loaded.mode, ProxyDnsMode::Doh);
        assert_eq!(loaded.servers, saved.servers);

        save_proxy_dns_in(&root, "system", &[]).expect("save system mode");
        assert_eq!(load_proxy_dns_in(&root).mode, ProxyDnsMode::System);
        fs::remove_dir_all(root).expect("remove fixture");
    }

    #[test]
    fn invalid_modes_and_servers_are_rejected() {
        let root = temp_dir("manis-proxy-dns-reject");
        assert!(save_proxy_dns_in(&root, "dhcp", &[]).is_err());
        assert!(save_proxy_dns_in(&root, "doh", &["not a resolver".to_owned()]).is_err());
        fs::remove_dir_all(root).expect("remove fixture");
    }
}
