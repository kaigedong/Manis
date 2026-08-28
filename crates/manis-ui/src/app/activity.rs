use gpui::{AnyElement, Div, FontWeight, ParentElement, Styled, div, prelude::*, px};
use manis_core::WindowSizeClass;
use manis_mihomo::Connection;
use manis_profile::MANIS_GLOBAL_GROUP_NAME;

use super::{ManisApp, format_bytes};
use crate::{
    components::{ActionRole, StatusTone, action_button, empty_state, page_heading, status_badge},
    localization::{CountNoun, Language, Message},
    theme::{ControlSize, Space, TextRole, Theme},
};

impl ManisApp {
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
        let visible_count = visible_connections.len();
        let total_count = self.active_connections.len();
        let summary = activity_summary(
            language,
            query.is_empty(),
            visible_count,
            total_count,
            total_download,
            total_upload,
        );
        let reconnect: AnyElement = action_button(
            "refresh-activity",
            language.message(Message::RefreshData),
            ActionRole::Secondary,
            ControlSize::Compact,
        )
        .accessibility_label(language.text(
            "Refresh activity data by reconnecting the kernel",
            "重新连接内核并刷新网络活动数据",
        ))
        .border_color(theme.outline_subtle)
        .bg(theme.surface_high)
        .text_color(theme.text_primary)
        .on_click(cx.listener(|this, _, _, cx| this.connect_mihomo(cx)))
        .into_any_element();
        let mut rows = div()
            .id("activity-scroll")
            .flex_1()
            .overflow_y_scroll()
            .flex()
            .flex_col();
        if visible_connections.is_empty() {
            rows = rows.child(div().flex_1().p(Space::Xl.px()).child(if query.is_empty() {
                empty_state(
                    language.message(Message::NoActiveConnections),
                    language.text(
                        "Live traffic appears here as soon as the kernel reports a connection.",
                        "内核上报新连接后，实时流量会显示在这里。",
                    ),
                    Some(
                        action_button(
                            "refresh-activity-empty",
                            language.message(Message::RefreshData),
                            ActionRole::Secondary,
                            ControlSize::Compact,
                        )
                        .accessibility_label(language.text(
                            "Refresh activity data by reconnecting the kernel",
                            "重新连接内核并刷新网络活动数据",
                        ))
                        .border_color(theme.outline_subtle)
                        .bg(theme.surface_high)
                        .text_color(theme.text_primary)
                        .on_click(cx.listener(|this, _, _, cx| this.connect_mihomo(cx)))
                        .into_any_element(),
                    ),
                    theme,
                )
            } else {
                empty_state(
                    language.message(Message::NoFilterMatches),
                    language.text(
                        "Try a target host, process name, rule, or route stage.",
                        "可以尝试输入目标域名、进程名、规则或路径节点。",
                    ),
                    None,
                    theme,
                )
            }));
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
                    .flex_shrink_0()
                    .px(Space::Xl.px())
                    .py(Space::Lg.px())
                    .flex()
                    .items_center()
                    .gap(Space::Lg.px())
                    .when(compact, |header| header.flex_col().items_start())
                    .border_b_1()
                    .border_color(theme.outline_subtle)
                    .child(div().flex_1().min_w_0().child(page_heading(
                        language.message(Message::NetworkActivity),
                        summary,
                        None,
                        theme,
                    )))
                    .child(
                        div()
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .gap(Space::Sm.px())
                            .when(compact, gpui::Styled::w_full)
                            .child(status_badge(
                                self.live_status.activity.clone(),
                                StatusTone::Neutral,
                                theme,
                            ))
                            .when_some(self.activity_search_input.clone(), |tools, input| {
                                tools.child(
                                    div()
                                        .w(if compact { px(210.0) } else { px(320.0) })
                                        .child(input),
                                )
                            })
                            .child(reconnect),
                    ),
            )
            .child(rows)
    }
}

fn activity_summary(
    language: Language,
    unfiltered: bool,
    visible_count: usize,
    total_count: usize,
    total_download: u64,
    total_upload: u64,
) -> String {
    let count = if unfiltered {
        language.count(CountNoun::Connection, total_count)
    } else {
        format!(
            "{}/{} {}",
            visible_count,
            total_count,
            language.text("connections", "条连接")
        )
    };
    format!(
        "{count} · ↓ {} · ↑ {}",
        format_bytes(total_download),
        format_bytes(total_upload)
    )
}

fn connection_matches_query(connection: &Connection, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    let query = query.to_lowercase();
    [
        connection.metadata.sniff_host.as_deref(),
        connection.metadata.host.as_deref(),
        connection.metadata.destination_ip.as_deref(),
        connection.metadata.remote_destination.as_deref(),
        connection.metadata.destination_port.as_deref(),
        connection.metadata.process.as_deref(),
        connection.metadata.process_path.as_deref(),
        connection.rule.as_deref(),
        connection.rule_payload.as_deref(),
    ]
    .into_iter()
    .flatten()
    .chain(user_route_stages(&connection.chains))
    .any(|value| value.to_lowercase().contains(&query))
}

fn activity_row(connection: &Connection, theme: Theme, compact: bool, language: Language) -> Div {
    let target = connection_target(connection, language);
    let metadata = connection_metadata(connection, language);
    let chain = route_summary(&connection.chains, language)
        .unwrap_or_else(|| language.text("Route unavailable", "路由未返回").to_owned());

    div()
        .min_h(px(if compact { 78.0 } else { 60.0 }))
        .px(Space::Xl.px())
        .py(Space::Md.px())
        .flex()
        .items_center()
        .gap(Space::Lg.px())
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
                        .text_size(TextRole::Body.size())
                        .line_height(TextRole::Body.line_height())
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(target),
                )
                .child(
                    div()
                        .text_size(TextRole::Metadata.size())
                        .line_height(TextRole::Metadata.line_height())
                        .text_color(theme.text_tertiary)
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(metadata),
                )
                .when(compact, |column| {
                    column.child(
                        div()
                            .text_size(TextRole::Data.size())
                            .line_height(TextRole::Data.line_height())
                            .text_color(theme.action_primary)
                            .child(chain.clone()),
                    )
                }),
        )
        .when(!compact, |row| {
            row.child(
                div()
                    .w(px(250.0))
                    .text_size(TextRole::Data.size())
                    .line_height(TextRole::Data.line_height())
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
                .text_size(TextRole::Data.size())
                .line_height(TextRole::Data.line_height())
                .text_color(theme.text_secondary)
                .child(format!(
                    "↓ {}  ↑ {}",
                    format_bytes(connection.download),
                    format_bytes(connection.upload)
                )),
        )
}

fn user_route_stages(chains: &[String]) -> impl DoubleEndedIterator<Item = &str> {
    chains.iter().map(|stage| stage.trim()).filter(|stage| {
        !stage.is_empty() && *stage != MANIS_GLOBAL_GROUP_NAME && *stage != "GLOBAL"
    })
}

fn route_stage_label(stage: &str, language: Language) -> &str {
    match stage {
        "DIRECT" => language.text("Direct", "直连"),
        _ => stage,
    }
}

pub(super) fn route_summary(chains: &[String], language: Language) -> Option<String> {
    let route = user_route_stages(chains)
        .rev()
        .map(|stage| route_stage_label(stage, language))
        .collect::<Vec<_>>()
        .join(" → ");
    (!route.is_empty()).then(|| match language {
        Language::English => format!("Route · {route}"),
        Language::SimplifiedChinese => format!("路由 · {route}"),
    })
}

fn connection_metadata(connection: &Connection, language: Language) -> String {
    let process = connection
        .metadata
        .process
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| {
            connection
                .metadata
                .process_path
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
        })
        .unwrap_or_else(|| language.text("Unknown process", "未知进程"));
    let rule = connection
        .rule
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| language.text("No matched rule", "未匹配规则"));
    let rule = connection
        .rule_payload
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map_or_else(|| rule.to_owned(), |payload| format!("{rule} · {payload}"));
    let destination_ip = connection
        .metadata
        .destination_ip
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .filter(|address| Some(*address) != connection.metadata.target_host());
    destination_ip.map_or_else(
        || format!("{process} · {rule}"),
        |address| format!("{process} · {address} · {rule}"),
    )
}

fn connection_target(connection: &Connection, language: Language) -> String {
    let host = connection.metadata.target_host();
    let port = connection
        .metadata
        .destination_port
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    match (host, port) {
        (Some(host), Some(port)) if host.contains(':') && !host.starts_with('[') => {
            format!("[{host}]:{port}")
        }
        (Some(host), Some(port)) => format!("{host}:{port}"),
        (Some(host), None) => host.to_owned(),
        (None, Some(port)) => match language {
            Language::English => format!("Unknown target · port {port}"),
            Language::SimplifiedChinese => format!("未知目标 · 端口 {port}"),
        },
        (None, None) => language.text("Unknown target", "未知目标").to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use manis_mihomo::{Connection, ConnectionMetadata};

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
            chains: vec![
                "Hong Kong".to_owned(),
                "__MANIS_GLOBAL__".to_owned(),
                "GLOBAL".to_owned(),
            ],
            provider_chains: Vec::new(),
            rule: Some("DomainSuffix".to_owned()),
            rule_payload: Some("example.com".to_owned()),
        };

        for query in ["EXAMPLE", "browser", "domainsuffix", "hong kong", ""] {
            assert!(super::connection_matches_query(&connection, query));
        }
        assert!(!super::connection_matches_query(&connection, "telegram"));
        assert!(!super::connection_matches_query(
            &connection,
            "__manis_global__"
        ));
    }

    #[test]
    fn route_summary_hides_runtime_groups_and_reads_from_policy_to_node() {
        let global_route = vec![
            "HK05".to_owned(),
            "__MANIS_GLOBAL__".to_owned(),
            "GLOBAL".to_owned(),
        ];
        assert_eq!(
            super::route_summary(
                &global_route,
                crate::localization::Language::SimplifiedChinese
            )
            .as_deref(),
            Some("路由 · HK05")
        );

        let policy_route = vec![
            "HK05".to_owned(),
            "Hong Kong".to_owned(),
            "Streaming".to_owned(),
        ];
        assert_eq!(
            super::route_summary(&policy_route, crate::localization::Language::English).as_deref(),
            Some("Route · Streaming → Hong Kong → HK05")
        );

        assert_eq!(
            super::route_summary(
                &["DIRECT".to_owned()],
                crate::localization::Language::SimplifiedChinese
            )
            .as_deref(),
            Some("路由 · 直连")
        );
    }

    #[test]
    fn activity_target_skips_blank_hosts_and_uses_every_mihomo_fallback() {
        let mut connection = Connection {
            id: Some("fixture".to_owned()),
            metadata: ConnectionMetadata {
                host: Some(String::new()),
                destination_ip: Some("93.184.216.34".to_owned()),
                destination_port: Some("443".to_owned()),
                ..ConnectionMetadata::default()
            },
            upload: 0,
            download: 0,
            start: None,
            chains: Vec::new(),
            provider_chains: Vec::new(),
            rule: None,
            rule_payload: None,
        };

        assert_eq!(
            super::connection_target(&connection, crate::localization::Language::English),
            "93.184.216.34:443"
        );

        connection.metadata.sniff_host = Some("www.example.com".to_owned());
        connection.metadata.remote_destination = Some("203.0.113.10".to_owned());
        assert_eq!(
            super::connection_target(&connection, crate::localization::Language::English),
            "www.example.com:443"
        );

        connection.metadata.sniff_host = None;
        connection.metadata.destination_ip = Some("2001:db8::1".to_owned());
        assert_eq!(
            super::connection_target(&connection, crate::localization::Language::English),
            "[2001:db8::1]:443"
        );

        connection.metadata.destination_ip = None;
        connection.metadata.remote_destination = None;
        assert_eq!(
            super::connection_target(
                &connection,
                crate::localization::Language::SimplifiedChinese
            ),
            "未知目标 · 端口 443"
        );
    }
}
