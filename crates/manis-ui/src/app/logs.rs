use gpui::{Div, FontWeight, ParentElement, Rgba, Styled, div, prelude::*, px};
use gpui_component::{
    Sizable,
    button::{Button, ButtonVariant, ButtonVariants},
};
use manis_core::WindowSizeClass;

use super::ManisApp;
use crate::{
    diagnostics::{UiLogEntry, recent_ui_logs},
    localization::Language,
    mihomo::KernelLogEntry,
    theme::Theme,
};

impl ManisApp {
    #[allow(clippy::too_many_lines)]
    pub(super) fn logs_workspace(
        &self,
        theme: Theme,
        size_class: WindowSizeClass,
        cx: &mut gpui::Context<Self>,
    ) -> Div {
        let compact = size_class == WindowSizeClass::Compact;
        let query = self
            .logs_search_input
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
        let mut rows = div()
            .id("logs-scroll")
            .flex_1()
            .overflow_y_scroll()
            .flex()
            .flex_col();
        if logs.is_empty() && kernel_logs.is_empty() {
            rows = rows.child(
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(theme.text_tertiary)
                    .child(if query.is_empty() {
                        language.text(
                            "No kernel logs or Manis UI events yet",
                            "还没有内核日志或 Manis UI 事件",
                        )
                    } else {
                        language.text("No log entry matches this filter", "没有符合当前筛选的日志")
                    }),
            );
        } else {
            for entry in kernel_logs.into_iter().rev() {
                rows = rows.child(
                    div()
                        .min_h(px(48.0))
                        .px_5()
                        .py_2()
                        .flex()
                        .items_center()
                        .gap_4()
                        .border_b_1()
                        .border_color(theme.outline_subtle)
                        .child(
                            div()
                                .w(px(86.0))
                                .flex_shrink_0()
                                .font_family("monospace")
                                .text_size(px(11.0))
                                .text_color(theme.text_tertiary)
                                .child(format!("K#{:04}", entry.sequence)),
                        )
                        .child(
                            div()
                                .w(px(64.0))
                                .flex_shrink_0()
                                .text_size(px(10.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(log_level_color(&entry.level, theme))
                                .child(entry.level.to_uppercase()),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .font_family("monospace")
                                .text_size(px(11.0))
                                .text_color(theme.text_primary)
                                .child(entry.payload.clone()),
                        )
                        .child(
                            div()
                                .font_family("monospace")
                                .text_size(px(10.0))
                                .text_color(theme.text_tertiary)
                                .child(format_log_time(entry.timestamp_ms)),
                        ),
                );
            }
            for entry in logs.into_iter().rev() {
                let reference = entry.operation_id.map_or_else(
                    || format!("#{:04}", entry.sequence),
                    |operation| format!("#{:04} · OP-{operation:04}", entry.sequence),
                );
                rows = rows.child(
                    div()
                        .min_h(px(48.0))
                        .px_5()
                        .py_2()
                        .flex()
                        .items_center()
                        .gap_4()
                        .border_b_1()
                        .border_color(theme.outline_subtle)
                        .child(
                            div()
                                .w(px(86.0))
                                .flex_shrink_0()
                                .font_family("monospace")
                                .text_size(px(11.0))
                                .text_color(theme.text_tertiary)
                                .child(reference),
                        )
                        .child(
                            div()
                                .w(px(64.0))
                                .flex_shrink_0()
                                .text_size(px(10.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(log_level_color(&entry.level, theme))
                                .child(entry.level),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .flex_col()
                                .gap_1()
                                .font_family("monospace")
                                .text_size(px(11.0))
                                .text_color(theme.text_primary)
                                .child(entry.event)
                                .when_some(entry.detail, |row, detail| {
                                    row.child(div().text_color(theme.text_secondary).child(detail))
                                }),
                        )
                        .child(
                            div()
                                .font_family("monospace")
                                .text_size(px(10.0))
                                .text_color(theme.text_tertiary)
                                .child(format_log_time(entry.timestamp_ms)),
                        ),
                );
            }
        }

        div()
            .flex_1()
            .h_full()
            .min_w_0()
            .flex()
            .flex_col()
            .bg(theme.surface_base)
            .child(
                div()
                    .h(px(72.0))
                    .flex_shrink_0()
                    .px_5()
                    .flex()
                    .items_center()
                    .gap_4()
                    .border_b_1()
                    .border_color(theme.outline_subtle)
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(
                                div()
                                    .text_size(px(18.0))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(language.text("Logs", "日志")),
                            )
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(theme.text_tertiary)
                                    .child(logs_summary(
                                        language,
                                        count,
                                        &self.live_status.logs,
                                        self.dropped_kernel_logs,
                                    )),
                            ),
                    )
                    .child(div().flex_1())
                    .when_some(self.logs_search_input.clone(), |header, input| {
                        header.child(
                            div()
                                .w(if compact { px(210.0) } else { px(320.0) })
                                .child(input),
                        )
                    })
                    .child(
                        Button::new("refresh-logs")
                            .accessibility_label(language.text("Reconnect", "重新连接"))
                            .label(language.text("Reconnect", "重新连接"))
                            .with_variant(ButtonVariant::Default)
                            .with_size(px(34.0))
                            .cursor_pointer()
                            .h(px(34.0))
                            .px_3()
                            .border_color(theme.outline_subtle)
                            .bg(theme.surface_high)
                            .on_click(cx.listener(|this, _, _, cx| this.connect_mihomo(cx))),
                    ),
            )
            .child(rows)
    }
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

fn logs_summary(language: Language, count: usize, live_status: &str, dropped: u64) -> String {
    match language {
        Language::English => format!(
            "{count} entries · persistent event chain with OP IDs · Mihomo {live_status} · dropped {dropped} overloaded logs · URLs/tokens redacted"
        ),
        Language::SimplifiedChinese => {
            format!(
                "{count} 条 · 操作链与 OP 编号已持久化 · Mihomo {live_status} · 已丢弃 {dropped} 条过载日志 · URL/令牌已脱敏"
            )
        }
    }
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
        "error" => theme.status_error,
        "warn" | "warning" => theme.status_warning,
        "info" => theme.action_primary,
        "debug" | "trace" => theme.text_tertiary,
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
        assert_eq!(super::log_level_color("INFO", theme), theme.action_primary);
        assert_eq!(super::log_level_color("DEBUG", theme), theme.text_tertiary);
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
        assert_eq!(super::log_level_color("info", theme), theme.action_primary);
        // An unknown level must stay legible rather than borrow a severity colour.
        assert_eq!(super::log_level_color("silly", theme), theme.text_secondary);
    }

    #[test]
    fn error_and_warning_are_visually_distinct() {
        for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
            assert_ne!(theme.status_error, theme.status_warning);
            assert_ne!(theme.status_warning, theme.action_primary);
        }
    }

    #[test]
    fn log_summary_uses_selected_language() {
        assert_eq!(
            super::logs_summary(crate::localization::Language::English, 2, "connected", 1),
            "2 entries · persistent event chain with OP IDs · Mihomo connected · dropped 1 overloaded logs · URLs/tokens redacted"
        );
        assert_eq!(
            super::logs_summary(
                crate::localization::Language::SimplifiedChinese,
                2,
                "已连接",
                1
            ),
            "2 条 · 操作链与 OP 编号已持久化 · Mihomo 已连接 · 已丢弃 1 条过载日志 · URL/令牌已脱敏"
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
