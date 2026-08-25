use std::time::{SystemTime, UNIX_EPOCH};

const TRACE_ENV: &str = "RELAY_UI_TRACE";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum UiEvent {
    WorkspacePoliciesOpened,
    WorkspaceConfigurationOpened,
    ConfigurationGroupsOpened,
    ConfigurationRulesOpened,
    SourceDiagnosticsOpened,
    SourceDiagnosticsClosed,
    PolicyPreviewOpened,
    RulePreviewOpened,
    MihomoConnectStarted,
    MihomoConnectSucceeded,
    MihomoConnectFailed,
    ThemeLightSelected,
    ThemeDarkSelected,
    SystemProxyEnabled,
    SystemProxyDisabled,
    RouteInspectorOpened,
    RouteInspectorClosed,
    RoutePredictionRequested,
}

impl UiEvent {
    fn as_str(self) -> &'static str {
        match self {
            Self::WorkspacePoliciesOpened => "workspace.policies.opened",
            Self::WorkspaceConfigurationOpened => "workspace.configuration.opened",
            Self::ConfigurationGroupsOpened => "configuration.groups.opened",
            Self::ConfigurationRulesOpened => "configuration.rules.opened",
            Self::SourceDiagnosticsOpened => "configuration.source_diagnostics.opened",
            Self::SourceDiagnosticsClosed => "configuration.source_diagnostics.closed",
            Self::PolicyPreviewOpened => "configuration.policy_preview.opened",
            Self::RulePreviewOpened => "configuration.rule_preview.opened",
            Self::MihomoConnectStarted => "mihomo.connect.started",
            Self::MihomoConnectSucceeded => "mihomo.connect.succeeded",
            Self::MihomoConnectFailed => "mihomo.connect.failed",
            Self::ThemeLightSelected => "theme.light.selected",
            Self::ThemeDarkSelected => "theme.dark.selected",
            Self::SystemProxyEnabled => "system_proxy.enabled",
            Self::SystemProxyDisabled => "system_proxy.disabled",
            Self::RouteInspectorOpened => "route_inspector.opened",
            Self::RouteInspectorClosed => "route_inspector.closed",
            Self::RoutePredictionRequested => "route_prediction.requested",
        }
    }
}

pub(crate) fn trace_ui(event: UiEvent) {
    if !trace_enabled() {
        return;
    }
    let timestamp_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    eprintln!("{}", format_event(timestamp_ms, event));
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
    use super::{UiEvent, format_event};

    #[test]
    fn ui_trace_is_structured_and_contains_no_dynamic_values() {
        let line = format_event(42, UiEvent::SourceDiagnosticsOpened);

        assert_eq!(
            line,
            "relay_ui ts_ms=42 level=DEBUG event=configuration.source_diagnostics.opened"
        );
        assert!(!line.contains("token"));
        assert!(!line.contains("http"));
        assert!(!line.contains('/'));
    }
}
