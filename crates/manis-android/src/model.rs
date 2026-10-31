//! Mobile workspace model and compiler.
//!
//! The phone stores one JSON document describing subscriptions, single nodes, user policy groups,
//! QX rules and the routing mode. This module turns it into the same [`Profile`] the desktop uses,
//! so rules, group semantics and the rendered Mihomo configuration cannot drift between platforms.

use manis_profile::{
    MANIS_GLOBAL_GROUP_NAME, Name, PolicyRef, Profile, ProfileMode, ProxyDnsServer, QxRuleList,
    Rule, SecretUrl, UserPolicyGroup, UserPolicyGroupKind, VlessProxy,
    render_mihomo_yaml_for_android,
};
use serde::Deserialize;
use serde_json::json;

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct MobileConfig {
    pub mode: String,
    pub mixed_port: u16,
    pub subscriptions: Vec<MobileSubscription>,
    pub nodes: Vec<MobileNode>,
    pub groups: Vec<MobileGroup>,
    /// Legacy QX text, still accepted from workspaces written before structured rules existed.
    pub rules: String,
    /// Structured rules written by the current app; takes precedence over [`Self::rules`].
    #[serde(default)]
    pub rule_entries: Vec<MobileRule>,
    /// Resolvers of the active Android network; proxy server hostnames are resolved with these.
    #[serde(default)]
    pub system_dns: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct MobileRule {
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub value: String,
    #[serde(default)]
    pub policy: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Deserialize)]
pub struct MobileSubscription {
    #[serde(default)]
    pub name: String,
    pub url: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Relative provider cache path written by the app before connecting.
    #[serde(default)]
    pub cache_path: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct MobileNode {
    pub link: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Deserialize)]
pub struct MobileGroup {
    pub name: String,
    #[serde(default)]
    pub strategy: String,
    #[serde(default)]
    pub subscription_indexes: Vec<usize>,
    #[serde(default)]
    pub node_indexes: Vec<usize>,
    #[serde(default)]
    pub filter: Option<String>,
    #[serde(default)]
    pub tolerance: Option<u16>,
    #[serde(default)]
    pub interval_secs: Option<u32>,
}

const fn default_true() -> bool {
    true
}

const DEFAULT_MIXED_PORT: u16 = 7890;

/// Compiles the mobile document and returns a JSON envelope:
/// `{"yaml": "...", "warnings": [...], "providers": n, "nodes": n, "groups": n, "rules": n}`.
pub fn compile(config_json: &str, file_descriptor: i32) -> Result<String, String> {
    let config: MobileConfig =
        serde_json::from_str(config_json).map_err(|error| format!("配置格式无效：{error}"))?;
    let mut warnings = Vec::new();

    let subscriptions = compile_subscriptions(&config.subscriptions, &mut warnings);
    let nodes = compile_nodes(&config.nodes, &mut warnings);
    if subscriptions.is_empty() && nodes.is_empty() {
        return Err("至少需要一个启用的订阅或节点".to_owned());
    }

    let enabled_subscription_indexes = config
        .subscriptions
        .iter()
        .enumerate()
        .filter(|(_, subscription)| subscription.enabled)
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    let enabled_node_indexes = config
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, node)| node.enabled)
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    let node_names = nodes
        .iter()
        .map(|node| node.name().as_str().to_owned())
        .collect::<Vec<_>>();

    let groups = compile_groups(
        &config.groups,
        &enabled_subscription_indexes,
        &enabled_node_indexes,
        &node_names,
        &mut warnings,
    )?;
    let mut profile = Profile::qx_sources_with_groups(
        subscriptions,
        nodes,
        groups,
        if config.mixed_port == 0 {
            DEFAULT_MIXED_PORT
        } else {
            config.mixed_port
        },
    )
    .map_err(|error| format!("生成配置失败：{error}"))?;
    apply_subscription_cache_paths(&mut profile, &config.subscriptions);

    profile.mode = parse_mode(&config.mode);
    if !config.system_dns.is_empty() {
        let resolvers = config
            .system_dns
            .iter()
            .filter_map(|value| match ProxyDnsServer::parse_any(value) {
                Ok(server) => Some(server),
                Err(_error) => {
                    warnings.push(format!("系统 DNS {value} 无效，已忽略"));
                    None
                }
            })
            .take(4)
            .collect::<Vec<_>>();
        if !resolvers.is_empty() {
            profile.set_proxy_server_nameservers(resolvers);
        }
    }
    let group_names = profile
        .groups
        .iter()
        .map(|group| group.name.as_str().to_owned())
        .collect::<Vec<_>>();
    profile.rules = compile_rules(&config, &group_names, &mut warnings)?;
    profile
        .ensure_global_match()
        .map_err(|error| format!("生成默认规则失败：{error}"))?;

    let yaml = render_mihomo_yaml_for_android(&profile, file_descriptor)
        .map_err(|error| format!("渲染配置失败：{error}"))?;

    Ok(json!({
        "yaml": yaml,
        "warnings": warnings,
        "providers": profile.providers.len(),
        "nodes": profile.proxies.len(),
        "groups": profile.groups.len(),
        "rules": profile.rules.len(),
    })
    .to_string())
}

/// Uses the app-downloaded provider cache when the workspace supplies a safe relative path.
///
/// The app names each cache after a hash of the subscription URL, so a replaced or reordered
/// subscription can never load another airport's cached nodes.
fn apply_subscription_cache_paths(profile: &mut Profile, values: &[MobileSubscription]) {
    let paths = values
        .iter()
        .filter(|subscription| subscription.enabled)
        .filter(|subscription| SecretUrl::parse_subscription(&subscription.url).is_ok())
        .map(|subscription| subscription.cache_path.as_deref())
        .collect::<Vec<_>>();
    for (provider, path) in profile.providers.iter_mut().zip(paths) {
        if let Some(path) = path.filter(|path| is_safe_cache_path(path)) {
            provider.path = path.to_owned();
        }
    }
}

fn is_safe_cache_path(path: &str) -> bool {
    path.starts_with("./proxy_providers/")
        && path.ends_with(".yaml")
        && path.len() <= 256
        && !path.contains("..")
        && !path.contains('\\')
}

fn compile_subscriptions(
    values: &[MobileSubscription],
    warnings: &mut Vec<String>,
) -> Vec<SecretUrl> {
    let mut parsed = Vec::new();
    for (index, subscription) in values.iter().enumerate() {
        if !subscription.enabled {
            continue;
        }
        let label = if subscription.name.trim().is_empty() {
            format!("订阅 {}", index + 1)
        } else {
            subscription.name.trim().to_owned()
        };
        match SecretUrl::parse_subscription(&subscription.url) {
            Ok(secret) => parsed.push(secret),
            Err(_error) => warnings.push(format!("{label}：订阅地址无效，已跳过")),
        }
    }
    parsed
}

fn compile_nodes(values: &[MobileNode], warnings: &mut Vec<String>) -> Vec<VlessProxy> {
    let mut parsed: Vec<VlessProxy> = Vec::new();
    for (index, node) in values.iter().enumerate() {
        if !node.enabled {
            continue;
        }
        match VlessProxy::parse_share_link(&node.link) {
            Ok(proxy) => {
                if parsed
                    .iter()
                    .any(|existing| existing.name() == proxy.name())
                {
                    warnings.push(format!("节点 {}：名称重复，已跳过", proxy.name().as_str()));
                    continue;
                }
                parsed.push(proxy);
            }
            Err(_error) => warnings.push(format!("节点 {}：分享链接无效，已跳过", index + 1)),
        }
    }
    parsed
}

fn compile_groups(
    values: &[MobileGroup],
    enabled_subscription_indexes: &[usize],
    enabled_node_indexes: &[usize],
    node_names: &[String],
    warnings: &mut Vec<String>,
) -> Result<Vec<UserPolicyGroup>, String> {
    let mut groups: Vec<UserPolicyGroup> = Vec::new();
    for (position, group) in values.iter().enumerate() {
        let name = Name::parse(group.name.trim())
            .map_err(|_error| format!("策略组 {} 的名称无效", position + 1))?;
        if name.as_str() == MANIS_GLOBAL_GROUP_NAME
            || groups.iter().any(|existing| existing.name == name)
        {
            return Err(format!("策略组 {} 的名称重复或保留", position + 1));
        }

        let mut providers = Vec::new();
        for index in &group.subscription_indexes {
            match enabled_subscription_indexes
                .iter()
                .position(|value| value == index)
            {
                Some(provider) => providers.push(provider),
                None => warnings.push(format!(
                    "策略组 {}：订阅 {} 未启用，已忽略",
                    name.as_str(),
                    index + 1
                )),
            }
        }

        let mut proxies = Vec::new();
        for index in &group.node_indexes {
            match enabled_node_indexes.iter().position(|value| value == index) {
                Some(position) => proxies.push(
                    Name::parse(&node_names[position])
                        .map_err(|_error| format!("策略组 {}：节点名称无效", name.as_str()))?,
                ),
                None => warnings.push(format!(
                    "策略组 {}：节点 {} 未启用，已忽略",
                    name.as_str(),
                    index + 1
                )),
            }
        }

        if providers.is_empty() && proxies.is_empty() {
            warnings.push(format!("策略组 {}：没有可用成员", name.as_str()));
            continue;
        }

        let kind = if group.strategy.eq_ignore_ascii_case("urltest") {
            UserPolicyGroupKind::UrlTest {
                tolerance: group.tolerance.unwrap_or(50),
                interval_secs: group.interval_secs.unwrap_or(300),
            }
        } else {
            UserPolicyGroupKind::Select
        };
        groups.push(UserPolicyGroup {
            name,
            icon: None,
            kind,
            provider_indexes: providers,
            direct_proxies: proxies,
            direct_policies: Vec::new(),
            filter: group
                .filter
                .clone()
                .filter(|value| !value.trim().is_empty()),
        });
    }
    Ok(groups)
}

fn compile_rules(
    config: &MobileConfig,
    group_names: &[String],
    warnings: &mut Vec<String>,
) -> Result<Vec<Rule>, String> {
    if config.rule_entries.is_empty() {
        return compile_qx_rules(&config.rules, group_names, warnings);
    }
    compile_structured_rules(&config.rule_entries, group_names, warnings)
}

/// Compiles the structured rule rows used by the Android editor, mirroring the desktop rule kinds.
fn compile_structured_rules(
    entries: &[MobileRule],
    group_names: &[String],
    warnings: &mut Vec<String>,
) -> Result<Vec<Rule>, String> {
    let global = MANIS_GLOBAL_GROUP_NAME.to_owned();
    let mut rules = Vec::new();
    let mut final_seen = false;
    for (index, entry) in entries.iter().enumerate() {
        if !entry.enabled {
            continue;
        }
        let number = index + 1;
        let policy = match resolve_policy(&entry.policy, group_names, &global) {
            Some(policy) => policy,
            None => {
                warnings.push(format!(
                    "规则 {number}：策略 {} 不存在，已改走全局组",
                    entry.policy
                ));
                PolicyRef::Group(
                    Name::parse(&global).map_err(|error| format!("全局组名称无效：{error}"))?,
                )
            }
        };
        let kind = entry.kind.trim().to_ascii_uppercase();
        if kind == "FINAL" || kind == "MATCH" {
            if final_seen {
                warnings.push(format!("规则 {number}：最终规则重复，已忽略"));
                continue;
            }
            final_seen = true;
            rules.push(Rule::Match { policy });
            continue;
        }
        if final_seen {
            warnings.push(format!("规则 {number}：位于最终规则之后，永远不会生效"));
        }
        let value = entry.value.trim();
        if value.is_empty()
            || value.len() > 1_024
            || value.contains(',')
            || value.chars().any(char::is_control)
        {
            warnings.push(format!("规则 {number}：匹配内容无效，已忽略"));
            continue;
        }
        let rule = match kind.as_str() {
            "DOMAIN" | "HOST" => Rule::Domain {
                value: value.to_owned(),
                policy,
            },
            "DOMAIN-SUFFIX" | "HOST-SUFFIX" => Rule::DomainSuffix {
                value: value.to_owned(),
                policy,
            },
            "DOMAIN-KEYWORD" | "HOST-KEYWORD" => Rule::DomainKeyword {
                value: value.to_owned(),
                policy,
            },
            "DOMAIN-WILDCARD" | "HOST-WILDCARD" => Rule::DomainWildcard {
                value: value.to_owned(),
                policy,
            },
            "IP-CIDR" | "IP6-CIDR" => {
                if !value.contains('/') {
                    warnings.push(format!("规则 {number}：CIDR 缺少前缀长度，已忽略"));
                    continue;
                }
                Rule::IpCidr {
                    value: value.to_owned(),
                    policy,
                    no_resolve: true,
                }
            }
            "GEOIP" => {
                if value.len() != 2
                    || !value
                        .chars()
                        .all(|character| character.is_ascii_alphabetic())
                {
                    warnings.push(format!("规则 {number}：国家代码无效，已忽略"));
                    continue;
                }
                Rule::GeoIp {
                    country: value.to_ascii_uppercase(),
                    policy,
                    no_resolve: false,
                }
            }
            "IP-ASN" => {
                let Ok(asn) = value
                    .trim_start_matches("AS")
                    .trim_start_matches("as")
                    .parse::<u32>()
                else {
                    warnings.push(format!("规则 {number}：ASN 无效，已忽略"));
                    continue;
                };
                Rule::IpAsn {
                    asn,
                    policy,
                    no_resolve: true,
                }
            }
            "DST-PORT" => {
                let Ok(port) = value.parse::<u16>() else {
                    warnings.push(format!("规则 {number}：端口无效，已忽略"));
                    continue;
                };
                Rule::DstPort { port, policy }
            }
            _ => {
                warnings.push(format!("规则 {number}：不支持的类型 {}", entry.kind));
                continue;
            }
        };
        rules.push(rule);
    }
    Ok(rules)
}

fn resolve_policy(raw: &str, group_names: &[String], global: &str) -> Option<PolicyRef> {
    let raw = raw.trim();
    if raw.eq_ignore_ascii_case("DIRECT") {
        return Some(PolicyRef::Direct);
    }
    if raw.eq_ignore_ascii_case("REJECT") {
        return Some(PolicyRef::Reject);
    }
    if raw.eq_ignore_ascii_case("PROXY") || raw.eq_ignore_ascii_case("GLOBAL") {
        return Name::parse(global).ok().map(PolicyRef::Group);
    }
    group_names
        .iter()
        .find(|name| name.eq_ignore_ascii_case(raw))
        .and_then(|name| Name::parse(name).ok())
        .map(PolicyRef::Group)
}

/// Legacy path for workspaces written before the structured editor.
fn compile_qx_rules(
    source: &str,
    group_names: &[String],
    warnings: &mut Vec<String>,
) -> Result<Vec<Rule>, String> {
    if source.trim().is_empty() {
        return Ok(Vec::new());
    }
    let parsed = QxRuleList::parse(source);
    for diagnostic in &parsed.diagnostics {
        warnings.push(format!(
            "规则第 {} 行：{}",
            diagnostic.line_number, diagnostic.detail
        ));
    }
    let global = MANIS_GLOBAL_GROUP_NAME.to_owned();
    parsed
        .to_profile_rules(|policy| {
            let raw = policy.as_str();
            if let Some(policy) = resolve_policy(raw, group_names, &global) {
                return Some(policy);
            }
            warnings.push(format!("规则策略 {raw} 不存在，已改走全局组"));
            Name::parse(&global).ok().map(PolicyRef::Group)
        })
        .map_err(|error| format!("规则解析失败：{error}"))
}

fn parse_mode(value: &str) -> ProfileMode {
    match value.trim().to_ascii_lowercase().as_str() {
        "global" => ProfileMode::Global,
        "direct" => ProfileMode::Direct,
        _ => ProfileMode::Rule,
    }
}

#[cfg(test)]
mod tests {
    use super::compile;

    fn config() -> String {
        serde_json::json!({
            "mode": "rule",
            "mixed_port": 7890,
            "subscriptions": [{
                "name": "测试订阅",
                "url": "https://example.com/sub.yaml",
                "enabled": true
            }],
            "nodes": [{
                "link": "vless://00000000-0000-4000-8000-000000000000@edge.example.invalid:443?encryption=none&security=tls&type=ws&path=%2Fmanis#手动节点",
                "enabled": true
            }],
            "groups": [{
                "name": "自动选择",
                "strategy": "urltest",
                "subscription_indexes": [0],
                "node_indexes": [0]
            }],
            "rules": "HOST-SUFFIX,example.com,DIRECT\nHOST-KEYWORD,test,自动选择"
        })
        .to_string()
    }

    #[test]
    fn compiles_the_same_profile_shape_as_desktop() {
        let envelope = compile(&config(), 3).expect("config compiles");
        let value: serde_json::Value = serde_json::from_str(&envelope).expect("envelope is json");
        let yaml = value["yaml"].as_str().expect("yaml is present");

        assert!(yaml.contains("file-descriptor: 3"));
        assert!(yaml.contains("MATCH,__MANIS_GLOBAL__"));
        assert!(yaml.contains("\"自动选择\""));
        assert!(yaml.contains("url-test"));
        assert!(yaml.contains("DOMAIN-SUFFIX,example.com,DIRECT"));
        assert_eq!(value["providers"], 1);
        assert_eq!(value["nodes"], 1);
        assert_eq!(value["groups"], 2); // the user group plus the hidden global selector
    }

    #[test]
    fn reports_a_missing_policy_and_falls_back_to_global() {
        let mut value: serde_json::Value = serde_json::from_str(&config()).expect("fixture parses");
        value["rules"] = serde_json::json!("HOST-SUFFIX,example.com,不存在的组");
        let envelope = compile(&value.to_string(), 3).expect("config compiles");
        let result: serde_json::Value = serde_json::from_str(&envelope).expect("json");
        let warnings = result["warnings"].as_array().expect("warnings");
        assert!(
            warnings
                .iter()
                .any(|warning| warning.as_str().unwrap_or_default().contains("不存在的组"))
        );
    }

    #[test]
    fn rejects_an_empty_workspace() {
        let error = compile("{}", 3).expect_err("empty workspace fails");
        assert!(error.contains("订阅或节点"));
    }

    #[test]
    fn maps_direct_and_reject_policies() {
        let mut value: serde_json::Value = serde_json::from_str(&config()).expect("fixture parses");
        value["rules"] = serde_json::json!("HOST-SUFFIX,cn,DIRECT\nHOST-SUFFIX,ads.example,REJECT");
        let envelope = compile(&value.to_string(), 3).expect("config compiles");
        let result: serde_json::Value = serde_json::from_str(&envelope).expect("json");
        let yaml = result["yaml"].as_str().expect("yaml");
        assert!(yaml.contains("DOMAIN-SUFFIX,cn,DIRECT"));
        assert!(yaml.contains("DOMAIN-SUFFIX,ads.example,REJECT"));
    }

    #[test]
    fn compiles_structured_rules_with_desktop_kinds() {
        let mut value: serde_json::Value = serde_json::from_str(&config()).expect("fixture parses");
        value["subscriptions"][0]["cache_path"] =
            serde_json::json!("./proxy_providers/sub-test.yaml");
        value["rule_entries"] = serde_json::json!([
            {"kind": "DOMAIN-SUFFIX", "value": "example.com", "policy": "DIRECT"},
            {"kind": "IP-CIDR", "value": "10.0.0.0/8", "policy": "REJECT"},
            {"kind": "GEOIP", "value": "cn", "policy": "DIRECT"},
            {"kind": "DST-PORT", "value": "443", "policy": "自动选择"},
            {"kind": "FINAL", "value": "", "policy": "DIRECT"}
        ]);
        let envelope = compile(&value.to_string(), 3).expect("config compiles");
        let result: serde_json::Value = serde_json::from_str(&envelope).expect("json");
        let yaml = result["yaml"].as_str().expect("yaml");
        assert!(yaml.contains("path: \"./proxy_providers/sub-test.yaml\""));
        assert!(yaml.contains("DOMAIN-SUFFIX,example.com,DIRECT"));
        assert!(yaml.contains("IP-CIDR,10.0.0.0/8,REJECT,no-resolve"));
        assert!(yaml.contains("GEOIP,CN,DIRECT"));
        assert!(yaml.contains("DST-PORT,443,"));
        assert!(yaml.contains("MATCH,DIRECT"));
    }
}
