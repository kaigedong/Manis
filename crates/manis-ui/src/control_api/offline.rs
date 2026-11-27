use std::collections::BTreeMap;
use std::path::Path;

use manis_core::{ManagedPolicyGroup, ManagedPolicyStrategy, PolicyCandidateMatcher, RoutingMode};

use crate::{app_update, config_backup, core_update, manual_rule, mihomo};

use super::explain::{ExplainContext, explain_route, format_manual_rule_match};
use super::protocol::{
    ConfigSummaryData, ControlRequest, ControlResponse, LocalRuleGroupData, ManualRuleData,
    PoliciesData, PolicyCandidateData, PolicyGroupData, ProxyDnsEntryData, RedactedSingleNodeData,
    RedactedSubscriptionData, RemoteRuleSourceData, ResponseSource, RulesData, SelectPolicyData,
    SetRoutingModeData, StatusData, ValidateConfigData,
};

pub(crate) fn execute_offline(store_dir: &Path, request: &ControlRequest) -> ControlResponse {
    match request {
        ControlRequest::Status => offline_status(store_dir),
        ControlRequest::Policies => offline_policies(store_dir),
        ControlRequest::Connections(_) => ControlResponse::err(
            ResponseSource::OfflineStore,
            "daemon_not_running",
            "live connections require the Manis desktop app to be running",
        ),
        ControlRequest::Rules => offline_rules(store_dir),
        ControlRequest::Explain(params) => offline_explain(store_dir, params),
        ControlRequest::SelectPolicy(params) => {
            offline_select_policy(store_dir, &params.group, &params.node)
        }
        ControlRequest::SetRoutingMode(params) => offline_set_routing_mode(store_dir, &params.mode),
        ControlRequest::ConfigSummary => offline_config_summary(store_dir),
        ControlRequest::ValidateConfig(params) => validate_config(&params.content),
    }
}

pub(crate) fn validate_config(content: &str) -> ControlResponse {
    match config_backup::prepare_import(content) {
        Ok(prepared) => {
            let summary = prepared.summary();
            ControlResponse::ok(
                ResponseSource::OfflineStore,
                &ValidateConfigData {
                    valid: true,
                    subscriptions: summary.subscriptions,
                    single_nodes: summary.single_nodes,
                    policy_groups: summary.policy_groups,
                    rule_sources: summary.rule_sources,
                    manual_rules: summary.manual_rules,
                    local_rule_groups: summary.local_rule_groups,
                },
            )
        }
        Err(error) => ControlResponse::err(
            ResponseSource::OfflineStore,
            "invalid_configuration",
            error.to_string(),
        ),
    }
}

pub(crate) fn redact_url_host(url: &str) -> Option<String> {
    let (_, rest) = url.split_once("://")?;
    let authority = rest.split(['/', '?', '#']).next()?;
    let host_port = authority.rsplit('@').next()?;
    let host = if host_port.starts_with('[') {
        host_port
            .split_once(']')
            .map_or_else(|| host_port.to_owned(), |(ip, _)| format!("{ip}]"))
    } else {
        host_port
            .split_once(':')
            .map_or(host_port, |(host, _)| host)
            .to_owned()
    };
    (!host.is_empty()).then_some(host)
}

pub(crate) fn parse_routing_mode(input: &str) -> Option<RoutingMode> {
    match input.trim().to_ascii_lowercase().as_str() {
        "rule" => Some(RoutingMode::Rule),
        "global" => Some(RoutingMode::Global),
        "direct" => Some(RoutingMode::Direct),
        _ => None,
    }
}

pub(crate) const fn strategy_wire_name(strategy: ManagedPolicyStrategy) -> &'static str {
    strategy.key()
}

fn offline_status(store_dir: &Path) -> ControlResponse {
    let routing_mode = mihomo::load_routing_mode_in(store_dir).unwrap_or(RoutingMode::Rule);
    let policies = mihomo::load_managed_policy_groups_in(store_dir).unwrap_or_default();
    let doh_entries = mihomo::load_custom_proxy_dns_entries_in(store_dir).unwrap_or_default();
    let doh_enabled = doh_entries.iter().filter(|entry| entry.enabled).count();
    let core_version = core_update::probe_installed_core_version();

    ControlResponse::ok(
        ResponseSource::OfflineStore,
        &StatusData {
            app_version: app_update::current_version().to_owned(),
            core_version,
            controller_state: "offline".to_owned(),
            proxy_mode: "off".to_owned(),
            routing_mode: routing_mode.wire_value().to_owned(),
            active_connections: 0,
            upload_total_bytes: 0,
            download_total_bytes: 0,
            policy_group_count: policies.len(),
            custom_doh_enabled_count: doh_enabled,
            custom_doh_total_count: doh_entries.len(),
        },
    )
}

fn offline_policies(store_dir: &Path) -> ControlResponse {
    let routing_mode = mihomo::load_routing_mode_in(store_dir).unwrap_or(RoutingMode::Rule);
    let selections = mihomo::load_node_selection_preferences_in(store_dir).unwrap_or_default();
    let stored_groups = mihomo::load_managed_policy_groups_in(store_dir).unwrap_or_default();
    let groups = stored_groups
        .iter()
        .map(|group| offline_policy_group_data(group, &stored_groups, &selections))
        .collect();

    ControlResponse::ok(
        ResponseSource::OfflineStore,
        &PoliciesData {
            routing_mode: routing_mode.wire_value().to_owned(),
            global_target: selections
                .global()
                .map(|identity| identity.node_name.clone()),
            groups,
        },
    )
}

fn offline_policy_group_data(
    group: &ManagedPolicyGroup,
    all_groups: &[ManagedPolicyGroup],
    selections: &mihomo::NodeSelectionPreferences,
) -> PolicyGroupData {
    let saved_target = selections.policy_target(&group.name).map(str::to_owned);
    let candidates = match &group.matcher {
        PolicyCandidateMatcher::Explicit(members) => members
            .iter()
            .map(|member| {
                let is_group = member.source_id.starts_with("policy:")
                    || all_groups
                        .iter()
                        .any(|other| other.name == member.node_name);
                PolicyCandidateData {
                    name: member.node_name.clone(),
                    kind: if is_group {
                        "policy_group".to_owned()
                    } else {
                        "node".to_owned()
                    },
                    latency_ms: None,
                    alive: None,
                }
            })
            .collect::<Vec<_>>(),
        PolicyCandidateMatcher::All | PolicyCandidateMatcher::NameContains(_) => Vec::new(),
    };
    let current = saved_target
        .clone()
        .or_else(|| candidates.first().map(|candidate| candidate.name.clone()));
    PolicyGroupData {
        name: group.name.clone(),
        strategy: strategy_wire_name(group.strategy).to_owned(),
        allows_manual_selection: group.strategy == ManagedPolicyStrategy::Manual,
        current,
        saved_target,
        latency_ms: None,
        candidates,
    }
}

fn offline_rules(store_dir: &Path) -> ControlResponse {
    let routing_mode = mihomo::load_routing_mode_in(store_dir).unwrap_or(RoutingMode::Rule);
    let manual_rules = manual_rule::load_manual_rules_in(store_dir).unwrap_or_default();
    let local_groups = mihomo::load_local_routing_rule_groups_in(store_dir).unwrap_or_default();
    let remote_sources = mihomo::load_qx_rule_sources_in(store_dir).unwrap_or_default();
    let saved_order = mihomo::load_routing_rule_group_order_in(store_dir).unwrap_or_default();
    let has_manual_rules = manual_rules.iter().any(|rule| !rule.is_final());
    let order = mihomo::normalized_routing_rule_group_order(
        &saved_order,
        has_manual_rules,
        &remote_sources,
        &local_groups,
    );

    ControlResponse::ok(
        ResponseSource::OfflineStore,
        &build_rules_data(
            routing_mode,
            &order,
            &manual_rules,
            &local_groups,
            &remote_sources,
        ),
    )
}

pub(crate) fn build_rules_data(
    routing_mode: RoutingMode,
    order: &[String],
    manual_rules: &[manual_rule::ManualRule],
    local_groups: &[mihomo::LocalRoutingRuleGroup],
    remote_sources: &[mihomo::StoredQxRuleSource],
) -> RulesData {
    let final_target = manual_rules
        .iter()
        .rev()
        .find(|rule| rule.is_enabled() && rule.is_final())
        .map_or_else(|| "Proxy".to_owned(), |rule| rule.target().to_owned());

    RulesData {
        routing_mode: routing_mode.wire_value().to_owned(),
        group_order: order.to_vec(),
        manual_rules: manual_rules
            .iter()
            .enumerate()
            .map(|(index, rule)| ManualRuleData {
                index,
                enabled: rule.is_enabled(),
                is_final: rule.is_final(),
                match_expression: format_manual_rule_match(rule),
                target_policy: rule.target().to_owned(),
            })
            .collect(),
        local_groups: local_groups
            .iter()
            .map(|group| LocalRuleGroupData {
                id: group.id.clone(),
                name: group.name.clone(),
                enabled: group.enabled,
                target_policy: group.target_policy.as_str().to_owned(),
                rule_count: group.rules.len(),
            })
            .collect(),
        remote_sources: remote_sources
            .iter()
            .map(|source| RemoteRuleSourceData {
                id: source.id.clone(),
                name: source.name.clone().unwrap_or_else(|| source.id.clone()),
                enabled: source.enabled,
                target_policy: source.target_policy.as_str().to_owned(),
                rule_count: source.rule_count,
            })
            .collect(),
        final_target,
    }
}

fn offline_explain(store_dir: &Path, params: &super::protocol::ExplainParams) -> ControlResponse {
    if params.target.trim().is_empty() {
        return ControlResponse::err(
            ResponseSource::OfflineStore,
            "invalid_argument",
            "target host or IP must not be empty",
        );
    }
    let routing_mode = mihomo::load_routing_mode_in(store_dir).unwrap_or(RoutingMode::Rule);
    let manual_rules = manual_rule::load_manual_rules_in(store_dir).unwrap_or_default();
    let local_groups = mihomo::load_local_routing_rule_groups_in(store_dir).unwrap_or_default();
    let remote_sources = mihomo::load_qx_rule_sources_in(store_dir).unwrap_or_default();
    let saved_order = mihomo::load_routing_rule_group_order_in(store_dir).unwrap_or_default();
    let has_manual_rules = manual_rules.iter().any(|rule| !rule.is_final());
    let order = mihomo::normalized_routing_rule_group_order(
        &saved_order,
        has_manual_rules,
        &remote_sources,
        &local_groups,
    );
    let selections = mihomo::load_node_selection_preferences_in(store_dir).unwrap_or_default();
    let mut policy_exits = BTreeMap::new();
    for (group, target) in selections.iter_policy_targets() {
        policy_exits.insert(group.to_owned(), target.to_owned());
    }
    let ctx = ExplainContext {
        routing_mode,
        global_target: selections
            .global()
            .map(|identity| identity.node_name.as_str()),
        group_order: &order,
        manual_rules: &manual_rules,
        local_groups: &local_groups,
        remote_sources: &remote_sources,
        policy_exits: &policy_exits,
        live_connections: Vec::new(),
    };
    ControlResponse::ok(ResponseSource::OfflineStore, &explain_route(params, &ctx))
}

fn offline_select_policy(store_dir: &Path, group_name: &str, node_name: &str) -> ControlResponse {
    let groups = match mihomo::load_managed_policy_groups_in(store_dir) {
        Ok(groups) => groups,
        Err(error) => {
            return ControlResponse::err(
                ResponseSource::OfflineStore,
                "store_unavailable",
                error.to_string(),
            );
        }
    };
    let Some(group) = groups.iter().find(|group| group.name == group_name) else {
        return ControlResponse::err(
            ResponseSource::OfflineStore,
            "policy_not_found",
            format!("policy group '{group_name}' was not found"),
        );
    };
    if group.strategy != ManagedPolicyStrategy::Manual {
        return ControlResponse::err(
            ResponseSource::OfflineStore,
            "policy_not_manual",
            format!(
                "policy group '{group_name}' uses automatic selection and cannot be switched manually"
            ),
        );
    }
    let mut selections = mihomo::load_node_selection_preferences_in(store_dir).unwrap_or_default();
    if let Err(error) = selections.set_policy_target(group_name, node_name) {
        return ControlResponse::err(
            ResponseSource::OfflineStore,
            "invalid_argument",
            error.to_string(),
        );
    }
    if let Err(error) = mihomo::save_node_selection_preferences_in(store_dir, &selections) {
        return ControlResponse::err(
            ResponseSource::OfflineStore,
            "persistence_failed",
            error.to_string(),
        );
    }
    ControlResponse::ok(
        ResponseSource::OfflineStore,
        &SelectPolicyData {
            group: group_name.to_owned(),
            node: node_name.to_owned(),
            persisted: true,
            applied_live: false,
        },
    )
}

fn offline_set_routing_mode(store_dir: &Path, mode_str: &str) -> ControlResponse {
    let Some(mode) = parse_routing_mode(mode_str) else {
        return ControlResponse::err(
            ResponseSource::OfflineStore,
            "invalid_argument",
            format!("unsupported routing mode '{mode_str}' (expected rule, global, or direct)"),
        );
    };
    if let Err(error) = mihomo::save_routing_mode_in(store_dir, mode) {
        return ControlResponse::err(
            ResponseSource::OfflineStore,
            "persistence_failed",
            error.to_string(),
        );
    }
    ControlResponse::ok(
        ResponseSource::OfflineStore,
        &SetRoutingModeData {
            mode: mode.wire_value().to_owned(),
            persisted: true,
            applied_live: false,
        },
    )
}

fn offline_config_summary(store_dir: &Path) -> ControlResponse {
    let routing_mode = mihomo::load_routing_mode_in(store_dir).unwrap_or(RoutingMode::Rule);
    let subscriptions = mihomo::load_subscription_sources_in(store_dir)
        .unwrap_or_default()
        .into_iter()
        .map(|sub| RedactedSubscriptionData {
            id: sub.id,
            name: sub.name,
            enabled: sub.enabled,
            url_host: sub.source.expose_to(redact_url_host),
            proxy_dns_count: sub.proxy_server_nameservers.len(),
        })
        .collect::<Vec<_>>();
    let single_nodes = mihomo::load_single_node_sources_in(store_dir).unwrap_or_default();
    let custom_doh = mihomo::load_custom_proxy_dns_entries_in(store_dir).unwrap_or_default();
    let policy_groups = mihomo::load_managed_policy_groups_in(store_dir).unwrap_or_default();
    let manual_rules = manual_rule::load_manual_rules_in(store_dir).unwrap_or_default();
    let local_groups = mihomo::load_local_routing_rule_groups_in(store_dir).unwrap_or_default();
    let remote_sources = mihomo::load_qx_rule_sources_in(store_dir).unwrap_or_default();

    ControlResponse::ok(
        ResponseSource::OfflineStore,
        &build_config_summary(
            routing_mode,
            subscriptions,
            &single_nodes,
            &custom_doh,
            &policy_groups,
            manual_rules.len(),
            local_groups.len(),
            remote_sources.len(),
        ),
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn build_config_summary(
    routing_mode: RoutingMode,
    subscriptions: Vec<RedactedSubscriptionData>,
    single_nodes: &[mihomo::StoredSingleNode],
    custom_doh: &[mihomo::StoredProxyDnsEntry],
    policy_groups: &[ManagedPolicyGroup],
    manual_rule_count: usize,
    local_rule_group_count: usize,
    remote_rule_source_count: usize,
) -> ConfigSummaryData {
    ConfigSummaryData {
        redacted: true,
        routing_mode: routing_mode.wire_value().to_owned(),
        subscriptions,
        single_nodes: single_nodes
            .iter()
            .map(|node| RedactedSingleNodeData {
                id: node.id.clone(),
                name: node.name.clone(),
            })
            .collect(),
        custom_proxy_dns: custom_doh
            .iter()
            .map(|entry| ProxyDnsEntryData {
                url: entry.server.as_str().to_owned(),
                enabled: entry.enabled,
            })
            .collect(),
        policy_groups: policy_groups.iter().map(|g| g.name.clone()).collect(),
        manual_rule_count,
        local_rule_group_count,
        remote_rule_source_count,
    }
}
