use std::collections::{HashMap, HashSet};

use manis_core::{
    EmptyPolicyCatalog, PolicyCandidateKind, PolicyCatalog, PolicyGroup, PolicyGroupId,
    PolicyGroupKind as CorePolicyGroupKind, PolicyNode, PolicyRule, ProxyId, RoutingRule,
};

use crate::{GroupKind, MihomoSnapshot, Proxy};

// Keep this wire name aligned with `manis_profile::MANIS_GLOBAL_GROUP_NAME` without creating a
// reverse dependency from the Mihomo adapter to the profile renderer.
const MANIS_GLOBAL_GROUP_NAME: &str = "__MANIS_GLOBAL__";

/// Converts one read-only Mihomo snapshot into the UI's owned policy catalog.
///
/// Hidden groups are omitted. Rules retain Mihomo's source order and are grouped by their target
/// policy name.
///
/// # Errors
///
/// Returns [`EmptyPolicyCatalog`] when Mihomo exposes no visible supported policy groups.
pub fn to_policy_catalog(snapshot: &MihomoSnapshot) -> Result<PolicyCatalog, EmptyPolicyCatalog> {
    let routing_rules = snapshot
        .rules
        .iter()
        .map(|rule| RoutingRule {
            index: u32::try_from(rule.index).unwrap_or(u32::MAX),
            kind: rule.kind.clone(),
            payload: rule.payload.clone(),
            target: rule.proxy.clone(),
            disabled: rule.extra.disabled.unwrap_or(false),
        })
        .collect();
    let proxies: HashMap<_, _> = snapshot
        .proxies
        .iter()
        .map(|proxy| (proxy.name.as_str(), proxy))
        .collect();
    let provider_nodes: HashMap<_, _> = snapshot
        .providers
        .iter()
        .flat_map(|provider| {
            provider
                .proxies
                .iter()
                .map(move |proxy| (proxy.name.as_str(), (provider.name.as_str(), proxy)))
        })
        .collect();

    let runtime_groups = snapshot.policy_groups();
    let group_names = runtime_groups
        .iter()
        .map(|group| group.name.clone())
        .collect::<HashSet<_>>();

    let groups = runtime_groups
        .into_iter()
        .filter(|group| {
            group.hidden != Some(true)
                && group.name != "GLOBAL"
                && group.name != MANIS_GLOBAL_GROUP_NAME
        })
        .map(|group| {
            let nodes = group
                .nodes
                .iter()
                .map(|name| {
                    policy_node(
                        name,
                        proxies.get(name.as_str()).copied(),
                        provider_nodes.get(name.as_str()).copied(),
                        group_names.contains(name.as_str()),
                    )
                })
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
                kind: policy_group_kind(group.kind),
                target: group.current.or_else(|| group.nodes.first().cloned()),
                nodes,
                rules_total: rules.len(),
                rules,
            }
        })
        .collect();

    PolicyCatalog::try_new_with_rules(groups, routing_rules)
}

fn policy_node(
    name: &str,
    runtime_proxy: Option<&Proxy>,
    provider_node: Option<(&str, &Proxy)>,
    is_group: bool,
) -> PolicyNode {
    let metadata_proxy = if is_group {
        runtime_proxy
    } else {
        provider_node.map(|(_, proxy)| proxy).or(runtime_proxy)
    };
    PolicyNode {
        id: ProxyId::new(name),
        name: name.to_owned(),
        kind: if is_group {
            PolicyCandidateKind::PolicyGroup
        } else {
            PolicyCandidateKind::Node
        },
        provider: if is_group {
            runtime_proxy.and_then(|proxy| proxy.provider_name.clone())
        } else {
            provider_node
                .map(|(provider, _)| provider.to_owned())
                .or_else(|| runtime_proxy.and_then(|proxy| proxy.provider_name.clone()))
        },
        detail: metadata_proxy.map_or_else(String::new, |proxy| proxy.proxy_type.clone()),
        latency_ms: runtime_proxy
            .and_then(|proxy| rounded_latency(proxy.latest_latency_ms()))
            .or_else(|| {
                provider_node.and_then(|(_, proxy)| rounded_latency(proxy.latest_latency_ms()))
            }),
        alive: runtime_proxy
            .and_then(|proxy| proxy.alive)
            .or_else(|| provider_node.and_then(|(_, proxy)| proxy.alive)),
    }
}

fn rounded_latency(latency: Option<f64>) -> Option<u16> {
    use num_traits::ToPrimitive as _;

    latency.and_then(|latency| latency.round().clamp(0.0, f64::from(u16::MAX)).to_u16())
}

fn policy_group_kind(kind: GroupKind) -> CorePolicyGroupKind {
    match kind {
        GroupKind::Selector => CorePolicyGroupKind::Selector,
        GroupKind::UrlTest => CorePolicyGroupKind::UrlTest,
        GroupKind::Fallback => CorePolicyGroupKind::Fallback,
        GroupKind::LoadBalance => CorePolicyGroupKind::LoadBalance,
    }
}
