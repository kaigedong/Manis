use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

const TRACE_ENV: &str = "RELAY_UI_TRACE";
const UI_LOG_CAPACITY: usize = 256;
static NEXT_LOG_SEQUENCE: AtomicU64 = AtomicU64::new(1);
static UI_LOGS: OnceLock<Mutex<VecDeque<UiLogEntry>>> = OnceLock::new();

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct UiLogEntry {
    pub sequence: u64,
    pub timestamp_ms: u128,
    pub event: &'static str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum UiEvent {
    WorkspacePoliciesOpened,
    WorkspaceNodesOpened,
    WorkspaceActivityOpened,
    WorkspaceLogsOpened,
    WorkspaceConfigurationOpened,
    ConfigurationGroupsOpened,
    ConfigurationRulesOpened,
    SubscriptionInputFocused,
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
    RulePreviewOpened,
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
            Self::WorkspaceActivityOpened => "workspace.activity.opened",
            Self::WorkspaceLogsOpened => "workspace.logs.opened",
            Self::WorkspaceConfigurationOpened => "workspace.configuration.opened",
            Self::ConfigurationGroupsOpened => "configuration.groups.opened",
            Self::ConfigurationRulesOpened => "configuration.rules.opened",
            Self::SubscriptionInputFocused => "configuration.subscription_input.focused",
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
            Self::RulePreviewOpened => "configuration.rule_preview.opened",
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

pub(crate) fn trace_ui(event: UiEvent) {
    let timestamp_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let sequence = NEXT_LOG_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let logs = UI_LOGS.get_or_init(|| Mutex::new(VecDeque::with_capacity(UI_LOG_CAPACITY)));
    let mut logs = logs
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if logs.len() == UI_LOG_CAPACITY {
        logs.pop_front();
    }
    logs.push_back(UiLogEntry {
        sequence,
        timestamp_ms,
        event: event.as_str(),
    });
    drop(logs);

    if !trace_enabled() {
        return;
    }
    eprintln!("{}", format_event(timestamp_ms, event));
}

pub(crate) fn recent_ui_logs() -> Vec<UiLogEntry> {
    UI_LOGS
        .get_or_init(|| Mutex::new(VecDeque::with_capacity(UI_LOG_CAPACITY)))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .iter()
        .cloned()
        .collect()
}

fn trace_enabled() -> bool {
    matches!(
        std::env::var(TRACE_ENV).as_deref(),
        Ok("1" | "true" | "debug")
    )
}

fn format_event(timestamp_ms: u128, event: UiEvent) -> String {
    format!(
        "relay_ui ts_ms={timestamp_ms} level=DEBUG event={}",
        event.as_str()
    )
}

#[cfg(test)]
mod tests {
    use super::{UiEvent, format_event, recent_ui_logs, trace_ui};

    #[test]
    fn ui_trace_is_structured_and_contains_no_dynamic_values() {
        let line = format_event(42, UiEvent::SubscriptionInputFocused);

        assert_eq!(
            line,
            "relay_ui ts_ms=42 level=DEBUG event=configuration.subscription_input.focused"
        );
        assert!(!line.contains("token"));
        assert!(!line.contains("http"));
        assert!(!line.contains('/'));
    }

    #[test]
    fn ui_trace_keeps_a_safe_in_memory_log_even_when_stderr_trace_is_disabled() {
        trace_ui(UiEvent::WorkspaceLogsOpened);
        let logs = recent_ui_logs();
        let entry = logs.last().expect("the event should enter the ring buffer");

        assert_eq!(entry.event, "workspace.logs.opened");
        assert!(!entry.event.contains("token"));
        assert!(!entry.event.contains("http"));
    }
}
