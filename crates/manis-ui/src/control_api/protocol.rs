use serde::{Deserialize, Serialize};

pub(crate) const PROTOCOL_VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(tag = "method", content = "params", rename_all = "snake_case")]
pub(crate) enum ControlRequest {
    Status,
    Policies,
    Connections(ConnectionsFilter),
    Rules,
    Explain(ExplainParams),
    SelectPolicy(SelectPolicyParams),
    SetRoutingMode(SetRoutingModeParams),
    ConfigSummary,
    ValidateConfig(ValidateConfigParams),
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
pub(crate) struct ConnectionsFilter {
    #[serde(default)]
    pub(crate) host: Option<String>,
    #[serde(default)]
    pub(crate) process: Option<String>,
    #[serde(default)]
    pub(crate) policy: Option<String>,
    #[serde(default)]
    pub(crate) limit: Option<usize>,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub(crate) struct ExplainParams {
    pub(crate) target: String,
    #[serde(default)]
    pub(crate) process: Option<String>,
    #[serde(default)]
    pub(crate) process_path: Option<String>,
    #[serde(default)]
    pub(crate) port: Option<u16>,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub(crate) struct SelectPolicyParams {
    pub(crate) group: String,
    pub(crate) node: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub(crate) struct SetRoutingModeParams {
    pub(crate) mode: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub(crate) struct ValidateConfigParams {
    pub(crate) content: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub(crate) struct ControlResponse {
    pub(crate) protocol_version: u32,
    pub(crate) ok: bool,
    pub(crate) source: ResponseSource,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) error: Option<ControlError>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ResponseSource {
    LiveDaemon,
    OfflineStore,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub(crate) struct ControlError {
    pub(crate) code: String,
    pub(crate) message: String,
}

impl ControlResponse {
    pub(crate) fn ok<T: Serialize>(source: ResponseSource, data: &T) -> Self {
        match serde_json::to_value(data) {
            Ok(value) => Self {
                protocol_version: PROTOCOL_VERSION,
                ok: true,
                source,
                data: Some(value),
                error: None,
            },
            Err(error) => Self::err(source, "serialization_failed", error.to_string()),
        }
    }

    pub(crate) fn err(
        source: ResponseSource,
        code: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            ok: false,
            source,
            data: None,
            error: Some(ControlError {
                code: code.into(),
                message: message.into(),
            }),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub(crate) struct StatusData {
    pub(crate) app_version: String,
    pub(crate) core_version: Option<String>,
    pub(crate) controller_state: String,
    pub(crate) proxy_mode: String,
    pub(crate) routing_mode: String,
    pub(crate) active_connections: usize,
    pub(crate) upload_total_bytes: u64,
    pub(crate) download_total_bytes: u64,
    pub(crate) policy_group_count: usize,
    pub(crate) custom_doh_enabled_count: usize,
    pub(crate) custom_doh_total_count: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub(crate) struct PoliciesData {
    pub(crate) routing_mode: String,
    pub(crate) global_target: Option<String>,
    pub(crate) groups: Vec<PolicyGroupData>,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub(crate) struct PolicyGroupData {
    pub(crate) name: String,
    pub(crate) strategy: String,
    pub(crate) allows_manual_selection: bool,
    pub(crate) current: Option<String>,
    pub(crate) saved_target: Option<String>,
    pub(crate) latency_ms: Option<u64>,
    pub(crate) candidates: Vec<PolicyCandidateData>,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub(crate) struct PolicyCandidateData {
    pub(crate) name: String,
    pub(crate) kind: String,
    pub(crate) latency_ms: Option<u64>,
    pub(crate) alive: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub(crate) struct ConnectionsData {
    pub(crate) total_active: usize,
    pub(crate) returned: usize,
    pub(crate) connections: Vec<ConnectionData>,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub(crate) struct ConnectionData {
    pub(crate) target: String,
    pub(crate) host: Option<String>,
    pub(crate) destination_ip: Option<String>,
    pub(crate) destination_port: Option<String>,
    pub(crate) network: Option<String>,
    pub(crate) process: Option<String>,
    pub(crate) process_path: Option<String>,
    pub(crate) rule: Option<String>,
    pub(crate) rule_payload: Option<String>,
    pub(crate) chains: Vec<String>,
    pub(crate) exit: Option<String>,
    pub(crate) upload_bytes: u64,
    pub(crate) download_bytes: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub(crate) struct RulesData {
    pub(crate) routing_mode: String,
    pub(crate) group_order: Vec<String>,
    pub(crate) manual_rules: Vec<ManualRuleData>,
    pub(crate) local_groups: Vec<LocalRuleGroupData>,
    pub(crate) remote_sources: Vec<RemoteRuleSourceData>,
    pub(crate) final_target: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub(crate) struct ManualRuleData {
    pub(crate) index: usize,
    pub(crate) enabled: bool,
    pub(crate) is_final: bool,
    pub(crate) match_expression: String,
    pub(crate) target_policy: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub(crate) struct LocalRuleGroupData {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) enabled: bool,
    pub(crate) target_policy: String,
    pub(crate) rule_count: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub(crate) struct RemoteRuleSourceData {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) enabled: bool,
    pub(crate) target_policy: String,
    pub(crate) rule_count: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub(crate) struct ExplainData {
    pub(crate) target: String,
    pub(crate) process: Option<String>,
    pub(crate) routing_mode: String,
    pub(crate) matched_source: String,
    pub(crate) matched_rule: String,
    pub(crate) target_policy: String,
    pub(crate) policy_chain: Vec<String>,
    pub(crate) resolved_exit: Option<String>,
    pub(crate) note: Option<String>,
    pub(crate) matching_live_connections: Vec<ConnectionData>,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub(crate) struct SelectPolicyData {
    pub(crate) group: String,
    pub(crate) node: String,
    pub(crate) persisted: bool,
    pub(crate) applied_live: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub(crate) struct SetRoutingModeData {
    pub(crate) mode: String,
    pub(crate) persisted: bool,
    pub(crate) applied_live: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub(crate) struct ConfigSummaryData {
    pub(crate) redacted: bool,
    pub(crate) routing_mode: String,
    pub(crate) subscriptions: Vec<RedactedSubscriptionData>,
    pub(crate) single_nodes: Vec<RedactedSingleNodeData>,
    pub(crate) custom_proxy_dns: Vec<ProxyDnsEntryData>,
    pub(crate) policy_groups: Vec<String>,
    pub(crate) manual_rule_count: usize,
    pub(crate) local_rule_group_count: usize,
    pub(crate) remote_rule_source_count: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub(crate) struct RedactedSubscriptionData {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) enabled: bool,
    pub(crate) url_host: Option<String>,
    pub(crate) proxy_dns_count: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub(crate) struct RedactedSingleNodeData {
    pub(crate) id: String,
    pub(crate) name: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub(crate) struct ProxyDnsEntryData {
    pub(crate) url: String,
    pub(crate) enabled: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub(crate) struct ValidateConfigData {
    pub(crate) valid: bool,
    pub(crate) subscriptions: usize,
    pub(crate) single_nodes: usize,
    pub(crate) policy_groups: usize,
    pub(crate) rule_sources: usize,
    pub(crate) manual_rules: usize,
    pub(crate) local_rule_groups: usize,
}
