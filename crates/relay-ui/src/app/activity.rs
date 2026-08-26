use gpui::{Div, FontWeight, ParentElement, Role, Styled, div, prelude::*, px};
use relay_core::WindowSizeClass;
use relay_mihomo::Connection;

use super::{RelayApp, format_bytes};
use crate::localization::Language;
use crate::theme::Theme;

impl RelayApp {
    #[allow(clippy::too_many_lines)]
    pub(super) fn activity_workspace(
        &self,
        theme: Theme,
        size_class: WindowSizeClass,
        cx: &mut gpui::Context<Self>,
    ) -> Div {
        let compact = size_class == WindowSizeClass::Compact;
        let query = self
            .activity_search_input
            .as_ref()
            .map(|input| input.read(cx).value().trim().to_owned())
            .unwrap_or_default();
        let visible_connections = self
            .active_connections
            .iter()
            .filter(|connection| connection_matches_query(connection, &query))
            .collect::<Vec<_>>();
        let total_upload: u64 = visible_connections.iter().map(|item| item.upload).sum();
        let total_download: u64 = visible_connections.iter().map(|item| item.download).sum();
        let language = self.language();
        let mut rows = div()
            .id("activity-scroll")
            .flex_1()
            .overflow_y_scroll()
            .flex()
            .flex_col();
        if visible_connections.is_empty() {
            rows = rows.child(
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(theme.text_tertiary)
                    .child(if query.is_empty() {
                        language.text(
                            "No active connections. Live traffic appears after a new connection starts.",
                            "当前没有活动连接；实时流会在新连接建立后自动显示",
                        )
                    } else {
                        language.text(
                            "No active connection matches this filter.",
                            "没有符合当前筛选的活动连接",
                        )
                    }),
            );
        } else {
            for connection in visible_connections.iter().copied() {
                rows = rows.child(activity_row(connection, theme, compact, language));
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
                                    .child(language.text("Network Activity", "网络活动")),
                            )
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(theme.text_tertiary)
                                    .child(format!(
                                        "{} {} · ↓ {} · ↑ {} · {}",
                                        if query.is_empty() {
                                            self.active_connections.len().to_string()
                                        } else {
                                            format!(
                                                "{}/{}",
                                                visible_connections.len(),
                                                self.active_connections.len()
                                            )
                                        },
                                        language.text("active connections", "条活动连接"),
                                        format_bytes(total_download),
                                        format_bytes(total_upload),
                                        self.live_status.activity
                                    )),
                            ),
                    )
                    .child(div().flex_1())
                    .when_some(self.activity_search_input.clone(), |header, input| {
                        header.child(
                            div()
                                .w(if compact { px(210.0) } else { px(320.0) })
                                .child(input),
                        )
                    })
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
                            .child(language.text("Reconnect", "重新连接"))
                            .on_click(cx.listener(|this, _, _, cx| this.connect_mihomo(cx))),
                    ),
            )
            .child(rows)
    }
}

fn connection_matches_query(connection: &Connection, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    let query = query.to_lowercase();
    [
        connection.metadata.host.as_deref(),
        connection.metadata.destination_ip.as_deref(),
        connection.metadata.process.as_deref(),
        connection.metadata.process_path.as_deref(),
        connection.rule.as_deref(),
        connection.rule_payload.as_deref(),
    ]
    .into_iter()
    .flatten()
    .chain(connection.chains.iter().map(String::as_str))
    .any(|value| value.to_lowercase().contains(&query))
}

fn activity_row(connection: &Connection, theme: Theme, compact: bool, language: Language) -> Div {
    let host = connection
        .metadata
        .host
        .as_deref()
        .or(connection.metadata.destination_ip.as_deref())
        .unwrap_or_else(|| language.text("Unknown target", "未知目标"));
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
        .unwrap_or_else(|| language.text("Unknown process", "未知进程"));
    let rule = connection
        .rule
        .as_deref()
        .unwrap_or_else(|| language.text("No matched rule", "未匹配规则"));
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

#[cfg(test)]
mod tests {
    use relay_mihomo::{Connection, ConnectionMetadata};

    #[test]
    fn activity_filter_matches_target_process_rule_and_route() {
        let connection = Connection {
            id: Some("fixture".to_owned()),
            metadata: ConnectionMetadata {
                host: Some("www.example.com".to_owned()),
                process: Some("Browser".to_owned()),
                destination_port: Some("443".to_owned()),
                ..ConnectionMetadata::default()
            },
            upload: 12,
            download: 34,
            start: None,
            chains: vec!["Hong Kong".to_owned(), "Proxy".to_owned()],
            provider_chains: Vec::new(),
            rule: Some("DomainSuffix".to_owned()),
            rule_payload: Some("example.com".to_owned()),
        };

        for query in ["EXAMPLE", "browser", "domainsuffix", "hong kong", ""] {
            assert!(super::connection_matches_query(&connection, query));
        }
        assert!(!super::connection_matches_query(&connection, "telegram"));
    }
}
