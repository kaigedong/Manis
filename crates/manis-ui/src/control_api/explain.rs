use std::collections::{BTreeMap, BTreeSet};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use manis_core::RoutingMode;

use crate::manual_rule::{ManualRule, ManualRuleCondition, ManualRuleKind};
use crate::mihomo::{LocalRoutingRuleGroup, MANUAL_ROUTING_RULE_GROUP_ID, StoredQxRuleSource};

use super::protocol::{ConnectionData, ExplainData, ExplainParams};

pub(crate) struct ExplainContext<'a> {
    pub(crate) routing_mode: RoutingMode,
    pub(crate) global_target: Option<&'a str>,
    pub(crate) group_order: &'a [String],
    pub(crate) manual_rules: &'a [ManualRule],
    pub(crate) local_groups: &'a [LocalRoutingRuleGroup],
    pub(crate) remote_sources: &'a [StoredQxRuleSource],
    pub(crate) policy_exits: &'a BTreeMap<String, String>,
    pub(crate) live_connections: Vec<ConnectionData>,
}

pub(crate) fn explain_route(params: &ExplainParams, ctx: &ExplainContext<'_>) -> ExplainData {
    let target = params.target.trim().to_owned();
    let parsed_ip = target.parse::<IpAddr>().ok();
    let host_lower = if parsed_ip.is_none() {
        Some(target.to_ascii_lowercase())
    } else {
        None
    };

    if ctx.routing_mode == RoutingMode::Direct {
        return ExplainData {
            target,
            process: params.process.clone(),
            routing_mode: ctx.routing_mode.wire_value().to_owned(),
            matched_source: "routing_mode".to_owned(),
            matched_rule: "MODE DIRECT".to_owned(),
            target_policy: "DIRECT".to_owned(),
            policy_chain: vec!["DIRECT".to_owned()],
            resolved_exit: Some("DIRECT".to_owned()),
            note: None,
            matching_live_connections: ctx.live_connections.clone(),
        };
    }

    if ctx.routing_mode == RoutingMode::Global {
        let mut chain = vec!["GLOBAL".to_owned()];
        if let Some(exit) = ctx.global_target {
            chain.push(exit.to_owned());
        }
        return ExplainData {
            target,
            process: params.process.clone(),
            routing_mode: ctx.routing_mode.wire_value().to_owned(),
            matched_source: "routing_mode".to_owned(),
            matched_rule: "MODE GLOBAL".to_owned(),
            target_policy: "GLOBAL".to_owned(),
            policy_chain: chain.clone(),
            resolved_exit: chain.last().cloned(),
            note: None,
            matching_live_connections: ctx.live_connections.clone(),
        };
    }

    let query = MatchQuery {
        host_lower: host_lower.as_deref(),
        ip: parsed_ip,
        port: params.port,
        process: params.process.as_deref(),
        process_path: params.process_path.as_deref(),
    };

    let (matched, remote_skipped) = find_first_matching_rule(ctx, &query);
    let (matched_source, matched_rule, target_policy) = matched.unwrap_or_else(|| {
        let fallback = ctx
            .manual_rules
            .iter()
            .rev()
            .find(|rule| rule.is_enabled() && rule.is_final())
            .map_or_else(|| "Proxy".to_owned(), |rule| rule.target().to_owned());
        ("final_fallback".to_owned(), "FINAL".to_owned(), fallback)
    });

    let policy_chain = resolve_policy_chain(&target_policy, ctx.policy_exits);
    let resolved_exit = policy_chain.last().cloned();
    let note = if matched_source == "final_fallback" && remote_skipped > 0 {
        Some(format!(
            "{remote_skipped} enabled remote rule source(s) (and GeoIP/ASN lookups) are evaluated at runtime by Mihomo and not expanded offline; check matching_live_connections for live Mihomo verdicts"
        ))
    } else {
        None
    };

    ExplainData {
        target,
        process: params.process.clone(),
        routing_mode: ctx.routing_mode.wire_value().to_owned(),
        matched_source,
        matched_rule,
        target_policy,
        policy_chain,
        resolved_exit,
        note,
        matching_live_connections: ctx.live_connections.clone(),
    }
}

fn find_first_matching_rule(
    ctx: &ExplainContext<'_>,
    query: &MatchQuery<'_>,
) -> (Option<(String, String, String)>, usize) {
    let mut remote_skipped = 0usize;
    for group_id in ctx.group_order {
        if group_id == MANUAL_ROUTING_RULE_GROUP_ID {
            for rule in ctx
                .manual_rules
                .iter()
                .filter(|rule| rule.is_enabled() && !rule.is_final())
            {
                if rule_matches(rule, query) {
                    return (
                        Some((
                            "manual_rules".to_owned(),
                            format_manual_rule_match(rule),
                            rule.target().to_owned(),
                        )),
                        remote_skipped,
                    );
                }
            }
        } else if let Some(group) = ctx
            .local_groups
            .iter()
            .find(|group| group.id == *group_id && group.enabled)
        {
            for rule in group
                .rules
                .iter()
                .filter(|rule| rule.is_enabled() && !rule.is_final())
            {
                if rule_matches(rule, query) {
                    return (
                        Some((
                            format!("local_group:{}", group.name),
                            format_manual_rule_match(rule),
                            group.target_policy.as_str().to_owned(),
                        )),
                        remote_skipped,
                    );
                }
            }
        } else if ctx
            .remote_sources
            .iter()
            .any(|source| source.id == *group_id && source.enabled)
        {
            remote_skipped += 1;
        }
    }
    (None, remote_skipped)
}

pub(super) fn format_manual_rule_match(rule: &ManualRule) -> String {
    if rule.is_final() {
        return "FINAL".to_owned();
    }
    rule.conditions()
        .iter()
        .map(|condition| {
            format!(
                "{},{}",
                condition.kind().display_label(),
                condition.parameter()
            )
        })
        .collect::<Vec<_>>()
        .join(" AND ")
}

struct MatchQuery<'a> {
    host_lower: Option<&'a str>,
    ip: Option<IpAddr>,
    port: Option<u16>,
    process: Option<&'a str>,
    process_path: Option<&'a str>,
}

fn rule_matches(rule: &ManualRule, query: &MatchQuery<'_>) -> bool {
    !rule.conditions().is_empty()
        && rule
            .conditions()
            .iter()
            .all(|condition| condition_matches(condition, query))
}

fn condition_matches(condition: &ManualRuleCondition, query: &MatchQuery<'_>) -> bool {
    let param = condition.parameter();
    match condition.kind() {
        ManualRuleKind::Host => query
            .host_lower
            .is_some_and(|host| host == param.to_ascii_lowercase()),
        ManualRuleKind::HostSuffix => query.host_lower.is_some_and(|host| {
            let suffix = param.to_ascii_lowercase();
            host == suffix
                || host
                    .strip_suffix(&suffix)
                    .is_some_and(|prefix| prefix.ends_with('.'))
        }),
        ManualRuleKind::HostWildcard => query
            .host_lower
            .is_some_and(|host| wildcard_matches(&param.to_ascii_lowercase(), host)),
        ManualRuleKind::HostKeyword => query
            .host_lower
            .is_some_and(|host| host.contains(&param.to_ascii_lowercase())),
        ManualRuleKind::IpCidr => query.ip.is_some_and(|ip| matches_ipv4_cidr(ip, param)),
        ManualRuleKind::Ip6Cidr => query.ip.is_some_and(|ip| matches_ipv6_cidr(ip, param)),
        ManualRuleKind::DstPort => query
            .port
            .is_some_and(|port| param.parse::<u16>().ok() == Some(port)),
        ManualRuleKind::ProcessName => query
            .process
            .is_some_and(|process| process.eq_ignore_ascii_case(param)),
        ManualRuleKind::ProcessPath => query
            .process_path
            .is_some_and(|path| path.eq_ignore_ascii_case(param)),
        ManualRuleKind::GeoIp
        | ManualRuleKind::IpAsn
        | ManualRuleKind::UserAgent
        | ManualRuleKind::Final => false,
    }
}

fn matches_ipv4_cidr(ip: IpAddr, cidr: &str) -> bool {
    let IpAddr::V4(addr) = ip else {
        return false;
    };
    let Some((net_str, prefix_str)) = cidr.split_once('/') else {
        return false;
    };
    let (Ok(network), Ok(prefix)) = (net_str.parse::<Ipv4Addr>(), prefix_str.parse::<u32>()) else {
        return false;
    };
    if prefix > 32 {
        return false;
    }
    let mask = if prefix == 0 {
        0
    } else {
        u32::MAX << (32 - prefix)
    };
    (u32::from(addr) & mask) == (u32::from(network) & mask)
}

fn matches_ipv6_cidr(ip: IpAddr, cidr: &str) -> bool {
    let IpAddr::V6(addr) = ip else {
        return false;
    };
    let Some((net_str, prefix_str)) = cidr.split_once('/') else {
        return false;
    };
    let (Ok(network), Ok(prefix)) = (net_str.parse::<Ipv6Addr>(), prefix_str.parse::<u32>()) else {
        return false;
    };
    if prefix > 128 {
        return false;
    }
    let mask = if prefix == 0 {
        0
    } else {
        u128::MAX << (128 - prefix)
    };
    (u128::from(addr) & mask) == (u128::from(network) & mask)
}

fn wildcard_matches(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    let (mut pi, mut ti) = (0usize, 0usize);
    let (mut star_idx, mut match_idx) = (None, 0usize);
    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star_idx = Some(pi);
            match_idx = ti;
            pi += 1;
        } else if let Some(star) = star_idx {
            pi = star + 1;
            match_idx += 1;
            ti = match_idx;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

pub(super) fn resolve_policy_chain(
    start: &str,
    policy_exits: &BTreeMap<String, String>,
) -> Vec<String> {
    let mut chain = vec![start.to_owned()];
    let mut visited = BTreeSet::from([start.to_owned()]);
    let mut current = start.to_owned();
    while let Some(next) = policy_exits.get(&current) {
        if !visited.insert(next.clone()) {
            break;
        }
        chain.push(next.clone());
        current = next.clone();
    }
    chain
}
