use gpui::{Div, FontWeight, ParentElement, Role, Styled, div, prelude::*, px};
use relay_core::WindowSizeClass;

use super::RelayApp;
use crate::{diagnostics::recent_ui_logs, localization::Language, theme::Theme};

impl RelayApp {
    #[allow(clippy::too_many_lines)]
    pub(super) fn logs_workspace(
        &self,
        theme: Theme,
        _size_class: WindowSizeClass,
        cx: &mut gpui::Context<Self>,
    ) -> Div {
        let logs = recent_ui_logs();
        let count = logs.len() + self.kernel_logs.len();
        let language = self.language();
        let mut rows = div()
            .id("logs-scroll")
            .flex_1()
            .overflow_y_scroll()
            .flex()
            .flex_col();
        if logs.is_empty() && self.kernel_logs.is_empty() {
            rows = rows.child(
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(theme.text_tertiary)
                    .child(language.text(
                        "No kernel logs or Relay UI events yet",
                        "还没有内核日志或 Relay UI 事件",
                    )),
            );
        } else {
            for entry in self.kernel_logs.iter().rev() {
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
                                .text_color(theme.action_primary)
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
                                .text_color(theme.action_primary)
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
                    .child(
                        div()
                            .id("refresh-logs")
                            .role(Role::Button)
                            .tab_stop(true)
                            .focusable()
                            .cursor_pointer()
                            .h(px(34.0))
                            .px_3()
                            .rounded_md()
                            .border_1()
                            .border_color(theme.outline_subtle)
                            .bg(theme.surface_high)
                            .flex()
                            .items_center()
                            .child(language.text("Reconnect", "重新连接"))
                            .on_click(cx.listener(|this, _, _, cx| this.connect_mihomo(cx))),
                    ),
            )
            .child(rows)
    }
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
    let seconds = (timestamp_ms / 1_000) % 86_400;
    let hours = seconds / 3_600;
    let minutes = seconds % 3_600 / 60;
    let seconds = seconds % 60;
    format!("{hours:02}:{minutes:02}:{seconds:02} UTC")
}

#[cfg(test)]
mod tests {
    #[test]
    fn log_time_is_bounded_and_readable() {
        assert_eq!(super::format_log_time(3_723_000), "01:02:03 UTC");
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
}
