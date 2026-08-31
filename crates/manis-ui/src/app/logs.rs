use gpui::{Div, FontWeight, ParentElement, Rgba, Stateful, Styled, div, prelude::*, px};
use manis_core::WindowSizeClass;

use super::ManisApp;
use crate::{
    components::{ActionRole, action_button, empty_state, page_heading},
    diagnostics::{UiLogEntry, recent_ui_logs},
    localization::{Language, Message, copy},
    mihomo::KernelLogEntry,
    theme::{ControlSize, Radius, Space, TextRole, Theme},
};

impl ManisApp {
    pub(super) fn logs_workspace(
        &self,
        theme: Theme,
        size_class: WindowSizeClass,
        cx: &mut gpui::Context<Self>,
    ) -> Div {
        let compact = size_class == WindowSizeClass::Compact;
        let query = self
            .inputs
            .logs_search
            .as_ref()
            .map(|input| input.read(cx).value().trim().to_owned())
            .unwrap_or_default();
        let logs = recent_ui_logs()
            .into_iter()
            .filter(|entry| ui_log_matches_query(entry, &query))
            .collect::<Vec<_>>();
        let kernel_logs = self
            .kernel_logs
            .iter()
            .filter(|entry| kernel_log_matches_query(entry, &query))
            .collect::<Vec<_>>();
        let count = logs.len() + kernel_logs.len();
        let language = self.language();
        let rows = logs_rows(logs, kernel_logs, query.is_empty(), language, theme);

        let refresh_action = action_button(
            "refresh-logs",
            language.message(Message::RefreshData),
            ActionRole::Secondary,
            ControlSize::Compact,
        )
        .accessibility_label(language.localized(copy::logs::REFRESH_LOG_DATA))
        .border_color(theme.outline_subtle)
        .bg(theme.surface_base)
        .text_color(theme.text_primary)
        .on_click(cx.listener(|this, _, _, cx| this.connect_mihomo(cx)));
        let heading = page_heading(
            language.message(Message::Logs),
            logs_summary(language, count, self.dropped_kernel_logs),
            None,
            theme,
        );
        let header = div()
            .flex_shrink_0()
            .px(Space::Xl.px())
            .py(Space::Md.px())
            .border_b_1()
            .border_color(theme.outline_subtle)
            .when(compact, |header| {
                header.flex().flex_col().gap(Space::Md.px())
            })
            .when(!compact, |header| {
                header.flex().items_center().gap(Space::Lg.px())
            })
            .child(div().flex_1().min_w_0().child(heading))
            .child(
                div()
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .gap(Space::Sm.px())
                    .when(compact, gpui::Styled::w_full)
                    .when_some(self.inputs.logs_search.clone(), |tools, input| {
                        tools.child(
                            div()
                                .w(if compact { px(240.0) } else { px(320.0) })
                                .max_w_full()
                                .child(input),
                        )
                    })
                    .child(refresh_action),
            );

        div()
            .flex_1()
            .h_full()
            .min_w_0()
            .flex()
            .flex_col()
            .bg(theme.surface_base)
            .child(header)
            .child(rows)
    }
}

fn logs_rows(
    logs: Vec<UiLogEntry>,
    kernel_logs: Vec<&KernelLogEntry>,
    no_query: bool,
    language: Language,
    theme: Theme,
) -> Stateful<Div> {
    let mut rows = div()
        .id("logs-scroll")
        .flex_1()
        .overflow_y_scroll()
        .flex()
        .flex_col();
    if logs.is_empty() && kernel_logs.is_empty() {
        return rows.child(
            div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .p(Space::Xl.px())
                .child(logs_empty_state(language, no_query, theme)),
        );
    }
    for entry in kernel_logs.into_iter().rev() {
        rows = rows.child(kernel_log_row(entry, theme));
    }
    for entry in logs.into_iter().rev() {
        let reference = entry.operation_id.map_or_else(
            || format!("#{:04}", entry.sequence),
            |operation| format!("#{:04} · OP-{operation:04}", entry.sequence),
        );
        rows = rows.child(ui_log_row(entry, reference, theme));
    }
    rows
}

fn logs_empty_state(language: Language, no_query: bool, theme: Theme) -> Div {
    let (title, detail) = if no_query {
        (
            language.message(Message::NoLogs),
            language
                .localized(copy::logs::LOGS_WILL_APPEAR_HERE_AFTER_MIHOMO_STARTS_OR_MANIS_PERFORMS),
        )
    } else {
        (
            language.message(Message::NoFilterMatches),
            language
                .localized(copy::logs::CLEAR_THE_FILTER_OR_SEARCH_BY_OPERATION_ERROR_MESSAGE_OR),
        )
    };
    empty_state(title, detail, None, theme)
}

fn kernel_log_row(entry: &KernelLogEntry, theme: Theme) -> Div {
    log_row(
        format!("K#{:04}", entry.sequence),
        &entry.level,
        div()
            .font_family("monospace")
            .text_size(TextRole::Data.size())
            .line_height(TextRole::Data.line_height())
            .text_color(theme.text_primary)
            .child(entry.payload.clone()),
        format_log_time(entry.timestamp_ms),
        theme,
    )
}

fn ui_log_row(entry: UiLogEntry, reference: String, theme: Theme) -> Div {
    log_row(
        reference,
        &entry.level,
        div()
            .flex()
            .flex_col()
            .gap(Space::Xs.px())
            .font_family("monospace")
            .text_size(TextRole::Data.size())
            .line_height(TextRole::Data.line_height())
            .text_color(theme.text_primary)
            .child(entry.event)
            .when_some(entry.detail, |row, detail| {
                row.child(
                    div()
                        .font_family("monospace")
                        .text_size(TextRole::Metadata.size())
                        .line_height(TextRole::Metadata.line_height())
                        .text_color(theme.text_secondary)
                        .child(detail),
                )
            }),
        format_log_time(entry.timestamp_ms),
        theme,
    )
}

fn log_row(reference: String, level: &str, body: Div, timestamp: String, theme: Theme) -> Div {
    div()
        .min_h(px(52.0))
        .px(Space::Xl.px())
        .py(Space::Sm.px())
        .flex()
        .items_start()
        .gap(Space::Md.px())
        .border_b_1()
        .border_color(theme.outline_subtle)
        .child(log_reference(reference, theme))
        .child(log_level_badge(level, theme))
        .child(div().flex_1().min_w_0().child(body))
        .child(log_timestamp(timestamp, theme))
}

fn log_reference(reference: String, theme: Theme) -> Div {
    div()
        .w(px(112.0))
        .flex_shrink_0()
        .font_family("monospace")
        .text_size(TextRole::Metadata.size())
        .line_height(TextRole::Metadata.line_height())
        .text_color(theme.text_tertiary)
        .child(reference)
}

fn log_level_badge(level: &str, theme: Theme) -> Div {
    let color = log_level_color(level, theme);
    div()
        .w(px(72.0))
        .h(px(24.0))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .rounded(Radius::Control.px())
        .bg(theme.surface_base.blend(color.opacity(0.1)))
        .text_size(TextRole::Label.size())
        .line_height(TextRole::Label.line_height())
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(color)
        .child(level.to_uppercase())
}

fn log_timestamp(timestamp: String, theme: Theme) -> Div {
    div()
        .w(px(68.0))
        .flex_shrink_0()
        .font_family("monospace")
        .text_size(TextRole::Metadata.size())
        .line_height(TextRole::Metadata.line_height())
        .text_color(theme.text_tertiary)
        .child(timestamp)
}

fn ui_log_matches_query(entry: &UiLogEntry, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    let query = query.to_lowercase();
    let operation = entry
        .operation_id
        .map(|operation| format!("op-{operation:04}"));
    [
        Some(entry.event.as_str()),
        Some(entry.level.as_str()),
        entry.detail.as_deref(),
        operation.as_deref(),
    ]
    .into_iter()
    .flatten()
    .any(|value| value.to_lowercase().contains(&query))
        || entry.sequence.to_string().contains(&query)
}

fn kernel_log_matches_query(entry: &KernelLogEntry, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    let query = query.to_lowercase();
    entry.level.to_lowercase().contains(&query)
        || entry.payload.to_lowercase().contains(&query)
        || format!("k#{:04}", entry.sequence).contains(&query)
}

fn logs_summary(language: Language, count: usize, dropped: u64) -> String {
    copy::logs::summary(language, count, dropped)
}

fn format_log_time(timestamp_ms: u128) -> String {
    // `Local` silently falls back to UTC when the system zone is unreadable, so there is no
    // failure to report here — only an offset, which is zero in that case.
    format_local_time(
        timestamp_ms,
        chrono::Local::now().offset().local_minus_utc(),
    )
}

/// Renders a timestamp on the viewer's own wall clock.
fn format_local_time(timestamp_ms: u128, offset_seconds: i32) -> String {
    // Reducing to one day first keeps the value trivially in range for the signed arithmetic.
    let day_seconds = i64::try_from(timestamp_ms / 1_000 % 86_400).unwrap_or(0);
    // `rem_euclid` keeps a westward offset from underflowing into a negative clock.
    let seconds = (day_seconds + i64::from(offset_seconds)).rem_euclid(86_400);
    format!(
        "{:02}:{:02}:{:02}",
        seconds / 3_600,
        seconds % 3_600 / 60,
        seconds % 60
    )
}

/// Maps a log level to its severity colour.
///
/// Accepts both the UI's own spellings and the kernel's lowercase `warning`, so one glance
/// separates failures from noise in either list.
fn log_level_color(level: &str, theme: Theme) -> Rgba {
    match level.to_ascii_lowercase().as_str() {
        "error" | "fatal" | "panic" => theme.status_error,
        "warn" | "warning" => theme.status_warning,
        "info" => theme.log_info,
        "debug" => theme.log_debug,
        "trace" => theme.log_trace,
        _ => theme.text_secondary,
    }
}

#[cfg(test)]
mod tests {
    use crate::diagnostics::UiLogEntry;

    #[test]
    fn log_time_is_shown_in_the_local_offset() {
        // 01:02:03 UTC on each viewer's own wall clock.
        assert_eq!(super::format_local_time(3_723_000, 8 * 3_600), "09:02:03");
        assert_eq!(super::format_local_time(3_723_000, 0), "01:02:03");
        // Half-hour zones such as India must not be rounded away.
        assert_eq!(
            super::format_local_time(3_723_000, 5 * 3_600 + 1_800),
            "06:32:03"
        );
    }

    #[test]
    fn a_local_offset_may_move_the_clock_across_midnight() {
        // 02:00:00 UTC is the previous evening in New York.
        assert_eq!(super::format_local_time(7_200_000, -5 * 3_600), "21:00:00");
        // 23:00:00 UTC is the next morning in Shanghai.
        assert_eq!(super::format_local_time(82_800_000, 8 * 3_600), "07:00:00");
    }

    #[test]
    fn severity_drives_the_level_colour() {
        let theme = crate::theme::Theme::light();

        assert_eq!(super::log_level_color("ERROR", theme), theme.status_error);
        assert_eq!(super::log_level_color("WARN", theme), theme.status_warning);
        assert_eq!(super::log_level_color("INFO", theme), theme.log_info);
        assert_eq!(super::log_level_color("DEBUG", theme), theme.log_debug);
        assert_eq!(super::log_level_color("TRACE", theme), theme.log_trace);
    }

    #[test]
    fn kernel_level_spellings_map_to_the_same_colours() {
        let theme = crate::theme::Theme::dark();

        // Mihomo writes "warning" and lowercases everything.
        assert_eq!(
            super::log_level_color("warning", theme),
            theme.status_warning
        );
        assert_eq!(super::log_level_color("warn", theme), theme.status_warning);
        assert_eq!(super::log_level_color("error", theme), theme.status_error);
        assert_eq!(super::log_level_color("fatal", theme), theme.status_error);
        assert_eq!(super::log_level_color("panic", theme), theme.status_error);
        assert_eq!(super::log_level_color("info", theme), theme.log_info);
        assert_eq!(super::log_level_color("debug", theme), theme.log_debug);
        assert_eq!(super::log_level_color("trace", theme), theme.log_trace);
        // An unknown level must stay legible rather than borrow a severity colour.
        assert_eq!(super::log_level_color("silly", theme), theme.text_secondary);
    }

    #[test]
    fn log_levels_are_visually_distinct_in_both_themes() {
        for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
            let colors = [
                theme.log_trace,
                theme.log_debug,
                theme.log_info,
                theme.status_warning,
                theme.status_error,
            ];
            for (index, color) in colors.iter().enumerate() {
                for other in &colors[index + 1..] {
                    assert_ne!(color, other);
                }
                assert_ne!(*color, theme.action_primary);
                assert_ne!(*color, theme.text_tertiary);
            }
        }
    }

    #[test]
    fn log_summary_uses_selected_language() {
        assert_eq!(
            super::logs_summary(crate::localization::Language::English, 2, 1),
            "2 logs · 1 log dropped under load · sensitive data hidden"
        );
        assert_eq!(
            super::logs_summary(crate::localization::Language::SimplifiedChinese, 2, 1),
            "2 条日志 · 高负载时丢弃 1 条日志 · 敏感信息已隐藏"
        );
    }

    #[test]
    fn log_filter_matches_event_detail_level_and_operation_number() {
        let entry = UiLogEntry {
            sequence: 12,
            timestamp_ms: 0,
            level: "ERROR".to_owned(),
            operation_id: Some(5),
            event: "proxy.mode.failed".to_owned(),
            detail: Some("helper timed out".to_owned()),
        };

        for query in ["proxy", "ERROR", "timed out", "op-0005", "12", ""] {
            assert!(super::ui_log_matches_query(&entry, query));
        }
        assert!(!super::ui_log_matches_query(&entry, "routing"));
    }
}
