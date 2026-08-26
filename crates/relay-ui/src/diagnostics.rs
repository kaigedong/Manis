use std::collections::VecDeque;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

const TRACE_ENV: &str = "RELAY_UI_TRACE";
const UI_LOG_CAPACITY: usize = 512;
const LOG_DIRECTORY: &str = "logs";
const LOG_FILE: &str = "relay-events.log";
const MAX_LOG_BYTES: u64 = 4 * 1024 * 1024;
const MAX_DETAIL_CHARS: usize = 800;
static NEXT_LOG_SEQUENCE: AtomicU64 = AtomicU64::new(1);
static NEXT_OPERATION_ID: AtomicU64 = AtomicU64::new(1);
static DIAGNOSTICS: OnceLock<Mutex<Diagnostics>> = OnceLock::new();

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct UiLogEntry {
    pub sequence: u64,
    pub timestamp_ms: u128,
    pub level: String,
    pub operation_id: Option<u64>,
    pub event: String,
    pub detail: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

impl LogLevel {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Debug => "DEBUG",
            Self::Info => "INFO",
            Self::Warn => "WARN",
            Self::Error => "ERROR",
        }
    }
}

#[derive(Default)]
struct Diagnostics {
    logs: VecDeque<UiLogEntry>,
    path: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum UiEvent {
    WorkspacePoliciesOpened,
    WorkspaceNodesOpened,
    WorkspaceRoutingRulesOpened,
    WorkspaceActivityOpened,
    WorkspaceLogsOpened,
    WorkspaceConfigurationOpened,
    SourceRecognitionSucceeded,
    SourceRecognitionFailed,
    SourceImportStarted,
    SourceImportSucceeded,
    SourceImportFailed,
    SourceRestoreStarted,
    SourceRestoreSucceeded,
    SourceRestoreFailed,
    SourceRemoveStarted,
    SourceRemoveSucceeded,
    SourceRemoveFailed,
    SubscriptionDraftCleared,
    PolicyPreviewOpened,
    MihomoConnectStarted,
    MihomoConnectSucceeded,
    MihomoConnectFailed,
    GroupBenchmarkStarted,
    GroupBenchmarkSucceeded,
    GroupBenchmarkFailed,
    ThemeLightSelected,
    ThemeDarkSelected,
    SystemProxyEnabled,
    SystemProxyDisabled,
    TunProxyEnabled,
    ProxyModeFailed,
    RoutingModeChanged,
    RoutingModeFailed,
    GlobalNodeSelected,
    GlobalNodeSelectionFailed,
    RouteInspectorOpened,
    RouteInspectorClosed,
    RoutePredictionRequested,
}

impl UiEvent {
    fn as_str(self) -> &'static str {
        match self {
            Self::WorkspacePoliciesOpened => "workspace.policies.opened",
            Self::WorkspaceNodesOpened => "workspace.nodes.opened",
            Self::WorkspaceRoutingRulesOpened => "workspace.routing_rules.opened",
            Self::WorkspaceActivityOpened => "workspace.activity.opened",
            Self::WorkspaceLogsOpened => "workspace.logs.opened",
            Self::WorkspaceConfigurationOpened => "workspace.configuration.opened",
            Self::SourceRecognitionSucceeded => "configuration.source_recognition.succeeded",
            Self::SourceRecognitionFailed => "configuration.source_recognition.failed",
            Self::SourceImportStarted => "configuration.source_import.started",
            Self::SourceImportSucceeded => "configuration.source_import.succeeded",
            Self::SourceImportFailed => "configuration.source_import.failed",
            Self::SourceRestoreStarted => "configuration.source_restore.started",
            Self::SourceRestoreSucceeded => "configuration.source_restore.succeeded",
            Self::SourceRestoreFailed => "configuration.source_restore.failed",
            Self::SourceRemoveStarted => "configuration.source_remove.started",
            Self::SourceRemoveSucceeded => "configuration.source_remove.succeeded",
            Self::SourceRemoveFailed => "configuration.source_remove.failed",
            Self::SubscriptionDraftCleared => "configuration.subscription_draft.cleared",
            Self::PolicyPreviewOpened => "configuration.policy_preview.opened",
            Self::MihomoConnectStarted => "mihomo.connect.started",
            Self::MihomoConnectSucceeded => "mihomo.connect.succeeded",
            Self::MihomoConnectFailed => "mihomo.connect.failed",
            Self::GroupBenchmarkStarted => "group_benchmark.started",
            Self::GroupBenchmarkSucceeded => "group_benchmark.succeeded",
            Self::GroupBenchmarkFailed => "group_benchmark.failed",
            Self::ThemeLightSelected => "theme.light.selected",
            Self::ThemeDarkSelected => "theme.dark.selected",
            Self::SystemProxyEnabled => "system_proxy.enabled",
            Self::SystemProxyDisabled => "system_proxy.disabled",
            Self::TunProxyEnabled => "tun_proxy.enabled",
            Self::ProxyModeFailed => "proxy_mode.failed",
            Self::RoutingModeChanged => "routing_mode.changed",
            Self::RoutingModeFailed => "routing_mode.failed",
            Self::GlobalNodeSelected => "global_node.selected",
            Self::GlobalNodeSelectionFailed => "global_node.selection_failed",
            Self::RouteInspectorOpened => "route_inspector.opened",
            Self::RouteInspectorClosed => "route_inspector.closed",
            Self::RoutePredictionRequested => "route_prediction.requested",
        }
    }
}

/// Enables a durable, user-readable event log and restores its recent entries into the UI.
pub(crate) fn initialize(root: Option<&Path>) {
    let Some(root) = root else {
        return;
    };
    let path = root.join(LOG_DIRECTORY).join(LOG_FILE);
    let diagnostics = DIAGNOSTICS.get_or_init(|| Mutex::new(Diagnostics::default()));
    let mut diagnostics = diagnostics
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if diagnostics.path.is_some() {
        return;
    }
    diagnostics.path = Some(path.clone());
    if let Ok(contents) = fs::read_to_string(&path) {
        for entry in contents.lines().filter_map(parse_line) {
            NEXT_LOG_SEQUENCE.fetch_max(entry.sequence.saturating_add(1), Ordering::Relaxed);
            if let Some(operation_id) = entry.operation_id {
                NEXT_OPERATION_ID.fetch_max(operation_id.saturating_add(1), Ordering::Relaxed);
            }
            push_bounded(&mut diagnostics.logs, entry);
        }
    }
}

pub(crate) fn trace_ui(event: UiEvent) {
    record(LogLevel::Debug, None, event.as_str(), None);
}

/// Starts one user-visible operation and returns its correlation identifier.
pub(crate) fn begin_operation(event: &'static str, detail: impl Into<String>) -> u64 {
    let operation_id = NEXT_OPERATION_ID.fetch_add(1, Ordering::Relaxed);
    record(
        LogLevel::Info,
        Some(operation_id),
        event,
        Some(detail.into()),
    );
    operation_id
}

pub(crate) fn record_operation(
    operation_id: u64,
    level: LogLevel,
    event: &'static str,
    detail: impl Into<String>,
) {
    record(level, Some(operation_id), event, Some(detail.into()));
}

pub(crate) fn record_event(level: LogLevel, event: &'static str, detail: impl Into<String>) {
    record(level, None, event, Some(detail.into()));
}

fn record(level: LogLevel, operation_id: Option<u64>, event: &str, detail: Option<String>) {
    let timestamp_ms = now_ms();
    let sequence = NEXT_LOG_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let entry = UiLogEntry {
        sequence,
        timestamp_ms,
        level: level.as_str().to_owned(),
        operation_id,
        event: sanitize_fragment(event),
        detail: detail.map(|value| sanitize_detail(&value)),
    };
    let diagnostics = DIAGNOSTICS.get_or_init(|| Mutex::new(Diagnostics::default()));
    let mut diagnostics = diagnostics
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(path) = diagnostics.path.as_deref() {
        append_line(path, &entry);
    }
    push_bounded(&mut diagnostics.logs, entry.clone());
    drop(diagnostics);

    if trace_enabled() {
        eprintln!("{}", format_line(&entry));
    }
}

pub(crate) fn recent_ui_logs() -> Vec<UiLogEntry> {
    DIAGNOSTICS
        .get_or_init(|| Mutex::new(Diagnostics::default()))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .logs
        .iter()
        .cloned()
        .collect()
}

fn push_bounded(logs: &mut VecDeque<UiLogEntry>, entry: UiLogEntry) {
    if logs.len() == UI_LOG_CAPACITY {
        logs.pop_front();
    }
    logs.push_back(entry);
}

fn append_line(path: &Path, entry: &UiLogEntry) {
    let Some(parent) = path.parent() else {
        return;
    };
    if fs::create_dir_all(parent).is_err() {
        return;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let _ = fs::set_permissions(parent, fs::Permissions::from_mode(0o700));
    }
    if fs::metadata(path).is_ok_and(|metadata| metadata.len() > MAX_LOG_BYTES) {
        let previous = path.with_extension("log.previous");
        let _ = fs::rename(path, previous);
    }
    let mut options = OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    let Ok(mut file) = options.open(path) else {
        return;
    };
    let _ = writeln!(file, "{}", format_line(entry));
}

fn format_line(entry: &UiLogEntry) -> String {
    format!(
        "{}\t{}\t{}\t{}\t{}\t{}",
        entry.timestamp_ms,
        entry.sequence,
        entry.level,
        entry
            .operation_id
            .map_or_else(|| "-".to_owned(), |value| value.to_string()),
        entry.event,
        entry.detail.as_deref().unwrap_or("-")
    )
}

fn parse_line(line: &str) -> Option<UiLogEntry> {
    let mut fields = line.splitn(6, '\t');
    let timestamp_ms = fields.next()?.parse().ok()?;
    let sequence = fields.next()?.parse().ok()?;
    let level = fields.next()?.to_owned();
    let operation = fields.next()?;
    let operation_id = if operation == "-" {
        None
    } else {
        Some(operation.parse().ok()?)
    };
    let event = fields.next()?.to_owned();
    let detail = match fields.next()? {
        "-" => None,
        detail => Some(detail.to_owned()),
    };
    Some(UiLogEntry {
        sequence,
        timestamp_ms,
        level,
        operation_id,
        event,
        detail,
    })
}

fn sanitize_detail(value: &str) -> String {
    let mut sanitized = value
        .replace(['\r', '\n', '\t'], " ")
        .split_whitespace()
        .map(|word| {
            let lower = word.to_ascii_lowercase();
            if ["http://", "https://", "vless://"]
                .iter()
                .any(|prefix| lower.contains(prefix))
            {
                "<redacted-url>".to_owned()
            } else if lower.contains("token=") {
                "<redacted-token>".to_owned()
            } else {
                word.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    if sanitized.chars().count() > MAX_DETAIL_CHARS {
        sanitized = sanitized.chars().take(MAX_DETAIL_CHARS).collect();
        sanitized.push('…');
    }
    sanitized
}

fn sanitize_fragment(value: &str) -> String {
    value
        .chars()
        .filter(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-')
        })
        .take(120)
        .collect()
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn trace_enabled() -> bool {
    matches!(
        std::env::var(TRACE_ENV).as_deref(),
        Ok("1" | "true" | "debug")
    )
}

#[cfg(test)]
mod tests {
    use super::{
        LogLevel, UiEvent, UiLogEntry, format_line, parse_line, recent_ui_logs, sanitize_detail,
        trace_ui,
    };

    #[test]
    fn durable_line_round_trips_with_operation_and_detail() {
        let entry = UiLogEntry {
            sequence: 7,
            timestamp_ms: 42,
            level: LogLevel::Info.as_str().to_owned(),
            operation_id: Some(3),
            event: "proxy.mode.requested".to_owned(),
            detail: Some("from=off to=tun".to_owned()),
        };

        assert_eq!(parse_line(&format_line(&entry)), Some(entry));
    }

    #[test]
    fn dynamic_details_redact_urls_tokens_and_line_breaks() {
        let detail = sanitize_detail(
            "source=https://example.invalid/a?token=secret\nnode=vless://uuid@example.invalid",
        );

        assert!(!detail.contains("secret"));
        assert!(!detail.contains("example.invalid"));
        assert!(!detail.contains('\n'));
        assert!(detail.contains("<redacted-url>"));
    }

    #[test]
    fn ui_trace_keeps_a_safe_in_memory_log_even_when_stderr_trace_is_disabled() {
        trace_ui(UiEvent::WorkspaceLogsOpened);
        let logs = recent_ui_logs();
        let entry = logs.last().expect("the event should enter the ring buffer");

        assert_eq!(entry.event, "workspace.logs.opened");
        assert_eq!(entry.level, "DEBUG");
        assert!(!entry.event.contains("token"));
        assert!(!entry.event.contains("http"));
    }
}
