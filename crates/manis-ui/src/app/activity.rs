use std::net::IpAddr;

use gpui::{AnyElement, Div, FontWeight, ParentElement, Styled, div, prelude::*, px};
use manis_core::WindowSizeClass;
use manis_mihomo::Connection;

use crate::manual_rule::ManualRuleKind;

mod matching;
mod search;

use matching::ActivityRuleMatcher;
use search::{ConnectionSearchText, user_route_stages};

use super::{ManisApp, ManualRulePrefill, format_bytes};
use crate::{
    components::{
        ActionRole, RouteSegmentKind, StatusTone, action_button, card_surface, empty_state,
        latency_pill_with_tooltip, route_arrow, route_segment, status_badge,
    },
    localization::{Language, Message, copy},
    theme::{ControlSize, MONO_FAMILY, Space, TextRole, Theme},
};

/// What the rule editor should be prefilled with when a connection row is clicked.
struct RuleDraft {
    kind: ManualRuleKind,
    value: String,
    /// The process name and executable path this connection reported, offered as chips.
    prefills: Vec<ManualRulePrefill>,
}

struct ActivityConnectionView<'connection, 'matcher> {
    connection: &'connection Connection,
    rule_group: Option<&'matcher str>,
    search_text: ConnectionSearchText,
}

/// The route the kernel observed for one connection, in the order a user reads it.
///
/// `exit` keeps the raw stage so the view can decide whether it is a node with a measured
/// delay, a direct connection or a rejection.
struct RouteChain {
    rule: String,
    group: Option<String>,
    exit: Option<String>,
}

impl ManisApp {
    pub(super) fn activity_workspace(
        &self,
        theme: Theme,
        size_class: WindowSizeClass,
        cx: &mut gpui::Context<Self>,
    ) -> Div {
        let compact = size_class == WindowSizeClass::Compact;
        let query = self
            .inputs
            .activity_search
            .as_ref()
            .map(|input| input.read(cx).value().trim().to_owned())
            .unwrap_or_default();
        let normalized_query = query.to_lowercase();
        let language = self.language();
        let group_order = crate::mihomo::normalized_routing_rule_group_order(
            &self.rule_sources.group_order,
            self.has_regular_manual_rules(),
            &self.rule_sources.sources,
            &self.local_rule_groups,
        );
        let rule_matcher = ActivityRuleMatcher::new(
            &group_order,
            &self.manual_rules,
            &self.local_rule_groups,
            &self.rule_sources.sources,
            language,
        );
        let visible_connections = self
            .active_connections
            .iter()
            .map(|connection| {
                let rule_group = rule_matcher.matching_group(connection);
                ActivityConnectionView {
                    connection,
                    rule_group,
                    search_text: ConnectionSearchText::new(connection, rule_group),
                }
            })
            .filter(|view| view.search_text.matches(&normalized_query))
            .collect::<Vec<_>>();
        let total_upload: u64 = visible_connections
            .iter()
            .map(|item| item.connection.upload)
            .sum();
        let total_download: u64 = visible_connections
            .iter()
            .map(|item| item.connection.download)
            .sum();
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
        .accessibility_label(
            language.localized(copy::activity::REFRESH_ACTIVITY_DATA_BY_RECONNECTING_THE_KERNEL),
        )
        .on_click(cx.listener(|this, _, _, cx| this.connect_mihomo(cx)))
        .into_any_element();
        let rows = self.activity_rows(
            &visible_connections,
            query.is_empty(),
            compact,
            language,
            theme,
            cx,
        );

        div()
            .flex_1()
            .h_full()
            .min_w_0()
            .flex()
            .flex_col()
            .bg(theme.surface_base)
            .child(self.activity_toolbar(compact, summary, reconnect, theme))
            .child(rows)
    }

    /// Search, the live traffic summary and a refresh command. The command bar above already
    /// names the workspace, so the toolbar carries only what filters and refreshes the list.
    fn activity_toolbar(
        &self,
        compact: bool,
        summary: String,
        reconnect: AnyElement,
        theme: Theme,
    ) -> Div {
        div()
            .flex_shrink_0()
            .px(if compact {
                Space::Lg.px()
            } else {
                Space::Xl.px()
            })
            .py(Space::Md.px())
            .flex()
            .flex_wrap()
            .items_center()
            .gap(Space::Sm.px())
            .when_some(self.inputs.activity_search.clone(), |toolbar, input| {
                toolbar.child(
                    div()
                        .flex_1()
                        .min_w(px(180.0))
                        .when(compact, gpui::Styled::w_full)
                        .child(input),
                )
            })
            .child(
                div()
                    .flex_shrink_0()
                    .child(status_badge(summary, StatusTone::Neutral, theme)),
            )
            .child(reconnect)
    }

    fn activity_rows(
        &self,
        visible_connections: &[ActivityConnectionView<'_, '_>],
        no_query: bool,
        compact: bool,
        language: Language,
        theme: Theme,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::Stateful<Div> {
        let rows = div()
            .id("activity-scroll")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .p(if compact {
                Space::Lg.px()
            } else {
                Space::Xl.px()
            });
        if visible_connections.is_empty() {
            return rows.child(if no_query {
                empty_state(
                    language.message(Message::NoActiveConnections),
                    language.localized(
                        copy::activity::LIVE_TRAFFIC_APPEARS_HERE_AS_SOON_AS_THE_KERNEL_REPORTS,
                    ),
                    Some(
                        action_button(
                            "refresh-activity-empty",
                            language.message(Message::RefreshData),
                            ActionRole::Secondary,
                            ControlSize::Compact,
                        )
                        .accessibility_label(language.localized(
                            copy::activity::REFRESH_ACTIVITY_DATA_BY_RECONNECTING_THE_KERNEL,
                        ))
                        .border_color(theme.outline_subtle)
                        .bg(theme.surface_base)
                        .text_color(theme.text_primary)
                        .on_click(cx.listener(|this, _, _, cx| this.connect_mihomo(cx)))
                        .into_any_element(),
                    ),
                    theme,
                )
            } else {
                empty_state(
                    language.message(Message::NoFilterMatches),
                    language.localized(
                        copy::activity::TRY_A_TARGET_HOST_PROCESS_NAME_RULE_OR_ROUTE_STAGE,
                    ),
                    None,
                    theme,
                )
            });
        }
        let mut card = card_surface(theme).overflow_hidden().flex().flex_col();
        let last = visible_connections.len().saturating_sub(1);
        for (index, view) in visible_connections.iter().enumerate() {
            card = card.child(self.activity_row(
                view.connection,
                view.rule_group,
                theme,
                language,
                index,
                index == last,
                cx,
            ));
        }
        rows.child(card)
    }

    #[allow(clippy::too_many_arguments)] // One row needs its connection, route, theme and slot.
    fn activity_row(
        &self,
        connection: &Connection,
        rule_group: Option<&str>,
        theme: Theme,
        language: Language,
        index: usize,
        last: bool,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::Stateful<Div> {
        let chain = observed_route(connection, rule_group, language);
        let draft = rule_draft_from_connection(connection);
        let policy_hint = connection.chains.first().cloned();
        let mut row = div()
            .id(gpui::ElementId::Name(
                format!("activity-connection-{index}").into(),
            ))
            .debug_selector(move || format!("activity-connection-{index}"))
            .flex_shrink_0()
            .px(Space::Lg.px())
            .py(Space::Md.px())
            .flex()
            .flex_col()
            .gap(px(7.0))
            .when(!last, |row| {
                row.border_b_1().border_color(theme.outline_subtle)
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(Space::Md.px())
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(7.0))
                            .min_w(px(0.0))
                            .child(
                                div()
                                    .min_w(px(0.0))
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .font_family(MONO_FAMILY)
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_size(TextRole::Label.size())
                                    .line_height(TextRole::Label.line_height())
                                    .text_color(theme.text_primary)
                                    .child(connection_target(connection, language)),
                            )
                            .child(div().flex_shrink_0().child(status_badge(
                                connection_process(connection, language),
                                StatusTone::Neutral,
                                theme,
                            ))),
                    )
                    .child(traffic_cell(connection, theme)),
            )
            .child(self.route_chain_row(&chain, index, language, theme));
        if let Some(draft) = draft {
            let RuleDraft {
                kind,
                value,
                prefills,
            } = draft;
            row = row
                .cursor_pointer()
                .hover(move |style| style.bg(theme.surface_low));
            row = row.on_click(cx.listener(move |this, _, window, cx| {
                this.open_manual_rule_editor_for_connection(
                    kind,
                    value.clone(),
                    policy_hint.clone(),
                    prefills.clone(),
                    window,
                    cx,
                );
            }));
        }
        row
    }

    /// The signature Route Truth Chain: matched rule, policy group, then the observed exit.
    fn route_chain_row(
        &self,
        chain: &RouteChain,
        row_index: usize,
        language: Language,
        theme: Theme,
    ) -> Div {
        let mut line = div()
            .flex()
            .items_center()
            .flex_wrap()
            .gap(px(6.0))
            .min_w(px(0.0))
            .child(route_segment(
                chain.rule.clone(),
                RouteSegmentKind::Rule,
                None,
                theme,
            ));
        if let Some(group) = chain.group.as_deref() {
            line = line.child(route_arrow(theme)).child(route_segment(
                group,
                RouteSegmentKind::Group,
                None,
                theme,
            ));
        }
        match chain.exit.as_deref() {
            Some("DIRECT") => line.child(route_arrow(theme)).child(route_segment(
                route_stage_label("DIRECT", language),
                RouteSegmentKind::Direct,
                None,
                theme,
            )),
            Some("REJECT") => line.child(route_arrow(theme)).child(route_segment(
                "REJECT",
                RouteSegmentKind::Reject,
                None,
                theme,
            )),
            Some(node) => {
                let (latency, measured_at) = self.exit_latency(node);
                line.child(route_arrow(theme))
                    .child(route_segment(
                        node.to_owned(),
                        RouteSegmentKind::Exit,
                        latency,
                        theme,
                    ))
                    .child(latency_pill_with_tooltip(
                        format!("activity-latency-{row_index}"),
                        latency,
                        measured_at,
                        language,
                        theme,
                    ))
            }
            // The kernel reported no route for this connection, so the chain says so instead
            // of claiming an exit it never observed.
            None => line.child(route_segment(
                language.localized(copy::activity::ROUTE_UNAVAILABLE),
                RouteSegmentKind::Direct,
                None,
                theme,
            )),
        }
    }

    /// The last measured delay and measurement timestamp for an exit node.
    fn exit_latency(&self, node: &str) -> (Option<u64>, Option<u64>) {
        let catalog_ms = self
            .policy_groups()
            .flat_map(|group| group.nodes.iter())
            .find(|candidate| candidate.name == node)
            .and_then(|candidate| candidate.latency_ms)
            .map(u64::from);
        self.resolved_policy_exit_latency(node, catalog_ms)
    }
}

/// The toolbar summary. The active connection count already lives in the command bar, so an
/// unfiltered row shows only traffic; a filtered row adds the match count it cannot show there.
fn activity_summary(
    language: Language,
    unfiltered: bool,
    visible_count: usize,
    total_count: usize,
    total_download: u64,
    total_upload: u64,
) -> String {
    let traffic = format!(
        "↓ {} · ↑ {}",
        format_bytes(total_download),
        format_bytes(total_upload)
    );
    if unfiltered {
        return traffic;
    }
    format!(
        "{visible_count}/{total_count} {} · {traffic}",
        language.localized(copy::activity::CONNECTIONS)
    )
}

fn traffic_cell(connection: &Connection, theme: Theme) -> Div {
    div()
        .flex_shrink_0()
        .flex()
        .justify_end()
        .font_family(MONO_FAMILY)
        .text_size(TextRole::Metadata.size())
        .line_height(TextRole::Metadata.line_height())
        .text_color(theme.text_secondary)
        .child(format!(
            "↓ {} · ↑ {}",
            format_bytes(connection.download),
            format_bytes(connection.upload)
        ))
}

/// Reads the observed chain in display order, hiding the runtime groups the kernel adds.
fn observed_route(
    connection: &Connection,
    rule_group: Option<&str>,
    language: Language,
) -> RouteChain {
    let rule_kind = connection
        .rule
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let rule = match rule_kind {
        Some(kind) => match connection
            .rule_payload
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            Some(payload) => format!("{kind} {payload}"),
            None => kind.to_owned(),
        },
        None => language
            .localized(copy::activity::NO_MATCHED_RULE)
            .to_owned(),
    };
    let stages = user_route_stages(&connection.chains).collect::<Vec<_>>();
    let group = rule_group
        .map(str::trim)
        .filter(|group| !group.is_empty())
        .map(str::to_owned)
        .or_else(|| stages.get(1).map(|stage| (*stage).to_owned()));
    RouteChain {
        rule,
        group,
        exit: stages.first().map(|stage| (*stage).to_owned()),
    }
}

/// Picks the rule a click on this connection should create: its domain as a suffix rule, or the
/// destination address as a CIDR rule when no hostname was sniffed.
fn rule_draft_from_connection(connection: &Connection) -> Option<RuleDraft> {
    let host = [
        connection.metadata.sniff_host.as_deref(),
        connection.metadata.host.as_deref(),
    ]
    .into_iter()
    .flatten()
    .map(str::trim)
    .find(|value| !value.is_empty());
    let mut draft = if let Some(host) = host {
        ip_or_suffix_draft(host)
    } else {
        let destination = connection.metadata.destination_ip.as_deref()?.trim();
        if destination.parse::<IpAddr>().is_err() {
            return None;
        }
        ip_or_suffix_draft(destination)
    };
    draft.prefills = process_prefills(connection, &draft.value);
    Some(draft)
}

/// The process name first and the executable path second, both trimmed, without repeats and
/// without a value the draft already holds.
fn process_prefills(connection: &Connection, draft_value: &str) -> Vec<ManualRulePrefill> {
    let mut prefills: Vec<ManualRulePrefill> = Vec::new();
    for (kind, observed) in [
        (
            ManualRuleKind::ProcessName,
            connection.metadata.process.as_deref(),
        ),
        (
            ManualRuleKind::ProcessPath,
            connection.metadata.process_path.as_deref(),
        ),
    ] {
        let Some(value) = observed.map(str::trim).filter(|value| !value.is_empty()) else {
            continue;
        };
        if value == draft_value || prefills.iter().any(|prefill| prefill.value == value) {
            continue;
        }
        prefills.push(ManualRulePrefill {
            kind,
            value: value.to_owned(),
        });
    }
    prefills
}

fn ip_or_suffix_draft(value: &str) -> RuleDraft {
    let (kind, value) = match value.parse::<IpAddr>() {
        Ok(IpAddr::V4(address)) => (ManualRuleKind::IpCidr, format!("{address}/32")),
        Ok(IpAddr::V6(address)) => (ManualRuleKind::Ip6Cidr, format!("{address}/128")),
        Err(_) => (ManualRuleKind::HostSuffix, value.to_owned()),
    };
    RuleDraft {
        kind,
        value,
        prefills: Vec::new(),
    }
}

fn route_stage_label(stage: &str, language: Language) -> &str {
    match stage {
        "DIRECT" => language.localized(copy::common::DIRECT),
        _ => stage,
    }
}

fn connection_process(connection: &Connection, language: Language) -> String {
    connection
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
        .unwrap_or_else(|| language.localized(copy::activity::UNKNOWN_PROCESS))
        .to_owned()
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
        (None, Some(port)) => copy::activity::unknown_target_port(language, port),
        (None, None) => language
            .localized(copy::activity::UNKNOWN_TARGET)
            .to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use manis_mihomo::{Connection, ConnectionMetadata};

    fn connection(value: serde_json::Value) -> Connection {
        serde_json::from_value(value).expect("connection fixture")
    }

    #[test]
    fn rule_draft_uses_the_sniffed_host_as_a_suffix_rule() {
        let connection = connection(serde_json::json!({
            "metadata": {
                "sniffHost": "cache11-lax2.steamcontent.com",
                "host": "155.133.231.9",
                "destinationIP": "155.133.231.9",
                "destinationPort": "443"
            }
        }));
        let draft = super::rule_draft_from_connection(&connection).expect("draft");
        assert_eq!(draft.kind, crate::manual_rule::ManualRuleKind::HostSuffix);
        assert_eq!(draft.value, "cache11-lax2.steamcontent.com");
    }

    #[test]
    fn rule_draft_falls_back_to_the_destination_address() {
        let v4 = connection(serde_json::json!({"metadata": {"destinationIP": "203.0.113.7"}}));
        let draft = super::rule_draft_from_connection(&v4).expect("draft");
        assert_eq!(draft.kind, crate::manual_rule::ManualRuleKind::IpCidr);
        assert_eq!(draft.value, "203.0.113.7/32");

        let v6 = connection(serde_json::json!({"metadata": {"destinationIP": "2001:db8::1"}}));
        let draft = super::rule_draft_from_connection(&v6).expect("draft");
        assert_eq!(draft.kind, crate::manual_rule::ManualRuleKind::Ip6Cidr);
        assert_eq!(draft.value, "2001:db8::1/128");
    }

    #[gpui::test]
    fn activity_click_opens_the_rule_editor_prefilled(cx: &mut gpui::TestAppContext) {
        use gpui::AppContext as _;
        use gpui_component::WindowExt as _;

        cx.update(crate::init);
        cx.update(|cx| cx.set_reduce_motion(true));
        let mut app = None;
        let (_, cx) = cx.add_window_view(|window, cx| {
            let entity =
                cx.new(|_| super::ManisApp::with_fixture_controller("http://127.0.0.1:9090"));
            app = Some(entity.clone());
            crate::root(entity, window, cx)
        });
        let app = app.expect("fixture app");
        cx.update(|window, cx| {
            app.update(cx, |app, cx| {
                app.open_manual_rule_editor_for_connection(
                    crate::manual_rule::ManualRuleKind::HostSuffix,
                    "cache11-lax2.steamcontent.com".to_owned(),
                    Some("DIRECT".to_owned()),
                    Vec::new(),
                    window,
                    cx,
                );
            });
            window.draw(cx).clear(cx);
        });

        app.update(cx, |app, cx| {
            assert_eq!(
                app.manual_rule_editor_state,
                crate::app::ManualRuleEditorState::Creating
            );
            assert_eq!(
                app.manual_rule_insert,
                crate::app::ManualRuleInsert::Prepend
            );
            assert_eq!(app.manual_rule_target, "DIRECT");
            let condition = app.manual_rule_conditions.first().expect("condition");
            assert_eq!(
                condition.kind,
                crate::manual_rule::ManualRuleKind::HostSuffix
            );
            assert_eq!(
                condition.input.read(cx).value(),
                "cache11-lax2.steamcontent.com"
            );
        });
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
            assert!(window.has_active_dialog(cx), "the rule dialog stays open");
        });
    }

    #[test]
    fn rule_draft_offers_the_process_name_then_the_process_path() {
        let connection = connection(serde_json::json!({
            "metadata": {
                "sniffHost": "cache11-lax2.steamcontent.com",
                "process": " magpie ",
                "processPath": "/Applications/magpie.app/Contents/MacOS/magpie"
            }
        }));

        let draft = super::rule_draft_from_connection(&connection).expect("draft");

        assert_eq!(
            draft
                .prefills
                .iter()
                .map(|prefill| (prefill.kind, prefill.value.as_str()))
                .collect::<Vec<_>>(),
            vec![
                (crate::manual_rule::ManualRuleKind::ProcessName, "magpie"),
                (
                    crate::manual_rule::ManualRuleKind::ProcessPath,
                    "/Applications/magpie.app/Contents/MacOS/magpie"
                ),
            ]
        );
    }

    #[test]
    fn rule_draft_skips_prefills_that_repeat_its_own_value_or_each_other() {
        let repeated = connection(serde_json::json!({
            "metadata": { "sniffHost": "magpie", "process": "magpie" }
        }));
        assert!(
            super::rule_draft_from_connection(&repeated)
                .expect("draft")
                .prefills
                .is_empty()
        );

        let duplicated = connection(serde_json::json!({
            "metadata": {
                "destinationIP": "203.0.113.7",
                "process": "magpie",
                "processPath": "magpie"
            }
        }));
        let draft = super::rule_draft_from_connection(&duplicated).expect("draft");
        assert_eq!(
            draft
                .prefills
                .iter()
                .map(|prefill| (prefill.kind, prefill.value.as_str()))
                .collect::<Vec<_>>(),
            vec![(crate::manual_rule::ManualRuleKind::ProcessName, "magpie")]
        );
    }

    #[test]
    fn rule_draft_has_no_prefills_without_process_metadata() {
        let connection =
            connection(serde_json::json!({"metadata": {"destinationIP": "203.0.113.7"}}));

        assert!(
            super::rule_draft_from_connection(&connection)
                .expect("draft")
                .prefills
                .is_empty()
        );
    }

    #[test]
    fn rule_draft_skips_connections_without_a_target() {
        let empty = connection(serde_json::json!({"metadata": {}}));
        assert!(super::rule_draft_from_connection(&empty).is_none());
    }

    /// A rendered connection row is the whole click-to-prefill path: it must exist in the list,
    /// take the click, and open the editor on the host's suffix rule.
    #[gpui::test]
    fn a_rendered_connection_row_opens_the_rule_editor_prefilled(cx: &mut gpui::TestAppContext) {
        use gpui::{AppContext as _, Modifiers};

        cx.update(crate::init);
        let mut app = None;
        let (_, cx) = cx.add_window_view(|window, cx| {
            let entity = cx.new(|_| {
                let mut app = super::ManisApp::with_fixture_controller("http://127.0.0.1:1");
                app.primary_workspace = manis_core::PrimaryWorkspace::Activity;
                app.active_connections = vec![connection(serde_json::json!({
                    "metadata": {
                        "sniffHost": "chat.openai.com",
                        "process": "Safari",
                        "destinationPort": "443",
                        "destinationIP": "203.0.113.10"
                    },
                    "chains": ["SG-02", "AI 自动选择", "__MANIS_GLOBAL__", "GLOBAL"],
                    "rule": "DOMAIN-SUFFIX",
                    "rulePayload": "openai.com",
                    "download": 14_200_000,
                    "upload": 3_100_000
                }))];
                app
            });
            app = Some(entity.clone());
            crate::root(entity, window, cx)
        });
        let app = app.expect("fixture app");
        cx.update(|window, cx| window.draw(cx).clear(cx));

        let bounds = cx
            .debug_bounds("activity-connection-0")
            .expect("the connection row renders");
        cx.simulate_click(bounds.center(), Modifiers::none());

        app.read_with(cx, |app, cx| {
            assert_eq!(
                app.manual_rule_editor_state,
                crate::app::ManualRuleEditorState::Creating
            );
            let condition = app.manual_rule_conditions.first().expect("condition");
            assert_eq!(
                condition.kind,
                crate::manual_rule::ManualRuleKind::HostSuffix
            );
            assert_eq!(condition.input.read(cx).value(), "chat.openai.com");
            // The exit the chain reports becomes the draft's target; this fixture has no
            // catalog, so the editor falls back to DIRECT.
            assert!(!app.manual_rule_target.is_empty());
        });
    }

    /// The chain reads rule, then policy group, then exit, and never shows a runtime group.
    #[test]
    fn observed_route_reads_from_the_matched_group_to_the_exit_node() {
        let mut connection = connection(serde_json::json!({
            "metadata": {"host": "www.example.com"},
            "rule": "DomainSuffix",
            "rulePayload": "example.com",
            "chains": ["HK05", "__MANIS_GLOBAL__", "GLOBAL"]
        }));
        let chain = super::observed_route(
            &connection,
            Some("Manual rules"),
            crate::localization::Language::SimplifiedChinese,
        );
        assert_eq!(chain.rule, "DomainSuffix example.com");
        assert_eq!(chain.group.as_deref(), Some("Manual rules"));
        assert_eq!(chain.exit.as_deref(), Some("HK05"));

        connection.chains = vec![
            "HK05".to_owned(),
            "Hong Kong".to_owned(),
            "Streaming".to_owned(),
        ];
        let chain = super::observed_route(
            &connection,
            Some("Streaming rules"),
            crate::localization::Language::English,
        );
        assert_eq!(chain.group.as_deref(), Some("Streaming rules"));
        assert_eq!(chain.exit.as_deref(), Some("HK05"));

        // Without a matched rule group the chain falls back to the exit's own policy group.
        let chain =
            super::observed_route(&connection, None, crate::localization::Language::English);
        assert_eq!(chain.group.as_deref(), Some("Hong Kong"));
    }

    #[test]
    fn observed_route_names_direct_connections_and_missing_rules() {
        let connection = connection(serde_json::json!({
            "metadata": {"host": "www.example.com"},
            "chains": ["DIRECT"]
        }));
        let chain = super::observed_route(
            &connection,
            None,
            crate::localization::Language::SimplifiedChinese,
        );
        assert_eq!(chain.rule, "未匹配规则");
        assert_eq!(chain.group, None);
        assert_eq!(chain.exit.as_deref(), Some("DIRECT"));
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
