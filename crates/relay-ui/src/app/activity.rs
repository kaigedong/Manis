use gpui::{Div, FontWeight, ParentElement, Role, Styled, div, prelude::*, px};
use relay_core::WindowSizeClass;
use relay_mihomo::Connection;

use super::{RelayApp, format_bytes};
use crate::theme::Theme;

impl RelayApp {
    pub(super) fn activity_workspace(
        &self,
        theme: Theme,
        size_class: WindowSizeClass,
        cx: &mut gpui::Context<Self>,
    ) -> Div {
        let compact = size_class == WindowSizeClass::Compact;
        let total_upload: u64 = self.active_connections.iter().map(|item| item.upload).sum();
        let total_download: u64 = self
            .active_connections
            .iter()
            .map(|item| item.download)
            .sum();
        let mut rows = div()
            .id("activity-scroll")
            .flex_1()
            .overflow_y_scroll()
            .flex()
            .flex_col();
        if self.active_connections.is_empty() {
            rows = rows.child(
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(theme.text_tertiary)
                    .child("当前没有活动连接；刷新后会显示 Mihomo /connections 数据"),
            );
        } else {
            for connection in &self.active_connections {
                rows = rows.child(activity_row(connection, theme, compact));
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
                    .gap_5()
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
                                    .child("网络活动"),
                            )
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(theme.text_tertiary)
                                    .child(format!(
                                        "{} 条活动连接 · ↓ {} · ↑ {}",
                                        self.active_connections.len(),
                                        format_bytes(total_download),
                                        format_bytes(total_upload)
                                    )),
                            ),
                    )
                    .child(div().flex_1())
                    .child(
                        div()
                            .id("refresh-activity")
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
                            .on_click(cx.listener(|this, _, _, cx| this.connect_mihomo(cx))),
                    ),
            )
            .child(rows)
    }
}

fn activity_row(connection: &Connection, theme: Theme, compact: bool) -> Div {
    let host = connection
        .metadata
        .host
        .as_deref()
        .or(connection.metadata.destination_ip.as_deref())
        .unwrap_or("未知目标");
    let target = connection
        .metadata
        .destination_port
        .as_ref()
        .map_or_else(|| host.to_owned(), |port| format!("{host}:{port}"));
    let process = connection
        .metadata
        .process
        .as_deref()
        .or(connection.metadata.process_path.as_deref())
        .unwrap_or("未知进程");
    let rule = connection.rule.as_deref().unwrap_or("未匹配规则");
    let chain = if connection.chains.is_empty() {
        "DIRECT".to_owned()
    } else {
        connection.chains.join(" → ")
    };

    div()
        .min_h(px(if compact { 78.0 } else { 60.0 }))
        .px_5()
        .py_3()
        .flex()
        .items_center()
        .gap_4()
        .border_b_1()
        .border_color(theme.outline_subtle)
        .child(
            div()
                .min_w_0()
                .flex_1()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme.text_primary)
                        .overflow_hidden()
                        .child(target),
                )
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(theme.text_tertiary)
                        .child(format!("{process} · {rule}")),
                )
                .when(compact, |column| {
                    column.child(
                        div()
                            .text_size(px(11.0))
                            .text_color(theme.action_primary)
                            .child(chain.clone()),
                    )
                }),
        )
        .when(!compact, |row| {
            row.child(
                div()
                    .w(px(250.0))
                    .text_size(px(11.0))
                    .text_color(theme.action_primary)
                    .overflow_hidden()
                    .child(chain),
            )
        })
        .child(
            div()
                .w(px(126.0))
                .flex_shrink_0()
                .flex()
                .justify_end()
                .text_size(px(11.0))
                .text_color(theme.text_secondary)
                .child(format!(
                    "↓ {}  ↑ {}",
                    format_bytes(connection.download),
                    format_bytes(connection.upload)
                )),
        )
}
