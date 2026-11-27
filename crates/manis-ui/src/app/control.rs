use std::collections::BTreeMap;

use gpui::Context;
use manis_core::{PolicyCandidateKind, PolicyGroupId, PolicyGroupKind, ProxyId, ProxyMode};

use crate::{
    app_update,
    control_api::{
        ConnectionsFilter, ControlRequest, ControlResponse, ExplainContext, ExplainParams,
        PoliciesData, PolicyCandidateData, PolicyGroupData, RedactedSubscriptionData,
        ResponseSource, SelectPolicyData, SelectPolicyParams, SetRoutingModeData,
        SetRoutingModeParams, StatusData, build_config_summary, build_rules_data, explain_route,
        filter_connections, parse_routing_mode, redact_url_host, strategy_wire_name,
        validate_config,
    },
    mihomo::{self, ControllerState},
};

use super::{ManisApp, PolicySelectionRequest};

impl ManisApp {
    pub(crate) fn handle_control_request(
        &mut self,
        request: ControlRequest,
        cx: &mut Context<Self>,
    ) -> ControlResponse {
        match request {
            ControlRequest::Status => self.control_status(),
            ControlRequest::Policies => self.control_policies(),
            ControlRequest::Connections(filter) => self.control_connections(&filter),
            ControlRequest::Rules => self.control_rules(),
            ControlRequest::Explain(params) => self.control_explain(&params),
            ControlRequest::SelectPolicy(params) => self.control_select_policy(params, cx),
            ControlRequest::SetRoutingMode(params) => self.control_set_routing_mode(&params, cx),
            ControlRequest::ConfigSummary => self.control_config_summary(),
            ControlRequest::ValidateConfig(params) => validate_config(&params.content),
        }
    }

    fn control_status(&self) -> ControlResponse {
        let (controller_state, active_connections, upload_total_bytes, download_total_bytes) =
            match &self.controller {
                ControllerState::Disconnected => ("disconnected", 0, 0, 0),
                ControllerState::Connecting { .. } => ("connecting", 0, 0, 0),
                ControllerState::Connected {
                    active_connections,
                    upload_total,
                    download_total,
                    ..
                } => (
                    "connected",
                    *active_connections,
                    *upload_total,
                    *download_total,
                ),
                ControllerState::Failed { .. } => ("failed", 0, 0, 0),
            };
        let proxy_mode = match self.proxy_mode {
            ProxyMode::Off => "off",
            ProxyMode::System => "system",
            ProxyMode::Tun => "tun",
        };
        let core_version = self
            .mihomo_core_update_state
            .current_version()
            .map(str::to_owned)
            .or_else(|| match &self.controller {
                ControllerState::Connected { version, .. } if !version.trim().is_empty() => {
                    Some(version.clone())
                }
                _ => None,
            });
        let policy_group_count = if self.catalog.is_some() {
            self.policy_groups()
                .filter(|group| group.name != manis_profile::MANIS_GLOBAL_GROUP_NAME)
                .count()
        } else {
            self.managed_policies.groups.len()
        };
        let custom_doh_enabled_count = self
            .custom_proxy_dns_entries
            .iter()
            .filter(|entry| entry.enabled)
            .count();

        ControlResponse::ok(
            ResponseSource::LiveDaemon,
            &StatusData {
                app_version: app_update::current_version().to_owned(),
                core_version,
                controller_state: controller_state.to_owned(),
                proxy_mode: proxy_mode.to_owned(),
                routing_mode: self.routing_mode.wire_value().to_owned(),
                active_connections,
                upload_total_bytes,
                download_total_bytes,
                policy_group_count,
                custom_doh_enabled_count,
                custom_doh_total_count: self.custom_proxy_dns_entries.len(),
            },
        )
    }

    fn control_policies(&self) -> ControlResponse {
        let groups = if self.catalog.is_some() {
            self.policy_groups()
                .filter(|group| group.name != manis_profile::MANIS_GLOBAL_GROUP_NAME)
                .map(|group| {
                    let selected = self.selected_node_for_policy(group);
                    let latency_ms = selected.and_then(|node| {
                        self.resolved_policy_exit_latency(
                            &node.name,
                            node.latency_ms.map(u64::from),
                        )
                        .0
                    });
                    let strategy = match group.kind {
                        PolicyGroupKind::Selector => "manual",
                        PolicyGroupKind::UrlTest => "latency",
                        PolicyGroupKind::Fallback => "fallback",
                        PolicyGroupKind::LoadBalance => "load_balance",
                        PolicyGroupKind::Direct => "direct",
                    };
                    let candidates = group
                        .nodes
                        .iter()
                        .map(|node| PolicyCandidateData {
                            name: node.name.clone(),
                            kind: match node.kind {
                                PolicyCandidateKind::PolicyGroup => "policy_group".to_owned(),
                                PolicyCandidateKind::Node => "node".to_owned(),
                            },
                            latency_ms: node.latency_ms.map(u64::from),
                            alive: node.alive,
                        })
                        .collect();
                    PolicyGroupData {
                        name: group.name.clone(),
                        strategy: strategy.to_owned(),
                        allows_manual_selection: group.kind.allows_manual_selection(),
                        current: selected.map(|node| node.name.clone()),
                        saved_target: self
                            .managed_policies
                            .node_selections
                            .policy_target(&group.name)
                            .map(str::to_owned),
                        latency_ms,
                        candidates,
                    }
                })
                .collect()
        } else {
            self.managed_policies
                .groups
                .iter()
                .map(|group| {
                    let candidates = self
                        .managed_policy_candidate_nodes(group)
                        .into_iter()
                        .map(|node| PolicyCandidateData {
                            name: node.name,
                            kind: match node.kind {
                                PolicyCandidateKind::PolicyGroup => "policy_group".to_owned(),
                                PolicyCandidateKind::Node => "node".to_owned(),
                            },
                            latency_ms: node.latency_ms.map(u64::from),
                            alive: node.alive,
                        })
                        .collect::<Vec<_>>();
                    let saved_target = self
                        .managed_policies
                        .node_selections
                        .policy_target(&group.name)
                        .map(str::to_owned);
                    let current = saved_target
                        .clone()
                        .or_else(|| candidates.first().map(|candidate| candidate.name.clone()));
                    PolicyGroupData {
                        name: group.name.clone(),
                        strategy: strategy_wire_name(group.strategy).to_owned(),
                        allows_manual_selection: group.strategy
                            == manis_core::ManagedPolicyStrategy::Manual,
                        current,
                        saved_target,
                        latency_ms: None,
                        candidates,
                    }
                })
                .collect()
        };

        ControlResponse::ok(
            ResponseSource::LiveDaemon,
            &PoliciesData {
                routing_mode: self.routing_mode.wire_value().to_owned(),
                global_target: self.global_target().map(str::to_owned),
                groups,
            },
        )
    }

    fn control_connections(&self, filter: &ConnectionsFilter) -> ControlResponse {
        ControlResponse::ok(
            ResponseSource::LiveDaemon,
            &filter_connections(&self.active_connections, filter),
        )
    }

    fn control_rules(&self) -> ControlResponse {
        let has_manual_rules = self.manual_rules.iter().any(|rule| !rule.is_final());
        let order = mihomo::normalized_routing_rule_group_order(
            &self.rule_sources.group_order,
            has_manual_rules,
            &self.rule_sources.sources,
            &self.local_rule_groups,
        );
        ControlResponse::ok(
            ResponseSource::LiveDaemon,
            &build_rules_data(
                self.routing_mode,
                &order,
                &self.manual_rules,
                &self.local_rule_groups,
                &self.rule_sources.sources,
            ),
        )
    }

    fn control_explain(&self, params: &ExplainParams) -> ControlResponse {
        if params.target.trim().is_empty() {
            return ControlResponse::err(
                ResponseSource::LiveDaemon,
                "invalid_argument",
                "target host or IP must not be empty",
            );
        }
        let has_manual_rules = self.manual_rules.iter().any(|rule| !rule.is_final());
        let order = mihomo::normalized_routing_rule_group_order(
            &self.rule_sources.group_order,
            has_manual_rules,
            &self.rule_sources.sources,
            &self.local_rule_groups,
        );
        let mut policy_exits = BTreeMap::new();
        if self.catalog.is_some() {
            for group in self.policy_groups() {
                if let Some(selected) = self.selected_node_for_policy(group) {
                    policy_exits.insert(group.name.clone(), selected.name.clone());
                }
            }
        } else {
            for (group, target) in self.managed_policies.node_selections.iter_policy_targets() {
                policy_exits.insert(group.to_owned(), target.to_owned());
            }
        }
        let matching_live = filter_connections(
            &self.active_connections,
            &ConnectionsFilter {
                host: Some(params.target.clone()),
                process: params.process.clone(),
                policy: None,
                limit: Some(10),
            },
        )
        .connections;
        let ctx = ExplainContext {
            routing_mode: self.routing_mode,
            global_target: self.global_target(),
            group_order: &order,
            manual_rules: &self.manual_rules,
            local_groups: &self.local_rule_groups,
            remote_sources: &self.rule_sources.sources,
            policy_exits: &policy_exits,
            live_connections: matching_live,
        };
        ControlResponse::ok(ResponseSource::LiveDaemon, &explain_route(params, &ctx))
    }

    fn control_select_policy(
        &mut self,
        params: SelectPolicyParams,
        cx: &mut Context<Self>,
    ) -> ControlResponse {
        if self.configuration_transfer.active {
            return ControlResponse::err(
                ResponseSource::LiveDaemon,
                "busy",
                "configuration transfer is in progress",
            );
        }
        if self.policy_selection_busy.is_some() {
            return ControlResponse::err(
                ResponseSource::LiveDaemon,
                "busy",
                "another policy switch is already in progress",
            );
        }

        let (group_id, node_id) =
            match self.resolve_manual_policy_target(&params.group, &params.node) {
                Ok(ids) => ids,
                Err((code, message)) => {
                    return ControlResponse::err(ResponseSource::LiveDaemon, code, message);
                }
            };

        let applied_live = matches!(self.controller, ControllerState::Connected { .. })
            && matches!(&*self.runtime, mihomo::ControllerRuntime::Managed { .. });
        self.select_policy_node(
            PolicySelectionRequest {
                group_id,
                group_name: params.group.clone(),
                node_id,
                node_name: params.node.clone(),
            },
            cx,
        );

        ControlResponse::ok(
            ResponseSource::LiveDaemon,
            &SelectPolicyData {
                group: params.group,
                node: params.node,
                persisted: self.subscription_store_dir.is_some(),
                applied_live,
            },
        )
    }

    fn resolve_manual_policy_target(
        &self,
        group_name: &str,
        node_name: &str,
    ) -> Result<(PolicyGroupId, ProxyId), (&'static str, String)> {
        let resolved = self
            .policy_groups()
            .find(|group| group.name == group_name)
            .map(|group| {
                let node_id = group
                    .nodes
                    .iter()
                    .find(|node| node.name == node_name)
                    .map(|node| node.id.clone());
                (
                    group.id.clone(),
                    group.kind.allows_manual_selection(),
                    node_id,
                )
            });

        match resolved {
            Some((_, false, _)) => Err((
                "policy_not_manual",
                format!(
                    "policy group '{group_name}' uses automatic selection and cannot be switched manually"
                ),
            )),
            Some((group_id, true, Some(node_id))) => Ok((group_id, node_id)),
            Some((_, true, None)) => Err((
                "candidate_not_found",
                format!("candidate '{node_name}' is not in policy group '{group_name}'"),
            )),
            None => {
                let Some(stored) = self
                    .managed_policies
                    .groups
                    .iter()
                    .find(|group| group.name == group_name)
                else {
                    return Err((
                        "policy_not_found",
                        format!("policy group '{group_name}' was not found"),
                    ));
                };
                if stored.strategy != manis_core::ManagedPolicyStrategy::Manual {
                    return Err((
                        "policy_not_manual",
                        format!(
                            "policy group '{group_name}' uses automatic selection and cannot be switched manually"
                        ),
                    ));
                }
                if !self
                    .managed_policy_candidate_names(stored)
                    .iter()
                    .any(|candidate| candidate == node_name)
                {
                    return Err((
                        "candidate_not_found",
                        format!("candidate '{node_name}' is not in policy group '{group_name}'"),
                    ));
                }
                Ok((
                    PolicyGroupId::new(stored.id.clone()),
                    ProxyId::new(node_name.to_owned()),
                ))
            }
        }
    }

    fn control_set_routing_mode(
        &mut self,
        params: &SetRoutingModeParams,
        cx: &mut Context<Self>,
    ) -> ControlResponse {
        let Some(mode) = parse_routing_mode(&params.mode) else {
            return ControlResponse::err(
                ResponseSource::LiveDaemon,
                "invalid_argument",
                format!(
                    "unsupported routing mode '{}' (expected rule, global, or direct)",
                    params.mode
                ),
            );
        };
        if self.configuration_transfer.active || self.routing_mode_busy.is_some() {
            return ControlResponse::err(
                ResponseSource::LiveDaemon,
                "busy",
                "another routing mode change is already in progress",
            );
        }
        let applied_live = matches!(self.controller, ControllerState::Connected { .. })
            && !self.runtime.is_fixture();
        if applied_live {
            self.apply_routing_mode(mode, cx);
            ControlResponse::ok(
                ResponseSource::LiveDaemon,
                &SetRoutingModeData {
                    mode: mode.wire_value().to_owned(),
                    persisted: self.subscription_store_dir.is_some(),
                    applied_live: true,
                },
            )
        } else {
            if let Some(dir) = self.subscription_store_dir.as_deref()
                && let Err(error) = mihomo::save_routing_mode_in(dir, mode)
            {
                return ControlResponse::err(
                    ResponseSource::LiveDaemon,
                    "persistence_failed",
                    error.to_string(),
                );
            }
            self.routing_mode = mode;
            cx.notify();
            ControlResponse::ok(
                ResponseSource::LiveDaemon,
                &SetRoutingModeData {
                    mode: mode.wire_value().to_owned(),
                    persisted: self.subscription_store_dir.is_some(),
                    applied_live: false,
                },
            )
        }
    }

    fn control_config_summary(&self) -> ControlResponse {
        let subscriptions = self
            .imported_subscriptions
            .iter()
            .map(|sub| RedactedSubscriptionData {
                id: sub.id.clone(),
                name: sub.name.clone(),
                enabled: sub.enabled,
                url_host: sub.source.expose_to(redact_url_host),
                proxy_dns_count: sub.proxy_server_nameservers.len(),
            })
            .collect();
        ControlResponse::ok(
            ResponseSource::LiveDaemon,
            &build_config_summary(
                self.routing_mode,
                subscriptions,
                &self.saved_single_nodes,
                &self.custom_proxy_dns_entries,
                &self.managed_policies.groups,
                self.manual_rules.len(),
                self.local_rule_groups.len(),
                self.rule_sources.sources.len(),
            ),
        )
    }
}
