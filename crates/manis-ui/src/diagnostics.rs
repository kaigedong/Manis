use std::collections::VecDeque;
use std::fs::{self, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::field::{Field, Visit};
use tracing_subscriber::{Layer, layer::Context, prelude::*};

const TRACE_ENV: &str = "MANIS_UI_TRACE";
const LEGACY_RELAY_TRACE_ENV: &str = "RELAY_UI_TRACE";
const UI_LOG_CAPACITY: usize = 512;
const LOG_DIRECTORY: &str = "logs";
const LOG_FILE: &str = "manis-events.log";
const MAX_LOG_BYTES: u64 = 4 * 1024 * 1024;
const MAX_DETAIL_CHARS: usize = 800;
static NEXT_LOG_SEQUENCE: AtomicU64 = AtomicU64::new(1);
static NEXT_OPERATION_ID: AtomicU64 = AtomicU64::new(1);
static DIAGNOSTICS: OnceLock<Arc<Mutex<Diagnostics>>> = OnceLock::new();
static EVENT_DISPATCH: OnceLock<tracing::Dispatch> = OnceLock::new();
const EVENT_TARGET: &str = "manis::events";

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
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
        }
    }
}

/// Enables a durable, user-readable event log and restores its recent entries into the UI.
pub(crate) fn initialize(root: Option<&Path>) {
    let Some(root) = root else {
        return;
    };
    let path = root.join(LOG_DIRECTORY).join(LOG_FILE);
    let diagnostics = diagnostics_state();
    let mut diagnostics = diagnostics
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if diagnostics.path.is_some() {
        return;
    }
    diagnostics.path = Some(path.clone());
    if let Ok(contents) = read_history(&path) {
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

fn diagnostics_state() -> &'static Arc<Mutex<Diagnostics>> {
    DIAGNOSTICS.get_or_init(|| Arc::new(Mutex::new(Diagnostics::default())))
}

fn record(level: LogLevel, operation_id: Option<u64>, event: &str, detail: Option<String>) {
    // Sanitize before emission, not just at the file sink. This dispatch is deliberately private:
    // it neither replaces GPUI's subscriber nor captures third-party HTTP/proxy request logs.
    let event = sanitize_fragment(event);
    let detail = detail.map(|value| sanitize_detail(&value));
    let dispatch = EVENT_DISPATCH
        .get_or_init(|| event_dispatch(Arc::clone(diagnostics_state()), trace_enabled()));
    tracing::dispatcher::with_default(dispatch, || {
        macro_rules! emit {
            ($level:expr) => {
                tracing::event!(target: "manis::events", $level, operation_id, event = event.as_str(), detail = detail.as_deref())
            };
        }
        match level {
            LogLevel::Debug => emit!(tracing::Level::DEBUG),
            LogLevel::Info => emit!(tracing::Level::INFO),
            LogLevel::Warn => emit!(tracing::Level::WARN),
            LogLevel::Error => emit!(tracing::Level::ERROR),
        }
    });
}

fn event_dispatch(state: Arc<Mutex<Diagnostics>>, stderr: bool) -> tracing::Dispatch {
    tracing::Dispatch::new(tracing_subscriber::registry().with(
        EventLayer { state, stderr }.with_filter(tracing_subscriber::filter::filter_fn(
            |metadata| metadata.target() == EVENT_TARGET,
        )),
    ))
}

struct EventLayer {
    state: Arc<Mutex<Diagnostics>>,
    stderr: bool,
}

impl<S: tracing::Subscriber> Layer<S> for EventLayer {
    fn on_event(&self, event: &tracing::Event<'_>, _context: Context<'_, S>) {
        let mut fields = EventFields::default();
        event.record(&mut fields);
        let Some(name) = fields.event else {
            return;
        };
        let mut diagnostics = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let entry = UiLogEntry {
            // Assign under the ring/file lock so concurrent operations remain sequence ordered.
            sequence: NEXT_LOG_SEQUENCE.fetch_add(1, Ordering::Relaxed),
            timestamp_ms: now_ms(),
            level: event.metadata().level().as_str().to_owned(),
            operation_id: fields.operation_id,
            event: name,
            detail: fields.detail,
        };
        if let Some(path) = diagnostics.path.as_deref() {
            append_line(path, &entry);
        }
        if self.stderr {
            eprintln!("{}", format_line(&entry));
        }
        push_bounded(&mut diagnostics.logs, entry);
    }
}

#[derive(Default)]
struct EventFields {
    operation_id: Option<u64>,
    event: Option<String>,
    detail: Option<String>,
}

impl Visit for EventFields {
    fn record_u64(&mut self, field: &Field, value: u64) {
        if field.name() == "operation_id" {
            self.operation_id = Some(value);
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        match field.name() {
            "event" => self.event = Some(sanitize_fragment(value)),
            "detail" => self.detail = Some(sanitize_detail(value)),
            _ => {}
        }
    }

    fn record_debug(&mut self, _field: &Field, _value: &dyn std::fmt::Debug) {
        // Unknown/debug fields are not diagnostic data; never stringify arbitrary secret objects.
    }
}

pub(crate) fn recent_ui_logs() -> Vec<UiLogEntry> {
    diagnostics_state()
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
    serde_json::to_string(entry).expect("primitive diagnostic fields serialize as JSON")
}

/// Restore bounded history even if an external process has enlarged the file.
fn read_history(path: &Path) -> std::io::Result<String> {
    let mut file = fs::File::open(path)?;
    let truncated = file.metadata()?.len() > MAX_LOG_BYTES;
    if truncated {
        file.seek(SeekFrom::End(
            -i64::try_from(MAX_LOG_BYTES).expect("log limit fits i64"),
        ))?;
    }
    let mut bytes = Vec::new();
    file.take(MAX_LOG_BYTES).read_to_end(&mut bytes)?;
    if truncated {
        // The tail may start in the middle of a UTF-8 character or a JSON/legacy record.
        let start = bytes
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(bytes.len(), |pos| pos + 1);
        bytes.drain(..start);
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn parse_line(line: &str) -> Option<UiLogEntry> {
    if line.len() > 16 * 1024 {
        return None;
    }
    let mut entry: UiLogEntry = if line.starts_with('{') {
        serde_json::from_str(line).ok()?
    } else {
        parse_legacy_line(line)?
    };
    entry.event = sanitize_fragment(&entry.event);
    entry.level = sanitize_fragment(&entry.level);
    entry.detail = entry.detail.map(|detail| sanitize_detail(&detail));
    Some(entry)
}

fn parse_legacy_line(line: &str) -> Option<UiLogEntry> {
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
        crate::brand::env_var(TRACE_ENV, LEGACY_RELAY_TRACE_ENV).as_deref(),
        Some("1" | "true" | "debug")
    )
}

#[cfg(test)]
mod tests {

    struct TestDirectory(std::path::PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "manis-diagnostics-{}-{}-{}",
                std::process::id(),
                super::now_ms(),
                super::NEXT_OPERATION_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn tracing_persists_redacted_json_and_ignores_foreign_and_debug_fields() {
        use std::sync::{Arc, Mutex};
        let root = TestDirectory::new();
        let path = root.0.join("logs/manis-events.log");
        let state = Arc::new(Mutex::new(super::Diagnostics {
            path: Some(path.clone()),
            ..Default::default()
        }));
        let dispatch = super::event_dispatch(Arc::clone(&state), false);
        tracing::dispatcher::with_default(&dispatch, || {
            tracing::info!(target: "third_party", event = "foreign.event", detail = "must-not-be-captured");
            tracing::warn!(target: "manis::events", event = "proxy.mode.failed", operation_id = 91_u64,
                detail = "source=https://example.invalid/?token=private token=private",
                unexpected_secret = ?vec!["must-not-be-captured"]);
        });
        let logs = state.lock().unwrap().logs.clone();
        assert_eq!(logs.len(), 1);
        let entry = &logs[0];
        assert_eq!(entry.operation_id, Some(91));
        assert_eq!(entry.level, "WARN");
        let disk = std::fs::read_to_string(&path).unwrap();
        let document: serde_json::Value = serde_json::from_str(&disk).unwrap();
        assert_eq!(document["operation_id"], 91);
        assert!(!disk.contains("private"));
        assert!(!disk.contains("example.invalid"));
        assert!(!disk.contains("must-not-be-captured"));
        assert_eq!(parse_line(disk.trim_end()), Some(entry.clone()));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
            assert_eq!(
                std::fs::metadata(path.parent().unwrap())
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o700
            );
        }
    }

    #[test]
    fn legacy_history_remains_readable_and_is_sanitized() {
        let entry =
            parse_line("42\t7\tINFO\t3\tproxy.mode.requested\tfrom=off token=private").unwrap();
        assert_eq!(entry.sequence, 7);
        assert_eq!(entry.timestamp_ms, 42);
        assert_eq!(entry.operation_id, Some(3));
        assert_eq!(entry.detail.as_deref(), Some("from=off <redacted-token>"));
        assert_eq!(parse_line(&format_line(&entry)), Some(entry));
        assert!(parse_line("{broken").is_none());
        assert!(parse_line(&"x".repeat(16 * 1024 + 1)).is_none());
    }

    #[test]
    fn concurrent_tracing_events_keep_bounded_ordered_history() {
        use std::sync::{Arc, Mutex};
        let state = Arc::new(Mutex::new(super::Diagnostics::default()));
        let dispatch = super::event_dispatch(Arc::clone(&state), false);
        let workers = (0_u64..4).map(|id| {
            let dispatch = dispatch.clone();
            std::thread::spawn(move || tracing::dispatcher::with_default(&dispatch, || {
                for _ in 0..150 {
                    tracing::info!(target: "manis::events", event = "test.concurrent", operation_id = id);
                }
            }))
        }).collect::<Vec<_>>();
        for worker in workers {
            worker.join().unwrap();
        }
        let logs = state
            .lock()
            .unwrap()
            .logs
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(logs.len(), super::UI_LOG_CAPACITY);
        assert!(
            logs.windows(2)
                .all(|pair| pair[0].sequence < pair[1].sequence)
        );
        assert!(logs.iter().all(|entry| entry.operation_id.is_some()));
    }

    #[test]
    fn rotation_keeps_previous_log_and_json_file_is_immediately_readable() {
        let root = TestDirectory::new();
        let path = root.0.join("events.log");
        let file = std::fs::File::create(&path).unwrap();
        file.set_len(super::MAX_LOG_BYTES + 1).unwrap();
        drop(file);
        let entry = UiLogEntry {
            sequence: 1,
            timestamp_ms: 2,
            level: "INFO".into(),
            operation_id: None,
            event: "test.rotation".into(),
            detail: None,
        };
        super::append_line(&path, &entry);
        assert_eq!(
            std::fs::metadata(path.with_extension("log.previous"))
                .unwrap()
                .len(),
            super::MAX_LOG_BYTES + 1
        );
        assert_eq!(
            parse_line(super::read_history(&path).unwrap().trim_end()),
            Some(entry)
        );
    }
    use super::{
        UiEvent, UiLogEntry, format_line, parse_line, recent_ui_logs, sanitize_detail, trace_ui,
    };

    #[test]
    fn durable_line_round_trips_with_operation_and_detail() {
        let entry = UiLogEntry {
            sequence: 7,
            timestamp_ms: 42,
            level: "INFO".to_owned(),
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
        let entry = logs
            .iter()
            .rev()
            .find(|entry| entry.event == "workspace.logs.opened")
            .expect("the event should enter the ring buffer");

        assert_eq!(entry.event, "workspace.logs.opened");
        assert_eq!(entry.level, "DEBUG");
        assert!(!entry.event.contains("token"));
        assert!(!entry.event.contains("http"));
    }
}
