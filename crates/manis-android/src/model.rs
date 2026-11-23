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
    /// User rule groups; every rule whose `group_id` names one routes to that group's policy.
    ///
    /// Absent or empty keeps the single-list behaviour of workspaces written before groups.
    #[serde(default)]
    pub rule_groups: Vec<MobileRuleGroup>,
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
    /// Id of the [`MobileRuleGroup`] this rule belongs to; unset means the manual list.
    #[serde(default)]
    pub group_id: Option<String>,
}

/// A user rule group: one policy for every rule whose `group_id` names this group.
///
/// The name labels the group in warnings only, so it never reaches the rendered config.
#[derive(Debug, Deserialize)]
pub struct MobileRuleGroup {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
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
    let blocks = plan_rule_blocks(
        &config.rule_groups,
        &config.rule_entries,
        group_names,
        warnings,
    )?;
    let mut rules = Vec::new();
    for block in blocks {
        match block {
            RuleBlock::Group { policy, entries } => {
                rules.extend(compile_group_block(&entries, policy, warnings))
            }
            RuleBlock::Manual(entries) => {
                rules.extend(compile_manual_block(&entries, group_names, warnings)?);
            }
        }
    }
    Ok(rules)
}

/// One ordered block of the compiled rule list.
///
/// Group blocks come first, in `rule_groups` order, and every rule inside one carries that
/// group's resolved policy. The manual block comes last and each rule keeps its own policy.
/// Each entry keeps its position in `rule_entries` so warnings still name the row the user sees.
enum RuleBlock<'a> {
    Group {
        policy: PolicyRef,
        entries: Vec<(usize, &'a MobileRule)>,
    },
    Manual(Vec<(usize, &'a MobileRule)>),
}

/// A group that passed validation and can own rules.
struct PlannedGroup {
    id: String,
    name: String,
    policy: PolicyRef,
}

/// Where one rule row belongs, decided before any rule is converted.
enum RuleOwner {
    /// Index into the planned group blocks.
    Group(usize),
    /// The named group exists but is turned off, so its rules are off with it.
    DisabledGroup,
    /// No group names this row, or it names one that does not exist.
    Manual,
}

/// Sorts the enabled rule rows into group blocks followed by the manual block.
///
/// Every group-level diagnostic is emitted here, so emitting a block only has to convert kinds.
/// A row joins the first group whose id matches, and a row whose id names no group is manual.
fn plan_rule_blocks<'a>(
    groups: &[MobileRuleGroup],
    entries: &'a [MobileRule],
    group_names: &[String],
    warnings: &mut Vec<String>,
) -> Result<Vec<RuleBlock<'a>>, String> {
    let global = MANIS_GLOBAL_GROUP_NAME.to_owned();
    let mut planned: Vec<PlannedGroup> = Vec::new();
    let mut disabled: Vec<String> = Vec::new();
    for (position, group) in groups.iter().enumerate() {
        let name = group.name.trim();
        let policy = group.policy.trim();
        let id = group.id.trim();
        if name.is_empty() || id.is_empty() || policy.is_empty() {
            warnings.push(format!(
                "规则组 {}：名称、ID 或策略为空，已忽略",
                position + 1
            ));
            continue;
        }
        if !group.enabled {
            disabled.push(id.to_owned());
            continue;
        }
        if planned.iter().any(|existing| existing.name == name) {
            warnings.push(format!("规则组 {name}：名称重复，规则仍按 ID 生效"));
        }
        let policy = match resolve_policy(policy, group_names, &global) {
            Some(policy) => policy,
            None => {
                warnings.push(format!("规则组 {name}：策略 {policy} 不存在，已改走全局组"));
                PolicyRef::Group(
                    Name::parse(&global).map_err(|error| format!("全局组名称无效：{error}"))?,
                )
            }
        };
        planned.push(PlannedGroup {
            id: id.to_owned(),
            name: name.to_owned(),
            policy,
        });
    }

    let mut collected: Vec<Vec<(usize, &MobileRule)>> = vec![Vec::new(); planned.len()];
    let mut manual: Vec<(usize, &MobileRule)> = Vec::new();
    for (index, entry) in entries.iter().enumerate() {
        if !entry.enabled {
            continue;
        }
        match rule_owner(entry, &planned, &disabled) {
            RuleOwner::Group(owner) => {
                if is_final_rule(entry) {
                    warnings.push(format!("规则组 {} 内的最终规则已忽略", planned[owner].name));
                } else {
                    collected[owner].push((index, entry));
                }
            }
            RuleOwner::DisabledGroup => {}
            RuleOwner::Manual => manual.push((index, entry)),
        }
    }

    let mut blocks = Vec::with_capacity(planned.len() + 1);
    for (group, group_entries) in planned.into_iter().zip(collected) {
        blocks.push(RuleBlock::Group {
            policy: group.policy,
            entries: group_entries,
        });
    }
    blocks.push(RuleBlock::Manual(manual));
    Ok(blocks)
}

fn rule_owner(entry: &MobileRule, planned: &[PlannedGroup], disabled: &[String]) -> RuleOwner {
    let Some(id) = entry
        .group_id
        .as_deref()
        .map(str::trim)
        .filter(|id| !id.is_empty())
    else {
        return RuleOwner::Manual;
    };
    if let Some(owner) = planned.iter().position(|group| group.id == id) {
        return RuleOwner::Group(owner);
    }
    if disabled.iter().any(|group| group == id) {
        return RuleOwner::DisabledGroup;
    }
    RuleOwner::Manual
}

fn is_final_rule(entry: &MobileRule) -> bool {
    let kind = entry.kind.trim();
    kind.eq_ignore_ascii_case("FINAL") || kind.eq_ignore_ascii_case("MATCH")
}

/// Compiles a group block, pinning every rule to the group's one policy.
fn compile_group_block(
    entries: &[(usize, &MobileRule)],
    policy: PolicyRef,
    warnings: &mut Vec<String>,
) -> Vec<Rule> {
    entries
        .iter()
        .filter_map(|(index, entry)| rule_from_entry(entry, index + 1, policy.clone(), warnings))
        .collect()
}

/// Compiles the manual block, where each rule keeps its own policy and the final rule anchors.
fn compile_manual_block(
    entries: &[(usize, &MobileRule)],
    group_names: &[String],
    warnings: &mut Vec<String>,
) -> Result<Vec<Rule>, String> {
    let global = MANIS_GLOBAL_GROUP_NAME.to_owned();
    let mut rules = Vec::new();
    let mut final_seen = false;
    for (index, entry) in entries {
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
        if is_final_rule(entry) {
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
        if let Some(rule) = rule_from_entry(entry, number, policy, warnings) {
            rules.push(rule);
        }
    }
    Ok(rules)
}

/// Converts one enabled rule row with its policy already resolved.
///
/// Group and manual rules share this so both accept exactly the same kinds. Returns `None`
/// after pushing a warning for a row that cannot become a rule.
fn rule_from_entry(
    entry: &MobileRule,
    number: usize,
    policy: PolicyRef,
    warnings: &mut Vec<String>,
) -> Option<Rule> {
    let kind = entry.kind.trim().to_ascii_uppercase();
    let value = entry.value.trim();
    if value.is_empty()
        || value.len() > 1_024
        || value.contains(',')
        || value.chars().any(char::is_control)
    {
        warnings.push(format!("规则 {number}：匹配内容无效，已忽略"));
        return None;
    }
    Some(match kind.as_str() {
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
                return None;
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
                return None;
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
                return None;
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
                return None;
            };
            Rule::DstPort { port, policy }
        }
        _ => {
            warnings.push(format!("规则 {number}：不支持的类型 {}", entry.kind));
            return None;
        }
    })
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
    fn a_keyword_group_reaches_the_core_as_a_filter() {
        let mut value: serde_json::Value = serde_json::from_str(&config()).expect("fixture parses");
        value["groups"] = serde_json::json!([{
            "name": "香港",
            "strategy": "select",
            "subscription_indexes": [0],
            "node_indexes": [],
            "filter": "^(HK 01|HK 02)$"
        }]);
        let envelope = compile(&value.to_string(), 3).expect("config compiles");
        let result: serde_json::Value = serde_json::from_str(&envelope).expect("json");
        let yaml = result["yaml"].as_str().expect("yaml");
        assert!(
            yaml.contains("filter:"),
            "the group filter is missing from the rendered profile:\n{yaml}"
        );
        assert!(yaml.contains("HK 01"));
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

    fn fixture() -> serde_json::Value {
        serde_json::from_str(&config()).expect("fixture parses")
    }

    /// The rendered config plus its warnings, which is all the app can observe.
    fn compiled(value: &serde_json::Value) -> (String, Vec<String>) {
        let envelope = compile(&value.to_string(), 3).expect("config compiles");
        let result: serde_json::Value = serde_json::from_str(&envelope).expect("json");
        let yaml = result["yaml"].as_str().expect("yaml").to_owned();
        let warnings = result["warnings"]
            .as_array()
            .expect("warnings")
            .iter()
            .map(|warning| warning.as_str().unwrap_or_default().to_owned())
            .collect();
        (yaml, warnings)
    }

    /// Asserts these fragments appear in this order, which is the order the core evaluates.
    fn assert_in_order(yaml: &str, fragments: &[&str]) {
        let mut cursor = 0;
        for fragment in fragments {
            let found = yaml[cursor..].find(fragment).unwrap_or_else(|| {
                panic!("{fragment} is missing from the compiled rules:\n{yaml}")
            });
            cursor += found + fragment.len();
        }
    }

    fn desktop_kind_entries() -> serde_json::Value {
        serde_json::json!([
            {"kind": "DOMAIN-SUFFIX", "value": "example.com", "policy": "DIRECT"},
            {"kind": "IP-CIDR", "value": "10.0.0.0/8", "policy": "REJECT"},
            {"kind": "GEOIP", "value": "cn", "policy": "DIRECT"},
            {"kind": "DST-PORT", "value": "443", "policy": "自动选择"},
            {"kind": "FINAL", "value": "", "policy": "DIRECT"}
        ])
    }

    #[test]
    fn a_group_compiles_before_the_manual_list_and_pins_the_group_policy() {
        let mut value = fixture();
        value["rule_groups"] = serde_json::json!([
            {"id": "rg-1", "name": "工作", "policy": "自动选择", "enabled": true}
        ]);
        value["rule_entries"] = serde_json::json!([
            {"kind": "DOMAIN-SUFFIX", "value": "corp.example", "policy": "DIRECT", "group_id": "rg-1"},
            {"kind": "DOMAIN-SUFFIX", "value": "example.com", "policy": "REJECT"},
            {"kind": "DOMAIN-KEYWORD", "value": "intranet", "policy": "REJECT", "group_id": "rg-1"}
        ]);

        let (yaml, warnings) = compiled(&value);

        assert_in_order(
            &yaml,
            &[
                "DOMAIN-SUFFIX,corp.example,自动选择",
                "DOMAIN-KEYWORD,intranet,自动选择",
                "DOMAIN-SUFFIX,example.com,REJECT",
            ],
        );
        assert!(
            !yaml.contains("DOMAIN-SUFFIX,corp.example,DIRECT"),
            "a group rule must ignore its own policy:\n{yaml}"
        );
        assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
    }

    #[test]
    fn a_disabled_group_contributes_no_rules() {
        let mut value = fixture();
        value["rule_groups"] = serde_json::json!([
            {"id": "rg-1", "name": "工作", "policy": "自动选择", "enabled": false}
        ]);
        value["rule_entries"] = serde_json::json!([
            {"kind": "DOMAIN-SUFFIX", "value": "corp.example", "policy": "DIRECT", "group_id": "rg-1"},
            {"kind": "DOMAIN-SUFFIX", "value": "example.com", "policy": "REJECT"}
        ]);

        let (yaml, warnings) = compiled(&value);

        assert!(
            !yaml.contains("corp.example"),
            "a disabled group must not contribute rules:\n{yaml}"
        );
        assert!(yaml.contains("DOMAIN-SUFFIX,example.com,REJECT"));
        assert_eq!(warnings, Vec::<String>::new());
    }

    #[test]
    fn a_rule_naming_no_group_stays_in_the_manual_list() {
        let mut value = fixture();
        value["rule_groups"] = serde_json::json!([
            {"id": "rg-1", "name": "工作", "policy": "自动选择", "enabled": true}
        ]);
        value["rule_entries"] = serde_json::json!([
            {"kind": "DOMAIN-SUFFIX", "value": "unknown.example", "policy": "DIRECT", "group_id": "rg-gone"},
            {"kind": "DOMAIN-SUFFIX", "value": "empty.example", "policy": "DIRECT", "group_id": ""},
            {"kind": "DOMAIN-SUFFIX", "value": "absent.example", "policy": "DIRECT"},
            {"kind": "DOMAIN-SUFFIX", "value": "corp.example", "policy": "REJECT", "group_id": "rg-1"}
        ]);

        let (yaml, warnings) = compiled(&value);

        assert_in_order(
            &yaml,
            &[
                "DOMAIN-SUFFIX,corp.example,自动选择",
                "DOMAIN-SUFFIX,unknown.example,DIRECT",
                "DOMAIN-SUFFIX,empty.example,DIRECT",
                "DOMAIN-SUFFIX,absent.example,DIRECT",
            ],
        );
        assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
    }

    #[test]
    fn a_group_target_that_names_no_policy_falls_back_to_the_global_group() {
        let mut value = fixture();
        value["rule_groups"] = serde_json::json!([
            {"id": "rg-1", "name": "工作", "policy": "不存在的组", "enabled": true}
        ]);
        value["rule_entries"] = serde_json::json!([
            {"kind": "DOMAIN-SUFFIX", "value": "corp.example", "policy": "REJECT", "group_id": "rg-1"}
        ]);

        let (yaml, warnings) = compiled(&value);

        assert!(
            yaml.contains("DOMAIN-SUFFIX,corp.example,__MANIS_GLOBAL__"),
            "the group must fall back to the global group:\n{yaml}"
        );
        assert!(
            warnings
                .iter()
                .any(|warning| warning.contains("规则组 工作：策略 不存在的组 不存在")),
            "missing group policy warning: {warnings:?}"
        );
    }

    #[test]
    fn a_final_rule_inside_a_group_is_skipped() {
        let mut value = fixture();
        value["rule_groups"] = serde_json::json!([
            {"id": "rg-1", "name": "工作", "policy": "自动选择", "enabled": true}
        ]);
        value["rule_entries"] = serde_json::json!([
            {"kind": "FINAL", "value": "", "policy": "自动选择", "group_id": "rg-1"},
            {"kind": "DOMAIN-SUFFIX", "value": "corp.example", "policy": "REJECT", "group_id": "rg-1"},
            {"kind": "FINAL", "value": "", "policy": "DIRECT"}
        ]);

        let (yaml, warnings) = compiled(&value);

        assert!(
            warnings
                .iter()
                .any(|warning| warning == "规则组 工作 内的最终规则已忽略"),
            "missing skipped-final warning: {warnings:?}"
        );
        assert!(yaml.contains("DOMAIN-SUFFIX,corp.example,自动选择"));
        assert_eq!(
            yaml.matches("MATCH,").count(),
            1,
            "only the manual final rule may anchor the list:\n{yaml}"
        );
        assert!(yaml.contains("MATCH,DIRECT"));
    }

    #[test]
    fn a_workspace_without_rule_groups_compiles_the_same_rules() {
        let mut value = fixture();
        value["rule_entries"] = desktop_kind_entries();
        let mut explicit_empty = value.clone();
        explicit_empty["rule_groups"] = serde_json::json!([]);

        let (yaml, warnings) = compiled(&value);
        let (empty_yaml, empty_warnings) = compiled(&explicit_empty);

        assert_eq!(
            yaml, empty_yaml,
            "an absent or empty group list must not change the config"
        );
        assert_eq!(warnings, empty_warnings);
        assert!(yaml.contains("DOMAIN-SUFFIX,example.com,DIRECT"));
        assert!(yaml.contains("IP-CIDR,10.0.0.0/8,REJECT,no-resolve"));
        assert!(yaml.contains("GEOIP,CN,DIRECT"));
        assert!(yaml.contains("DST-PORT,443,"));
        assert!(yaml.contains("MATCH,DIRECT"));
    }

    #[test]
    fn duplicate_group_names_warn_and_each_group_keeps_its_own_policy() {
        let mut value = fixture();
        value["rule_groups"] = serde_json::json!([
            {"id": "rg-1", "name": "工作", "policy": "DIRECT", "enabled": true},
            {"id": "rg-2", "name": "工作", "policy": "REJECT", "enabled": true}
        ]);
        value["rule_entries"] = serde_json::json!([
            {"kind": "DOMAIN-SUFFIX", "value": "direct.example", "policy": "自动选择", "group_id": "rg-1"},
            {"kind": "DOMAIN-SUFFIX", "value": "reject.example", "policy": "自动选择", "group_id": "rg-2"}
        ]);

        let (yaml, warnings) = compiled(&value);

        assert_in_order(
            &yaml,
            &[
                "DOMAIN-SUFFIX,direct.example,DIRECT",
                "DOMAIN-SUFFIX,reject.example,REJECT",
            ],
        );
        assert!(
            warnings
                .iter()
                .any(|warning| warning.contains("规则组 工作：名称重复")),
            "missing duplicate name warning: {warnings:?}"
        );
    }

    #[test]
    fn a_group_with_a_blank_name_policy_or_id_is_skipped_with_a_warning() {
        let mut value = fixture();
        value["rule_groups"] = serde_json::json!([
            {"id": "rg-1", "name": "  ", "policy": "DIRECT", "enabled": true},
            {"id": "rg-2", "name": "工作", "policy": "", "enabled": true},
            {"id": "rg-3", "name": "出差", "policy": "DIRECT", "enabled": true}
        ]);
        value["rule_entries"] = serde_json::json!([
            {"kind": "DOMAIN-SUFFIX", "value": "corp.example", "policy": "REJECT", "group_id": "rg-3"}
        ]);

        let (yaml, warnings) = compiled(&value);

        assert!(yaml.contains("DOMAIN-SUFFIX,corp.example,DIRECT"));
        assert!(
            warnings
                .iter()
                .any(|warning| warning.contains("规则组 1：名称、ID 或策略为空")),
            "missing blank group warning: {warnings:?}"
        );
        assert!(
            warnings
                .iter()
                .any(|warning| warning.contains("规则组 2：名称、ID 或策略为空")),
            "missing blank policy warning: {warnings:?}"
        );
    }

    #[test]
    fn warnings_keep_the_rule_entry_number_for_group_and_manual_rows() {
        let mut value = fixture();
        value["rule_groups"] = serde_json::json!([
            {"id": "rg-1", "name": "工作", "policy": "DIRECT", "enabled": true}
        ]);
        value["rule_entries"] = serde_json::json!([
            {"kind": "DOMAIN-SUFFIX", "value": "corp.example", "policy": "自动选择", "group_id": "rg-1"},
            {"kind": "DOMAIN-SUFFIX", "value": "bad,value", "policy": "自动选择", "group_id": "rg-1"},
            {"kind": "NOPE", "value": "anything", "policy": "DIRECT"}
        ]);

        let (_, warnings) = compiled(&value);

        assert!(
            warnings
                .iter()
                .any(|warning| warning == "规则 2：匹配内容无效，已忽略"),
            "a group row keeps its index in rule_entries: {warnings:?}"
        );
        assert!(
            warnings
                .iter()
                .any(|warning| warning == "规则 3：不支持的类型 NOPE"),
            "a manual row keeps its index in rule_entries: {warnings:?}"
        );
    }
}
