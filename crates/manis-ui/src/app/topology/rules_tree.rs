use std::collections::{BTreeMap, BTreeSet};

use gpui::{
    AnyElement, Context, Div, FontWeight, InteractiveElement, IntoElement, ParentElement, Stateful,
    StatefulInteractiveElement, Styled, div, prelude::FluentBuilder as _, px,
};
use gpui_component::{Disableable as _, IconName, Sizable as _, switch::Switch};
use manis_core::WindowSizeClass;
use manis_profile::QxRuleList;

use super::{
    BezierEdge, EdgeOrientation, PortRegistry, PortSide, RulesTreeFocus, compact_protocol_label,
    legend_entry, new_port_registry, paint_edges_canvas, port_dot, port_tracker, with_alpha,
};
use crate::{
    app::{ManisApp, nodes::region_code, presentation::policy_kind_label},
    components::{
        ActionRole, RouteSegmentKind, StatusTone, action_button, card_surface, empty_state,
        region_code_badge, route_segment, status_badge,
    },
    localization::{Language, Message, copy},
    manual_rule::ManualRule,
    mihomo,
    theme::{ControlSize, MONO_FAMILY, Radius, Space, Theme},
};

const DIRECT_TARGET: &str = "DIRECT";
const REJECT_TARGET: &str = "REJECT";

enum RuleToggleAction {
    Remote(String),
    Local(usize),
}

struct RuleTreeItem {
    id: String,
    order: String,
    title: String,
    subtitle: String,
    samples: Vec<String>,
    target_policy: String,
    enabled: bool,
    toggle: Option<RuleToggleAction>,
}

struct PolicySlot {
    name: String,
    strategy: String,
    exit_name: String,
    latency_ms: Option<u64>,
    is_direct: bool,
    is_reject: bool,
}

struct ActiveExitSlot {
    name: String,
    region: String,
    protocol: String,
    latency_ms: Option<u64>,
    serving_count: usize,
    is_direct: bool,
    is_reject: bool,
}

impl ManisApp {
    /// Three-column routing flow workspace: `Rules -> Policy Group -> Active Exit Node`.
    #[allow(clippy::too_many_lines)] // Keeps the 3-column flow layout and edge builder together.
    pub(in crate::app) fn rules_tree_workspace(
        &self,
        theme: Theme,
        size_class: WindowSizeClass,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let compact = size_class == WindowSizeClass::Compact;
        let language = self.language();
        let (rule_items, policy_slots, exit_slots) = self.collect_rules_tree_data(language);

        let focus = self.topology.active_rules_focus().cloned();
        let inspector = Self::rules_tree_inspector(
            &rule_items,
            &policy_slots,
            &exit_slots,
            focus.as_ref(),
            language,
            theme,
            cx,
        );

        if rule_items.is_empty() {
            return div()
                .id("rules-tree-empty")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .px(if compact { Space::Md.px() } else { Space::Xl.px() })
                .py(Space::Lg.px())
                .flex()
                .flex_col()
                .gap(Space::Md.px())
                .child(inspector)
                .child(empty_state(
                    language.localized(copy::configuration::NO_ROUTING_RULES_YET),
                    language.localized(
                        copy::configuration::ADD_RULES_TO_SEND_MATCHING_CONNECTIONS_THROUGH_A_POLICY_GROUP,
                    ),
                    None,
                    theme,
                ));
        }

        let registry = new_port_registry();
        let edges = build_rules_tree_edges(&rule_items, &policy_slots, focus.as_ref(), theme);

        let (hl_rules, hl_policies, hl_exits) =
            compute_rules_tree_highlights(&rule_items, &policy_slots, focus.as_ref());
        let has_focus = focus.is_some();

        let col1_cards = rule_items
            .iter()
            .enumerate()
            .map(|(index, item)| {
                self.rules_tree_rule_card(
                    index,
                    item,
                    hl_rules.contains(item.id.as_str()),
                    has_focus,
                    registry.clone(),
                    theme,
                    cx,
                )
            })
            .collect::<Vec<_>>();

        let col2_cards = policy_slots
            .iter()
            .enumerate()
            .map(|(index, slot)| {
                Self::rules_tree_policy_card(
                    index,
                    slot,
                    hl_policies.contains(slot.name.as_str()),
                    has_focus,
                    registry.clone(),
                    language,
                    theme,
                    cx,
                )
            })
            .collect::<Vec<_>>();

        let col3_cards = exit_slots
            .iter()
            .enumerate()
            .map(|(index, exit)| {
                Self::rules_tree_exit_card(
                    index,
                    exit,
                    hl_exits.contains(exit.name.as_str()),
                    has_focus,
                    registry.clone(),
                    language,
                    theme,
                    cx,
                )
            })
            .collect::<Vec<_>>();

        let flow_inner = div()
            .relative()
            .w_full()
            .min_w(px(860.0))
            .flex()
            .items_start()
            .child(paint_edges_canvas(registry, edges))
            // Column 1: Rules
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_w(px(270.0))
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(col_tag(
                        match language {
                            Language::English => "01 · ROUTING RULES (TOP TO BOTTOM)",
                            Language::SimplifiedChinese => "01 · 分流规则（自上而下）",
                        },
                        theme.route_trace,
                        theme.route_soft,
                    ))
                    .children(col1_cards),
            )
            // Unobstructed Bézier bay 1 -> 2
            .child(div().w(px(88.0)).flex_shrink_0())
            // Column 2: Policy Groups
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_w(px(210.0))
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(col_tag(
                        match language {
                            Language::English => "02 · POLICY GROUPS",
                            Language::SimplifiedChinese => "02 · 策略组",
                        },
                        theme.action_primary,
                        theme.action_soft,
                    ))
                    .children(col2_cards),
            )
            // Unobstructed Bézier bay 2 -> 3
            .child(div().w(px(88.0)).flex_shrink_0())
            // Column 3: Active Exit Nodes
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_w(px(200.0))
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(col_tag(
                        match language {
                            Language::English => "03 · ACTIVE EXITS",
                            Language::SimplifiedChinese => "03 · 当前激活出口",
                        },
                        theme.status_success,
                        theme.status_success_soft,
                    ))
                    .children(col3_cards),
            );

        let canvas_card = card_surface(theme)
            .id("rules-tree-canvas")
            .debug_selector(|| "rules-tree-canvas".to_owned())
            .flex_1()
            .min_h(px(400.0))
            .p(px(18.0))
            .overflow_x_scroll()
            .on_click(cx.listener(|this, _, _, cx| {
                if this.topology.pinned_rules.take().is_some() {
                    cx.notify();
                }
            }))
            .child(flow_inner);

        div()
            .id("rules-tree-scroll")
            .debug_selector(|| "rules-tree-scroll".to_owned())
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .px(if compact {
                Space::Md.px()
            } else {
                Space::Xl.px()
            })
            .py(Space::Lg.px())
            .flex()
            .flex_col()
            .gap(Space::Md.px())
            .child(inspector)
            .child(canvas_card)
    }

    #[allow(clippy::too_many_lines)] // Gathers live rules, target policy groups, and deduplicated exits.
    fn collect_rules_tree_data(
        &self,
        language: Language,
    ) -> (Vec<RuleTreeItem>, Vec<PolicySlot>, Vec<ActiveExitSlot>) {
        // 1. Gather policy group active exits from live catalog or offline managed groups.
        let mut policy_lookup: BTreeMap<String, (String, String, Option<u64>, String)> =
            BTreeMap::new();
        let mut policy_order: Vec<String> = Vec::new();

        if self.catalog.is_some() {
            for group in self
                .policy_groups()
                .filter(|g| g.name != manis_profile::MANIS_GLOBAL_GROUP_NAME)
            {
                let selected = self.selected_node_for_policy(group);
                let exit_name =
                    selected.map_or_else(|| DIRECT_TARGET.to_owned(), |node| node.name.clone());
                let latency_ms = selected.and_then(|node| node.latency_ms).map(u64::from);
                let protocol = selected
                    .map(|node| compact_protocol_label(&node.detail))
                    .unwrap_or_default();
                let strategy = policy_kind_label(language, group.kind).to_owned();
                policy_order.push(group.name.clone());
                policy_lookup.insert(
                    group.name.clone(),
                    (strategy, exit_name, latency_ms, protocol),
                );
            }
        } else {
            for group in &self.managed_policies.groups {
                let candidates = self.managed_policy_candidate_nodes(group);
                let selected_name = self
                    .managed_policies
                    .node_selections
                    .policy_target(&group.name)
                    .map(str::to_owned)
                    .or_else(|| candidates.first().map(|node| node.name.clone()))
                    .unwrap_or_else(|| DIRECT_TARGET.to_owned());
                let selected_node = candidates.iter().find(|node| node.name == selected_name);
                let latency_ms = selected_node
                    .and_then(|node| node.latency_ms)
                    .map(u64::from);
                let protocol = selected_node
                    .map(|node| compact_protocol_label(&node.detail))
                    .unwrap_or_default();
                let strategy = Self::offline_policy_strategy(language, group.strategy);
                policy_order.push(group.name.clone());
                policy_lookup.insert(
                    group.name.clone(),
                    (strategy, selected_name, latency_ms, protocol),
                );
            }
        }

        // 2. Build ordered rule items from normalized routing rule group order.
        let group_order = mihomo::normalized_routing_rule_group_order(
            &self.rule_sources.group_order,
            !self.manual_rules.is_empty(),
            &self.rule_sources.sources,
            &self.local_rule_groups,
        );

        let mut rule_items = Vec::new();
        let mut seq = 1usize;

        for group_id in &group_order {
            if group_id == mihomo::MANUAL_ROUTING_RULE_GROUP_ID {
                for (rule_index, rule) in self.manual_rules.iter().enumerate() {
                    let target = self.effective_rule_target(rule.target(), language);
                    rule_items.push(RuleTreeItem {
                        id: format!("manual:{rule_index}"),
                        order: format!("{seq:02}"),
                        title: format_manual_rule_summary(rule),
                        subtitle: language.localized(copy::common::MANUAL_RULES).to_owned(),
                        samples: Vec::new(),
                        target_policy: target,
                        enabled: rule.is_enabled(),
                        toggle: None,
                    });
                    seq += 1;
                }
            } else if let Some((source_index, source)) = self
                .rule_sources
                .sources
                .iter()
                .enumerate()
                .find(|(_, source)| source.id == *group_id)
            {
                let parsed = QxRuleList::parse(&source.content);
                let target = self.effective_rule_target(source.target_policy.as_str(), language);
                let name = Self::qx_rule_source_name(source, source_index, language);
                let samples = parsed
                    .rules
                    .iter()
                    .take(2)
                    .map(|rule| rule.value.clone())
                    .collect();
                rule_items.push(RuleTreeItem {
                    id: format!("remote:{}", source.id),
                    order: format!("{seq:02}"),
                    title: name,
                    subtitle: match language {
                        Language::English => format!("Remote · {} rules", parsed.rules.len()),
                        Language::SimplifiedChinese => {
                            format!("远程规则源 · {} 条", parsed.rules.len())
                        }
                    },
                    samples,
                    target_policy: target,
                    enabled: source.enabled,
                    toggle: Some(RuleToggleAction::Remote(source.id.clone())),
                });
                seq += 1;
            } else if let Some((group_index, group)) = self
                .local_rule_groups
                .iter()
                .enumerate()
                .find(|(_, group)| group.id == *group_id)
            {
                let target = self.effective_rule_target(group.target_policy.as_str(), language);
                let samples = group
                    .rules
                    .iter()
                    .take(2)
                    .map(format_manual_rule_value)
                    .collect();
                rule_items.push(RuleTreeItem {
                    id: format!("local:{}", group.id),
                    order: format!("{seq:02}"),
                    title: group.name.clone(),
                    subtitle: match language {
                        Language::English => format!("Local · {} rules", group.rules.len()),
                        Language::SimplifiedChinese => {
                            format!("本地规则组 · {} 条", group.rules.len())
                        }
                    },
                    samples,
                    target_policy: target,
                    enabled: group.enabled,
                    toggle: Some(RuleToggleAction::Local(group_index)),
                });
                seq += 1;
            }
        }

        if !rule_items.is_empty() {
            let fallback_target = self.effective_rule_target("Proxy", language);
            rule_items.push(RuleTreeItem {
                id: "final".to_owned(),
                order: "END".to_owned(),
                title: match language {
                    Language::English => "FINAL · Default fallback".to_owned(),
                    Language::SimplifiedChinese => "FINAL · 默认兜底".to_owned(),
                },
                subtitle: match language {
                    Language::English => "Traffic that matched no rule above".to_owned(),
                    Language::SimplifiedChinese => "未命中以上规则的其余流量".to_owned(),
                },
                samples: vec!["MATCH".to_owned()],
                target_policy: fallback_target,
                enabled: true,
                toggle: None,
            });
        }

        // 3. Build Column 2 policy slots (all known policies + any built-in targets referenced).
        let mut policy_slots = Vec::new();
        let mut seen_policies = BTreeSet::new();
        let mut exit_protocols: BTreeMap<String, String> = BTreeMap::new();
        let mut exit_latencies: BTreeMap<String, Option<u64>> = BTreeMap::new();

        for name in &policy_order {
            if let Some((strategy, exit_name, latency_ms, proto)) = policy_lookup.get(name) {
                seen_policies.insert(name.clone());
                exit_protocols
                    .entry(exit_name.clone())
                    .or_insert_with(|| proto.clone());
                exit_latencies
                    .entry(exit_name.clone())
                    .or_insert(*latency_ms);
                policy_slots.push(PolicySlot {
                    name: name.clone(),
                    strategy: strategy.clone(),
                    exit_name: exit_name.clone(),
                    latency_ms: *latency_ms,
                    is_direct: false,
                    is_reject: false,
                });
            }
        }

        // Add any extra targets referenced by rules (e.g. DIRECT, REJECT, Global exit).
        for item in &rule_items {
            if seen_policies.insert(item.target_policy.clone()) {
                let is_direct = item.target_policy.eq_ignore_ascii_case(DIRECT_TARGET);
                let is_reject = item.target_policy.eq_ignore_ascii_case(REJECT_TARGET);
                let (strategy, exit_name) = if is_direct {
                    (
                        match language {
                            Language::English => "Built-in",
                            Language::SimplifiedChinese => "内置策略",
                        }
                        .to_owned(),
                        DIRECT_TARGET.to_owned(),
                    )
                } else if is_reject {
                    (
                        match language {
                            Language::English => "Built-in",
                            Language::SimplifiedChinese => "内置策略",
                        }
                        .to_owned(),
                        REJECT_TARGET.to_owned(),
                    )
                } else {
                    let global_exit = self
                        .managed_policies
                        .node_selections
                        .global()
                        .map_or_else(|| DIRECT_TARGET.to_owned(), |id| id.node_name.clone());
                    (
                        match language {
                            Language::English => "Global",
                            Language::SimplifiedChinese => "全局出口",
                        }
                        .to_owned(),
                        global_exit,
                    )
                };
                policy_slots.push(PolicySlot {
                    name: item.target_policy.clone(),
                    strategy,
                    exit_name,
                    latency_ms: None,
                    is_direct,
                    is_reject,
                });
            }
        }

        // 4. Build Column 3 deduplicated active exit nodes.
        let mut exit_counts: BTreeMap<String, usize> = BTreeMap::new();
        let mut exit_order: Vec<String> = Vec::new();
        for slot in &policy_slots {
            let count = exit_counts.entry(slot.exit_name.clone()).or_insert(0);
            if *count == 0 {
                exit_order.push(slot.exit_name.clone());
            }
            *count += 1;
        }

        let exit_slots = exit_order
            .into_iter()
            .map(|exit_name| {
                let is_direct = exit_name.eq_ignore_ascii_case(DIRECT_TARGET);
                let is_reject = exit_name.eq_ignore_ascii_case(REJECT_TARGET);
                let serving_count = exit_counts.get(&exit_name).copied().unwrap_or(0);
                let region = if is_direct {
                    "CN".to_owned()
                } else if is_reject {
                    "NO".to_owned()
                } else {
                    region_code(&exit_name)
                };
                let protocol = if is_direct {
                    DIRECT_TARGET.to_owned()
                } else if is_reject {
                    REJECT_TARGET.to_owned()
                } else {
                    exit_protocols.get(&exit_name).cloned().unwrap_or_default()
                };
                let latency_ms = exit_latencies.get(&exit_name).copied().flatten();
                ActiveExitSlot {
                    name: exit_name,
                    region,
                    protocol,
                    latency_ms,
                    serving_count,
                    is_direct,
                    is_reject,
                }
            })
            .collect();

        (rule_items, policy_slots, exit_slots)
    }

    #[expect(
        clippy::too_many_lines,
        reason = "Builds the single cohesive route-chain inspector bar for all focus states"
    )]
    fn rules_tree_inspector(
        rule_items: &[RuleTreeItem],
        policy_slots: &[PolicySlot],
        exit_slots: &[ActiveExitSlot],
        focus: Option<&RulesTreeFocus>,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let mut summary = div()
            .flex()
            .items_center()
            .gap(Space::Sm.px())
            .flex_wrap()
            .min_w_0()
            .flex_1();

        match focus {
            None => {
                summary = summary
                    .child(div().size(px(7.0)).rounded_full().bg(theme.route_trace))
                    .child(
                        div()
                            .text_size(px(12.5))
                            .line_height(px(18.0))
                            .text_color(theme.text_secondary)
                            .child(match language {
                                Language::English => {
                                    "Hover any rule to trace Rule → Policy Group → Active Exit, or hover an exit to find every matching rule."
                                }
                                Language::SimplifiedChinese => {
                                    "悬停左侧分流规则查看「规则 → 策略组 → 激活节点」完整链路；悬停右侧出口可反查所有经由该节点出站的规则。"
                                }
                            }),
                    );
            }
            Some(RulesTreeFocus::Rule(rule_id)) => {
                if let Some(item) = rule_items.iter().find(|r| r.id == *rule_id) {
                    let slot = policy_slots.iter().find(|p| p.name == item.target_policy);
                    let exit_name = slot.map_or(DIRECT_TARGET, |p| p.exit_name.as_str());
                    let exit_kind = if exit_name.eq_ignore_ascii_case(DIRECT_TARGET) {
                        RouteSegmentKind::Direct
                    } else if exit_name.eq_ignore_ascii_case(REJECT_TARGET) {
                        RouteSegmentKind::Reject
                    } else {
                        RouteSegmentKind::Exit
                    };
                    let exit_label = match slot.and_then(|s| s.latency_ms) {
                        Some(ms) => format!(
                            "{} · {ms} ms",
                            Self::policy_candidate_display_name(exit_name)
                        ),
                        None => Self::policy_candidate_display_name(exit_name).to_owned(),
                    };
                    summary = summary
                        .child(route_segment(
                            item.title.clone(),
                            RouteSegmentKind::Rule,
                            None,
                            theme,
                        ))
                        .child(arrow_sep(theme))
                        .child(route_segment(
                            item.target_policy.clone(),
                            RouteSegmentKind::Group,
                            None,
                            theme,
                        ))
                        .child(arrow_sep(theme))
                        .child(route_segment(
                            exit_label,
                            exit_kind,
                            slot.and_then(|s| s.latency_ms),
                            theme,
                        ))
                        .when(!item.enabled, |row| {
                            row.child(status_badge(
                                language.localized(copy::configuration::DISABLED),
                                StatusTone::Neutral,
                                theme,
                            ))
                        });
                }
            }
            Some(RulesTreeFocus::Policy(policy_name)) => {
                let upstream_count = rule_items
                    .iter()
                    .filter(|r| r.enabled && r.target_policy == *policy_name)
                    .count();
                let slot = policy_slots.iter().find(|p| p.name == *policy_name);
                let exit_name = slot.map_or(DIRECT_TARGET, |p| p.exit_name.as_str());
                summary = summary
                    .child(status_badge(
                        match language {
                            Language::English => format!("{upstream_count} upstream rules"),
                            Language::SimplifiedChinese => format!("{upstream_count} 条上游规则"),
                        },
                        StatusTone::Route,
                        theme,
                    ))
                    .child(arrow_sep(theme))
                    .child(route_segment(
                        policy_name.clone(),
                        RouteSegmentKind::Group,
                        None,
                        theme,
                    ))
                    .child(arrow_sep(theme))
                    .child(route_segment(
                        Self::policy_candidate_display_name(exit_name).to_owned(),
                        if exit_name.eq_ignore_ascii_case(DIRECT_TARGET) {
                            RouteSegmentKind::Direct
                        } else {
                            RouteSegmentKind::Exit
                        },
                        slot.and_then(|s| s.latency_ms),
                        theme,
                    ));
            }
            Some(RulesTreeFocus::Exit(exit_name)) => {
                let using_policies: BTreeSet<&str> = policy_slots
                    .iter()
                    .filter(|p| p.exit_name == *exit_name)
                    .map(|p| p.name.as_str())
                    .collect();
                let rule_count = rule_items
                    .iter()
                    .filter(|r| r.enabled && using_policies.contains(r.target_policy.as_str()))
                    .count();
                let exit = exit_slots.iter().find(|e| e.name == *exit_name);
                summary = summary
                    .child(route_segment(
                        Self::policy_candidate_display_name(exit_name).to_owned(),
                        if exit_name.eq_ignore_ascii_case(DIRECT_TARGET) {
                            RouteSegmentKind::Direct
                        } else {
                            RouteSegmentKind::Exit
                        },
                        exit.and_then(|e| e.latency_ms),
                        theme,
                    ))
                    .child(arrow_sep(theme))
                    .child(status_badge(
                        match language {
                            Language::English => {
                                format!("{} policy groups", using_policies.len())
                            }
                            Language::SimplifiedChinese => {
                                format!("承接 {} 个策略组", using_policies.len())
                            }
                        },
                        StatusTone::Neutral,
                        theme,
                    ))
                    .child(arrow_sep(theme))
                    .child(status_badge(
                        match language {
                            Language::English => format!("{rule_count} active rules"),
                            Language::SimplifiedChinese => format!("影响 {rule_count} 条生效规则"),
                        },
                        StatusTone::Route,
                        theme,
                    ));
            }
        }

        let actions = div()
            .flex()
            .items_center()
            .gap(Space::Md.px())
            .flex_shrink_0()
            .child(legend_entry(
                theme.route_trace,
                match language {
                    Language::English => "Rule match",
                    Language::SimplifiedChinese => "规则命中",
                },
                theme.text_tertiary,
            ))
            .child(legend_entry(
                theme.status_success,
                match language {
                    Language::English => "Active exit",
                    Language::SimplifiedChinese => "激活出口",
                },
                theme.text_tertiary,
            ))
            .child(
                action_button(
                    "open-local-rule-group-editor-tree",
                    language.localized(copy::configuration::ADD_LOCAL_RULE_GROUP),
                    ActionRole::Secondary,
                    ControlSize::Compact,
                )
                .icon(IconName::Plus)
                .on_click(cx.listener(|this, _, window, cx| {
                    this.open_local_rule_group_editor_new(window, cx);
                })),
            )
            .child(
                action_button(
                    "open-manual-rule-editor-tree",
                    language.message(Message::AddRule),
                    ActionRole::Primary,
                    ControlSize::Compact,
                )
                .icon(IconName::Plus)
                .on_click(cx.listener(|this, _, window, cx| {
                    this.open_manual_rule_editor(window, cx);
                })),
            );

        card_surface(theme)
            .id("rules-tree-inspector")
            .debug_selector(|| "rules-tree-inspector".to_owned())
            .px(px(14.0))
            .py(px(10.0))
            .border_color(if focus.is_some() {
                theme.route_trace
            } else {
                theme.outline_subtle
            })
            .flex()
            .items_center()
            .justify_between()
            .gap(Space::Md.px())
            .flex_wrap()
            .child(summary)
            .child(actions)
    }

    #[expect(
        clippy::too_many_arguments,
        clippy::too_many_lines,
        reason = "Single GPUI rule card builder with toggle switch, sample chips, and port dot"
    )]
    fn rules_tree_rule_card(
        &self,
        index: usize,
        item: &RuleTreeItem,
        highlighted: bool,
        has_focus: bool,
        registry: PortRegistry,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let dimmed = has_focus && !highlighted;
        let card_bg = if highlighted {
            theme.route_soft
        } else {
            theme.surface_elevated
        };
        let card_border = if highlighted {
            theme.route_trace
        } else {
            theme.outline_subtle
        };
        let hover_id = item.id.clone();
        let click_id = item.id.clone();
        let selector = format!("tree-rule-{index}");

        let switch_el: Option<AnyElement> = match &item.toggle {
            Some(RuleToggleAction::Remote(source_id)) => {
                let id = source_id.clone();
                let enabled = item.enabled;
                let interactive = !self.source_refresh_busy();
                Some(
                    Switch::new(format!("tree-rule-remote-switch-{index}"))
                        .checked(enabled)
                        .disabled(!interactive)
                        .small()
                        .on_click(cx.listener(move |this, checked: &bool, _, cx| {
                            cx.stop_propagation();
                            this.set_qx_rule_source_enabled(id.clone(), *checked, cx);
                        }))
                        .into_any_element(),
                )
            }
            Some(RuleToggleAction::Local(group_index)) => {
                let idx = *group_index;
                let enabled = item.enabled;
                let interactive =
                    !self.routing_apply_state.is_busy() && !self.configuration_transfer.active;
                Some(
                    Switch::new(format!("tree-rule-local-switch-{index}"))
                        .checked(enabled)
                        .disabled(!interactive)
                        .small()
                        .on_click(cx.listener(move |this, checked: &bool, _, cx| {
                            cx.stop_propagation();
                            this.set_local_rule_group_enabled(idx, *checked, cx);
                        }))
                        .into_any_element(),
                )
            }
            None => None,
        };

        div()
            .id(format!("tree-rule-card-{index}"))
            .debug_selector(move || selector)
            .relative()
            .px(px(11.0))
            .py(px(8.0))
            .rounded(px(11.0))
            .bg(card_bg)
            .border_1()
            .border_color(card_border)
            .when(dimmed, |el| el.opacity(0.28))
            .when(!item.enabled && !dimmed, |el| el.opacity(0.58))
            .cursor_pointer()
            .flex()
            .flex_col()
            .gap(px(4.0))
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                if *hovered {
                    this.topology.hovered_rules = Some(RulesTreeFocus::Rule(hover_id.clone()));
                    cx.notify();
                } else if matches!(
                    this.topology.hovered_rules.as_ref(),
                    Some(RulesTreeFocus::Rule(current)) if current == &hover_id
                ) {
                    this.topology.hovered_rules = None;
                    cx.notify();
                }
            }))
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                if matches!(
                    this.topology.pinned_rules.as_ref(),
                    Some(RulesTreeFocus::Rule(current)) if current == &click_id
                ) {
                    this.topology.pinned_rules = None;
                } else {
                    this.topology.pinned_rules = Some(RulesTreeFocus::Rule(click_id.clone()));
                }
                cx.notify();
            }))
            .child(port_tracker(registry, format!("r:{}", item.id)))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .min_w(px(30.0))
                            .px(px(5.0))
                            .py(px(1.5))
                            .rounded(Radius::Badge.px())
                            .bg(theme.surface_low)
                            .border_1()
                            .border_color(theme.outline_subtle)
                            .font_family(MONO_FAMILY)
                            .text_size(px(10.5))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.text_tertiary)
                            .text_center()
                            .child(item.order.clone()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .text_size(px(12.5))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.text_primary)
                            .child(item.title.clone()),
                    )
                    .when_some(switch_el, ParentElement::child),
            )
            .child(
                div()
                    .pl(px(38.0))
                    .flex()
                    .items_center()
                    .gap(px(5.0))
                    .flex_wrap()
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(theme.text_tertiary)
                            .child(item.subtitle.clone()),
                    )
                    .children(item.samples.iter().map(|sample| {
                        div()
                            .px(px(5.0))
                            .py(px(1.0))
                            .rounded(px(5.0))
                            .bg(theme.surface_low)
                            .border_1()
                            .border_color(theme.outline_subtle)
                            .font_family(MONO_FAMILY)
                            .text_size(px(10.0))
                            .text_color(theme.text_secondary)
                            .max_w(px(130.0))
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .child(sample.clone())
                    })),
            )
            .child(port_dot(
                PortSide::Right,
                if highlighted {
                    theme.route_trace
                } else {
                    theme.surface_elevated
                },
                if highlighted {
                    theme.route_trace
                } else {
                    theme.outline_strong
                },
                highlighted,
            ))
            .into_any_element()
    }

    #[expect(
        clippy::too_many_arguments,
        clippy::too_many_lines,
        reason = "Single GPUI policy slot card builder with dual anchor ports"
    )]
    fn rules_tree_policy_card(
        index: usize,
        slot: &PolicySlot,
        highlighted: bool,
        has_focus: bool,
        registry: PortRegistry,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let dimmed = has_focus && !highlighted;
        let neutral_builtin = slot.is_direct || slot.is_reject;
        let (card_bg, card_border) = if highlighted {
            if neutral_builtin {
                (theme.surface_high, theme.outline_strong)
            } else {
                (theme.action_soft, theme.action_primary)
            }
        } else {
            (theme.surface_elevated, theme.outline_subtle)
        };
        let hover_name = slot.name.clone();
        let click_name = slot.name.clone();
        let selector = format!("tree-rule-policy-{index}");

        let exit_display = if slot.is_direct {
            match language {
                Language::English => "Bypass proxy directly",
                Language::SimplifiedChinese => "跳过代理直连本地网络",
            }
            .to_owned()
        } else if slot.is_reject {
            match language {
                Language::English => "Drop matching traffic",
                Language::SimplifiedChinese => "拒绝命中连接",
            }
            .to_owned()
        } else {
            let exit = Self::policy_candidate_display_name(&slot.exit_name);
            match language {
                Language::English => format!("Active: {exit}"),
                Language::SimplifiedChinese => format!("激活：{exit}"),
            }
        };

        let latency_color = theme.latency_color(slot.latency_ms);
        let latency_label = if slot.is_direct {
            "0ms".to_owned()
        } else {
            slot.latency_ms
                .map_or_else(|| "—".to_owned(), |ms| format!("{ms}ms"))
        };

        div()
            .id(format!("tree-rule-policy-card-{index}"))
            .debug_selector(move || selector)
            .relative()
            .px(px(12.0))
            .py(px(9.0))
            .rounded(px(11.0))
            .bg(card_bg)
            .border_1()
            .border_color(card_border)
            .when(dimmed, |el| el.opacity(0.28))
            .cursor_pointer()
            .flex()
            .flex_col()
            .gap(px(5.0))
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                if *hovered {
                    this.topology.hovered_rules = Some(RulesTreeFocus::Policy(hover_name.clone()));
                    cx.notify();
                } else if matches!(
                    this.topology.hovered_rules.as_ref(),
                    Some(RulesTreeFocus::Policy(current)) if current == &hover_name
                ) {
                    this.topology.hovered_rules = None;
                    cx.notify();
                }
            }))
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                if matches!(
                    this.topology.pinned_rules.as_ref(),
                    Some(RulesTreeFocus::Policy(current)) if current == &click_name
                ) {
                    this.topology.pinned_rules = None;
                } else {
                    this.topology.pinned_rules = Some(RulesTreeFocus::Policy(click_name.clone()));
                }
                cx.notify();
            }))
            .child(port_tracker(registry, format!("rp:{}", slot.name)))
            .child(port_dot(
                PortSide::Left,
                if highlighted {
                    theme.route_trace
                } else {
                    theme.surface_elevated
                },
                if highlighted {
                    theme.route_trace
                } else {
                    theme.outline_strong
                },
                highlighted,
            ))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(6.0))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .text_size(px(12.5))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.text_primary)
                            .child(slot.name.clone()),
                    )
                    .child(status_badge(
                        slot.strategy.clone(),
                        StatusTone::Neutral,
                        theme,
                    )),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(6.0))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .text_size(px(11.0))
                            .text_color(theme.text_tertiary)
                            .child(exit_display),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .font_family(MONO_FAMILY)
                            .text_size(px(11.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(if slot.is_direct {
                                theme.text_tertiary
                            } else {
                                latency_color
                            })
                            .child(latency_label),
                    ),
            )
            .child(port_dot(
                PortSide::Right,
                if highlighted {
                    if neutral_builtin {
                        theme.text_primary
                    } else {
                        theme.status_success
                    }
                } else {
                    theme.surface_elevated
                },
                if highlighted {
                    if neutral_builtin {
                        theme.text_primary
                    } else {
                        theme.status_success
                    }
                } else {
                    theme.outline_strong
                },
                highlighted,
            ))
            .into_any_element()
    }

    #[expect(
        clippy::too_many_arguments,
        clippy::too_many_lines,
        reason = "Single GPUI active exit card builder with region badge and latency"
    )]
    fn rules_tree_exit_card(
        index: usize,
        exit: &ActiveExitSlot,
        highlighted: bool,
        has_focus: bool,
        registry: PortRegistry,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let dimmed = has_focus && !highlighted;
        let neutral_builtin = exit.is_direct || exit.is_reject;
        let (card_bg, card_border) = if highlighted {
            if neutral_builtin {
                (theme.surface_high, theme.outline_strong)
            } else {
                (theme.status_success_soft, theme.status_success)
            }
        } else {
            (theme.surface_elevated, theme.outline_subtle)
        };
        let hover_name = exit.name.clone();
        let click_name = exit.name.clone();
        let selector = format!("tree-rule-exit-{index}");

        let title = if exit.is_direct {
            match language {
                Language::English => "Direct exit".to_owned(),
                Language::SimplifiedChinese => "本地直连出口".to_owned(),
            }
        } else if exit.is_reject {
            match language {
                Language::English => "Reject".to_owned(),
                Language::SimplifiedChinese => "拒绝出口".to_owned(),
            }
        } else {
            Self::policy_candidate_display_name(&exit.name).to_owned()
        };

        let latency_color = theme.latency_color(exit.latency_ms);
        let right_pill = if exit.is_direct || exit.is_reject {
            status_badge(exit.protocol.clone(), StatusTone::Neutral, theme)
        } else {
            div()
                .flex_shrink_0()
                .font_family(MONO_FAMILY)
                .text_size(px(11.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(latency_color)
                .child(
                    exit.latency_ms
                        .map_or_else(|| "—".to_owned(), |ms| format!("{ms}ms")),
                )
        };

        div()
            .id(format!("tree-rule-exit-card-{index}"))
            .debug_selector(move || selector)
            .relative()
            .px(px(12.0))
            .py(px(9.0))
            .rounded(px(11.0))
            .bg(card_bg)
            .border_1()
            .border_color(card_border)
            .when(dimmed, |el| el.opacity(0.28))
            .cursor_pointer()
            .flex()
            .flex_col()
            .gap(px(5.0))
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                if *hovered {
                    this.topology.hovered_rules = Some(RulesTreeFocus::Exit(hover_name.clone()));
                    cx.notify();
                } else if matches!(
                    this.topology.hovered_rules.as_ref(),
                    Some(RulesTreeFocus::Exit(current)) if current == &hover_name
                ) {
                    this.topology.hovered_rules = None;
                    cx.notify();
                }
            }))
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                if matches!(
                    this.topology.pinned_rules.as_ref(),
                    Some(RulesTreeFocus::Exit(current)) if current == &click_name
                ) {
                    this.topology.pinned_rules = None;
                } else {
                    this.topology.pinned_rules = Some(RulesTreeFocus::Exit(click_name.clone()));
                }
                cx.notify();
            }))
            .child(port_tracker(registry, format!("re:{}", exit.name)))
            .child(port_dot(
                PortSide::Left,
                if highlighted {
                    if neutral_builtin {
                        theme.text_primary
                    } else {
                        theme.status_success
                    }
                } else {
                    theme.surface_elevated
                },
                if highlighted {
                    if neutral_builtin {
                        theme.text_primary
                    } else {
                        theme.status_success
                    }
                } else {
                    theme.outline_strong
                },
                highlighted,
            ))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(6.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(7.0))
                            .flex_1()
                            .min_w_0()
                            .child(region_code_badge(exit.region.clone(), theme))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .whitespace_nowrap()
                                    .text_size(px(12.5))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(theme.text_primary)
                                    .child(title),
                            ),
                    )
                    .child(right_pill),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .text_size(px(11.0))
                    .text_color(theme.text_tertiary)
                    .child(div().font_family(MONO_FAMILY).child(exit.protocol.clone()))
                    .child(match language {
                        Language::English => format!("Serves {} groups", exit.serving_count),
                        Language::SimplifiedChinese => {
                            format!("承接 {} 个策略组", exit.serving_count)
                        }
                    }),
            )
            .into_any_element()
    }
}

fn compute_rules_tree_highlights<'a>(
    rule_items: &'a [RuleTreeItem],
    policy_slots: &'a [PolicySlot],
    focus: Option<&RulesTreeFocus>,
) -> (BTreeSet<&'a str>, BTreeSet<&'a str>, BTreeSet<&'a str>) {
    let mut rules = BTreeSet::new();
    let mut policies = BTreeSet::new();
    let mut exits = BTreeSet::new();

    match focus {
        None => {}
        Some(RulesTreeFocus::Rule(rule_id)) => {
            if let Some(item) = rule_items.iter().find(|r| r.id == *rule_id) {
                rules.insert(item.id.as_str());
                if let Some(slot) = policy_slots.iter().find(|p| p.name == item.target_policy) {
                    policies.insert(slot.name.as_str());
                    exits.insert(slot.exit_name.as_str());
                }
            }
        }
        Some(RulesTreeFocus::Policy(policy_name)) => {
            if let Some(slot) = policy_slots.iter().find(|p| p.name == *policy_name) {
                policies.insert(slot.name.as_str());
                exits.insert(slot.exit_name.as_str());
                for item in rule_items {
                    if item.enabled && item.target_policy == slot.name {
                        rules.insert(item.id.as_str());
                    }
                }
            }
        }
        Some(RulesTreeFocus::Exit(exit_name)) => {
            for slot in policy_slots {
                if slot.exit_name == *exit_name {
                    exits.insert(slot.exit_name.as_str());
                    policies.insert(slot.name.as_str());
                }
            }
            for item in rule_items {
                if item.enabled && policies.contains(item.target_policy.as_str()) {
                    rules.insert(item.id.as_str());
                }
            }
        }
    }

    (rules, policies, exits)
}

fn build_rules_tree_edges(
    rule_items: &[RuleTreeItem],
    policy_slots: &[PolicySlot],
    focus: Option<&RulesTreeFocus>,
    theme: Theme,
) -> Vec<BezierEdge> {
    let (hl_rules, hl_policies, hl_exits) =
        compute_rules_tree_highlights(rule_items, policy_slots, focus);
    let has_focus = focus.is_some();
    let mut edges = Vec::new();

    // Stage 1 -> Stage 2: Rule -> Policy Group
    for item in rule_items {
        let highlighted = hl_rules.contains(item.id.as_str())
            && hl_policies.contains(item.target_policy.as_str());
        let (color, width, dashed, layer) = if highlighted {
            (theme.route_trace, 2.5, !item.enabled, 2)
        } else if !item.enabled {
            (with_alpha(theme.outline_strong, 0.14), 1.1, true, 0)
        } else if has_focus {
            (with_alpha(theme.outline_strong, 0.06), 1.0, false, 0)
        } else {
            (with_alpha(theme.route_trace, 0.28), 1.4, false, 1)
        };
        edges.push(BezierEdge {
            from_key: format!("r:{}", item.id),
            to_key: format!("rp:{}", item.target_policy),
            orientation: EdgeOrientation::Horizontal,
            color,
            width,
            dashed,
            layer,
        });
    }

    // Stage 2 -> Stage 3: Policy Group -> Active Exit
    for slot in policy_slots {
        let highlighted =
            hl_policies.contains(slot.name.as_str()) && hl_exits.contains(slot.exit_name.as_str());
        let (color, width, layer) = if highlighted {
            if slot.is_direct || slot.is_reject {
                (theme.text_primary, 2.4, 2)
            } else {
                (theme.status_success, 2.6, 2)
            }
        } else if has_focus {
            (with_alpha(theme.outline_strong, 0.06), 1.0, 0)
        } else {
            (with_alpha(theme.status_success, 0.34), 1.5, 1)
        };
        edges.push(BezierEdge {
            from_key: format!("rp:{}", slot.name),
            to_key: format!("re:{}", slot.exit_name),
            orientation: EdgeOrientation::Horizontal,
            color,
            width,
            dashed: false,
            layer,
        });
    }

    edges
}

fn col_tag(label: &'static str, fg: gpui::Rgba, bg: gpui::Rgba) -> Div {
    div()
        .self_start()
        .px(px(8.0))
        .py(px(2.5))
        .rounded(Radius::Badge.px())
        .bg(bg)
        .text_size(px(11.0))
        .line_height(px(16.0))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(fg)
        .child(label)
}

fn arrow_sep(theme: Theme) -> Div {
    div()
        .text_size(px(11.0))
        .text_color(theme.text_tertiary)
        .child("→")
}

fn format_manual_rule_summary(rule: &ManualRule) -> String {
    match rule.conditions().first() {
        Some(cond) => format!("{} {}", cond.kind().display_label(), cond.parameter()),
        None => "FINAL".to_owned(),
    }
}

fn format_manual_rule_value(rule: &ManualRule) -> String {
    match rule.conditions().first() {
        Some(cond) => cond.parameter().to_owned(),
        None => "FINAL".to_owned(),
    }
}
