use gpui::{Div, FontWeight, ParentElement, Role, Styled, div, prelude::*, px};
use relay_core::WindowSizeClass;

use super::RelayApp;
use crate::{diagnostics::recent_ui_logs, theme::Theme};

impl RelayApp {
    #[allow(clippy::too_many_lines)]
    pub(super) fn logs_workspace(
        theme: Theme,
        _size_class: WindowSizeClass,
        cx: &mut gpui::Context<Self>,
    ) -> Div {
        let logs = recent_ui_logs();
        let count = logs.len();
        let mut rows = div()
            .id("logs-scroll")
            .flex_1()
            .overflow_y_scroll()
            .flex()
            .flex_col();
        if logs.is_empty() {
            rows = rows.child(
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(theme.text_tertiary)
                    .child("还没有 UI 事件"),
            );
        } else {
            for entry in logs.into_iter().rev() {
                rows = rows.child(
                    div()
                        .min_h(px(42.0))
                        .px_5()
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
                                .child(format!("#{:04}", entry.sequence)),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .font_family("monospace")
                                .text_size(px(11.0))
                                .text_color(theme.text_primary)
                                .child(entry.event),
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
                                    .child("日志"),
                            )
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(theme.text_tertiary)
                                    .child(format!(
                                        "{count} 条安全事件 · 不记录订阅地址、令牌或节点 URI"
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
                            .child("刷新")
                            .on_click(cx.listener(|_this, _, _, cx| cx.notify())),
                    ),
            )
            .child(rows)
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
}
