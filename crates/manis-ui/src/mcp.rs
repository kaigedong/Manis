//! Built-in stdio Model Context Protocol (MCP) server (`manis mcp`).
//!
//! Exposes Manis observability, deterministic route explanation, and validated routing mutations
//! as JSON-Schema-typed MCP tools backed by the local Control API. All secrets are redacted.

use std::io::{self, BufRead as _, Write as _};
use std::path::Path;
use std::process::ExitCode;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::control_api::{
    self, ConnectionsFilter, ControlRequest, ExplainParams, SelectPolicyParams,
    SetRoutingModeParams, ValidateConfigParams,
};

const MCP_PROTOCOL_VERSION: &str = "2024-11-05";

#[derive(Deserialize)]
struct JsonRpcRequest {
    #[serde(default)]
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Serialize)]
struct JsonRpcResponse {
    jsonrpc: &'static str,
    id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Serialize)]
struct JsonRpcError {
    code: i64,
    message: String,
}

pub(crate) fn run_stdio(store_dir: Option<&Path>) -> ExitCode {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut out = stdout.lock();

    for line in stdin.lock().lines() {
        let Ok(line) = line else {
            break;
        };
        if line.trim().is_empty() {
            continue;
        }
        if let Some(reply) = handle_jsonrpc_line(store_dir, &line)
            && (writeln!(out, "{reply}").is_err() || out.flush().is_err())
        {
            break;
        }
    }
    ExitCode::SUCCESS
}

pub(crate) fn handle_jsonrpc_line(store_dir: Option<&Path>, line: &str) -> Option<String> {
    let request = match serde_json::from_str::<JsonRpcRequest>(line) {
        Ok(req) => req,
        Err(error) => {
            return encode_response(&JsonRpcResponse {
                jsonrpc: "2.0",
                id: Value::Null,
                result: None,
                error: Some(JsonRpcError {
                    code: -32700,
                    message: format!("parse error: {error}"),
                }),
            });
        }
    };

    // JSON-RPC notifications have no id and expect no reply.
    let id = request.id?;

    let response = match request.method.as_str() {
        "initialize" => JsonRpcResponse {
            jsonrpc: "2.0",
            id,
            result: Some(json!({
                "protocolVersion": MCP_PROTOCOL_VERSION,
                "capabilities": {
                    "tools": {}
                },
                "serverInfo": {
                    "name": "manis",
                    "version": crate::version()
                }
            })),
            error: None,
        },
        "ping" => JsonRpcResponse {
            jsonrpc: "2.0",
            id,
            result: Some(json!({})),
            error: None,
        },
        "tools/list" => JsonRpcResponse {
            jsonrpc: "2.0",
            id,
            result: Some(json!({ "tools": tool_definitions() })),
            error: None,
        },
        "tools/call" => handle_tool_call(id, store_dir, &request.params),
        other => JsonRpcResponse {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(JsonRpcError {
                code: -32601,
                message: format!("method not found: {other}"),
            }),
        },
    };
    encode_response(&response)
}

fn encode_response(response: &JsonRpcResponse) -> Option<String> {
    serde_json::to_string(response).ok()
}

fn handle_tool_call(id: Value, store_dir: Option<&Path>, params: &Value) -> JsonRpcResponse {
    let Some(name) = params.get("name").and_then(Value::as_str) else {
        return JsonRpcResponse {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(JsonRpcError {
                code: -32602,
                message: "tools/call requires string 'name'".to_owned(),
            }),
        };
    };
    let args = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));

    let control_request = match parse_tool_request(name, &args) {
        Ok(request) => request,
        Err(message) => {
            return JsonRpcResponse {
                jsonrpc: "2.0",
                id,
                result: Some(json!({
                    "content": [{ "type": "text", "text": message }],
                    "isError": true
                })),
                error: None,
            };
        }
    };

    let response = control_api::execute(store_dir, &control_request);
    let text = serde_json::to_string_pretty(&response)
        .unwrap_or_else(|error| format!("{{\"ok\":false,\"error\":\"{error}\"}}"));
    JsonRpcResponse {
        jsonrpc: "2.0",
        id,
        result: Some(json!({
            "content": [{ "type": "text", "text": text }],
            "isError": !response.ok
        })),
        error: None,
    }
}

fn parse_tool_request(name: &str, args: &Value) -> Result<ControlRequest, String> {
    match name {
        "manis_status" => Ok(ControlRequest::Status),
        "manis_list_policies" => Ok(ControlRequest::Policies),
        "manis_list_connections" => {
            let filter: ConnectionsFilter = serde_json::from_value(args.clone())
                .map_err(|error| format!("invalid arguments for {name}: {error}"))?;
            Ok(ControlRequest::Connections(filter))
        }
        "manis_list_rules" => Ok(ControlRequest::Rules),
        "manis_explain_route" => {
            let params: ExplainParams = serde_json::from_value(args.clone())
                .map_err(|error| format!("invalid arguments for {name}: {error}"))?;
            Ok(ControlRequest::Explain(params))
        }
        "manis_select_policy" => {
            let params: SelectPolicyParams = serde_json::from_value(args.clone())
                .map_err(|error| format!("invalid arguments for {name}: {error}"))?;
            Ok(ControlRequest::SelectPolicy(params))
        }
        "manis_set_routing_mode" => {
            let params: SetRoutingModeParams = serde_json::from_value(args.clone())
                .map_err(|error| format!("invalid arguments for {name}: {error}"))?;
            Ok(ControlRequest::SetRoutingMode(params))
        }
        "manis_config_summary" => Ok(ControlRequest::ConfigSummary),
        "manis_validate_config" => {
            let params: ValidateConfigParams = serde_json::from_value(args.clone())
                .map_err(|error| format!("invalid arguments for {name}: {error}"))?;
            Ok(ControlRequest::ValidateConfig(params))
        }
        other => Err(format!("unknown tool '{other}'")),
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "Single declarative MCP tool catalog keeps schemas, hints, and names aligned"
)]
fn tool_definitions() -> Value {
    json!([
        {
            "name": "manis_status",
            "description": "Inspect Manis runtime status: app version, Mihomo core version, controller state, proxy mode, routing mode, active connection count, and DoH count.",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "additionalProperties": false
            },
            "annotations": {
                "readOnlyHint": true,
                "destructiveHint": false,
                "idempotentHint": true
            }
        },
        {
            "name": "manis_list_policies",
            "description": "List all policy groups with their strategy (manual/latency/fallback/load_balance), current selected exit, saved target, latency, and candidates.",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "additionalProperties": false
            },
            "annotations": {
                "readOnlyHint": true,
                "destructiveHint": false,
                "idempotentHint": true
            }
        },
        {
            "name": "manis_list_connections",
            "description": "List active live network connections observed by Mihomo, optionally filtered by host/IP, process name, or policy chain.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "host": { "type": "string", "description": "Case-insensitive substring match against host or destination IP" },
                    "process": { "type": "string", "description": "Case-insensitive substring match against process name or path" },
                    "policy": { "type": "string", "description": "Case-insensitive substring match against policy/exit chain" },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 500, "description": "Maximum connections to return (default 50)" }
                },
                "additionalProperties": false
            },
            "annotations": {
                "readOnlyHint": true,
                "destructiveHint": false,
                "idempotentHint": true
            }
        },
        {
            "name": "manis_list_rules",
            "description": "List ordered manual routing rules, local rule groups, remote rule sources, and the final fallback policy.",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "additionalProperties": false
            },
            "annotations": {
                "readOnlyHint": true,
                "destructiveHint": false,
                "idempotentHint": true
            }
        },
        {
            "name": "manis_explain_route",
            "description": "Deterministically explain how a host, IP, or process is routed: matching rule -> policy group -> multi-hop resolved exit node, plus any matching live connections.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "target": { "type": "string", "description": "Hostname (e.g. api.openai.com) or IP address (e.g. 8.133.123.137)" },
                    "process": { "type": "string", "description": "Optional process name (e.g. agy)" },
                    "process_path": { "type": "string", "description": "Optional full executable path" },
                    "port": { "type": "integer", "minimum": 1, "maximum": 65535, "description": "Optional destination port" }
                },
                "required": ["target"],
                "additionalProperties": false
            },
            "annotations": {
                "readOnlyHint": true,
                "destructiveHint": false,
                "idempotentHint": true
            }
        },
        {
            "name": "manis_select_policy",
            "description": "Switch a manual-selection policy group to a validated candidate node or sub-group. Persists to config.toml and applies immediately if Manis is running.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "group": { "type": "string", "description": "Exact policy group name (e.g. Proxy or 🤖 AI Platforms)" },
                    "node": { "type": "string", "description": "Exact candidate node or sub-group name" }
                },
                "required": ["group", "node"],
                "additionalProperties": false
            },
            "annotations": {
                "readOnlyHint": false,
                "destructiveHint": false,
                "idempotentHint": true
            }
        },
        {
            "name": "manis_set_routing_mode",
            "description": "Switch Manis routing mode between 'rule', 'global', and 'direct'. Persists to config.toml and applies live when running.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "mode": { "type": "string", "enum": ["rule", "global", "direct"] }
                },
                "required": ["mode"],
                "additionalProperties": false
            },
            "annotations": {
                "readOnlyHint": false,
                "destructiveHint": false,
                "idempotentHint": true
            }
        },
        {
            "name": "manis_config_summary",
            "description": "Return a redacted summary of the Manis configuration (subscription names/hosts without tokens, DoH servers, policy groups, rule counts).",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "additionalProperties": false
            },
            "annotations": {
                "readOnlyHint": true,
                "destructiveHint": false,
                "idempotentHint": true
            }
        },
        {
            "name": "manis_validate_config",
            "description": "Dry-run validate a Manis JSONC configuration document without modifying disk state, returning counts or validation errors.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "content": { "type": "string", "description": "Full Manis JSONC configuration text to validate" }
                },
                "required": ["content"],
                "additionalProperties": false
            },
            "annotations": {
                "readOnlyHint": true,
                "destructiveHint": false,
                "idempotentHint": true
            }
        }
    ])
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::mihomo::save_managed_policy_in;
    use manis_core::{ManagedPolicyGroup, ManagedPolicyStrategy};

    #[cfg(not(windows))]
    #[test]
    fn mcp_initialize_list_and_call_tools_end_to_end() {
        let store = std::env::temp_dir().join(format!("manis-mcp-test-{}", std::process::id()));
        if store.exists() {
            fs::remove_dir_all(&store).expect("clean stale dir");
        }
        fs::create_dir_all(&store).expect("create temp store");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            fs::set_permissions(&store, fs::Permissions::from_mode(0o700)).expect("chmod 700");
        }
        let mut group = ManagedPolicyGroup::new("policy-1", "Proxy").expect("valid policy");
        group.strategy = ManagedPolicyStrategy::Manual;
        save_managed_policy_in(&store, &group).expect("save policy");

        // Initialize handshake
        let init_reply = handle_jsonrpc_line(
            Some(&store),
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
        )
        .expect("init reply");
        let init_json: Value = serde_json::from_str(&init_reply).expect("parse init");
        assert_eq!(init_json["result"]["serverInfo"]["name"], "manis");

        // Notification yields no response
        assert!(
            handle_jsonrpc_line(
                Some(&store),
                r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#
            )
            .is_none()
        );

        // List tools
        let list_reply = handle_jsonrpc_line(
            Some(&store),
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
        )
        .expect("list reply");
        let list_json: Value = serde_json::from_str(&list_reply).expect("parse list");
        let tools = list_json["result"]["tools"]
            .as_array()
            .expect("tools array");
        assert_eq!(tools.len(), 9);

        // Call manis_select_policy + manis_status
        let select_reply = handle_jsonrpc_line(
            Some(&store),
            r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"manis_select_policy","arguments":{"group":"Proxy","node":"JP-01"}}}"#,
        )
        .expect("select reply");
        let select_json: Value = serde_json::from_str(&select_reply).expect("parse select");
        assert_eq!(select_json["result"]["isError"], false);

        let status_reply = handle_jsonrpc_line(
            Some(&store),
            r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"manis_status","arguments":{}}}"#,
        )
        .expect("status reply");
        let status_json: Value = serde_json::from_str(&status_reply).expect("parse status");
        assert_eq!(status_json["result"]["isError"], false);
        assert!(
            status_json["result"]["content"][0]["text"]
                .as_str()
                .expect("text")
                .contains("\"policy_group_count\": 1")
        );

        fs::remove_dir_all(&store).expect("cleanup");
    }
}
