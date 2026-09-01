use super::{
    ControllerState, CountNoun, Language, ProxyMode, RoutingMode, StatusTone, Theme, copy,
};

pub(super) struct StatusBarValues {
    pub(super) endpoint: String,
    pub(super) download: String,
    pub(super) upload: String,
    pub(super) dot: gpui::Rgba,
    pub(super) tone: StatusTone,
}

pub(super) fn status_bar_values(
    controller: &ControllerState,
    language: Language,
    theme: Theme,
) -> StatusBarValues {
    match controller {
        ControllerState::Disconnected => StatusBarValues {
            endpoint: language.localized(copy::app::NO_RUNTIME_DATA).to_owned(),
            download: "↓ —".to_owned(),
            upload: "↑ —".to_owned(),
            dot: theme.route_trace,
            tone: StatusTone::Warning,
        },
        ControllerState::Connecting { endpoint } => StatusBarValues {
            endpoint: endpoint.clone(),
            download: "↓ —".to_owned(),
            upload: "↑ —".to_owned(),
            dot: theme.route_trace,
            tone: StatusTone::Route,
        },
        ControllerState::Failed { endpoint, .. } => StatusBarValues {
            endpoint: endpoint.clone(),
            download: "↓ —".to_owned(),
            upload: "↑ —".to_owned(),
            dot: theme.status_error,
            tone: StatusTone::Error,
        },
        ControllerState::Connected {
            endpoint,
            download_total,
            upload_total,
            ..
        } => StatusBarValues {
            endpoint: endpoint.clone(),
            download: format!(
                "{}↓ {}",
                language.localized(copy::app::TOTAL),
                format_bytes(*download_total)
            ),
            upload: format!(
                "{}↑ {}",
                language.localized(copy::app::TOTAL),
                format_bytes(*upload_total)
            ),
            dot: theme.status_success,
            tone: StatusTone::Success,
        },
    }
}

pub(super) fn format_bytes(bytes: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = KIB * 1024;
    const GIB: u64 = MIB * 1024;

    if bytes >= GIB {
        format_bytes_in_unit(bytes, GIB, "GiB")
    } else if bytes >= MIB {
        format_bytes_in_unit(bytes, MIB, "MiB")
    } else if bytes >= KIB {
        format_bytes_in_unit(bytes, KIB, "KiB")
    } else {
        format!("{bytes} B")
    }
}

fn format_bytes_in_unit(bytes: u64, unit: u64, suffix: &str) -> String {
    let whole = bytes / unit;
    let tenth = (bytes % unit) * 10 / unit;
    format!("{whole}.{tenth} {suffix}")
}

/// Builds the one-line controller summary shown in the status bar.
///
/// The kernel name is supplied by the caller so the line always names the kernel that is
/// actually running rather than assuming Mihomo.
pub(super) fn controller_status_label(
    controller: &ControllerState,
    kernel_name: &str,
    language: Language,
) -> String {
    match controller {
        ControllerState::Disconnected => {
            format!(
                "{kernel_name} {}",
                language.localized(copy::app::DISCONNECTED)
            )
        }
        ControllerState::Connecting { .. } => {
            format!(
                "{kernel_name} {}",
                language.localized(copy::app::CONNECTING)
            )
        }
        ControllerState::Connected {
            version,
            active_connections,
            ..
        } => format!(
            "{kernel_name} {version} · {}",
            language.count(CountNoun::Connection, *active_connections)
        ),
        // The reason travels with the label: the sidebar used to be the only place it appeared.
        ControllerState::Failed { message, .. } => format!(
            "{kernel_name} {} · {message}",
            language.localized(copy::app::CONNECTION_FAILED)
        ),
    }
}

pub(super) fn proxy_mode_label(language: Language, mode: ProxyMode) -> &'static str {
    match mode {
        ProxyMode::Off => language.localized(copy::common::OFF),
        ProxyMode::System => language.localized(copy::common::SYSTEM_PROXY),
        ProxyMode::Tun => language.localized(copy::common::TUN_PROXY),
    }
}

pub(super) fn compact_proxy_mode_label(
    language: Language,
    current: ProxyMode,
    pending: Option<ProxyMode>,
) -> &'static str {
    match pending {
        Some(ProxyMode::Tun) => language.localized(copy::app::PREPARING_TUN),
        Some(ProxyMode::System) => language.localized(copy::app::ENABLING),
        Some(ProxyMode::Off) => language.localized(copy::app::TURNING_OFF),
        None => match current {
            ProxyMode::Off => language.localized(copy::app::OFF),
            ProxyMode::System => language.localized(copy::app::SYSTEM),
            ProxyMode::Tun => "TUN",
        },
    }
}

pub(super) fn routing_mode_label(language: Language, mode: RoutingMode) -> &'static str {
    match mode {
        RoutingMode::Direct => language.localized(copy::common::DIRECT),
        RoutingMode::Global => language.localized(copy::app::GLOBAL),
        RoutingMode::Rule => language.localized(copy::app::RULES),
    }
}

pub(super) fn controller_state_label(state: &ControllerState) -> &'static str {
    match state {
        ControllerState::Disconnected => "disconnected",
        ControllerState::Connecting { .. } => "connecting",
        ControllerState::Connected { .. } => "connected",
        ControllerState::Failed { .. } => "failed",
    }
}

pub(super) fn policy_target_is_selectable(
    connected: bool,
    catalog_allows: Option<bool>,
    stored_group_allows: Option<bool>,
) -> bool {
    if connected {
        catalog_allows == Some(true)
    } else {
        stored_group_allows == Some(true)
    }
}

pub(super) fn policy_kind_label(
    language: Language,
    kind: manis_core::PolicyGroupKind,
) -> &'static str {
    match kind {
        manis_core::PolicyGroupKind::Selector => language.localized(copy::app::MANUAL),
        manis_core::PolicyGroupKind::UrlTest => language.localized(copy::app::AUTO_SELECT),
        manis_core::PolicyGroupKind::Fallback => language.localized(copy::app::FALLBACK),
        manis_core::PolicyGroupKind::LoadBalance => language.localized(copy::app::LOAD_BALANCE),
        manis_core::PolicyGroupKind::Direct => language.localized(copy::common::DIRECT),
    }
}
