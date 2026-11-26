use std::collections::{BTreeMap, BTreeSet};

use gpui::{
    AnyElement, Context, Div, FontWeight, InteractiveElement, IntoElement, ParentElement, Stateful,
    StatefulInteractiveElement, Styled, div, prelude::FluentBuilder as _, px,
};
use gpui_component::{Disableable as _, Icon, IconName, Sizable as _, switch::Switch};
use manis_core::WindowSizeClass;
use manis_profile::QxRuleList;

use super::{
    BezierEdge, EdgeOrientation, LayoutRegistry, PortSide, RulesTreeFocus,
    build_column_auto_scroll_track, card_port_tracker, col_tag, compact_protocol_label,
    legend_entry, new_layout_registry, paint_edges_canvas, port_dot, scrollable_tree_column,
    with_alpha,
};
use crate::{
    app::{ManisApp, RuleRowScope, nodes::region_code, presentation::policy_kind_label},
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
    ManualRule(usize),
    Remote(String),
    Local(usize),
}

#[derive(Clone)]
enum RuleRetargetAction {
    ManualRule(usize),
    RemoteSource(String),
    LocalGroup(usize),
    FinalFallback,
}

impl RuleRetargetAction {
    fn accepts_policy(
        &self,
        policy_name: &str,
        global_label: &str,
        managed_policy_names: &BTreeSet<String>,
    ) -> bool {
        match self {
            Self::ManualRule(_) | Self::LocalGroup(_) => {
                policy_name == DIRECT_TARGET
                    || policy_name == REJECT_TARGET
                    || managed_policy_names.contains(policy_name)
            }
            Self::RemoteSource(_) => {
                policy_name == DIRECT_TARGET || managed_policy_names.contains(policy_name)
            }
            Self::FinalFallback => {
                policy_name == global_label
                    || policy_name == DIRECT_TARGET
                    || policy_name == REJECT_TARGET
                    || managed_policy_names.contains(policy_name)
            }
        }
    }
}

struct RuleTreeItem {
    id: String,
    order: String,
    title: String,
    subtitle: String,
    samples: Vec<String>,
    target_policy: String,
    enabled: bool,
    is_manual_child: bool,
    group_order_id: Option<String>,
    group_position: Option<(usize, usize)>,
    toggle: Option<RuleToggleAction>,
    retarget: Option<RuleRetargetAction>,
}

enum RuleColumnEntry {
    ManualGroup {
        order: String,
        group_position: (usize, usize),
        children: Vec<RuleTreeItem>,
    },
    Single(RuleTreeItem),
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

struct RulesTreeData {
    entries: Vec<RuleColumnEntry>,
    policy_slots: Vec<PolicySlot>,
    exit_slots: Vec<ActiveExitSlot>,
    managed_policy_names: BTreeSet<String>,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum RuleCardEmphasis {
    SuppressedByManual,
    Pinned,
    Highlighted,
    Dimmed,
    Normal,
}

enum PolicyPinRole {
    None,
    CurrentTarget,
    SwitchCandidate {
        action: RuleRetargetAction,
        hovered: bool,
    },
}

struct PolicyCardViewState {
    highlighted: bool,
    has_focus: bool,
    pin_role: PolicyPinRole,
}

fn active_rules_in_mode(entries: &[RuleColumnEntry], manual_expanded: bool) -> Vec<&RuleTreeItem> {
    let mut items = Vec::new();
    for entry in entries {
        match entry {
            RuleColumnEntry::ManualGroup { children, .. } if manual_expanded => {
                items.extend(children.iter());
            }
            RuleColumnEntry::Single(item) if !manual_expanded => {
                items.push(item);
            }
            RuleColumnEntry::ManualGroup { .. } | RuleColumnEntry::Single(_) => {}
        }
    }
    items
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
        let manual_expanded =
            self.topology.rules_manual_group_expanded && self.has_regular_manual_rules();
        let pinned_rule_id = match self.topology.pinned_rules.as_ref() {
            Some(RulesTreeFocus::Rule(id)) => Some(id.as_str()),
            _ => None,
        };
        let hovered_policy_name = match self.topology.hovered_rules.as_ref() {
            Some(RulesTreeFocus::Policy(name)) => Some(name.as_str()),
            _ => None,
        };
        let data = self.collect_rules_tree_data(language, pinned_rule_id, manual_expanded);
        let active_rules = active_rules_in_mode(&data.entries, manual_expanded);
        let pinned_rule =
            pinned_rule_id.and_then(|id| active_rules.iter().copied().find(|item| item.id == id));

        let focus = self.topology.active_rules_focus().cloned();
        let inspector = Self::rules_tree_inspector(
            &active_rules,
            &data.policy_slots,
            &data.exit_slots,
            focus.as_ref(),
            pinned_rule,
            manual_expanded,
            language,
            theme,
            cx,
        );

        if data.entries.is_empty() {
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

        let global_label = language.localized(copy::configuration::GLOBAL_EXIT);
        let preview_switch_target = pinned_rule.and_then(|rule| {
            let candidate = hovered_policy_name?;
            if candidate == rule.target_policy {
                return None;
            }
            let action = rule.retarget.as_ref()?;
            action
                .accepts_policy(candidate, global_label, &data.managed_policy_names)
                .then_some((rule.id.as_str(), candidate))
        });

        let registry = new_layout_registry();
        self.topology.replace_rules_layout(registry.clone());
        let edges = build_rules_tree_edges(
            &active_rules,
            &data.policy_slots,
            focus.as_ref(),
            preview_switch_target,
            manual_expanded,
            theme,
        );

        let (hl_rules, hl_policies, hl_exits) =
            compute_rules_tree_highlights(&active_rules, &data.policy_slots, focus.as_ref());
        let has_focus = focus.is_some();

        let col1_cards = data
            .entries
            .iter()
            .enumerate()
            .map(|(index, entry)| match entry {
                RuleColumnEntry::ManualGroup {
                    order,
                    group_position,
                    children,
                } => self.rules_tree_manual_group_card(
                    index,
                    order,
                    *group_position,
                    children,
                    manual_expanded,
                    pinned_rule_id,
                    &hl_rules,
                    has_focus,
                    &registry,
                    language,
                    theme,
                    cx,
                ),
                RuleColumnEntry::Single(item) => {
                    let highlighted = hl_rules.contains(item.id.as_str());
                    let is_pinned = pinned_rule_id == Some(item.id.as_str());
                    let emphasis = if manual_expanded && !item.is_manual_child {
                        RuleCardEmphasis::SuppressedByManual
                    } else if is_pinned {
                        RuleCardEmphasis::Pinned
                    } else if highlighted {
                        RuleCardEmphasis::Highlighted
                    } else if has_focus {
                        RuleCardEmphasis::Dimmed
                    } else {
                        RuleCardEmphasis::Normal
                    };
                    self.rules_tree_rule_card(
                        index,
                        item,
                        emphasis,
                        registry.clone(),
                        language,
                        theme,
                        cx,
                    )
                }
            })
            .collect::<Vec<_>>();

        let col2_cards = data
            .policy_slots
            .iter()
            .enumerate()
            .map(|(index, slot)| {
                let pin_role = if pinned_rule.is_some_and(|rule| rule.target_policy == slot.name) {
                    PolicyPinRole::CurrentTarget
                } else if let Some(action) = pinned_rule.and_then(|rule| {
                    let action = rule.retarget.as_ref()?;
                    action
                        .accepts_policy(&slot.name, global_label, &data.managed_policy_names)
                        .then(|| action.clone())
                }) {
                    PolicyPinRole::SwitchCandidate {
                        action,
                        hovered: hovered_policy_name == Some(slot.name.as_str()),
                    }
                } else {
                    PolicyPinRole::None
                };
                Self::rules_tree_policy_card(
                    index,
                    slot,
                    PolicyCardViewState {
                        highlighted: hl_policies.contains(slot.name.as_str()),
                        has_focus,
                        pin_role,
                    },
                    global_label,
                    registry.clone(),
                    language,
                    theme,
                    cx,
                )
            })
            .collect::<Vec<_>>();

        let col3_cards = data
            .exit_slots
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

        let bay_width = if compact { px(28.0) } else { px(44.0) };
        let col1_header = col_tag(
            match language {
                Language::English => format!("01 · ROUTING RULES ({})", data.entries.len()),
                Language::SimplifiedChinese => {
                    format!("01 · 分流规则（{}）", data.entries.len())
                }
            },
            theme.route_trace,
            theme.route_soft,
        )
        .into_any_element();
        let col2_header = col_tag(
            match language {
                Language::English => format!("02 · POLICY GROUPS ({})", data.policy_slots.len()),
                Language::SimplifiedChinese => {
                    format!("02 · 策略组（{}）", data.policy_slots.len())
                }
            },
            theme.action_primary,
            theme.action_soft,
        )
        .into_any_element();
        let col3_header = col_tag(
            match language {
                Language::English => format!("03 · ACTIVE EXITS ({})", data.exit_slots.len()),
                Language::SimplifiedChinese => {
                    format!("03 · 当前激活出口（{}）", data.exit_slots.len())
                }
            },
            theme.status_success,
            theme.status_success_soft,
        )
        .into_any_element();

        let flow_inner = div()
            .relative()
            .size_full()
            .min_h_0()
            .flex()
            .items_stretch()
            .child(paint_edges_canvas(registry.clone(), edges))
            // Column 1: Rules (independently scrollable)
            .child(scrollable_tree_column(
                "rules-tree-col-rules-scroll",
                "rules-tree-col-rules-scroll".to_owned(),
                "col:rules".to_owned(),
                col1_header,
                col1_cards,
                px(5.0),
                &self.topology.rules_col_scrolls[0],
                registry.clone(),
                None,
                true,
                theme,
            ))
            // Unobstructed Bézier bay 1 -> 2
            .child(div().w(bay_width).flex_shrink_0())
            // Column 2: Policy Groups (independently scrollable)
            .child(scrollable_tree_column(
                "rules-tree-col-policies-scroll",
                "rules-tree-col-policies-scroll".to_owned(),
                "col:policies".to_owned(),
                col2_header,
                col2_cards,
                px(5.0),
                &self.topology.rules_col_scrolls[1],
                registry.clone(),
                None,
                true,
                theme,
            ))
            // Unobstructed Bézier bay 2 -> 3
            .child(div().w(bay_width).flex_shrink_0())
            // Column 3: Active Exit Nodes (independently scrollable)
            .child(scrollable_tree_column(
                "rules-tree-col-exits-scroll",
                "rules-tree-col-exits-scroll".to_owned(),
                "col:exits".to_owned(),
                col3_header,
                col3_cards,
                px(5.0),
                &self.topology.rules_col_scrolls[2],
                registry,
                None,
                true,
                theme,
            ));

        let canvas_card = card_surface(theme)
            .id("rules-tree-canvas")
            .debug_selector(|| "rules-tree-canvas".to_owned())
            .flex_1()
            .min_h_0()
            .w_full()
            .overflow_hidden()
            .p(px(14.0))
            .flex()
            .flex_col()
            .on_click(cx.listener(|this, _, _, cx| {
                this.topology.cancel_auto_scroll();
                if this.topology.pinned_rules.take().is_some() {
                    cx.notify();
                }
            }))
            .child(flow_inner);

        let horizontal_pad = if compact {
            Space::Md.px()
        } else {
            Space::Xl.px()
        };
        div()
            .id("rules-tree-workspace")
            .flex_1()
            .min_h_0()
            .w_full()
            .flex()
            .flex_col()
            .pt(Space::Lg.px())
            .gap(Space::Md.px())
            .child(div().flex_shrink_0().px(horizontal_pad).child(inspector))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .px(horizontal_pad)
                    .pb(Space::Lg.px())
                    .flex()
                    .flex_col()
                    .child(canvas_card),
            )
    }

    fn start_rules_tree_auto_scroll(&mut self, focus: &RulesTreeFocus, cx: &mut Context<Self>) {
        let layout_registry = self.topology.rules_layout();
        let layout = layout_registry.borrow();
        let language = self.language();
        let manual_expanded =
            self.topology.rules_manual_group_expanded && self.has_regular_manual_rules();
        let pinned_rule_id = match focus {
            RulesTreeFocus::Rule(id) => Some(id.as_str()),
            _ => None,
        };
        let data = self.collect_rules_tree_data(language, pinned_rule_id, manual_expanded);
        let active_rules = active_rules_in_mode(&data.entries, manual_expanded);
        let (hl_rules, hl_policies, hl_exits) =
            compute_rules_tree_highlights(&active_rules, &data.policy_slots, Some(focus));

        let mut tracks = Vec::new();
        if !matches!(focus, RulesTreeFocus::Rule(_))
            && let Some(viewport) = layout.viewport("col:rules")
        {
            let active_bounds: Vec<_> = hl_rules
                .iter()
                .filter_map(|id| layout.port_bounds(&format!("r:{id}")))
                .collect();
            if let Some(track) = build_column_auto_scroll_track(
                self.topology.rules_col_scrolls[0].clone(),
                viewport,
                &active_bounds,
            ) {
                tracks.push(track);
            }
        }
        if !matches!(focus, RulesTreeFocus::Policy(_))
            && let Some(viewport) = layout.viewport("col:policies")
        {
            let active_bounds: Vec<_> = hl_policies
                .iter()
                .filter_map(|name| layout.port_bounds(&format!("rp:{name}")))
                .collect();
            if let Some(track) = build_column_auto_scroll_track(
                self.topology.rules_col_scrolls[1].clone(),
                viewport,
                &active_bounds,
            ) {
                tracks.push(track);
            }
        }
        if !matches!(focus, RulesTreeFocus::Exit(_))
            && let Some(viewport) = layout.viewport("col:exits")
        {
            let active_bounds: Vec<_> = hl_exits
                .iter()
                .filter_map(|name| layout.port_bounds(&format!("re:{name}")))
                .collect();
            if let Some(track) = build_column_auto_scroll_track(
                self.topology.rules_col_scrolls[2].clone(),
                viewport,
                &active_bounds,
            ) {
                tracks.push(track);
            }
        }
        drop(layout);

        self.start_topology_auto_scroll(tracks, cx);
    }

    #[allow(clippy::too_many_lines)] // Gathers live rules, target policy groups, and deduplicated exits.
    fn collect_rules_tree_data(
        &self,
        language: Language,
        pinned_rule_id: Option<&str>,
        manual_expanded: bool,
    ) -> RulesTreeData {
        // 1. Gather policy group active exits from live catalog or offline managed groups.
        let mut policy_lookup: BTreeMap<String, (String, String, Option<u64>, String)> =
            BTreeMap::new();
        let mut policy_order: Vec<String> = Vec::new();
        let managed_policy_names: BTreeSet<String> = self
            .managed_policies
            .groups
            .iter()
            .map(|group| group.name.clone())
            .collect();

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

        // 2. Build ordered rule groups from normalized routing rule group order.
        let group_order = mihomo::normalized_routing_rule_group_order(
            &self.rule_sources.group_order,
            self.has_regular_manual_rules(),
            &self.rule_sources.sources,
            &self.local_rule_groups,
        );
        let group_count = group_order.len();

        let mut entries = Vec::new();
        for (position, group_id) in group_order.iter().enumerate() {
            let order_label = format!("{:02}", position + 1);
            if group_id == mihomo::MANUAL_ROUTING_RULE_GROUP_ID {
                let children = self
                    .manual_rules
                    .iter()
                    .enumerate()
                    .filter(|(_, rule)| !rule.is_final())
                    .enumerate()
                    .map(|(child_idx, (rule_index, rule))| {
                        let target = self.effective_rule_target(rule.target(), language);
                        RuleTreeItem {
                            id: format!("manual:{rule_index}"),
                            order: format!("#{}", child_idx + 1),
                            title: format_manual_rule_summary(rule),
                            subtitle: format!("→ {target}"),
                            samples: Vec::new(),
                            target_policy: target,
                            enabled: rule.is_enabled(),
                            is_manual_child: true,
                            group_order_id: None,
                            group_position: None,
                            toggle: Some(RuleToggleAction::ManualRule(rule_index)),
                            retarget: Some(RuleRetargetAction::ManualRule(rule_index)),
                        }
                    })
                    .collect::<Vec<_>>();
                if !children.is_empty() {
                    entries.push(RuleColumnEntry::ManualGroup {
                        order: order_label,
                        group_position: (position, group_count),
                        children,
                    });
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
                entries.push(RuleColumnEntry::Single(RuleTreeItem {
                    id: format!("remote:{}", source.id),
                    order: order_label,
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
                    is_manual_child: false,
                    group_order_id: Some(source.id.clone()),
                    group_position: Some((position, group_count)),
                    toggle: Some(RuleToggleAction::Remote(source.id.clone())),
                    retarget: Some(RuleRetargetAction::RemoteSource(source.id.clone())),
                }));
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
                entries.push(RuleColumnEntry::Single(RuleTreeItem {
                    id: format!("local:{}", group.id),
                    order: order_label,
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
                    is_manual_child: false,
                    group_order_id: Some(group.id.clone()),
                    group_position: Some((position, group_count)),
                    toggle: Some(RuleToggleAction::Local(group_index)),
                    retarget: Some(RuleRetargetAction::LocalGroup(group_index)),
                }));
            }
        }

        if !entries.is_empty() || self.final_fallback_target_raw().is_some() {
            let fallback_target = self.final_fallback_target_label(language);
            entries.push(RuleColumnEntry::Single(RuleTreeItem {
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
                is_manual_child: false,
                group_order_id: None,
                group_position: None,
                toggle: None,
                retarget: Some(RuleRetargetAction::FinalFallback),
            }));
        }

        // 3. Build Column 2 policy slots (all known policies + targets referenced by active rules
        //    + valid switch targets when a rule is pinned).
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

        let active_rules = active_rules_in_mode(&entries, manual_expanded);
        let mut extra_targets: Vec<String> = active_rules
            .iter()
            .map(|item| item.target_policy.clone())
            .collect();
        if let Some(pinned_id) = pinned_rule_id
            && let Some(pinned) = active_rules.iter().find(|item| item.id == pinned_id)
            && let Some(retarget) = pinned.retarget.as_ref()
        {
            match retarget {
                RuleRetargetAction::ManualRule(_) | RuleRetargetAction::LocalGroup(_) => {
                    extra_targets.push(DIRECT_TARGET.to_owned());
                    extra_targets.push(REJECT_TARGET.to_owned());
                }
                RuleRetargetAction::RemoteSource(_) => {
                    extra_targets.push(DIRECT_TARGET.to_owned());
                }
                RuleRetargetAction::FinalFallback => {
                    extra_targets.push(
                        language
                            .localized(copy::configuration::GLOBAL_EXIT)
                            .to_owned(),
                    );
                    extra_targets.push(DIRECT_TARGET.to_owned());
                    extra_targets.push(REJECT_TARGET.to_owned());
                }
            }
        }

        // Add any extra targets referenced by active rules or selectable by the pinned rule.
        for target_name in extra_targets {
            if seen_policies.insert(target_name.clone()) {
                let is_direct = target_name.eq_ignore_ascii_case(DIRECT_TARGET);
                let is_reject = target_name.eq_ignore_ascii_case(REJECT_TARGET);
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
                    let live_global = self
                        .policy_groups()
                        .find(|group| group.name == manis_profile::MANIS_GLOBAL_GROUP_NAME)
                        .and_then(|group| self.selected_node_for_policy(group))
                        .map(|node| node.name.clone());
                    let global_exit = live_global
                        .or_else(|| {
                            self.managed_policies
                                .node_selections
                                .global()
                                .map(|id| id.node_name.clone())
                        })
                        .unwrap_or_else(|| DIRECT_TARGET.to_owned());
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
                    name: target_name,
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

        RulesTreeData {
            entries,
            policy_slots,
            exit_slots,
            managed_policy_names,
        }
    }

    #[expect(
        clippy::too_many_arguments,
        clippy::too_many_lines,
        reason = "Builds the single cohesive route-chain inspector bar for all focus states"
    )]
    fn rules_tree_inspector(
        rule_items: &[&RuleTreeItem],
        policy_slots: &[PolicySlot],
        exit_slots: &[ActiveExitSlot],
        focus: Option<&RulesTreeFocus>,
        pinned_rule: Option<&RuleTreeItem>,
        manual_expanded: bool,
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
                if manual_expanded {
                    summary = summary.child(status_badge(
                        match language {
                            Language::English => {
                                "Manual rules expanded · showing manual wires only"
                            }
                            Language::SimplifiedChinese => "手动规则已展开 · 仅展示手动规则连线",
                        },
                        StatusTone::Neutral,
                        theme,
                    ));
                }
            }
            Some(RulesTreeFocus::Rule(rule_id)) => {
                if let Some(item) = rule_items.iter().copied().find(|r| r.id == *rule_id) {
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
                        })
                        .when(
                            pinned_rule.is_some_and(|pinned| pinned.id == item.id),
                            |row| {
                                row.child(status_badge(
                                    match language {
                                        Language::English => {
                                            "Pinned · Click a policy group to switch target"
                                        }
                                        Language::SimplifiedChinese => {
                                            "已锁定 · 点击右侧策略组切换目标"
                                        }
                                    },
                                    StatusTone::Route,
                                    theme,
                                ))
                            },
                        );
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

    fn rules_tree_reorder_controls(
        &self,
        group_id: &str,
        position: usize,
        total: usize,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if total <= 1 {
            return None;
        }
        let busy = self.routing_apply_state.is_busy() || self.configuration_transfer.active;
        let can_up = !busy && position > 0;
        let can_down = !busy && position + 1 < total;
        let up_group_id = group_id.to_owned();
        let down_group_id = group_id.to_owned();
        let up_selector = format!("tree-rule-move-up-{group_id}");
        let down_selector = format!("tree-rule-move-down-{group_id}");

        let make_btn = |id: String,
                        selector: String,
                        icon: IconName,
                        enabled: bool,
                        target_group_id: String,
                        direction: mihomo::MoveDirection,
                        cx: &mut Context<Self>| {
            div()
                .id(id)
                .debug_selector(move || selector)
                .size(px(20.0))
                .rounded(px(5.0))
                .bg(theme.surface_low)
                .border_1()
                .border_color(theme.outline_subtle)
                .flex()
                .items_center()
                .justify_center()
                .when(enabled, |el| {
                    el.cursor_pointer()
                        .hover(move |style| {
                            style
                                .bg(theme.surface_high)
                                .border_color(theme.outline_strong)
                        })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.topology.cancel_auto_scroll();
                            this.move_routing_rule_group_step(&target_group_id, direction, cx);
                        }))
                })
                .when(!enabled, |el| {
                    el.opacity(0.32).on_click(|_, _, cx| cx.stop_propagation())
                })
                .child(
                    Icon::new(icon)
                        .size(px(11.0))
                        .text_color(theme.text_secondary),
                )
        };

        Some(
            div()
                .flex_shrink_0()
                .flex()
                .items_center()
                .gap(px(2.0))
                .child(make_btn(
                    format!("tree-rule-up-{group_id}"),
                    up_selector,
                    IconName::ArrowUp,
                    can_up,
                    up_group_id,
                    mihomo::MoveDirection::Up,
                    cx,
                ))
                .child(make_btn(
                    format!("tree-rule-down-{group_id}"),
                    down_selector,
                    IconName::ArrowDown,
                    can_down,
                    down_group_id,
                    mihomo::MoveDirection::Down,
                    cx,
                ))
                .into_any_element(),
        )
    }

    #[expect(
        clippy::too_many_arguments,
        clippy::too_many_lines,
        reason = "Renders the visually distinct collapsible Manual Rules group container and its indented child cards"
    )]
    fn rules_tree_manual_group_card(
        &self,
        index: usize,
        order: &str,
        group_position: (usize, usize),
        children: &[RuleTreeItem],
        expanded: bool,
        pinned_rule_id: Option<&str>,
        hl_rules: &BTreeSet<&str>,
        has_focus: bool,
        registry: &LayoutRegistry,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let enabled_count = children.iter().filter(|rule| rule.enabled).count();
        let selector = format!("tree-rule-{index}");
        let (pos, total) = group_position;
        let reorder_controls = self.rules_tree_reorder_controls(
            mihomo::MANUAL_ROUTING_RULE_GROUP_ID,
            pos,
            total,
            theme,
            cx,
        );
        let subtitle = if expanded {
            match language {
                Language::English => {
                    format!("{} rules · showing manual wires only", children.len())
                }
                Language::SimplifiedChinese => {
                    format!("{} 条独立规则 · 仅展示本组连线", children.len())
                }
            }
        } else {
            match language {
                Language::English => format!(
                    "{} rules ({} active) · wires hidden",
                    children.len(),
                    enabled_count
                ),
                Language::SimplifiedChinese => {
                    format!(
                        "{} 条规则（{} 生效）· 折叠不连线",
                        children.len(),
                        enabled_count
                    )
                }
            }
        };
        let toggle_label = if expanded {
            match language {
                Language::English => "Collapse",
                Language::SimplifiedChinese => "收起",
            }
        } else {
            match language {
                Language::English => "Expand",
                Language::SimplifiedChinese => "展开",
            }
        };

        let header = div()
            .id("tree-rule-manual-header")
            .debug_selector(|| "tree-rule-manual-toggle".to_owned())
            .px(px(10.0))
            .py(px(7.5))
            .cursor_pointer()
            .flex()
            .flex_col()
            .gap(px(4.0))
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                this.topology.cancel_auto_scroll();
                let next_expanded = !this.topology.rules_manual_group_expanded;
                this.topology.rules_manual_group_expanded = next_expanded;
                this.topology.hovered_rules = None;
                // Clear any pin belonging to the opposite visibility mode.
                if let Some(RulesTreeFocus::Rule(id)) = this.topology.pinned_rules.as_ref() {
                    let is_manual_pin = id.starts_with("manual:");
                    if is_manual_pin != next_expanded {
                        this.topology.pinned_rules = None;
                    }
                }
                cx.notify();
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(7.0))
                    .child(
                        div()
                            .min_w(px(30.0))
                            .px(px(5.0))
                            .py(px(1.5))
                            .rounded(Radius::Badge.px())
                            .bg(theme.action_soft)
                            .border_1()
                            .border_color(with_alpha(theme.action_primary, 0.35))
                            .font_family(MONO_FAMILY)
                            .text_size(px(10.5))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.action_primary)
                            .text_center()
                            .child(order.to_owned()),
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
                            .child(language.localized(copy::common::MANUAL_RULES)),
                    )
                    .when_some(reorder_controls, ParentElement::child)
                    .child(
                        div()
                            .flex_shrink_0()
                            .px(px(7.0))
                            .py(px(2.0))
                            .rounded_full()
                            .bg(if expanded {
                                theme.action_primary
                            } else {
                                theme.surface_elevated
                            })
                            .border_1()
                            .border_color(if expanded {
                                theme.action_primary
                            } else {
                                theme.outline_strong
                            })
                            .flex()
                            .items_center()
                            .gap(px(3.0))
                            .text_size(px(10.5))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(if expanded {
                                theme.action_on_primary
                            } else {
                                theme.text_secondary
                            })
                            .child(toggle_label)
                            .child(
                                Icon::new(if expanded {
                                    IconName::ChevronDown
                                } else {
                                    IconName::ChevronRight
                                })
                                .size(px(11.0))
                                .text_color(if expanded {
                                    theme.action_on_primary
                                } else {
                                    theme.text_secondary
                                }),
                            ),
                    ),
            )
            .child(
                div()
                    .pl(px(37.0))
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .flex_wrap()
                    .child(
                        div()
                            .px(px(5.0))
                            .py(px(1.0))
                            .rounded(px(4.0))
                            .bg(theme.action_soft)
                            .text_size(px(10.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.action_primary)
                            .child(match language {
                                Language::English => "Per-rule targets",
                                Language::SimplifiedChinese => "独立分流组",
                            }),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(theme.text_tertiary)
                            .child(subtitle),
                    ),
            );

        let mut container = div()
            .id(format!("tree-rule-manual-group-{index}"))
            .debug_selector(move || selector)
            .relative()
            .rounded(px(11.0))
            .bg(if expanded {
                theme.surface_low
            } else {
                theme.surface_chrome
            })
            .border_1()
            .border_color(if expanded {
                with_alpha(theme.action_primary, 0.55)
            } else {
                theme.outline_strong
            })
            .border_l_4()
            .when(!expanded && has_focus, |el| el.opacity(0.45))
            .flex()
            .flex_col()
            .child(header);

        if expanded {
            let child_cards = children
                .iter()
                .enumerate()
                .map(|(child_idx, item)| {
                    self.rules_tree_manual_child_card(
                        child_idx,
                        item,
                        hl_rules.contains(item.id.as_str()),
                        pinned_rule_id == Some(item.id.as_str()),
                        has_focus,
                        registry.clone(),
                        language,
                        theme,
                        cx,
                    )
                })
                .collect::<Vec<_>>();
            container = container.child(
                div()
                    .mx(px(8.0))
                    .mb(px(8.0))
                    .pl(px(10.0))
                    .pt(px(4.0))
                    .border_l_2()
                    .border_color(with_alpha(theme.action_primary, 0.42))
                    .flex()
                    .flex_col()
                    .gap(px(5.0))
                    .children(child_cards),
            );
        }

        container.into_any_element()
    }

    #[expect(
        clippy::too_many_arguments,
        clippy::too_many_lines,
        reason = "Indented compact sub-card for each individual manual rule inside the expanded Manual Rules group"
    )]
    fn rules_tree_manual_child_card(
        &self,
        child_index: usize,
        item: &RuleTreeItem,
        highlighted: bool,
        is_pinned: bool,
        has_focus: bool,
        registry: LayoutRegistry,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let dimmed = has_focus && !highlighted;
        let card_bg = if highlighted {
            theme.route_soft
        } else {
            theme.surface_elevated
        };
        let card_border = if is_pinned || highlighted {
            theme.route_trace
        } else {
            theme.outline_subtle
        };
        let hover_id = item.id.clone();
        let click_id = item.id.clone();
        let selector = format!("tree-manual-rule-{child_index}");

        let switch_el: Option<AnyElement> = match &item.toggle {
            Some(RuleToggleAction::ManualRule(rule_index)) => {
                let idx = *rule_index;
                let enabled = item.enabled;
                let interactive =
                    !self.routing_apply_state.is_busy() && !self.configuration_transfer.active;
                Some(
                    Switch::new(format!("tree-manual-rule-switch-{child_index}"))
                        .checked(enabled)
                        .disabled(!interactive)
                        .small()
                        .on_click(cx.listener(move |this, checked: &bool, _, cx| {
                            cx.stop_propagation();
                            this.set_rule_enabled(RuleRowScope::Manual, idx, *checked, cx);
                        }))
                        .into_any_element(),
                )
            }
            _ => None,
        };

        div()
            .id(format!("tree-manual-rule-card-{child_index}"))
            .debug_selector(move || selector)
            .relative()
            .px(px(9.0))
            .py(px(5.5))
            .rounded(px(8.0))
            .bg(card_bg)
            .border_1()
            .border_color(card_border)
            .when(dimmed, |el| el.opacity(0.28))
            .when(!item.enabled && !dimmed, |el| el.opacity(0.55))
            .cursor_pointer()
            .flex()
            .flex_col()
            .gap(px(2.5))
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
                    this.topology.cancel_auto_scroll();
                } else {
                    let focus = RulesTreeFocus::Rule(click_id.clone());
                    this.topology.pinned_rules = Some(focus.clone());
                    this.start_rules_tree_auto_scroll(&focus, cx);
                }
                cx.notify();
            }))
            .child(card_port_tracker(
                registry,
                format!("r:{}", item.id),
                "col:rules".to_owned(),
            ))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(
                        div()
                            .px(px(4.5))
                            .py(px(1.0))
                            .rounded(px(4.0))
                            .bg(theme.surface_low)
                            .font_family(MONO_FAMILY)
                            .text_size(px(10.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.text_tertiary)
                            .child(item.order.clone()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .font_family(MONO_FAMILY)
                            .text_size(px(11.5))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.text_primary)
                            .child(item.title.clone()),
                    )
                    .when(is_pinned, |row| {
                        row.child(
                            div()
                                .flex_shrink_0()
                                .px(px(5.0))
                                .py(px(1.0))
                                .rounded(Radius::Badge.px())
                                .bg(theme.route_trace)
                                .text_size(px(9.5))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(theme.action_on_primary)
                                .child(match language {
                                    Language::English => "Pinned",
                                    Language::SimplifiedChinese => "已锁定",
                                }),
                        )
                    })
                    .when_some(switch_el, ParentElement::child),
            )
            .child(
                div()
                    .pl(px(26.0))
                    .text_size(px(10.5))
                    .text_color(theme.text_tertiary)
                    .child(item.subtitle.clone()),
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
        reason = "Single GPUI rule card builder with reorder arrows, toggle switch, sample chips, and port dot"
    )]
    fn rules_tree_rule_card(
        &self,
        index: usize,
        item: &RuleTreeItem,
        emphasis: RuleCardEmphasis,
        registry: LayoutRegistry,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let suppressed_by_manual = emphasis == RuleCardEmphasis::SuppressedByManual;
        let is_pinned = emphasis == RuleCardEmphasis::Pinned;
        let highlighted = matches!(
            emphasis,
            RuleCardEmphasis::Pinned | RuleCardEmphasis::Highlighted
        );
        let dimmed = matches!(
            emphasis,
            RuleCardEmphasis::SuppressedByManual | RuleCardEmphasis::Dimmed
        );
        let card_bg = if !suppressed_by_manual && highlighted {
            theme.route_soft
        } else {
            theme.surface_elevated
        };
        let card_border = if !suppressed_by_manual && (is_pinned || highlighted) {
            theme.route_trace
        } else {
            theme.outline_subtle
        };
        let hover_id = item.id.clone();
        let click_id = item.id.clone();
        let selector = format!("tree-rule-{index}");

        let reorder_controls = item
            .group_order_id
            .as_deref()
            .zip(item.group_position)
            .and_then(|(group_id, (pos, total))| {
                self.rules_tree_reorder_controls(group_id, pos, total, theme, cx)
            });

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
            None if item.id == "final" => Some(self.final_fallback_target_select(
                "tree-rule-fallback-target",
                true,
                language,
                theme,
                cx,
            )),
            Some(RuleToggleAction::ManualRule(_)) | None => None,
        };

        div()
            .id(format!("tree-rule-card-{index}"))
            .debug_selector(move || selector)
            .relative()
            .px(px(10.0))
            .py(px(6.5))
            .rounded(px(10.0))
            .bg(card_bg)
            .border_1()
            .border_color(card_border)
            .when(dimmed, |el| {
                el.opacity(if suppressed_by_manual { 0.44 } else { 0.28 })
            })
            .when(!item.enabled && !dimmed, |el| el.opacity(0.58))
            .cursor_pointer()
            .flex()
            .flex_col()
            .gap(px(3.0))
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                if this.topology.rules_manual_group_expanded {
                    return;
                }
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
                // Clicking another top-level rule group while Manual Rules is expanded
                // collapses Manual Rules so this group's wire becomes active immediately.
                if this.topology.rules_manual_group_expanded {
                    this.topology.rules_manual_group_expanded = false;
                    let focus = RulesTreeFocus::Rule(click_id.clone());
                    this.topology.pinned_rules = Some(focus.clone());
                    this.start_rules_tree_auto_scroll(&focus, cx);
                    cx.notify();
                    return;
                }
                if matches!(
                    this.topology.pinned_rules.as_ref(),
                    Some(RulesTreeFocus::Rule(current)) if current == &click_id
                ) {
                    this.topology.pinned_rules = None;
                    this.topology.cancel_auto_scroll();
                } else {
                    let focus = RulesTreeFocus::Rule(click_id.clone());
                    this.topology.pinned_rules = Some(focus.clone());
                    this.start_rules_tree_auto_scroll(&focus, cx);
                }
                cx.notify();
            }))
            .child(card_port_tracker(
                registry,
                format!("r:{}", item.id),
                "col:rules".to_owned(),
            ))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
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
                    .when(is_pinned && !suppressed_by_manual, |row| {
                        row.child(
                            div()
                                .flex_shrink_0()
                                .px(px(5.0))
                                .py(px(1.0))
                                .rounded(Radius::Badge.px())
                                .bg(theme.route_trace)
                                .text_size(px(9.5))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(theme.action_on_primary)
                                .child(match language {
                                    Language::English => "Pinned",
                                    Language::SimplifiedChinese => "已锁定",
                                }),
                        )
                    })
                    .when_some(reorder_controls, ParentElement::child)
                    .when_some(switch_el, ParentElement::child),
            )
            .child(
                div()
                    .pl(px(36.0))
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
            .when(!suppressed_by_manual, |card| {
                card.child(port_dot(
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
            })
            .into_any_element()
    }

    #[expect(
        clippy::too_many_arguments,
        clippy::too_many_lines,
        reason = "Single GPUI policy slot card builder with dual anchor ports and pin-to-retarget support"
    )]
    fn rules_tree_policy_card(
        index: usize,
        slot: &PolicySlot,
        state: PolicyCardViewState,
        global_label: &str,
        registry: LayoutRegistry,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let PolicyCardViewState {
            highlighted,
            has_focus,
            pin_role,
        } = state;
        let is_current_pinned_target = matches!(pin_role, PolicyPinRole::CurrentTarget);
        let (switch_action, is_hovered_switch) = match pin_role {
            PolicyPinRole::SwitchCandidate { action, hovered } => (Some(action), hovered),
            PolicyPinRole::None | PolicyPinRole::CurrentTarget => (None, false),
        };
        let can_switch = switch_action.is_some();
        let dimmed = has_focus && !highlighted && !can_switch;
        let neutral_builtin = slot.is_direct || slot.is_reject;
        let (card_bg, card_border) = if is_hovered_switch {
            (theme.route_soft, theme.route_trace)
        } else if highlighted {
            if neutral_builtin {
                (theme.surface_high, theme.outline_strong)
            } else {
                (theme.action_soft, theme.action_primary)
            }
        } else if can_switch {
            (theme.surface_elevated, theme.outline_strong)
        } else {
            (theme.surface_elevated, theme.outline_subtle)
        };
        let hover_name = slot.name.clone();
        let click_name = slot.name.clone();
        let global_label_owned = global_label.to_owned();
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

        let right_badge: AnyElement = if is_current_pinned_target {
            status_badge(
                match language {
                    Language::English => "Current",
                    Language::SimplifiedChinese => "当前目标",
                },
                StatusTone::Success,
                theme,
            )
            .into_any_element()
        } else if is_hovered_switch {
            status_badge(
                match language {
                    Language::English => "Switch",
                    Language::SimplifiedChinese => "点击切换",
                },
                StatusTone::Route,
                theme,
            )
            .into_any_element()
        } else {
            status_badge(slot.strategy.clone(), StatusTone::Neutral, theme).into_any_element()
        };

        div()
            .id(format!("tree-rule-policy-card-{index}"))
            .debug_selector(move || selector)
            .relative()
            .px(px(11.0))
            .py(px(7.5))
            .rounded(px(10.0))
            .bg(card_bg)
            .border_1()
            .border_color(card_border)
            .when(dimmed, |el| el.opacity(0.28))
            .when(can_switch && !is_hovered_switch, |el| el.opacity(0.92))
            .cursor_pointer()
            .flex()
            .flex_col()
            .gap(px(4.0))
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
                if let Some(action) = switch_action.clone() {
                    this.topology.cancel_auto_scroll();
                    match action {
                        RuleRetargetAction::ManualRule(rule_index) => {
                            this.update_manual_rule_target(rule_index, &click_name, cx);
                        }
                        RuleRetargetAction::RemoteSource(source_id) => {
                            this.update_qx_rule_source_target(source_id, click_name.clone(), cx);
                        }
                        RuleRetargetAction::LocalGroup(group_index) => {
                            this.update_local_rule_group_target(group_index, &click_name, cx);
                        }
                        RuleRetargetAction::FinalFallback => {
                            let target =
                                (click_name != global_label_owned).then_some(click_name.as_str());
                            this.set_final_fallback_target(target, cx);
                        }
                    }
                    cx.notify();
                    return;
                }
                if matches!(
                    this.topology.pinned_rules.as_ref(),
                    Some(RulesTreeFocus::Policy(current)) if current == &click_name
                ) {
                    this.topology.pinned_rules = None;
                    this.topology.cancel_auto_scroll();
                } else {
                    let focus = RulesTreeFocus::Policy(click_name.clone());
                    this.topology.pinned_rules = Some(focus.clone());
                    this.start_rules_tree_auto_scroll(&focus, cx);
                }
                cx.notify();
            }))
            .child(card_port_tracker(
                registry,
                format!("rp:{}", slot.name),
                "col:policies".to_owned(),
            ))
            .child(port_dot(
                PortSide::Left,
                if highlighted || is_hovered_switch {
                    theme.route_trace
                } else {
                    theme.surface_elevated
                },
                if highlighted || is_hovered_switch {
                    theme.route_trace
                } else {
                    theme.outline_strong
                },
                highlighted || is_hovered_switch,
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
                    .child(right_badge),
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
        registry: LayoutRegistry,
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
            .px(px(11.0))
            .py(px(7.5))
            .rounded(px(10.0))
            .bg(card_bg)
            .border_1()
            .border_color(card_border)
            .when(dimmed, |el| el.opacity(0.28))
            .cursor_pointer()
            .flex()
            .flex_col()
            .gap(px(4.0))
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
                    this.topology.cancel_auto_scroll();
                } else {
                    let focus = RulesTreeFocus::Exit(click_name.clone());
                    this.topology.pinned_rules = Some(focus.clone());
                    this.start_rules_tree_auto_scroll(&focus, cx);
                }
                cx.notify();
            }))
            .child(card_port_tracker(
                registry,
                format!("re:{}", exit.name),
                "col:exits".to_owned(),
            ))
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
    rule_items: &[&'a RuleTreeItem],
    policy_slots: &'a [PolicySlot],
    focus: Option<&RulesTreeFocus>,
) -> (BTreeSet<&'a str>, BTreeSet<&'a str>, BTreeSet<&'a str>) {
    let mut rules = BTreeSet::new();
    let mut policies = BTreeSet::new();
    let mut exits = BTreeSet::new();

    match focus {
        None => {}
        Some(RulesTreeFocus::Rule(rule_id)) => {
            if let Some(item) = rule_items.iter().copied().find(|r| r.id == *rule_id) {
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
    rule_items: &[&RuleTreeItem],
    policy_slots: &[PolicySlot],
    focus: Option<&RulesTreeFocus>,
    preview_switch_target: Option<(&str, &str)>,
    manual_expanded: bool,
    theme: Theme,
) -> Vec<BezierEdge> {
    let (hl_rules, hl_policies, hl_exits) =
        compute_rules_tree_highlights(rule_items, policy_slots, focus);
    let has_focus = focus.is_some();
    let referenced_policies: BTreeSet<&str> = rule_items
        .iter()
        .map(|item| item.target_policy.as_str())
        .collect();
    let mut edges = Vec::new();

    // Stage 1 -> Stage 2: Rule -> Policy Group (only active-mode rules emit wires).
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
            (with_alpha(theme.route_trace, 0.30), 1.45, false, 1)
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

    // Live dashed preview wire when hovering a candidate policy while a rule is pinned.
    if let Some((rule_id, candidate_policy)) = preview_switch_target {
        edges.push(BezierEdge {
            from_key: format!("r:{rule_id}"),
            to_key: format!("rp:{candidate_policy}"),
            orientation: EdgeOrientation::Horizontal,
            color: with_alpha(theme.route_trace, 0.88),
            width: 2.2,
            dashed: true,
            layer: 3,
        });
    }

    // Stage 2 -> Stage 3: Policy Group -> Active Exit
    for slot in policy_slots {
        let is_preview_policy =
            preview_switch_target.is_some_and(|(_, target)| target == slot.name);
        if manual_expanded
            && !referenced_policies.contains(slot.name.as_str())
            && !hl_policies.contains(slot.name.as_str())
            && !is_preview_policy
        {
            continue;
        }
        let highlighted = is_preview_policy
            || (hl_policies.contains(slot.name.as_str())
                && hl_exits.contains(slot.exit_name.as_str()));
        let (color, width, dashed, layer) = if is_preview_policy {
            (with_alpha(theme.status_success, 0.85), 2.2, true, 3)
        } else if highlighted {
            if slot.is_direct || slot.is_reject {
                (theme.text_primary, 2.4, false, 2)
            } else {
                (theme.status_success, 2.6, false, 2)
            }
        } else if has_focus {
            (with_alpha(theme.outline_strong, 0.06), 1.0, false, 0)
        } else {
            (with_alpha(theme.status_success, 0.34), 1.5, false, 1)
        };
        edges.push(BezierEdge {
            from_key: format!("rp:{}", slot.name),
            to_key: format!("re:{}", slot.exit_name),
            orientation: EdgeOrientation::Horizontal,
            color,
            width,
            dashed,
            layer,
        });
    }

    edges
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
