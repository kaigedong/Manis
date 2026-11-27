//! Agent-first CLI frontend sharing the `manis` binary with the GPUI desktop app.
//!
//! Invoking `manis` with no arguments launches the desktop GUI. Passing a subcommand runs
//! headless against the live desktop control socket (falling back to the local config store
//! when offline) and exits without creating a window.

use std::fs;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::control_api::{
    self, ConnectionsData, ConnectionsFilter, ControlRequest, ControlResponse, ExplainParams,
    PoliciesData, ResponseSource, RulesData, SelectPolicyParams, SetRoutingModeParams, StatusData,
    ValidateConfigParams, protocol,
};
use crate::mcp;

const HELP_TEXT: &str = "\
Manis — policy-based proxy routing workbench & Agent CLI

USAGE:
  manis                                  Launch desktop GUI
  manis <COMMAND> [OPTIONS]

COMMANDS:
  status                                 Show runtime, version, proxy/routing mode, traffic
  policies                               List policy groups, current exits, and candidates
  connections                            List active live connections (requires running app)
  rules                                  List ordered manual rules, groups, and fallback
  explain <target>                       Trace rule -> policy group -> exit for host/IP
  select <group> <node>                  Switch a manual policy group's target node
  routing-mode <rule|global|direct>      Switch routing mode
  config show                            Show redacted configuration summary (no secrets)
  config validate <path|->               Dry-run validate a JSONC configuration file or stdin
  mcp                                    Start stdio MCP server for AI agents

COMMON OPTIONS:
  --json                                 Emit machine-readable JSON output
  --store-dir <path>                     Override Manis configuration/socket directory
  -V, --version                          Print version
  -h, --help                             Print help

EXPLAIN OPTIONS:
  --process <name>                       Match process-name rules (e.g. agy)
  --process-path <path>                  Match process-path rules
  --port <port>                          Match destination port rules

CONNECTIONS OPTIONS:
  --host <substr>                        Filter by host or destination IP
  --process <substr>                     Filter by process name or path
  --policy <substr>                      Filter by policy or exit chain
  --limit <n>                            Maximum rows to return (default 50, max 500)";

#[derive(Clone, Debug, Eq, PartialEq)]
enum CliAction {
    Version,
    Help,
    Mcp {
        store_dir: Option<PathBuf>,
    },
    Control {
        request: ControlRequest,
        json: bool,
        store_dir: Option<PathBuf>,
    },
}

/// Checks process arguments and runs the CLI if a subcommand or flag was supplied.
///
/// Returns `None` when the process should launch the GPUI desktop window normally.
#[must_use]
pub fn try_run_from_env() -> Option<ExitCode> {
    let raw_args: Vec<String> = std::env::args().skip(1).collect();
    if raw_args.is_empty() || raw_args.iter().all(|arg| arg.starts_with("-psn_")) {
        return None;
    }

    let args: Vec<&str> = raw_args.iter().map(String::as_str).collect();
    match parse_args(&args) {
        Ok(CliAction::Version) => {
            println!("Manis {}", crate::version());
            Some(ExitCode::SUCCESS)
        }
        Ok(CliAction::Help) => {
            println!("{HELP_TEXT}");
            Some(ExitCode::SUCCESS)
        }
        Ok(CliAction::Mcp { store_dir }) => Some(mcp::run_stdio(store_dir.as_deref())),
        Ok(CliAction::Control {
            request,
            json,
            store_dir,
        }) => Some(run_control_action(&request, json, store_dir.as_deref())),
        Err(message) => {
            eprintln!("error: {message}\n\nRun `manis --help` for usage.");
            Some(ExitCode::from(2))
        }
    }
}

fn run_control_action(request: &ControlRequest, json: bool, store_dir: Option<&Path>) -> ExitCode {
    let response = control_api::execute(store_dir, request);
    if json {
        match serde_json::to_string_pretty(&response) {
            Ok(encoded) => println!("{encoded}"),
            Err(error) => {
                eprintln!("error: failed to serialize response: {error}");
                return ExitCode::FAILURE;
            }
        }
    } else {
        print_human_response(request, &response);
    }
    if response.ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn parse_args(args: &[&str]) -> Result<CliAction, String> {
    let Some((&command, rest)) = args.split_first() else {
        return Ok(CliAction::Help);
    };
    match command {
        "-V" | "--version" => Ok(CliAction::Version),
        "-h" | "--help" | "help" => Ok(CliAction::Help),
        "mcp" => parse_mcp_args(rest),
        "status" => parse_simple_command(ControlRequest::Status, rest),
        "policies" => parse_simple_command(ControlRequest::Policies, rest),
        "rules" => parse_simple_command(ControlRequest::Rules, rest),
        "connections" => parse_connections_args(rest),
        "explain" => parse_explain_args(rest),
        "select" => parse_select_args(rest),
        "routing-mode" => parse_routing_mode_args(rest),
        "config" => parse_config_args(rest),
        other => Err(format!("unknown command '{other}'")),
    }
}

fn parse_mcp_args(args: &[&str]) -> Result<CliAction, String> {
    let mut store_dir = None;
    let mut index = 0;
    while index < args.len() {
        match args[index] {
            "--store-dir" => {
                let value = next_arg(args, &mut index, "--store-dir")?;
                store_dir = Some(PathBuf::from(value));
            }
            "-h" | "--help" => return Ok(CliAction::Help),
            other => return Err(format!("unexpected argument '{other}' for mcp")),
        }
        index += 1;
    }
    Ok(CliAction::Mcp { store_dir })
}

fn parse_simple_command(request: ControlRequest, args: &[&str]) -> Result<CliAction, String> {
    let mut json = false;
    let mut store_dir = None;
    let mut index = 0;
    while index < args.len() {
        match args[index] {
            "--json" => json = true,
            "--store-dir" => {
                let value = next_arg(args, &mut index, "--store-dir")?;
                store_dir = Some(PathBuf::from(value));
            }
            "-h" | "--help" => return Ok(CliAction::Help),
            other => return Err(format!("unexpected argument '{other}'")),
        }
        index += 1;
    }
    Ok(CliAction::Control {
        request,
        json,
        store_dir,
    })
}

fn parse_connections_args(args: &[&str]) -> Result<CliAction, String> {
    let mut filter = ConnectionsFilter::default();
    let mut json = false;
    let mut store_dir = None;
    let mut index = 0;
    while index < args.len() {
        match args[index] {
            "--json" => json = true,
            "--store-dir" => {
                store_dir = Some(PathBuf::from(next_arg(args, &mut index, "--store-dir")?));
            }
            "--host" => {
                filter.host = Some(next_arg(args, &mut index, "--host")?.to_owned());
            }
            "--process" => {
                filter.process = Some(next_arg(args, &mut index, "--process")?.to_owned());
            }
            "--policy" => {
                filter.policy = Some(next_arg(args, &mut index, "--policy")?.to_owned());
            }
            "--limit" => {
                let raw = next_arg(args, &mut index, "--limit")?;
                let parsed = raw
                    .parse::<usize>()
                    .map_err(|_| format!("invalid --limit '{raw}'"))?;
                filter.limit = Some(parsed);
            }
            "-h" | "--help" => return Ok(CliAction::Help),
            other => return Err(format!("unexpected argument '{other}' for connections")),
        }
        index += 1;
    }
    Ok(CliAction::Control {
        request: ControlRequest::Connections(filter),
        json,
        store_dir,
    })
}

fn parse_explain_args(args: &[&str]) -> Result<CliAction, String> {
    let mut target: Option<String> = None;
    let mut process = None;
    let mut process_path = None;
    let mut port = None;
    let mut json = false;
    let mut store_dir = None;
    let mut index = 0;
    while index < args.len() {
        match args[index] {
            "--json" => json = true,
            "--store-dir" => {
                store_dir = Some(PathBuf::from(next_arg(args, &mut index, "--store-dir")?));
            }
            "--process" => {
                process = Some(next_arg(args, &mut index, "--process")?.to_owned());
            }
            "--process-path" => {
                process_path = Some(next_arg(args, &mut index, "--process-path")?.to_owned());
            }
            "--port" => {
                let raw = next_arg(args, &mut index, "--port")?;
                let parsed = raw
                    .parse::<u16>()
                    .map_err(|_| format!("invalid --port '{raw}'"))?;
                port = Some(parsed);
            }
            "-h" | "--help" => return Ok(CliAction::Help),
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option '{flag}' for explain"));
            }
            value if target.is_none() => target = Some(value.to_owned()),
            other => return Err(format!("unexpected extra argument '{other}' for explain")),
        }
        index += 1;
    }
    let Some(target) = target else {
        return Err("explain requires a <target> host or IP".to_owned());
    };
    Ok(CliAction::Control {
        request: ControlRequest::Explain(ExplainParams {
            target,
            process,
            process_path,
            port,
        }),
        json,
        store_dir,
    })
}

fn parse_select_args(args: &[&str]) -> Result<CliAction, String> {
    let mut positionals = Vec::new();
    let mut json = false;
    let mut store_dir = None;
    let mut index = 0;
    while index < args.len() {
        match args[index] {
            "--json" => json = true,
            "--store-dir" => {
                store_dir = Some(PathBuf::from(next_arg(args, &mut index, "--store-dir")?));
            }
            "-h" | "--help" => return Ok(CliAction::Help),
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option '{flag}' for select"));
            }
            value => positionals.push(value.to_owned()),
        }
        index += 1;
    }
    if positionals.len() != 2 {
        return Err("select requires exactly <group> and <node>".to_owned());
    }
    let node = positionals.pop().unwrap_or_default();
    let group = positionals.pop().unwrap_or_default();
    Ok(CliAction::Control {
        request: ControlRequest::SelectPolicy(SelectPolicyParams { group, node }),
        json,
        store_dir,
    })
}

fn parse_routing_mode_args(args: &[&str]) -> Result<CliAction, String> {
    let mut mode = None;
    let mut json = false;
    let mut store_dir = None;
    let mut index = 0;
    while index < args.len() {
        match args[index] {
            "--json" => json = true,
            "--store-dir" => {
                store_dir = Some(PathBuf::from(next_arg(args, &mut index, "--store-dir")?));
            }
            "-h" | "--help" => return Ok(CliAction::Help),
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option '{flag}' for routing-mode"));
            }
            value if mode.is_none() => mode = Some(value.to_owned()),
            other => return Err(format!("unexpected argument '{other}' for routing-mode")),
        }
        index += 1;
    }
    let Some(mode) = mode else {
        return Err("routing-mode requires <rule|global|direct>".to_owned());
    };
    Ok(CliAction::Control {
        request: ControlRequest::SetRoutingMode(SetRoutingModeParams { mode }),
        json,
        store_dir,
    })
}

fn parse_config_args(args: &[&str]) -> Result<CliAction, String> {
    let Some((&sub, rest)) = args.split_first() else {
        return Err("config requires a subcommand: show | validate <path|->".to_owned());
    };
    match sub {
        "show" => parse_simple_command(ControlRequest::ConfigSummary, rest),
        "validate" => {
            let mut path_arg = None;
            let mut json = false;
            let mut store_dir = None;
            let mut index = 0;
            while index < rest.len() {
                match rest[index] {
                    "--json" => json = true,
                    "--store-dir" => {
                        store_dir = Some(PathBuf::from(next_arg(rest, &mut index, "--store-dir")?));
                    }
                    "-h" | "--help" => return Ok(CliAction::Help),
                    "-" if path_arg.is_none() => path_arg = Some("-".to_owned()),
                    flag if flag.starts_with('-') => {
                        return Err(format!("unknown option '{flag}' for config validate"));
                    }
                    value if path_arg.is_none() => path_arg = Some(value.to_owned()),
                    other => {
                        return Err(format!("unexpected argument '{other}' for config validate"));
                    }
                }
                index += 1;
            }
            let Some(path_arg) = path_arg else {
                return Err("config validate requires <path> or '-' for stdin".to_owned());
            };
            let content = if path_arg == "-" {
                let mut buffer = String::new();
                std::io::stdin()
                    .read_to_string(&mut buffer)
                    .map_err(|error| format!("failed to read stdin: {error}"))?;
                buffer
            } else {
                fs::read_to_string(&path_arg)
                    .map_err(|error| format!("failed to read '{path_arg}': {error}"))?
            };
            Ok(CliAction::Control {
                request: ControlRequest::ValidateConfig(ValidateConfigParams { content }),
                json,
                store_dir,
            })
        }
        "-h" | "--help" => Ok(CliAction::Help),
        other => Err(format!("unknown config subcommand '{other}'")),
    }
}

fn next_arg<'a>(args: &[&'a str], index: &mut usize, flag: &str) -> Result<&'a str, String> {
    *index += 1;
    args.get(*index)
        .copied()
        .ok_or_else(|| format!("missing value for {flag}"))
}

fn print_human_response(request: &ControlRequest, response: &ControlResponse) {
    if !response.ok {
        if let Some(error) = &response.error {
            eprintln!("error [{}]: {}", error.code, error.message);
        } else {
            eprintln!("error: request failed");
        }
        return;
    }
    let Some(data) = &response.data else {
        return;
    };
    let source_tag = match response.source {
        ResponseSource::LiveDaemon => "live",
        ResponseSource::OfflineStore => "offline",
    };

    match request {
        ControlRequest::Status => print_status(data, source_tag),
        ControlRequest::Policies => print_policies(data, source_tag),
        ControlRequest::Connections(_) => print_connections(data),
        ControlRequest::Rules => print_rules(data, source_tag),
        ControlRequest::Explain(_) => print_explain(data, source_tag),
        ControlRequest::SelectPolicy(_)
        | ControlRequest::SetRoutingMode(_)
        | ControlRequest::ConfigSummary
        | ControlRequest::ValidateConfig(_) => {
            if let Ok(pretty) = serde_json::to_string_pretty(data) {
                println!("{pretty}");
            }
        }
    }
}

fn print_status(data: &serde_json::Value, source_tag: &str) {
    if let Ok(status) = serde_json::from_value::<StatusData>(data.clone()) {
        println!(
            "Manis {} ({source_tag})\n  core:        {}\n  controller:  {}\n  proxy mode:  {}\n  routing:     {}\n  policies:    {}\n  connections: {}\n  custom DoH:  {}/{}",
            status.app_version,
            status.core_version.as_deref().unwrap_or("unknown"),
            status.controller_state,
            status.proxy_mode,
            status.routing_mode,
            status.policy_group_count,
            status.active_connections,
            status.custom_doh_enabled_count,
            status.custom_doh_total_count
        );
    }
}

fn print_policies(data: &serde_json::Value, source_tag: &str) {
    if let Ok(policies) = serde_json::from_value::<PoliciesData>(data.clone()) {
        println!("Routing mode: {} ({source_tag})", policies.routing_mode);
        for group in policies.groups {
            let current = group.current.as_deref().unwrap_or("—");
            let latency = group
                .latency_ms
                .map_or_else(String::new, |ms| format!(" ({ms}ms)"));
            println!(
                "  {} [{}] -> {current}{latency} ({} candidates)",
                group.name,
                group.strategy,
                group.candidates.len()
            );
        }
    }
}

fn print_connections(data: &serde_json::Value) {
    if let Ok(conns) = serde_json::from_value::<ConnectionsData>(data.clone()) {
        println!(
            "Active connections: {} (showing {})",
            conns.total_active, conns.returned
        );
        for conn in conns.connections {
            let proc = conn.process.as_deref().unwrap_or("-");
            let rule = conn.rule.as_deref().unwrap_or("-");
            let exit = conn.exit.as_deref().unwrap_or("-");
            println!("  {} [{}]  {rule} -> {exit}", conn.target, proc);
        }
    }
}

fn print_rules(data: &serde_json::Value, source_tag: &str) {
    if let Ok(rules) = serde_json::from_value::<RulesData>(data.clone()) {
        println!(
            "Rules ({source_tag}, mode={}, final={})",
            rules.routing_mode, rules.final_target
        );
        for rule in rules.manual_rules {
            let state = if rule.enabled { "on " } else { "off" };
            println!(
                "  [{state}] #{} {} -> {}",
                rule.index, rule.match_expression, rule.target_policy
            );
        }
        for group in rules.local_groups {
            println!(
                "  [local] {} -> {} ({} rules)",
                group.name, group.target_policy, group.rule_count
            );
        }
        for source in rules.remote_sources {
            println!(
                "  [remote] {} -> {} ({} rules)",
                source.name, source.target_policy, source.rule_count
            );
        }
    }
}

fn print_explain(data: &serde_json::Value, source_tag: &str) {
    if let Ok(explain) = serde_json::from_value::<protocol::ExplainData>(data.clone()) {
        println!("Target:  {} ({source_tag})", explain.target);
        println!(
            "Match:   {} ({})",
            explain.matched_rule, explain.matched_source
        );
        println!("Route:   {}", explain.policy_chain.join(" -> "));
        if let Some(note) = explain.note {
            println!("Note:    {note}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_core_cli_subcommands_and_flags() {
        assert_eq!(parse_args(&["--version"]), Ok(CliAction::Version));
        assert_eq!(parse_args(&["-h"]), Ok(CliAction::Help));
        assert_eq!(
            parse_args(&["status", "--json"]),
            Ok(CliAction::Control {
                request: ControlRequest::Status,
                json: true,
                store_dir: None,
            })
        );
        assert_eq!(
            parse_args(&[
                "explain",
                "api.openai.com",
                "--process",
                "agy",
                "--port",
                "443",
                "--json"
            ]),
            Ok(CliAction::Control {
                request: ControlRequest::Explain(ExplainParams {
                    target: "api.openai.com".to_owned(),
                    process: Some("agy".to_owned()),
                    process_path: None,
                    port: Some(443),
                }),
                json: true,
                store_dir: None,
            })
        );
        assert_eq!(
            parse_args(&["select", "🤖 AI Platforms", "233boy-tcp-66.98.118.202"]),
            Ok(CliAction::Control {
                request: ControlRequest::SelectPolicy(SelectPolicyParams {
                    group: "🤖 AI Platforms".to_owned(),
                    node: "233boy-tcp-66.98.118.202".to_owned(),
                }),
                json: false,
                store_dir: None,
            })
        );
        assert_eq!(
            parse_args(&["routing-mode", "global", "--json"]),
            Ok(CliAction::Control {
                request: ControlRequest::SetRoutingMode(SetRoutingModeParams {
                    mode: "global".to_owned(),
                }),
                json: true,
                store_dir: None,
            })
        );
        assert_eq!(
            parse_args(&["mcp", "--store-dir", "/tmp/manis-test"]),
            Ok(CliAction::Mcp {
                store_dir: Some(PathBuf::from("/tmp/manis-test")),
            })
        );
        assert!(parse_args(&["explain"]).is_err());
        assert!(parse_args(&["select", "only-one"]).is_err());
        assert!(parse_args(&["nonexistent"]).is_err());
    }
}
