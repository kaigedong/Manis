use std::collections::HashMap;

use relay_core::{
    EmptyPolicyCatalog, PolicyCatalog, PolicyGroup, PolicyGroupId, PolicyNode, PolicyRule, ProxyId,
};

use crate::{GroupKind, MihomoSnapshot, Proxy};

/// Converts one read-only Mihomo snapshot into the UI's owned policy catalog.
///
/// Hidden groups are omitted. Rules retain Mihomo's source order and are grouped by their target
/// policy name.
///
/// # Errors
///
/// Returns [`EmptyPolicyCatalog`] when Mihomo exposes no visible supported policy groups.
pub fn to_policy_catalog(snapshot: &MihomoSnapshot) -> Result<PolicyCatalog, EmptyPolicyCatalog> {
    let proxies: HashMap<_, _> = snapshot
        .proxies
        .iter()
        .map(|proxy| (proxy.name.as_str(), proxy))
        .collect();

    let groups = snapshot
        .policy_groups()
        .into_iter()
        .filter(|group| group.hidden != Some(true))
        .map(|group| {
            let nodes = group
                .nodes
                .iter()
                .map(|name| policy_node(name, proxies.get(name.as_str()).copied()))
                .collect();
            let rules: Vec<_> = snapshot
                .rules
                .iter()
                .filter(|rule| rule.proxy == group.name)
                .map(|rule| PolicyRule {
                    index: u32::try_from(rule.index).unwrap_or(u32::MAX),
                    kind: rule.kind.clone(),
                    payload: rule.payload.clone(),
                    hit_count: rule.extra.hit,
                    disabled: rule.extra.disabled.unwrap_or(false),
                })
                .collect();

            PolicyGroup {
                id: PolicyGroupId::new(group.name.clone()),
                name: group.name,
                kind: group_kind_label(group.kind).to_owned(),
                target: group
                    .current
                    .or_else(|| group.nodes.first().cloned())
                    .unwrap_or_else(|| "暂无可用节点".to_owned()),
                nodes,
                rules_total: rules.len(),
                rules,
            }
        })
        .collect();

    PolicyCatalog::try_new(groups)
}

fn policy_node(name: &str, proxy: Option<&Proxy>) -> PolicyNode {
    PolicyNode {
        id: ProxyId::new(name),
        name: name.to_owned(),
        provider: proxy.and_then(|proxy| proxy.provider_name.clone()),
        detail: proxy.map_or_else(|| "类型未知".to_owned(), |proxy| proxy.proxy_type.clone()),
        latency_ms: proxy.and_then(|proxy| rounded_latency(proxy.latest_latency_ms())),
        alive: proxy.and_then(|proxy| proxy.alive),
    }
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn rounded_latency(latency: Option<f64>) -> Option<u16> {
    latency.map(|latency| latency.round().clamp(0.0, f64::from(u16::MAX)) as u16)
}

fn group_kind_label(kind: GroupKind) -> &'static str {
    match kind {
        GroupKind::Selector => "手动选择",
        GroupKind::UrlTest => "自动测速",
        GroupKind::Fallback => "故障转移",
        GroupKind::LoadBalance => "负载均衡",
    }
}
