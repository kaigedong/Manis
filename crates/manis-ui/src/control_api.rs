//! Local agent-first Control API (live Unix domain socket + offline config store fallback).
//!
//! All agent-facing responses redact subscription credentials, node secrets, and controller
//! tokens by default.

use std::path::{Path, PathBuf};

use manis_mihomo::Connection;

mod explain;
mod offline;
pub(crate) mod protocol;
mod server;

pub(crate) use explain::{ExplainContext, explain_route};
pub(crate) use offline::{
    build_config_summary, build_rules_data, parse_routing_mode, redact_url_host,
    strategy_wire_name, validate_config,
};
pub(crate) use protocol::{
    ConnectionData, ConnectionsData, ConnectionsFilter, ControlRequest, ControlResponse,
    ExplainParams, PoliciesData, PolicyCandidateData, PolicyGroupData, RedactedSubscriptionData,
    ResponseSource, RulesData, SelectPolicyData, SelectPolicyParams, SetRoutingModeData,
    SetRoutingModeParams, StatusData, ValidateConfigParams,
};
pub(crate) use server::install_in;

pub(crate) fn default_store_dir() -> Option<PathBuf> {
    crate::brand::data_dir()
}

/// Executes a control request, talking to the live Manis desktop daemon when available and
/// falling back to the local configuration store when offline.
pub(crate) fn execute(store_dir: Option<&Path>, request: &ControlRequest) -> ControlResponse {
    if let ControlRequest::ValidateConfig(params) = request {
        return validate_config(&params.content);
    }
    let resolved = store_dir.map(Path::to_path_buf).or_else(default_store_dir);
    let Some(directory) = resolved else {
        return ControlResponse::err(
            ResponseSource::OfflineStore,
            "data_dir_unavailable",
            "could not locate the Manis data directory",
        );
    };
    execute_in_store(&directory, request)
}

pub(crate) fn execute_in_store(directory: &Path, request: &ControlRequest) -> ControlResponse {
    if let ControlRequest::ValidateConfig(params) = request {
        return validate_config(&params.content);
    }
    if let Ok(response) = server::send_request(directory, request) {
        return response;
    }
    offline::execute_offline(directory, request)
}

pub(crate) fn connection_to_data(connection: &Connection) -> ConnectionData {
    let host = connection
        .metadata
        .sniff_host
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            connection
                .metadata
                .host
                .as_deref()
                .filter(|value| !value.trim().is_empty())
        })
        .map(str::to_owned);
    let dest_ip = connection
        .metadata
        .destination_ip
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .map(str::to_owned);
    let dest_port = connection
        .metadata
        .destination_port
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .map(str::to_owned);
    let base_target = host.as_deref().or(dest_ip.as_deref()).unwrap_or("unknown");
    let target = match dest_port.as_deref() {
        Some(port) => format!("{base_target}:{port}"),
        None => base_target.to_owned(),
    };
    let exit = connection.chains.first().cloned();

    ConnectionData {
        target,
        host,
        destination_ip: dest_ip,
        destination_port: dest_port,
        network: connection.metadata.network.clone(),
        process: connection.metadata.process.clone(),
        process_path: connection.metadata.process_path.clone(),
        rule: connection.rule.clone(),
        rule_payload: connection.rule_payload.clone(),
        chains: connection.chains.clone(),
        exit,
        upload_bytes: connection.upload,
        download_bytes: connection.download,
    }
}

pub(crate) fn filter_connections(
    connections: &[Connection],
    filter: &ConnectionsFilter,
) -> ConnectionsData {
    let host_needle = filter
        .host
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_ascii_lowercase);
    let process_needle = filter
        .process
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_ascii_lowercase);
    let policy_needle = filter
        .policy
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_ascii_lowercase);
    let limit = filter.limit.unwrap_or(50).min(500);

    let mut matched = Vec::new();
    for connection in connections {
        let data = connection_to_data(connection);
        if let Some(needle) = host_needle.as_deref() {
            let matches_host = data.target.to_ascii_lowercase().contains(needle)
                || data
                    .host
                    .as_deref()
                    .is_some_and(|value| value.to_ascii_lowercase().contains(needle))
                || data
                    .destination_ip
                    .as_deref()
                    .is_some_and(|value| value.to_ascii_lowercase().contains(needle));
            if !matches_host {
                continue;
            }
        }
        if let Some(needle) = process_needle.as_deref() {
            let matches_process = data
                .process
                .as_deref()
                .is_some_and(|value| value.to_ascii_lowercase().contains(needle))
                || data
                    .process_path
                    .as_deref()
                    .is_some_and(|value| value.to_ascii_lowercase().contains(needle));
            if !matches_process {
                continue;
            }
        }
        if let Some(needle) = policy_needle.as_deref() {
            let matches_policy = data
                .chains
                .iter()
                .any(|chain| chain.to_ascii_lowercase().contains(needle));
            if !matches_policy {
                continue;
            }
        }
        matched.push(data);
        if matched.len() >= limit {
            break;
        }
    }

    ConnectionsData {
        total_active: connections.len(),
        returned: matched.len(),
        connections: matched,
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use manis_core::{ManagedPolicyGroup, ManagedPolicyStrategy, RoutingMode};

    use super::*;
    use crate::manual_rule::{ManualRule, ManualRuleKind, save_manual_rules_in};
    use crate::mihomo::{
        NodeSelectionPreferences, save_managed_policy_in, save_node_selection_preferences_in,
        save_routing_mode_in, save_subscription_source_in,
    };

    fn temp_store(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("manis-control-api-{name}-{}", std::process::id()));
        if dir.exists() {
            fs::remove_dir_all(&dir).expect("clean stale temp dir");
        }
        fs::create_dir_all(&dir).expect("create temp dir");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).expect("chmod 700");
        }
        dir
    }

    #[cfg(not(windows))]
    #[test]
    fn offline_store_supports_status_policies_explain_select_and_redaction() {
        let store = temp_store("offline");

        let mut ai_group =
            ManagedPolicyGroup::new("policy-1", "🤖 AI Platforms").expect("valid policy");
        ai_group.strategy = ManagedPolicyStrategy::Manual;
        save_managed_policy_in(&store, &ai_group).expect("save ai group");

        let mut proxy_group = ManagedPolicyGroup::new("policy-2", "Proxy").expect("valid policy");
        proxy_group.strategy = ManagedPolicyStrategy::Manual;
        save_managed_policy_in(&store, &proxy_group).expect("save proxy group");

        let mut selections = NodeSelectionPreferences::default();
        selections
            .set_policy_target("🤖 AI Platforms", "Proxy")
            .expect("set ai");
        selections
            .set_policy_target("Proxy", "233boy-tcp-66.98.118.202")
            .expect("set proxy");
        save_node_selection_preferences_in(&store, &selections).expect("save selections");

        let rules = vec![
            ManualRule::parse(ManualRuleKind::HostSuffix, "openai.com", "🤖 AI Platforms")
                .expect("rule"),
            ManualRule::parse(ManualRuleKind::ProcessName, "agy", "🤖 AI Platforms")
                .expect("process rule"),
            ManualRule::final_rule("Proxy").expect("final"),
        ];
        save_manual_rules_in(&store, &rules).expect("save manual rules");
        save_routing_mode_in(&store, RoutingMode::Rule).expect("save mode");
        save_subscription_source_in(
            &store,
            "https://sub.example.com/api/v1/subscribe?token=SUPER_SECRET_TOKEN_123",
        )
        .expect("save subscription");

        // Explain resolves full multi-hop chain: api.openai.com -> 🤖 AI Platforms -> Proxy -> 233boy
        let explain = execute_in_store(
            &store,
            &ControlRequest::Explain(ExplainParams {
                target: "api.openai.com".to_owned(),
                process: None,
                process_path: None,
                port: Some(443),
            }),
        );
        assert!(explain.ok, "{explain:?}");
        let explain_data: protocol::ExplainData =
            serde_json::from_value(explain.data.expect("data")).expect("decode explain");
        assert_eq!(explain_data.target_policy, "🤖 AI Platforms");
        assert_eq!(
            explain_data.policy_chain,
            vec![
                "🤖 AI Platforms".to_owned(),
                "Proxy".to_owned(),
                "233boy-tcp-66.98.118.202".to_owned()
            ]
        );
        assert_eq!(
            explain_data.resolved_exit.as_deref(),
            Some("233boy-tcp-66.98.118.202")
        );

        // SelectPolicy persists to config.toml and updates explain immediately
        let switched = execute_in_store(
            &store,
            &ControlRequest::SelectPolicy(SelectPolicyParams {
                group: "🤖 AI Platforms".to_owned(),
                node: "Tokyo-01".to_owned(),
            }),
        );
        assert!(switched.ok, "{switched:?}");
        let explain_after = execute_in_store(
            &store,
            &ControlRequest::Explain(ExplainParams {
                target: "142.251.170.95".to_owned(),
                process: Some("agy".to_owned()),
                process_path: None,
                port: Some(443),
            }),
        );
        let explain_after: protocol::ExplainData =
            serde_json::from_value(explain_after.data.expect("data")).expect("decode explain");
        assert_eq!(explain_after.resolved_exit.as_deref(), Some("Tokyo-01"));

        // ConfigSummary must NEVER leak raw subscription token
        let summary = execute_in_store(&store, &ControlRequest::ConfigSummary);
        assert!(summary.ok, "{summary:?}");
        let summary_json = serde_json::to_string(&summary).expect("serialize summary");
        assert!(!summary_json.contains("SUPER_SECRET_TOKEN_123"));
        assert!(summary_json.contains("sub.example.com"));

        // ValidateConfig dry-runs valid and rejects invalid JSONC
        let valid = execute_in_store(
            &store,
            &ControlRequest::ValidateConfig(ValidateConfigParams {
                content: r#"{"format":"manis-config-v1","routing":{"mode":"rule"}}"#.to_owned(),
            }),
        );
        assert!(valid.ok, "{valid:?}");
        let invalid = execute_in_store(
            &store,
            &ControlRequest::ValidateConfig(ValidateConfigParams {
                content: "{not valid json".to_owned(),
            }),
        );
        assert!(!invalid.ok);

        fs::remove_dir_all(&store).expect("cleanup");
    }

    #[cfg(unix)]
    #[test]
    fn unix_socket_transport_enforces_permissions_and_round_trips() {
        use std::os::unix::fs::PermissionsExt as _;

        let store = temp_store("unix-transport");
        let store_for_server = store.clone();
        let socket_path = server::spawn_test_socket_server(&store, move |request| {
            let mut response = offline::execute_offline(&store_for_server, &request);
            response.source = ResponseSource::LiveDaemon;
            response
        })
        .expect("spawn test socket");
        let mode = fs::symlink_metadata(&socket_path)
            .expect("stat socket")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);

        let response =
            server::send_request(&store, &ControlRequest::Status).expect("socket round trip");
        assert!(response.ok, "{response:?}");
        assert_eq!(response.source, ResponseSource::LiveDaemon);

        fs::remove_dir_all(&store).expect("cleanup");
    }

    #[gpui::test]
    fn live_manis_app_control_requests_round_trip(cx: &mut gpui::TestAppContext) {
        use gpui::AppContext as _;

        let store = temp_store("live-app");
        let app = cx.new(|_cx| {
            crate::ManisApp::with_fixture_controller_and_subscription_store(
                "http://127.0.0.1:9090",
                store.clone(),
            )
        });

        let status = cx.update(|cx| {
            app.update(cx, |app, cx| {
                app.handle_control_request(ControlRequest::Status, cx)
            })
        });
        assert!(status.ok, "{status:?}");
        assert_eq!(status.source, ResponseSource::LiveDaemon);

        let policies = cx.update(|cx| {
            app.update(cx, |app, cx| {
                app.handle_control_request(ControlRequest::Policies, cx)
            })
        });
        assert!(policies.ok, "{policies:?}");

        let mode_switch = cx.update(|cx| {
            app.update(cx, |app, cx| {
                app.handle_control_request(
                    ControlRequest::SetRoutingMode(SetRoutingModeParams {
                        mode: "global".to_owned(),
                    }),
                    cx,
                )
            })
        });
        assert!(mode_switch.ok, "{mode_switch:?}");

        let explain = cx.update(|cx| {
            app.update(cx, |app, cx| {
                app.handle_control_request(
                    ControlRequest::Explain(ExplainParams {
                        target: "api.openai.com".to_owned(),
                        process: None,
                        process_path: None,
                        port: Some(443),
                    }),
                    cx,
                )
            })
        });
        assert!(explain.ok, "{explain:?}");
        let explain_data: protocol::ExplainData =
            serde_json::from_value(explain.data.expect("data")).expect("decode explain");
        assert_eq!(explain_data.routing_mode, "global");

        fs::remove_dir_all(&store).expect("cleanup");
    }
}
