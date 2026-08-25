use std::collections::HashMap;

use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct VersionInfo {
    #[serde(default)]
    pub meta: bool,
    #[serde(default)]
    pub version: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MihomoSnapshot {
    pub version: VersionInfo,
    pub proxies: Vec<Proxy>,
    pub providers: Vec<ProxyProvider>,
    pub rules: Vec<Rule>,
    pub connections: ConnectionsState,
    pub runtime: RuntimeConfig,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProxyProvider {
    pub name: String,
    pub vehicle_type: Option<String>,
    pub proxies: Vec<Proxy>,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct RuntimeConfig {
    #[serde(default)]
    pub port: Option<u16>,
    #[serde(rename = "socks-port", default, alias = "socksPort")]
    pub socks_port: Option<u16>,
    #[serde(rename = "mixed-port", default, alias = "mixedPort")]
    pub mixed_port: Option<u16>,
    #[serde(default)]
    pub tun: RuntimeTunConfig,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct RuntimeTunConfig {
    #[serde(default)]
    pub enable: bool,
}

impl MihomoSnapshot {
    #[must_use]
    pub fn policy_groups(&self) -> Vec<PolicyGroup> {
        let mut groups: Vec<_> = self
            .proxies
            .iter()
            .filter_map(PolicyGroup::from_proxy)
            .collect();
        groups.sort_by(|left, right| {
            left.name
                .eq_ignore_ascii_case("GLOBAL")
                .cmp(&right.name.eq_ignore_ascii_case("GLOBAL"))
                .then_with(|| left.kind.sort_rank().cmp(&right.kind.sort_rank()))
                .then_with(|| left.name.cmp(&right.name))
        });
        groups
    }

    #[must_use]
    pub fn observed_routes(&self) -> Vec<ObservedRouteEvidence> {
        self.connections
            .connections
            .iter()
            .map(ObservedRouteEvidence::from_connection)
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Proxy {
    pub name: String,
    pub proxy_type: String,
    pub current: Option<String>,
    pub all: Vec<String>,
    pub alive: Option<bool>,
    pub history: Vec<DelayHistory>,
    pub provider_name: Option<String>,
    pub hidden: Option<bool>,
}

impl Proxy {
    #[must_use]
    pub fn latest_latency_ms(&self) -> Option<f64> {
        self.history.iter().rev().find_map(|history| {
            history
                .delay
                .filter(|delay| delay.is_finite() && *delay > 0.0)
        })
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct DelayHistory {
    #[serde(default)]
    pub time: Option<String>,
    #[serde(default)]
    pub delay: Option<f64>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum GroupKind {
    Selector,
    UrlTest,
    Fallback,
    LoadBalance,
}

impl GroupKind {
    fn from_proxy_type(proxy_type: &str) -> Option<Self> {
        let normalized: String = proxy_type
            .chars()
            .filter(|character| !matches!(character, '-' | '_' | ' '))
            .flat_map(char::to_lowercase)
            .collect();

        match normalized.as_str() {
            "selector" | "select" => Some(Self::Selector),
            "urltest" => Some(Self::UrlTest),
            "fallback" => Some(Self::Fallback),
            "loadbalance" => Some(Self::LoadBalance),
            _ => None,
        }
    }

    fn sort_rank(self) -> u8 {
        match self {
            Self::Selector => 0,
            Self::UrlTest => 1,
            Self::Fallback => 2,
            Self::LoadBalance => 3,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PolicyGroup {
    pub name: String,
    pub kind: GroupKind,
    pub current: Option<String>,
    pub nodes: Vec<String>,
    pub latest_latency_ms: Option<f64>,
    pub alive: Option<bool>,
    pub provider_name: Option<String>,
    pub hidden: Option<bool>,
}

impl PolicyGroup {
    fn from_proxy(proxy: &Proxy) -> Option<Self> {
        Some(Self {
            name: proxy.name.clone(),
            kind: GroupKind::from_proxy_type(&proxy.proxy_type)?,
            current: proxy.current.clone(),
            nodes: proxy.all.clone(),
            latest_latency_ms: proxy.latest_latency_ms(),
            alive: proxy.alive,
            provider_name: proxy.provider_name.clone(),
            hidden: proxy.hidden,
        })
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Rule {
    #[serde(default)]
    pub index: usize,
    #[serde(rename = "type", default)]
    pub kind: String,
    #[serde(default)]
    pub payload: String,
    #[serde(default)]
    pub proxy: String,
    #[serde(default)]
    pub size: i64,
    #[serde(default)]
    pub extra: RuleExtra,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct RuleExtra {
    #[serde(default, alias = "hits", alias = "hitCount")]
    pub hit: Option<u64>,
    #[serde(default, alias = "misses", alias = "missCount")]
    pub miss: Option<u64>,
    #[serde(default)]
    pub disabled: Option<bool>,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct ConnectionsState {
    #[serde(rename = "downloadTotal", default)]
    pub download_total: u64,
    #[serde(rename = "uploadTotal", default)]
    pub upload_total: u64,
    #[serde(default)]
    pub connections: Vec<Connection>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Connection {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub metadata: ConnectionMetadata,
    #[serde(default)]
    pub upload: u64,
    #[serde(default)]
    pub download: u64,
    #[serde(default)]
    pub start: Option<String>,
    #[serde(default, deserialize_with = "deserialize_string_vec")]
    pub chains: Vec<String>,
    #[serde(
        rename = "providerChains",
        default,
        deserialize_with = "deserialize_provider_chains"
    )]
    pub provider_chains: Vec<Vec<String>>,
    #[serde(default)]
    pub rule: Option<String>,
    #[serde(rename = "rulePayload", default)]
    pub rule_payload: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct ConnectionMetadata {
    #[serde(default)]
    pub network: Option<String>,
    #[serde(rename = "type", default)]
    pub connection_type: Option<String>,
    #[serde(default)]
    pub host: Option<String>,
    #[serde(rename = "sourceIP", default, alias = "source_ip")]
    pub source_ip: Option<String>,
    #[serde(rename = "destinationIP", default, alias = "destination_ip")]
    pub destination_ip: Option<String>,
    #[serde(
        rename = "sourcePort",
        default,
        alias = "source_port",
        deserialize_with = "deserialize_optional_string"
    )]
    pub source_port: Option<String>,
    #[serde(
        rename = "destinationPort",
        default,
        alias = "destination_port",
        deserialize_with = "deserialize_optional_string"
    )]
    pub destination_port: Option<String>,
    #[serde(default)]
    pub process: Option<String>,
    #[serde(rename = "processPath", default, alias = "process_path")]
    pub process_path: Option<String>,
    #[serde(rename = "dnsMode", default, alias = "dns_mode")]
    pub dns_mode: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ObservedRouteEvidence {
    pub host: Option<String>,
    pub process: Option<String>,
    pub rule: Option<String>,
    pub rule_payload: Option<String>,
    pub chains: Vec<String>,
    pub provider_chains: Vec<Vec<String>>,
    pub upload: u64,
    pub download: u64,
}

impl ObservedRouteEvidence {
    fn from_connection(connection: &Connection) -> Self {
        Self {
            host: connection
                .metadata
                .host
                .as_ref()
                .filter(|host| !host.trim().is_empty())
                .cloned()
                .or_else(|| {
                    connection
                        .metadata
                        .destination_ip
                        .as_ref()
                        .filter(|address| !address.trim().is_empty())
                        .cloned()
                }),
            process: connection.metadata.process.clone(),
            rule: connection.rule.clone(),
            rule_payload: connection.rule_payload.clone(),
            chains: connection.chains.clone(),
            provider_chains: connection.provider_chains.clone(),
            upload: connection.upload,
            download: connection.download,
        }
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct ProxiesResponse {
    #[serde(default)]
    proxies: HashMap<String, RawProxy>,
}

impl ProxiesResponse {
    pub(crate) fn into_proxies(self) -> Vec<Proxy> {
        self.proxies
            .into_iter()
            .map(|(map_name, proxy)| proxy.into_proxy(map_name))
            .collect()
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct ProvidersResponse {
    #[serde(default)]
    providers: HashMap<String, RawProvider>,
}

impl ProvidersResponse {
    pub(crate) fn into_providers(self) -> Vec<ProxyProvider> {
        let mut providers: Vec<_> = self
            .providers
            .into_iter()
            .map(|(map_name, provider)| provider.into_provider(map_name))
            .collect();
        providers.sort_by(|left, right| left.name.cmp(&right.name));
        providers
    }
}

#[derive(Debug, Deserialize)]
struct RawProvider {
    #[serde(default)]
    name: Option<String>,
    #[serde(rename = "vehicleType", default, alias = "vehicle-type")]
    vehicle_type: Option<String>,
    #[serde(default)]
    proxies: Vec<RawProxy>,
}

impl RawProvider {
    fn into_provider(self, map_name: String) -> ProxyProvider {
        let name = self.name.unwrap_or(map_name);
        let proxies = self
            .proxies
            .into_iter()
            .enumerate()
            .map(|(index, proxy)| proxy.into_proxy(format!("节点 {}", index + 1)))
            .collect();
        ProxyProvider {
            name,
            vehicle_type: self.vehicle_type,
            proxies,
        }
    }
}

#[derive(Debug, Deserialize)]
struct RawProxy {
    #[serde(default)]
    name: Option<String>,
    #[serde(rename = "type", default)]
    proxy_type: String,
    #[serde(rename = "now", default)]
    current: Option<String>,
    #[serde(default, deserialize_with = "deserialize_string_vec")]
    all: Vec<String>,
    #[serde(default)]
    alive: Option<bool>,
    #[serde(default, deserialize_with = "deserialize_history_vec")]
    history: Vec<DelayHistory>,
    #[serde(
        rename = "providerName",
        default,
        alias = "provider-name",
        alias = "provider_name"
    )]
    provider_name: Option<String>,
    #[serde(default)]
    hidden: Option<bool>,
}

impl RawProxy {
    fn into_proxy(self, map_name: String) -> Proxy {
        Proxy {
            name: match self.name {
                Some(name) => name,
                None => map_name,
            },
            proxy_type: self.proxy_type,
            current: self.current,
            all: self.all,
            alive: self.alive,
            history: self.history,
            provider_name: self.provider_name,
            hidden: self.hidden,
        }
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct RulesResponse {
    #[serde(default)]
    pub(crate) rules: Vec<Rule>,
}

fn deserialize_string_vec<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    match Option::<Vec<String>>::deserialize(deserializer)? {
        Some(values) => Ok(values),
        None => Ok(Vec::new()),
    }
}

fn deserialize_history_vec<'de, D>(deserializer: D) -> Result<Vec<DelayHistory>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    match Option::<Vec<DelayHistory>>::deserialize(deserializer)? {
        Some(values) => Ok(values),
        None => Ok(Vec::new()),
    }
}

fn deserialize_provider_chains<'de, D>(deserializer: D) -> Result<Vec<Vec<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let Some(value) = Option::<Value>::deserialize(deserializer)? else {
        return Ok(Vec::new());
    };

    let Value::Array(items) = value else {
        return Ok(Vec::new());
    };

    if items.iter().all(Value::is_string) {
        return Ok(vec![
            items
                .into_iter()
                .filter_map(|item| item.as_str().map(ToOwned::to_owned))
                .collect(),
        ]);
    }

    Ok(items
        .into_iter()
        .filter_map(|item| {
            let Value::Array(chain) = item else {
                return None;
            };
            Some(
                chain
                    .into_iter()
                    .filter_map(|entry| entry.as_str().map(ToOwned::to_owned))
                    .collect(),
            )
        })
        .collect())
}

fn deserialize_optional_string<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let Some(value) = Option::<Value>::deserialize(deserializer)? else {
        return Ok(None);
    };

    Ok(match value {
        Value::String(value) => Some(value),
        Value::Number(value) => Some(value.to_string()),
        _ => None,
    })
}
