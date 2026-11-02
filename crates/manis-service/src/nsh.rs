//! Desktop bridge shared by every NSH shell.
//!
//! The `GTK`, `AppKit` and `Win32` hosts call these functions for the root payload
//! and every page command. Nothing here depends on a UI toolkit, so the bridge
//! is identical on every platform and testable without a window.

use std::path::Path;

use crate::mihomo;
use crate::stored_workspace::StoredWorkspace;

/// Read-only root payload for the desktop NSH host.
///
/// It reuses the desktop storage loader so the NSH shell sees the same saved
/// subscriptions, nodes, groups and routing mode. Connection state and writes
/// still belong to the running Manis app.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn nsh_root_data() -> serde_json::Value {
    use serde_json::json;

    let directory = crate::brand::config_dir();
    let workspace = StoredWorkspace::load(directory.as_deref());
    let subscriptions_page = manis_presentation::SubscriptionsPageView::build(
        manis_presentation::SubscriptionsPageInput {
            subscriptions: workspace
                .imported_subscriptions
                .iter()
                .map(|subscription| manis_presentation::SubscriptionRowInput {
                    name: subscription.name.clone(),
                    url: subscription.source.expose_to(manis_presentation::mask_url),
                    enabled: subscription.enabled,
                    id: Some(subscription.id.clone()),
                })
                .collect(),
            nodes: workspace
                .saved_single_nodes
                .iter()
                .map(|node| manis_presentation::SubscriptionNodeInput {
                    name: node.name.clone(),
                    enabled: node.enabled,
                    id: Some(node.id.clone()),
                })
                .collect(),
            groups: workspace
                .managed_policy_groups
                .iter()
                .map(|group| {
                    let (subscription_indexes, node_indexes) = group_source_indexes(
                        group,
                        &workspace.imported_subscriptions,
                        &workspace.saved_single_nodes,
                    );
                    manis_presentation::SubscriptionGroupInput {
                        name: group.name.clone(),
                        strategy: match group.strategy {
                            manis_core::ManagedPolicyStrategy::LowestLatency => "urltest".into(),
                            manis_core::ManagedPolicyStrategy::Manual => "select".into(),
                        },
                        subscription_indexes,
                        node_indexes,
                    }
                })
                .collect(),
        },
    );
    let live = live_snapshot();
    let active = live
        .as_ref()
        .is_some_and(|snapshot| snapshot.runtime.tun.enable);
    let global = live.as_ref().and_then(|snapshot| {
        snapshot.policy_groups().into_iter().find(|group| {
            group.name == "__MANIS_GLOBAL__" || group.name.eq_ignore_ascii_case("GLOBAL")
        })
    });
    let live_node = global.and_then(|group| group.current).unwrap_or_default();
    let mode = live.as_ref().map_or_else(
        || workspace.routing_mode.wire_value(),
        |snapshot| snapshot.runtime.mode.wire_value(),
    );
    let kernel_version = live
        .as_ref()
        .and_then(|snapshot| snapshot.version.version.clone())
        .filter(|version| !version.is_empty())
        .unwrap_or_else(|| "—".into());
    let summary = live.as_ref().map_or_else(
        || "—".into(),
        |snapshot| {
            format!(
                "{} 个代理 · {} 条规则",
                snapshot.proxies.len(),
                snapshot.rules.len()
            )
        },
    );
    let subscription_count = workspace.imported_subscriptions.len();
    let node_count = workspace.saved_single_nodes.len();
    let group_count = workspace.managed_policy_groups.len();
    let has_sources = subscription_count > 0 || node_count > 0;

    let node_page = live.as_ref().map_or_else(
        || {
            json!({
                "global": { "label": "全局", "current": null, "total": 0 },
                "sources": [],
                "policyGroups": []
            })
        },
        |snapshot| nsh_node_page(snapshot, &workspace, &live_node),
    );
    let dns = directory
        .as_deref()
        .map_or_else(mihomo::ProxyDnsSettings::default, mihomo::load_proxy_dns_in);
    let manual_rules = directory
        .as_deref()
        .and_then(|directory| crate::manual_rule::load_manual_rules_in(directory).ok())
        .unwrap_or_default();
    let rules_page = live.as_ref().map_or_else(
        || {
            json!({
                "rules": [],
                "kinds": [
                    "DOMAIN-SUFFIX", "DOMAIN", "DOMAIN-KEYWORD", "DOMAIN-WILDCARD",
                    "IP-CIDR", "IP6-CIDR", "GEOIP", "IP-ASN", "DST-PORT", "FINAL"
                ],
                "policies": ["DIRECT", "REJECT", "__MANIS_GLOBAL__"],
                "warnings": [],
                "summary": ""
            })
        },
        |snapshot| nsh_rules_page(snapshot, &manual_rules),
    );

    json!({
        "pending": false,
        "nodes": node_page,
        "subscriptions": serde_json::to_value(subscriptions_page).unwrap_or_else(|_| json!({})),
        "rules": rules_page,
        "settings": serde_json::to_value(manis_presentation::SettingsPageView::build(
            manis_presentation::SettingsPageInput {
                status: if active { "已连接" } else { "未连接" }.into(),
                mode: mode.to_owned(),
                proxy_dns: manis_presentation::SettingsProxyDns {
                    mode: dns.mode.as_str().into(),
                    custom: dns.servers.clone(),
                    discovered: discovered_proxy_dns(&workspace),
                    label: dns.mode.label().into(),
                },
                kernel_version,
                summary: summary.clone(),
                subscriptions: subscription_count,
                nodes: node_count,
                groups: group_count,
                abi: "desktop".into(),
                data_dir: directory.map_or_else(|| "—".into(), |path| path.display().to_string()),
                installed_version: crate::version().to_owned(),
                auto_start: Some(auto_start_enabled()),
                update: manis_presentation::SettingsUpdate::default(),
            }
        ))
        .unwrap_or_else(|_| json!({})),
        "connection": serde_json::to_value(manis_presentation::ConnectionPageView::build(
            manis_presentation::ConnectionPageInput {
                active,
                node: live_node,
                mode: mode.to_owned(),
                summary: summary.clone(),
                error: String::new(),
                unreachable: false,
                warnings: Vec::new(),
                warning_count: 0,
                has_sources,
            }
        ))
        .unwrap_or_else(|_| json!({}))
    })
}

/// Path of the desktop controller socket, for platform shells that start or
/// probe the headless engine before connecting.
#[must_use]
pub fn nsh_controller_socket() -> Option<std::path::PathBuf> {
    #[cfg(unix)]
    {
        crate::brand::data_dir().map(|directory| directory.join("mihomo/controller.sock"))
    }
    #[cfg(not(unix))]
    {
        None
    }
}

#[cfg(unix)]
fn live_client() -> Option<manis_mihomo::MihomoClient<manis_mihomo::UnixSocketTransport>> {
    let socket = crate::brand::data_dir()?.join("mihomo/controller.sock");
    if !socket.exists() {
        return None;
    }
    Some(manis_mihomo::MihomoClient::new(
        manis_mihomo::ControllerConfig::default(),
        manis_mihomo::UnixSocketTransport::new(socket),
    ))
}

#[cfg(windows)]
fn live_client() -> Option<manis_mihomo::MihomoClient<manis_mihomo::NamedPipeTransport>> {
    Some(manis_mihomo::MihomoClient::new(
        manis_mihomo::ControllerConfig::default(),
        manis_mihomo::NamedPipeTransport::new(r"\\.\pipe\manis-mihomo"),
    ))
}

#[cfg(any(unix, windows))]
fn live_snapshot() -> Option<manis_mihomo::MihomoSnapshot> {
    live_client()?.fetch_snapshot().ok()
}

#[cfg(not(any(unix, windows)))]
fn live_snapshot() -> Option<manis_mihomo::MihomoSnapshot> {
    None
}

/// Selects a node in the live desktop controller's global group.
///
/// # Errors
/// Returns a controller error when no live controller or group exists.
pub fn nsh_select_global_node(name: &str) -> Result<(), String> {
    #[cfg(any(unix, windows))]
    {
        let client = live_client().ok_or_else(|| "桌面内核未运行".to_owned())?;
        let snapshot = client.fetch_snapshot().map_err(|error| error.to_string())?;
        let group = snapshot
            .policy_groups()
            .into_iter()
            .find(|group| {
                group.name == "__MANIS_GLOBAL__" || group.name.eq_ignore_ascii_case("GLOBAL")
            })
            .ok_or_else(|| "找不到全局策略组".to_owned())?;
        client
            .select_policy_group_node(&group.name, name)
            .map_err(|error| error.to_string())
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = name;
        Err("桌面控制器只在 Unix/Windows 上接入".to_owned())
    }
}

/// Selects a node in one live desktop policy group.
///
/// # Errors
/// Returns a controller error when no live controller exists.
pub fn nsh_select_group_node(group: &str, name: &str) -> Result<(), String> {
    #[cfg(any(unix, windows))]
    {
        live_client()
            .ok_or_else(|| "桌面内核未运行".to_owned())?
            .select_policy_group_node(group, name)
            .map_err(|error| error.to_string())
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (group, name);
        Err("桌面控制器只在 Unix/Windows 上接入".to_owned())
    }
}

/// Enables or disables the live desktop controller's TUN interface.
///
/// # Errors
/// Returns a controller error when no live controller exists.
pub fn nsh_set_tun(enabled: bool) -> Result<(), String> {
    #[cfg(any(unix, windows))]
    {
        live_client()
            .ok_or_else(|| "桌面内核未运行".to_owned())?
            .set_tun_enabled(enabled)
            .map_err(|error| error.to_string())
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = enabled;
        Err("桌面控制器只在 Unix/Windows 上接入".to_owned())
    }
}

/// Switches the live desktop controller's routing mode.
///
/// # Errors
/// Returns a controller error for an unknown mode or unavailable controller.
pub fn nsh_set_routing_mode(mode: &str) -> Result<(), String> {
    #[cfg(any(unix, windows))]
    {
        let mode = manis_mihomo::RoutingMode::parse_wire_value(mode)
            .ok_or_else(|| "未知路由模式".to_owned())?;
        live_client()
            .ok_or_else(|| "桌面内核未运行".to_owned())?
            .set_routing_mode(mode)
            .map_err(|error| error.to_string())
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = mode;
        Err("桌面控制器只在 Unix/Windows 上接入".to_owned())
    }
}

#[allow(clippy::too_many_lines)]
fn nsh_node_page(
    snapshot: &manis_mihomo::MihomoSnapshot,
    workspace: &StoredWorkspace,
    global_current: &str,
) -> serde_json::Value {
    use serde_json::json;

    let delay = |name: &str| {
        snapshot
            .proxies
            .iter()
            .find(|proxy| proxy.name == name)
            .and_then(manis_mihomo::Proxy::latest_latency_ms)
            .map(|value| {
                #[allow(clippy::cast_possible_truncation)]
                let value = value.round() as i64;
                value
            })
    };
    let node_item = |name: &str, current: &str| {
        json!({
            "name": name,
            "displayName": name,
            "delayMs": delay(name),
            "current": name == current,
        })
    };

    let subscription_providers: Vec<_> = snapshot
        .providers
        .iter()
        .filter(|provider| provider.name.starts_with("Subscription "))
        .collect();
    let sources: Vec<_> = workspace
        .imported_subscriptions
        .iter()
        .enumerate()
        .map(|(index, subscription)| {
            let provider = subscription_providers.get(index).copied();
            let nodes = provider.map_or_else(Vec::new, |provider| {
                provider
                    .proxies
                    .iter()
                    .filter(|proxy| !is_subscription_info_node(&proxy.name))
                    .map(|proxy| node_item(&proxy.name, global_current))
                    .collect()
            });
            json!({
                "id": format!("subscription-{index}"),
                "name": subscription.name,
                "detail": provider.map_or("订阅 · 节点未加载", |_| "订阅 · 实时节点"),
                "kind": "subscription",
                "enabled": subscription.enabled,
                "nodes": nodes,
            })
        })
        .collect();
    let manual_nodes: Vec<_> = workspace
        .saved_single_nodes
        .iter()
        .map(|node| node_item(&node.name, global_current))
        .collect();
    let mut sources = sources;
    if !manual_nodes.is_empty() {
        sources.push(json!({
            "id": "manual",
            "name": "手动节点",
            "detail": "分享链接导入",
            "kind": "manual",
            "enabled": true,
            "nodes": manual_nodes,
        }));
    }

    let policy_groups: Vec<_> = snapshot
        .policy_groups()
        .into_iter()
        .filter(|group| {
            group.name != "__MANIS_GLOBAL__" && !group.name.eq_ignore_ascii_case("GLOBAL")
        })
        .map(|group| {
            let members: Vec<_> = group
                .nodes
                .iter()
                .filter(|name| !is_subscription_info_node(name))
                .map(|name| node_item(name, group.current.as_deref().unwrap_or("")))
                .collect();
            json!({
                "id": group.name,
                "label": group.name,
                "kind": match group.kind {
                    manis_mihomo::GroupKind::UrlTest => "urltest",
                    manis_mihomo::GroupKind::Fallback => "fallback",
                    manis_mihomo::GroupKind::LoadBalance => "load-balance",
                    manis_mihomo::GroupKind::Selector => "select",
                },
                "current": group.current,
                "members": members,
            })
        })
        .collect();

    json!({
        "global": {
            "label": "全局",
            "current": if global_current.is_empty() { serde_json::Value::Null } else { json!(global_current) },
            "total": snapshot
                .providers
                .iter()
                .filter(|provider| provider.name != "default")
                .flat_map(|provider| provider.proxies.iter())
                .filter(|proxy| !is_subscription_info_node(&proxy.name))
                .count(),
        },
        "sources": sources,
        "policyGroups": policy_groups,
    })
}

fn is_subscription_info_node(name: &str) -> bool {
    [
        "剩余流量",
        "距离下次重置",
        "套餐到期",
        "到期时间",
        "过期时间",
        "防失联",
        "流量重置",
        "官网",
    ]
    .iter()
    .any(|marker| name.contains(marker))
}

fn nsh_rules_page(
    snapshot: &manis_mihomo::MihomoSnapshot,
    manual_rules: &[crate::manual_rule::ManualRule],
) -> serde_json::Value {
    use serde_json::json;

    let mut rules: Vec<_> = manual_rules
        .iter()
        .map(|rule| manis_presentation::RuleRowInput {
            kind: rule.condition_kind_label().to_owned(),
            value: rule
                .conditions()
                .iter()
                .map(crate::manual_rule::ManualRuleCondition::parameter)
                .collect::<Vec<_>>()
                .join(" / "),
            policy: rule.target().to_owned(),
            enabled: rule.enabled(),
            source: Some("manual".into()),
        })
        .collect();
    rules.extend(snapshot.rules.iter().map(|rule| {
        let kind = match rule.kind.as_str() {
            "Match" | "Final" => "FINAL",
            "DomainSuffix" => "DOMAIN-SUFFIX",
            "Domain" => "DOMAIN",
            "DomainKeyword" => "DOMAIN-KEYWORD",
            "DomainWildcard" => "DOMAIN-WILDCARD",
            "IPCIDR" => "IP-CIDR",
            "IPCIDR6" => "IP6-CIDR",
            "GeoIP" => "GEOIP",
            "IPASN" => "IP-ASN",
            "DstPort" => "DST-PORT",
            other => other,
        };
        manis_presentation::RuleRowInput {
            kind: kind.to_owned(),
            value: if kind == "FINAL" {
                String::new()
            } else {
                rule.payload.clone()
            },
            policy: rule.proxy.clone(),
            enabled: !rule.extra.disabled.unwrap_or(false),
            source: Some("runtime".into()),
        }
    }));
    let policies: Vec<_> = snapshot
        .policy_groups()
        .into_iter()
        .map(|group| group.name)
        .filter(|name| name != "__MANIS_GLOBAL__" && name != "DIRECT" && name != "REJECT")
        .collect();
    serde_json::to_value(manis_presentation::RulesPageView::build(
        manis_presentation::RulesPageInput {
            rules,
            policies,
            warnings: Vec::new(),
            summary: format!(
                "{} 条手动规则 · {} 条运行时规则",
                manual_rules.len(),
                snapshot.rules.len()
            ),
        },
    ))
    .unwrap_or_else(|_| json!({}))
}

/// Adds or replaces one saved manual desktop rule, then applies the profile.
///
/// # Errors
/// Returns a parse, store or reload error.
pub fn nsh_save_rule(
    index: Option<usize>,
    kind: &str,
    value: &str,
    policy: &str,
) -> Result<(), String> {
    let directory = crate::brand::config_dir().ok_or_else(|| "找不到配置目录".to_owned())?;
    let mut rules =
        crate::manual_rule::load_manual_rules_in(&directory).map_err(|error| error.to_string())?;
    let kind = manual_rule_kind(kind).ok_or_else(|| "不支持的规则类型".to_owned())?;
    let rule = crate::manual_rule::ManualRule::parse(kind, value.trim(), policy.trim())
        .map_err(|_error| "规则内容无效".to_owned())?;
    if let Some(index) = index {
        crate::manual_rule::replace_manual_rule(&mut rules, index, rule)
            .map_err(|_error| "规则不存在、重复或无法组合".to_owned())?;
    } else {
        rules.push(rule);
    }
    crate::manual_rule::save_manual_rules_in(&directory, &rules)
        .map_err(|error| error.to_string())?;
    nsh_apply_profile()
}

/// Enables or disables one saved manual desktop rule, then applies the profile.
///
/// # Errors
/// Returns a store or reload error.
pub fn nsh_toggle_rule(index: usize, enabled: bool) -> Result<(), String> {
    let directory = crate::brand::config_dir().ok_or_else(|| "找不到配置目录".to_owned())?;
    let mut rules =
        crate::manual_rule::load_manual_rules_in(&directory).map_err(|error| error.to_string())?;
    let rule = rules
        .get_mut(index)
        .ok_or_else(|| "规则已变化，请刷新".to_owned())?;
    rule.set_enabled(enabled);
    crate::manual_rule::save_manual_rules_in(&directory, &rules)
        .map_err(|error| error.to_string())?;
    nsh_apply_profile()
}

/// Deletes one saved manual desktop rule, then applies the profile.
///
/// # Errors
/// Returns a store or reload error.
pub fn nsh_delete_rule(index: usize) -> Result<(), String> {
    let directory = crate::brand::config_dir().ok_or_else(|| "找不到配置目录".to_owned())?;
    let mut rules =
        crate::manual_rule::load_manual_rules_in(&directory).map_err(|error| error.to_string())?;
    if index >= rules.len() {
        return Err("运行时规则不能直接删除".to_owned());
    }
    rules.remove(index);
    crate::manual_rule::save_manual_rules_in(&directory, &rules)
        .map_err(|error| error.to_string())?;
    nsh_apply_profile()
}

fn manual_rule_kind(label: &str) -> Option<crate::manual_rule::ManualRuleKind> {
    Some(match label {
        "DOMAIN" => crate::manual_rule::ManualRuleKind::Host,
        "DOMAIN-SUFFIX" => crate::manual_rule::ManualRuleKind::HostSuffix,
        "DOMAIN-WILDCARD" => crate::manual_rule::ManualRuleKind::HostWildcard,
        "DOMAIN-KEYWORD" => crate::manual_rule::ManualRuleKind::HostKeyword,
        "IP-CIDR" => crate::manual_rule::ManualRuleKind::IpCidr,
        "IP6-CIDR" => crate::manual_rule::ManualRuleKind::Ip6Cidr,
        "GEOIP" => crate::manual_rule::ManualRuleKind::GeoIp,
        "IP-ASN" => crate::manual_rule::ManualRuleKind::IpAsn,
        "DST-PORT" => crate::manual_rule::ManualRuleKind::DstPort,
        "FINAL" => crate::manual_rule::ManualRuleKind::Final,
        _ => return None,
    })
}

/// Recompiles the saved desktop profile and reloads the live controller.
///
/// This is the desktop equivalent of Apply: it makes newly added/removed
/// sources, toggles and saved rules visible to the running Mihomo. The previous
/// generated YAML is restored if the reload is rejected.
///
/// # Errors
/// Returns a profile, render, controller or rollback error.
pub fn nsh_apply_profile() -> Result<(), String> {
    #[cfg(any(unix, windows))]
    {
        let data_dir = crate::brand::data_dir().ok_or_else(|| "找不到数据目录".to_owned())?;
        let config_dir = crate::brand::config_dir().ok_or_else(|| "找不到配置目录".to_owned())?;
        mihomo::sync_single_node_provider_files(&config_dir, &data_dir)
            .map_err(|error| error.to_string())?;
        let profile =
            mihomo::compile_saved_profile(&config_dir, None).map_err(|error| error.to_string())?;
        let client = live_client().ok_or_else(|| "桌面内核未运行".to_owned())?;
        let snapshot = client.fetch_snapshot().map_err(|error| error.to_string())?;
        let rendered =
            manis_profile::render_mihomo_yaml_with_tun(&profile, snapshot.runtime.tun.enable)
                .map_err(|error| error.to_string())?;
        let previous = std::fs::read_to_string(data_dir.join("mihomo/manis-generated.yaml")).ok();
        if let Err(error) = client.reload_config_payload(&rendered) {
            if let Some(previous) = previous.as_deref() {
                let _ = client.reload_config_payload(previous);
            }
            return Err(error.to_string());
        }
        Ok(())
    }
    #[cfg(not(any(unix, windows)))]
    {
        Err("桌面控制器只在 Unix/Windows 上接入".to_owned())
    }
}

/// Adds a subscription URL to the desktop store.
///
/// # Errors
/// Returns a store/validation error for an invalid URL or failed write.
pub fn nsh_add_subscription(name: &str, url: &str) -> Result<(), String> {
    let directory = crate::brand::config_dir().ok_or_else(|| "找不到配置目录".to_owned())?;
    mihomo::save_subscription_source_with_options_in(
        &directory,
        url.trim(),
        name.trim(),
        mihomo::RemoteSourceRefreshInterval::Manual,
        true,
    )
    .map(|_| ())
    .map_err(|error| error.to_string())
}

/// Adds a single VLESS/share-link node to the desktop store.
///
/// # Errors
/// Returns a store/validation error for an invalid link or failed write.
pub fn nsh_add_node(link: &str) -> Result<(), String> {
    let directory = crate::brand::config_dir().ok_or_else(|| "找不到配置目录".to_owned())?;
    let name = link
        .split_once('#')
        .map_or("手动节点", |(_, name)| name)
        .trim();
    mihomo::save_single_node_source_with_options_in(&directory, link.trim(), name, true)
        .map(|_| ())
        .map_err(|error| error.to_string())
}

/// Enables or disables one saved desktop subscription by list index.
///
/// # Errors
/// Returns a store error when the index is stale or the write fails.
pub fn nsh_toggle_subscription(index: usize, enabled: bool) -> Result<(), String> {
    let directory = crate::brand::config_dir().ok_or_else(|| "找不到配置目录".to_owned())?;
    let sources =
        mihomo::load_subscription_sources_in(&directory).map_err(|error| error.to_string())?;
    let source = sources
        .get(index)
        .ok_or_else(|| "订阅已变化，请刷新".to_owned())?;
    mihomo::update_subscription_source_enabled_in(&directory, &source.id, enabled)
        .map(|_| ())
        .map_err(|error| error.to_string())
}

/// Deletes one saved desktop subscription by list index.
///
/// # Errors
/// Returns a store error when the index is stale or the write fails.
pub fn nsh_delete_subscription(index: usize) -> Result<(), String> {
    let directory = crate::brand::config_dir().ok_or_else(|| "找不到配置目录".to_owned())?;
    let sources =
        mihomo::load_subscription_sources_in(&directory).map_err(|error| error.to_string())?;
    let source = sources
        .get(index)
        .ok_or_else(|| "订阅已变化，请刷新".to_owned())?;
    mihomo::remove_subscription_source_in(&directory, &source.id).map_err(|error| error.to_string())
}

/// Enables or disables one saved desktop manual node by list index.
///
/// # Errors
/// Returns a store error when the index is stale or the write fails.
pub fn nsh_toggle_node(index: usize, enabled: bool) -> Result<(), String> {
    let directory = crate::brand::config_dir().ok_or_else(|| "找不到配置目录".to_owned())?;
    let nodes =
        mihomo::load_single_node_sources_in(&directory).map_err(|error| error.to_string())?;
    let node = nodes
        .get(index)
        .ok_or_else(|| "节点已变化，请刷新".to_owned())?;
    mihomo::update_single_node_source_enabled_in(&directory, &node.id, enabled)
        .map(|_| ())
        .map_err(|error| error.to_string())
}

/// Deletes one saved desktop manual node by list index.
///
/// # Errors
/// Returns a store error when the index is stale or the write fails.
pub fn nsh_delete_node(index: usize) -> Result<(), String> {
    let directory = crate::brand::config_dir().ok_or_else(|| "找不到配置目录".to_owned())?;
    let nodes =
        mihomo::load_single_node_sources_in(&directory).map_err(|error| error.to_string())?;
    let node = nodes
        .get(index)
        .ok_or_else(|| "节点已变化，请刷新".to_owned())?;
    mihomo::remove_single_node_source_in(&directory, &node.id).map_err(|error| error.to_string())
}

fn auto_start_enabled() -> bool {
    crate::autostart::load_preference(crate::brand::config_dir().as_deref())
}

/// Reads or changes the desktop login-startup preference.
///
/// # Errors
/// Returns an autostart error when the platform login item cannot be updated.
pub fn nsh_set_autostart(enabled: bool) -> Result<(), String> {
    let directory = crate::brand::config_dir().ok_or_else(|| "找不到配置目录".to_owned())?;
    crate::autostart::save_preference_in(&directory, enabled).map_err(|error| error.to_string())?;
    crate::autostart::set_enabled(enabled).map_err(|error| error.to_string())
}

/// Checks GitHub Releases for a newer desktop version.
///
/// # Errors
/// Returns a network or metadata error from the desktop updater.
pub fn nsh_check_update() -> Result<Option<String>, String> {
    crate::app_update::check_for_update()
        .map(|update| update.map(|update| update.version))
        .map_err(|error| error.to_string())
}

/// Last lines of the desktop kernel log for the NSH settings page.
#[must_use]
pub fn nsh_logs() -> Vec<String> {
    let Some(path) = crate::brand::data_dir().map(|dir| dir.join("mihomo/manis-core.log")) else {
        return vec!["还没有日志".to_owned()];
    };
    let Ok(contents) = std::fs::read_to_string(path) else {
        return vec!["还没有日志".to_owned()];
    };
    let mut lines: Vec<String> = contents.lines().map(str::to_owned).collect();
    if lines.len() > 300 {
        lines.drain(..lines.len() - 300);
    }
    lines
}

/// Saved policy groups name their members by source; the NSH page edits them
/// with the indexes of the lists it shows, so the root payload decodes back.
fn group_source_indexes(
    group: &manis_core::ManagedPolicyGroup,
    subscriptions: &[mihomo::StoredSubscription],
    nodes: &[mihomo::StoredSingleNode],
) -> (Vec<usize>, Vec<usize>) {
    let mut subscription_indexes = Vec::new();
    let mut node_indexes = Vec::new();
    let manis_core::PolicyCandidateMatcher::Explicit(members) = &group.matcher else {
        return (subscription_indexes, node_indexes);
    };
    for member in members {
        if let Some(id) = member.source_id.strip_prefix("subscription:") {
            if let Some(index) = subscriptions
                .iter()
                .position(|subscription| subscription.id == id)
                && !subscription_indexes.contains(&index)
            {
                subscription_indexes.push(index);
            }
        } else if member.source_id == "saved"
            && let Some(index) = nodes.iter().position(|node| node.name == member.node_name)
            && !node_indexes.contains(&index)
        {
            node_indexes.push(index);
        }
    }
    (subscription_indexes, node_indexes)
}

/// Resolvers discovered in the saved subscription documents, never edited by
/// hand; they apply only while no custom resolver is configured.
fn discovered_proxy_dns(workspace: &StoredWorkspace) -> Vec<String> {
    let mut servers: Vec<String> = Vec::new();
    for subscription in &workspace.imported_subscriptions {
        if !subscription.enabled {
            continue;
        }
        for server in &subscription.proxy_server_nameservers {
            let value = server.as_str().to_owned();
            if !servers.contains(&value) {
                servers.push(value);
            }
            if servers.len() >= mihomo::MAX_SUBSCRIPTION_PROXY_DNS_SERVERS {
                return servers;
            }
        }
    }
    servers
}

/// Saves how proxy-server hostnames are resolved (`settings.dns.set`).
///
/// # Errors
/// Returns a message when the data directory is unavailable or a resolver is
/// not an HTTPS endpoint or plain IP literal.
pub fn nsh_set_proxy_dns(mode: &str, servers: &[String]) -> Result<(), String> {
    let directory = crate::brand::config_dir().ok_or_else(|| "找不到 Manis 数据目录".to_owned())?;
    mihomo::save_proxy_dns_in(&directory, mode, servers).map_err(|error| error.to_string())?;
    let _ = nsh_apply_profile();
    Ok(())
}

/// Creates a policy group from the sources the subscriptions page selected
/// (`subscriptions.addGroup`).
///
/// # Errors
/// Returns a message when a selected index does not exist, the group has no
/// member, or the store cannot write it.
pub fn nsh_add_group(
    name: &str,
    strategy: &str,
    subscription_indexes: &[usize],
    node_indexes: &[usize],
) -> Result<(), String> {
    let directory = crate::brand::config_dir().ok_or_else(|| "找不到 Manis 数据目录".to_owned())?;
    let subscriptions =
        mihomo::load_subscription_sources_in(&directory).map_err(|error| error.to_string())?;
    let nodes =
        mihomo::load_single_node_sources_in(&directory).map_err(|error| error.to_string())?;

    let mut members = std::collections::BTreeSet::new();
    for index in subscription_indexes {
        let subscription = subscriptions
            .get(*index)
            .ok_or_else(|| "订阅不存在".to_owned())?;
        members.insert(
            manis_core::NodeIdentity::whole_source(&format!("subscription:{}", subscription.id))
                .map_err(|_error| "订阅成员无效".to_owned())?,
        );
    }
    for index in node_indexes {
        let node = nodes.get(*index).ok_or_else(|| "节点不存在".to_owned())?;
        members.insert(
            manis_core::NodeIdentity::new("saved", &node.name)
                .map_err(|_error| "节点成员无效".to_owned())?,
        );
    }
    if members.is_empty() {
        return Err("策略组需要名称和至少一个成员".to_owned());
    }

    let id = mihomo::new_managed_policy_id();
    let mut group =
        manis_core::ManagedPolicyGroup::new(&id, name.trim()).map_err(|error| error.to_string())?;
    group.strategy = if strategy == "urltest" {
        manis_core::ManagedPolicyStrategy::LowestLatency
    } else {
        manis_core::ManagedPolicyStrategy::Manual
    };
    group.matcher = manis_core::PolicyCandidateMatcher::Explicit(members);
    mihomo::save_managed_policy_in(&directory, &group).map_err(|error| error.to_string())?;
    Ok(())
}

/// Exports the saved store as one portable document (`settings.copyBackup`).
///
/// The document embeds `config.toml`, which is the whole saved workspace:
/// subscriptions, saved nodes, policy groups, rule sources, manual rules,
/// routing mode and the DNS policy. It contains plaintext credentials, so the
/// shell must copy it only to the user's own clipboard.
///
/// # Errors
/// Returns a message when the data directory or the store cannot be read.
pub fn nsh_export_backup() -> Result<String, String> {
    let directory = crate::brand::config_dir().ok_or_else(|| "找不到 Manis 数据目录".to_owned())?;
    export_backup_in(&directory)
}

fn export_backup_in(directory: &Path) -> Result<String, String> {
    let store = crate::config_toml::read_source(directory)
        .map_err(|_error| "无法读取本地配置".to_owned())?;
    serde_json::to_string(&serde_json::json!({
        "format": "manis-backup",
        "version": 1,
        "store": store,
    }))
    .map_err(|_error| "无法生成备份".to_owned())
}

/// Replaces the saved store from a backup document and reloads the profile
/// (`settings.importBackup`).
///
/// # Errors
/// Returns a message when the document is not a supported Manis backup or the
/// store cannot be written.
pub fn nsh_import_backup(text: &str) -> Result<(), String> {
    let directory = crate::brand::config_dir().ok_or_else(|| "找不到 Manis 数据目录".to_owned())?;
    import_backup_in(&directory, text)?;
    let _ = nsh_apply_profile();
    Ok(())
}

fn import_backup_in(directory: &Path, text: &str) -> Result<(), String> {
    let document: serde_json::Value =
        serde_json::from_str(text).map_err(|_error| "备份格式无法识别".to_owned())?;
    if document.get("format").and_then(serde_json::Value::as_str) != Some("manis-backup") {
        return Err("这不是 Manis 备份".to_owned());
    }
    if document.get("version").and_then(serde_json::Value::as_u64) != Some(1) {
        return Err("备份版本不受支持".to_owned());
    }
    let store = document
        .get("store")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "备份内容为空".to_owned())?;
    crate::config_toml::entries_from_source(store)
        .map_err(|_error| "备份内容无法识别".to_owned())?;
    crate::config_toml::replace_source(directory, store)
        .map_err(|_error| "无法写入本地配置".to_owned())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{export_backup_in, import_backup_in};
    use std::path::PathBuf;

    fn temp_dir(name: &str) -> PathBuf {
        let base = std::fs::canonicalize(std::env::temp_dir()).expect("canonical temp dir");
        let root = base.join(format!("{name}-{}", std::process::id()));
        if root.exists() {
            std::fs::remove_dir_all(&root).expect("remove stale fixture");
        }
        std::fs::create_dir_all(&root).expect("create fixture");
        root
    }

    #[test]
    fn a_backup_restores_the_saved_store() {
        let root = temp_dir("manis-backup-roundtrip");
        crate::config_toml::write_entry(&root, "routing.mode", "global").expect("seed store");
        let backup = export_backup_in(&root).expect("export");

        crate::config_toml::write_entry(&root, "routing.mode", "rule").expect("change store");
        import_backup_in(&root, &backup).expect("import");

        let restored = crate::config_toml::read_entry(&root, "routing.mode", 1024)
            .expect("read entry")
            .expect("entry exists");
        assert_eq!(restored, "global");
        std::fs::remove_dir_all(root).expect("remove fixture");
    }

    #[test]
    fn a_foreign_document_is_rejected() {
        let root = temp_dir("manis-backup-foreign");
        assert!(import_backup_in(&root, "{\"format\":\"other\",\"version\":1}").is_err());
        assert!(import_backup_in(&root, "not json").is_err());
        std::fs::remove_dir_all(root).expect("remove fixture");
    }
}
