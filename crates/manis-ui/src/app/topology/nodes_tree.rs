use std::collections::{BTreeMap, BTreeSet};

use gpui::{
    AnyElement, Context, Div, FontWeight, InteractiveElement, IntoElement, ParentElement, Stateful,
    StatefulInteractiveElement, Styled, div, prelude::FluentBuilder as _, px,
};
use gpui_component::Icon;
use manis_core::{ManagedPolicyStrategy, NodeIdentity, PolicyGroupId, ProxyId, WindowSizeClass};

use super::{
    BezierEdge, EdgeOrientation, NodesTreeFocus, PortRegistry, PortSide, compact_protocol_label,
    legend_entry, new_port_registry, paint_edges_canvas, port_dot, port_tracker, with_alpha,
};
use crate::{
    app::{
        ManisApp, PolicySelectionRequest,
        nodes::{parse_latency_ms, region_code},
        presentation::policy_kind_label,
    },
    components::{
        RouteSegmentKind, StatusTone, card_surface, region_code_badge, route_segment, status_badge,
    },
    localization::{CountNoun, Language},
    mihomo::ControllerState,
    theme::{MONO_FAMILY, Radius, Space, Theme},
};

struct TreePolicyData {
    id: String,
    group_id: PolicyGroupId,
    name: String,
    strategy: String,
    allows_manual_selection: bool,
    current_exit: Option<String>,
    current_latency_ms: Option<u64>,
    members: Vec<(ProxyId, String)>,
}

struct TreeNodeData {
    name: String,
    region: String,
    protocol: String,
    latency_ms: Option<u64>,
    source_id: String,
    source_index: usize,
    identity: Option<NodeIdentity>,
}

struct TreeSourceChip {
    id: String,
    name: String,
    count: usize,
    color_index: usize,
}

impl ManisApp {
    /// Policy-to-nodes tree workspace: Tier 01 policy groups on top, unobstructed cubic Bézier
    /// connection bay in the middle, and Tier 02 all nodes below.
    #[allow(clippy::too_many_lines)] // Keeps the two-tier tree layout and edge builder cohesive.
    pub(in crate::app) fn nodes_tree_workspace(
        &self,
        theme: Theme,
        size_class: WindowSizeClass,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let compact = size_class == WindowSizeClass::Compact;
        let language = self.language();
        let (policies, all_nodes, source_chips) = self.collect_nodes_tree_data(language);

        let focus = self.topology.active_nodes_focus().cloned();
        let pinned_policy_id = match self.topology.pinned_nodes.as_ref() {
            Some(NodesTreeFocus::Policy(id)) => Some(id.clone()),
            _ => None,
        };

        let inspector = self.nodes_tree_inspector(
            &policies,
            &all_nodes,
            focus.as_ref(),
            pinned_policy_id.as_deref(),
            language,
            theme,
            cx,
        );

        if policies.is_empty() && all_nodes.is_empty() {
            return div()
                .id("nodes-tree-empty")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .px(if compact { px(12.0) } else { px(24.0) })
                .py(Space::Lg.px())
                .flex()
                .flex_col()
                .gap(Space::Lg.px())
                .child(inspector)
                .child(self.empty_policy_content(language, theme))
                .child(Self::node_empty_state(compact, language, theme, cx));
        }

        let active_filter = self
            .topology
            .node_source_filter
            .as_deref()
            .filter(|id| source_chips.iter().any(|chip| chip.id == *id));
        let visible_nodes: Vec<&TreeNodeData> = match active_filter {
            Some(filter_id) => all_nodes
                .iter()
                .filter(|node| node.source_id == filter_id)
                .collect(),
            None => all_nodes.iter().collect(),
        };

        let registry = new_port_registry();
        let edges = build_nodes_tree_edges(&policies, &visible_nodes, focus.as_ref(), theme);

        let policy_cards = policies
            .iter()
            .enumerate()
            .map(|(index, policy)| {
                Self::nodes_tree_policy_card(
                    index,
                    policy,
                    focus.as_ref(),
                    pinned_policy_id.as_deref(),
                    registry.clone(),
                    language,
                    theme,
                    cx,
                )
            })
            .collect::<Vec<_>>();

        let node_cards = visible_nodes
            .iter()
            .enumerate()
            .map(|(index, node)| {
                Self::nodes_tree_node_card(
                    index,
                    node,
                    &policies,
                    focus.as_ref(),
                    pinned_policy_id.as_deref(),
                    registry.clone(),
                    language,
                    theme,
                    cx,
                )
            })
            .collect::<Vec<_>>();

        // When there are many nodes (> 10), keep each tile readable with a minimum width so the
        // inner tree scrolls horizontally as a single unified surface with its Bézier curves.
        let column_count =
            u16::try_from(visible_nodes.len().max(policies.len()).max(5)).unwrap_or(u16::MAX);
        let min_tree_width = (f32::from(column_count) * 102.0).max(760.0);

        let canvas_inner = div()
            .relative()
            .w_full()
            .min_w(px(min_tree_width))
            .flex()
            .flex_col()
            .child(paint_edges_canvas(registry, edges))
            // Tier 01 header + policy cards
            .child(
                div()
                    .relative()
                    .flex()
                    .flex_col()
                    .gap(px(10.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap(Space::Sm.px())
                            .child(tier_tag(
                                match language {
                                    Language::English => {
                                        format!("TIER 01 · POLICY GROUPS ({})", policies.len())
                                    }
                                    Language::SimplifiedChinese => {
                                        format!("TIER 01 · 策略组（{}）", policies.len())
                                    }
                                },
                                theme,
                            ))
                            .child(
                                div()
                                    .text_size(px(11.5))
                                    .line_height(px(16.0))
                                    .text_color(theme.text_tertiary)
                                    .child(match language {
                                        Language::English => {
                                            "Hover a policy group to trace all member nodes · Click to pin & switch exit"
                                        }
                                        Language::SimplifiedChinese => {
                                            "悬停任意策略组高亮全部成员连线 · 点击锁定后可直接点下方节点切换出口"
                                        }
                                    }),
                            ),
                    )
                    .child(if policy_cards.is_empty() {
                        div()
                            .py(Space::Lg.px())
                            .child(self.empty_policy_content(language, theme))
                    } else {
                        div()
                            .flex()
                            .items_stretch()
                            .gap(px(12.0))
                            .children(policy_cards)
                    }),
            )
            // Unobstructed Bézier curve bay
            .child(div().h(px(164.0)).flex_shrink_0())
            // Tier 02 node cards + footer filter row strictly below cards
            .child(
                div()
                    .relative()
                    .flex()
                    .flex_col()
                    .gap(px(12.0))
                    .child(if node_cards.is_empty() {
                        div()
                            .py(Space::Lg.px())
                            .child(Self::node_empty_state(compact, language, theme, cx))
                    } else {
                        div()
                            .flex()
                            .items_stretch()
                            .gap(px(8.0))
                            .children(node_cards)
                    })
                    .child(Self::nodes_tree_tier2_footer(
                        visible_nodes.len(),
                        all_nodes.len(),
                        &source_chips,
                        active_filter,
                        language,
                        theme,
                        cx,
                    )),
            );

        let canvas_card = card_surface(theme)
            .id("nodes-tree-canvas")
            .debug_selector(|| "nodes-tree-canvas".to_owned())
            .flex_1()
            .min_h(px(430.0))
            .p(px(18.0))
            .overflow_x_scroll()
            .on_click(cx.listener(|this, _, _, cx| {
                if this.topology.pinned_nodes.take().is_some() {
                    cx.notify();
                }
            }))
            .child(canvas_inner);

        div()
            .id("nodes-tree-scroll")
            .debug_selector(|| "nodes-tree-scroll".to_owned())
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .px(if compact { px(12.0) } else { px(24.0) })
            .py(Space::Lg.px())
            .flex()
            .flex_col()
            .gap(Space::Md.px())
            .child(inspector)
            .child(canvas_card)
    }

    #[expect(
        clippy::too_many_lines,
        reason = "Gathers policies, sources, and fallback runtime nodes in one pass"
    )]
    fn collect_nodes_tree_data(
        &self,
        language: Language,
    ) -> (Vec<TreePolicyData>, Vec<TreeNodeData>, Vec<TreeSourceChip>) {
        let mut policies = Vec::new();
        if self.catalog.is_some() {
            for group in self
                .policy_groups()
                .filter(|group| group.name != manis_profile::MANIS_GLOBAL_GROUP_NAME)
            {
                let selected = self.selected_node_for_policy(group);
                policies.push(TreePolicyData {
                    id: group.id.as_str().to_owned(),
                    group_id: group.id.clone(),
                    name: group.name.clone(),
                    strategy: policy_kind_label(language, group.kind).to_owned(),
                    allows_manual_selection: group.kind.allows_manual_selection(),
                    current_exit: selected.map(|node| node.name.clone()),
                    current_latency_ms: selected.and_then(|node| node.latency_ms).map(u64::from),
                    members: group
                        .nodes
                        .iter()
                        .map(|node| (node.id.clone(), node.name.clone()))
                        .collect(),
                });
            }
        } else {
            for group in &self.managed_policies.groups {
                let candidates = self.managed_policy_candidate_nodes(group);
                let selected_name = self
                    .managed_policies
                    .node_selections
                    .policy_target(&group.name)
                    .map(str::to_owned)
                    .or_else(|| candidates.first().map(|node| node.name.clone()));
                let current_latency_ms = selected_name.as_deref().and_then(|name| {
                    candidates
                        .iter()
                        .find(|node| node.name == name)
                        .and_then(|node| node.latency_ms)
                        .map(u64::from)
                });
                policies.push(TreePolicyData {
                    id: group.id.clone(),
                    group_id: PolicyGroupId::new(group.id.clone()),
                    name: group.name.clone(),
                    strategy: Self::offline_policy_strategy(language, group.strategy),
                    allows_manual_selection: group.strategy == ManagedPolicyStrategy::Manual,
                    current_exit: selected_name,
                    current_latency_ms,
                    members: candidates
                        .into_iter()
                        .map(|node| (node.id, node.name))
                        .collect(),
                });
            }
        }

        let has_local_sources = self
            .imported_subscriptions
            .iter()
            .any(|subscription| subscription.enabled)
            || !self.saved_single_nodes.is_empty();
        let source_groups = self.node_source_groups(has_local_sources, language);

        let mut nodes = Vec::new();
        let mut seen = BTreeSet::new();
        let mut source_chips = Vec::new();

        for (source_index, source) in source_groups.iter().enumerate() {
            let mut count_in_source = 0usize;
            for (provider_index, provider) in source.providers.iter().enumerate() {
                let runtime_provider = source
                    .runtime_provider_names
                    .get(provider_index)
                    .cloned()
                    .unwrap_or_else(|| provider.name.clone());
                for node in &provider.nodes {
                    if !seen.insert(node.name.clone()) {
                        continue;
                    }
                    count_in_source += 1;
                    let latency_ms = node.latency_label.as_deref().and_then(parse_latency_ms);
                    nodes.push(TreeNodeData {
                        region: region_code(&node.name),
                        protocol: compact_protocol_label(&node.protocol),
                        latency_ms,
                        source_id: source.id.clone(),
                        source_index,
                        identity: NodeIdentity::new(&runtime_provider, &node.name).ok(),
                        name: node.name.clone(),
                    });
                }
            }
            for saved in &source.saved_nodes {
                if !seen.insert(saved.name.clone()) {
                    continue;
                }
                count_in_source += 1;
                nodes.push(TreeNodeData {
                    region: region_code(&saved.name),
                    protocol: compact_protocol_label(saved.protocol),
                    latency_ms: None,
                    source_id: source.id.clone(),
                    source_index,
                    identity: NodeIdentity::new(&source.id, &saved.name).ok(),
                    name: saved.name.clone(),
                });
            }
            if count_in_source > 0 {
                source_chips.push(TreeSourceChip {
                    id: source.id.clone(),
                    name: source.name.clone(),
                    count: count_in_source,
                    color_index: source_index,
                });
            }
        }

        // Fill in latency from catalog nodes when source provider snapshot lacks it, and ensure
        // every member node referenced by a policy group exists in Tier 02.
        let mut catalog_meta: BTreeMap<&str, (&str, Option<u64>)> = BTreeMap::new();
        for group in self.policy_groups() {
            for node in &group.nodes {
                catalog_meta
                    .entry(node.name.as_str())
                    .or_insert((node.detail.as_str(), node.latency_ms.map(u64::from)));
            }
        }
        for node in &mut nodes {
            if node.latency_ms.is_none()
                && let Some((_, Some(ms))) = catalog_meta.get(node.name.as_str())
            {
                node.latency_ms = Some(*ms);
            }
        }
        for policy in &policies {
            for (_, member_name) in &policy.members {
                if seen.insert(member_name.clone()) {
                    let (proto, ms) = catalog_meta
                        .get(member_name.as_str())
                        .copied()
                        .unwrap_or(("", None));
                    nodes.push(TreeNodeData {
                        region: region_code(member_name),
                        protocol: compact_protocol_label(proto),
                        latency_ms: ms,
                        source_id: "runtime".to_owned(),
                        source_index: source_chips.len(),
                        identity: NodeIdentity::new("runtime", member_name).ok(),
                        name: member_name.clone(),
                    });
                }
            }
        }

        (policies, nodes, source_chips)
    }

    #[expect(
        clippy::too_many_arguments,
        clippy::too_many_lines,
        reason = "Builds the single cohesive inspector bar for all three focus states"
    )]
    fn nodes_tree_inspector(
        &self,
        policies: &[TreePolicyData],
        all_nodes: &[TreeNodeData],
        focus: Option<&NodesTreeFocus>,
        pinned_policy_id: Option<&str>,
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
                    .child(div().size(px(7.0)).rounded_full().bg(theme.status_success))
                    .child(
                        div()
                            .text_size(px(12.5))
                            .line_height(px(18.0))
                            .text_color(theme.text_secondary)
                            .child(match language {
                                Language::English => {
                                    "Hover any policy group to highlight its path to every member node, or hover a node to trace its groups."
                                }
                                Language::SimplifiedChinese => {
                                    "将鼠标移到任意策略组上，即可高亮通往全部成员节点的路径；移到下方节点上可反查所属策略组。"
                                }
                            }),
                    );
            }
            Some(NodesTreeFocus::Policy(policy_id)) => {
                if let Some(policy) = policies.iter().find(|item| item.id == *policy_id) {
                    let exit_label = policy.current_exit.as_deref().map_or_else(
                        || {
                            match language {
                                Language::English => "No exit",
                                Language::SimplifiedChinese => "无可用出口",
                            }
                            .to_owned()
                        },
                        |name| {
                            let display = Self::policy_candidate_display_name(name);
                            match policy.current_latency_ms {
                                Some(ms) => format!("{display} · {ms} ms"),
                                None => display.to_owned(),
                            }
                        },
                    );
                    summary = summary
                        .child(route_segment(
                            policy.name.clone(),
                            RouteSegmentKind::Group,
                            None,
                            theme,
                        ))
                        .child(status_badge(
                            format!(
                                "{} · {}",
                                policy.strategy,
                                language.count(CountNoun::Node, policy.members.len())
                            ),
                            StatusTone::Neutral,
                            theme,
                        ))
                        .child(
                            div()
                                .text_size(px(11.0))
                                .text_color(theme.text_tertiary)
                                .child("→"),
                        )
                        .child(route_segment(
                            exit_label,
                            RouteSegmentKind::Exit,
                            policy.current_latency_ms,
                            theme,
                        ));
                    if pinned_policy_id == Some(policy.id.as_str()) {
                        let pin_hint = if policy.allows_manual_selection {
                            match language {
                                Language::English => "Pinned · Click a highlighted node to switch",
                                Language::SimplifiedChinese => {
                                    "已锁定 · 点击下方高亮节点即可切换出口"
                                }
                            }
                        } else {
                            match language {
                                Language::English => "Pinned · Automatic policy (read-only)",
                                Language::SimplifiedChinese => "已锁定 · 自动策略（只读）",
                            }
                        };
                        summary = summary.child(status_badge(pin_hint, StatusTone::Route, theme));
                    }
                }
            }
            Some(NodesTreeFocus::Node(node_name)) => {
                let belong_count = policies
                    .iter()
                    .filter(|policy| policy.members.iter().any(|(_, name)| name == node_name))
                    .count();
                let active_count = policies
                    .iter()
                    .filter(|policy| policy.current_exit.as_deref() == Some(node_name.as_str()))
                    .count();
                let node = all_nodes.iter().find(|item| item.name == *node_name);
                summary = summary
                    .child(region_code_badge(
                        node.map_or_else(|| region_code(node_name), |n| n.region.clone()),
                        theme,
                    ))
                    .child(
                        div()
                            .text_size(px(12.5))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.text_primary)
                            .child(Self::policy_candidate_display_name(node_name).to_owned()),
                    )
                    .child(status_badge(
                        match language {
                            Language::English => format!("In {belong_count} policy groups"),
                            Language::SimplifiedChinese => format!("属于 {belong_count} 个策略组"),
                        },
                        StatusTone::Neutral,
                        theme,
                    ))
                    .when(active_count > 0, |row| {
                        row.child(status_badge(
                            match language {
                                Language::English => format!("Active in {active_count}"),
                                Language::SimplifiedChinese => {
                                    format!("其中 {active_count} 组当前激活")
                                }
                            },
                            StatusTone::Success,
                            theme,
                        ))
                    });
            }
        }

        let mut right = div()
            .flex()
            .items_center()
            .gap(Space::Md.px())
            .flex_shrink_0()
            .child(legend_entry(
                theme.status_success,
                match language {
                    Language::English => "Active exit",
                    Language::SimplifiedChinese => "当前激活出口",
                },
                theme.text_tertiary,
            ))
            .child(legend_entry(
                theme.action_primary,
                match language {
                    Language::English => "Group member",
                    Language::SimplifiedChinese => "策略组成员节点",
                },
                theme.text_tertiary,
            ));

        if !matches!(self.controller, ControllerState::Connected { .. }) {
            right = right.child(self.connection_button(theme, cx));
        }
        right = right.child(Self::managed_policy_add_button(
            "add-policy-group-tree",
            language,
            theme,
            cx,
        ));

        card_surface(theme)
            .id("nodes-tree-inspector")
            .debug_selector(|| "nodes-tree-inspector".to_owned())
            .px(px(14.0))
            .py(px(10.0))
            .border_color(if focus.is_some() {
                theme.action_primary
            } else {
                theme.outline_subtle
            })
            .flex()
            .items_center()
            .justify_between()
            .gap(Space::Md.px())
            .flex_wrap()
            .child(summary)
            .child(right)
    }

    #[expect(
        clippy::too_many_arguments,
        clippy::too_many_lines,
        reason = "Single GPUI card builder with hover, pin, exit chip, and port dot"
    )]
    fn nodes_tree_policy_card(
        index: usize,
        policy: &TreePolicyData,
        focus: Option<&NodesTreeFocus>,
        pinned_policy_id: Option<&str>,
        registry: PortRegistry,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let is_pinned = pinned_policy_id == Some(policy.id.as_str());
        let highlighted = match focus {
            Some(NodesTreeFocus::Policy(id)) => *id == policy.id,
            Some(NodesTreeFocus::Node(node_name)) => {
                policy.members.iter().any(|(_, name)| name == node_name)
            }
            None => false,
        };
        let dimmed = focus.is_some() && !highlighted;

        let card_bg = if highlighted {
            theme.action_soft
        } else {
            theme.surface_elevated
        };
        let card_border = if highlighted {
            theme.action_primary
        } else {
            theme.outline_subtle
        };

        let exit_display = policy.current_exit.as_deref().map_or_else(
            || "—".to_owned(),
            |name| Self::policy_candidate_display_name(name).to_owned(),
        );
        let latency_color = theme.latency_color(policy.current_latency_ms);
        let latency_label = policy
            .current_latency_ms
            .map_or_else(|| "—".to_owned(), |ms| format!("{ms}ms"));

        let hover_id = policy.id.clone();
        let click_id = policy.id.clone();
        let selector = format!("tree-policy-{index}");

        let card = div()
            .id(format!("tree-policy-card-{index}"))
            .debug_selector(move || selector)
            .relative()
            .flex_1()
            .min_w(px(148.0))
            .p(px(12.0))
            .rounded(Radius::Card.px())
            .bg(card_bg)
            .border_1()
            .border_color(card_border)
            .when(dimmed, |el| el.opacity(0.32))
            .cursor_pointer()
            .flex()
            .flex_col()
            .gap(px(7.0))
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                if *hovered {
                    this.topology.hovered_nodes = Some(NodesTreeFocus::Policy(hover_id.clone()));
                    cx.notify();
                } else if matches!(
                    this.topology.hovered_nodes.as_ref(),
                    Some(NodesTreeFocus::Policy(current)) if current == &hover_id
                ) {
                    this.topology.hovered_nodes = None;
                    cx.notify();
                }
            }))
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                if matches!(
                    this.topology.pinned_nodes.as_ref(),
                    Some(NodesTreeFocus::Policy(current)) if current == &click_id
                ) {
                    this.topology.pinned_nodes = None;
                } else {
                    this.topology.pinned_nodes = Some(NodesTreeFocus::Policy(click_id.clone()));
                }
                cx.notify();
            }))
            .child(port_tracker(registry, format!("p:{}", policy.id)))
            // Title + optional pinned badge
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
                            .text_size(px(13.0))
                            .line_height(px(18.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.text_primary)
                            .child(policy.name.clone()),
                    )
                    .when(is_pinned, |row| {
                        row.child(
                            div()
                                .flex_shrink_0()
                                .flex()
                                .items_center()
                                .gap(px(3.0))
                                .px(px(6.0))
                                .h(px(18.0))
                                .rounded_full()
                                .bg(theme.action_primary)
                                .text_color(theme.action_on_primary)
                                .text_size(px(10.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(
                                    Icon::new(gpui_component_assets::IconName::Pin)
                                        .size(px(10.0))
                                        .text_color(theme.action_on_primary),
                                )
                                .child(match language {
                                    Language::English => "Pinned",
                                    Language::SimplifiedChinese => "已锁定",
                                }),
                        )
                    }),
            )
            // Strategy + member count
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(6.0))
                    .child(status_badge(
                        policy.strategy.clone(),
                        StatusTone::Neutral,
                        theme,
                    ))
                    .child(
                        div()
                            .font_family(MONO_FAMILY)
                            .text_size(px(11.0))
                            .text_color(theme.text_tertiary)
                            .child(language.count(CountNoun::Node, policy.members.len())),
                    ),
            )
            // Current exit chip
            .child(
                div()
                    .mt(px(2.0))
                    .px(px(8.0))
                    .py(px(5.0))
                    .rounded(px(8.0))
                    .bg(theme.surface_low)
                    .border_1()
                    .border_color(theme.outline_subtle)
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(6.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(5.0))
                            .min_w_0()
                            .flex_1()
                            .child(div().size(px(6.0)).rounded_full().flex_shrink_0().bg(
                                if policy.current_exit.is_some() {
                                    theme.status_success
                                } else {
                                    theme.text_tertiary
                                },
                            ))
                            .child(
                                div()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .whitespace_nowrap()
                                    .text_size(px(11.5))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(theme.text_primary)
                                    .child(exit_display),
                            ),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .font_family(MONO_FAMILY)
                            .text_size(px(11.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(latency_color)
                            .child(latency_label),
                    ),
            )
            // Bottom anchor port
            .child(port_dot(
                PortSide::Bottom,
                if highlighted {
                    theme.action_primary
                } else {
                    theme.surface_elevated
                },
                if highlighted {
                    theme.action_primary
                } else {
                    theme.outline_strong
                },
                highlighted,
            ));

        card.into_any_element()
    }

    #[expect(
        clippy::too_many_arguments,
        clippy::too_many_lines,
        reason = "Single GPUI node tile builder with highlight roles and click-to-switch"
    )]
    fn nodes_tree_node_card(
        index: usize,
        node: &TreeNodeData,
        policies: &[TreePolicyData],
        focus: Option<&NodesTreeFocus>,
        pinned_policy_id: Option<&str>,
        registry: PortRegistry,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let active_anywhere = policies
            .iter()
            .any(|policy| policy.current_exit.as_deref() == Some(node.name.as_str()));

        let (is_active_hl, is_member_hl, role_label) = match focus {
            Some(NodesTreeFocus::Policy(policy_id)) => {
                if let Some(policy) = policies.iter().find(|p| p.id == *policy_id) {
                    if policy.current_exit.as_deref() == Some(node.name.as_str()) {
                        (
                            true,
                            false,
                            Some(match language {
                                Language::English => "Active",
                                Language::SimplifiedChinese => "当前出口",
                            }),
                        )
                    } else if policy.members.iter().any(|(_, name)| name == &node.name) {
                        let can_click_switch = pinned_policy_id == Some(policy.id.as_str())
                            && policy.allows_manual_selection;
                        (
                            false,
                            true,
                            Some(if can_click_switch {
                                match language {
                                    Language::English => "Switch",
                                    Language::SimplifiedChinese => "点击切换",
                                }
                            } else {
                                match language {
                                    Language::English => "Candidate",
                                    Language::SimplifiedChinese => "候选成员",
                                }
                            }),
                        )
                    } else {
                        (false, false, None)
                    }
                } else {
                    (false, false, None)
                }
            }
            Some(NodesTreeFocus::Node(focused_name)) => {
                if focused_name == &node.name {
                    (
                        active_anywhere,
                        !active_anywhere,
                        Some(if active_anywhere {
                            match language {
                                Language::English => "Active",
                                Language::SimplifiedChinese => "已激活",
                            }
                        } else {
                            match language {
                                Language::English => "Standby",
                                Language::SimplifiedChinese => "备用",
                            }
                        }),
                    )
                } else {
                    (false, false, None)
                }
            }
            None => (false, false, None),
        };

        let dimmed = focus.is_some() && !is_active_hl && !is_member_hl;
        let (card_bg, card_border, port_fill, port_border) = if is_active_hl {
            (
                theme.status_success_soft,
                theme.status_success,
                theme.status_success,
                theme.status_success,
            )
        } else if is_member_hl {
            (
                theme.action_soft,
                theme.action_primary,
                theme.action_primary,
                theme.action_primary,
            )
        } else {
            (
                theme.surface_elevated,
                theme.outline_subtle,
                theme.surface_elevated,
                theme.outline_strong,
            )
        };

        // Precompute click target: if a manual policy is pinned and owns this node, switch it;
        // otherwise select this node as the global exit.
        let pinned_switch: Option<PolicySelectionRequest> = pinned_policy_id.and_then(|pid| {
            let policy = policies.iter().find(|p| p.id == pid)?;
            if !policy.allows_manual_selection {
                return None;
            }
            let (proxy_id, _) = policy.members.iter().find(|(_, name)| name == &node.name)?;
            Some(PolicySelectionRequest {
                group_id: policy.group_id.clone(),
                group_name: policy.name.clone(),
                node_id: proxy_id.clone(),
                node_name: node.name.clone(),
            })
        });
        let keep_pinned_on_hover = pinned_policy_id.is_some_and(|pid| {
            policies
                .iter()
                .find(|p| p.id == pid)
                .is_some_and(|p| p.members.iter().any(|(_, name)| name == &node.name))
        });

        let source_bar_color = match node.source_index % 3 {
            0 => with_alpha(theme.action_primary, 0.40),
            1 => with_alpha(theme.route_trace, 0.40),
            _ => with_alpha(theme.status_success, 0.40),
        };
        let latency_color = theme.latency_color(node.latency_ms);
        let latency_label = node
            .latency_ms
            .map_or_else(|| "—".to_owned(), |ms| format!("{ms}ms"));

        let hover_name = node.name.clone();
        let fallback_identity = node.identity.clone();
        let selector = format!("tree-node-{index}");

        div()
            .id(format!("tree-node-card-{index}"))
            .debug_selector(move || selector)
            .relative()
            .flex_1()
            .min_w(px(90.0))
            .pt(px(12.0))
            .pb(px(9.0))
            .px(px(9.0))
            .rounded(px(12.0))
            .bg(card_bg)
            .border_1()
            .border_color(card_border)
            .when(dimmed, |el| el.opacity(0.30))
            .cursor_pointer()
            .flex()
            .flex_col()
            .gap(px(5.0))
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                if *hovered {
                    if keep_pinned_on_hover {
                        return;
                    }
                    this.topology.hovered_nodes = Some(NodesTreeFocus::Node(hover_name.clone()));
                    cx.notify();
                } else if matches!(
                    this.topology.hovered_nodes.as_ref(),
                    Some(NodesTreeFocus::Node(current)) if current == &hover_name
                ) {
                    this.topology.hovered_nodes = None;
                    cx.notify();
                }
            }))
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                if let Some(request) = pinned_switch.clone() {
                    this.select_policy_node(request, cx);
                } else if let Some(identity) = fallback_identity.clone() {
                    this.select_global_node(identity, cx);
                }
            }))
            .child(port_tracker(registry, format!("n:{}", node.name)))
            .child(port_dot(
                PortSide::Top,
                port_fill,
                port_border,
                is_active_hl || is_member_hl,
            ))
            // Region badge + status dot
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(4.0))
                    .child(region_code_badge(node.region.clone(), theme))
                    .child(div().size(px(6.5)).rounded_full().bg(if active_anywhere {
                        theme.status_success
                    } else {
                        with_alpha(theme.text_tertiary, 0.35)
                    })),
            )
            // Node name
            .child(
                div()
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .text_size(px(12.0))
                    .line_height(px(16.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.text_primary)
                    .child(Self::policy_candidate_display_name(&node.name).to_owned()),
            )
            // Protocol + latency
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(3.0))
                    .child(
                        div()
                            .font_family(MONO_FAMILY)
                            .text_size(px(10.0))
                            .text_color(theme.text_tertiary)
                            .child(node.protocol.clone()),
                    )
                    .child(
                        div()
                            .font_family(MONO_FAMILY)
                            .text_size(px(10.5))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(latency_color)
                            .child(latency_label),
                    ),
            )
            // Role pill slot (keeps height uniform whether highlighted or idle)
            .child(
                div()
                    .h(px(18.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .when_some(role_label, |slot, label| {
                        let (fg, bg) = if is_active_hl {
                            (theme.action_on_primary, theme.status_success)
                        } else {
                            (theme.action_primary, theme.surface_elevated)
                        };
                        slot.child(
                            div()
                                .w_full()
                                .h_full()
                                .rounded(px(5.0))
                                .bg(bg)
                                .border_1()
                                .border_color(if is_active_hl {
                                    theme.status_success
                                } else {
                                    with_alpha(theme.action_primary, 0.32)
                                })
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_size(px(10.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(fg)
                                .child(label),
                        )
                    }),
            )
            // Source bar
            .child(div().h(px(2.5)).rounded_full().bg(source_bar_color))
            .into_any_element()
    }

    #[expect(
        clippy::too_many_lines,
        reason = "Renders the Tier 02 badge and source filter chips together"
    )]
    fn nodes_tree_tier2_footer(
        visible_count: usize,
        total_count: usize,
        source_chips: &[TreeSourceChip],
        active_filter: Option<&str>,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        let mut filters = div().flex().items_center().gap(px(6.0)).flex_wrap();

        if source_chips.len() > 1 {
            let all_selected = active_filter.is_none();
            filters = filters.child(
                div()
                    .id("tree-source-filter-all")
                    .px(px(9.0))
                    .h(px(24.0))
                    .rounded_full()
                    .cursor_pointer()
                    .flex()
                    .items_center()
                    .bg(if all_selected {
                        theme.action_soft
                    } else {
                        theme.surface_low
                    })
                    .border_1()
                    .border_color(if all_selected {
                        theme.action_primary
                    } else {
                        theme.outline_subtle
                    })
                    .text_size(px(11.0))
                    .font_weight(if all_selected {
                        FontWeight::SEMIBOLD
                    } else {
                        FontWeight::MEDIUM
                    })
                    .text_color(if all_selected {
                        theme.action_primary
                    } else {
                        theme.text_secondary
                    })
                    .on_click(cx.listener(|this, _, _, cx| {
                        cx.stop_propagation();
                        this.topology.node_source_filter = None;
                        cx.notify();
                    }))
                    .child(match language {
                        Language::English => format!("All sources ({total_count})"),
                        Language::SimplifiedChinese => format!("全部来源 ({total_count})"),
                    }),
            );

            for chip in source_chips {
                let selected = active_filter == Some(chip.id.as_str());
                let dot_color = match chip.color_index % 3 {
                    0 => theme.action_primary,
                    1 => theme.route_trace,
                    _ => theme.status_success,
                };
                let chip_id = chip.id.clone();
                filters = filters.child(
                    div()
                        .id(format!("tree-source-filter-{}", chip.color_index))
                        .px(px(9.0))
                        .h(px(24.0))
                        .rounded_full()
                        .cursor_pointer()
                        .flex()
                        .items_center()
                        .gap(px(5.0))
                        .bg(if selected {
                            theme.action_soft
                        } else {
                            theme.surface_low
                        })
                        .border_1()
                        .border_color(if selected {
                            theme.action_primary
                        } else {
                            theme.outline_subtle
                        })
                        .text_size(px(11.0))
                        .font_weight(if selected {
                            FontWeight::SEMIBOLD
                        } else {
                            FontWeight::MEDIUM
                        })
                        .text_color(if selected {
                            theme.action_primary
                        } else {
                            theme.text_secondary
                        })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.topology.node_source_filter = Some(chip_id.clone());
                            cx.notify();
                        }))
                        .child(div().size(px(6.0)).rounded_full().bg(dot_color))
                        .child(format!("{} ({})", chip.name, chip.count)),
                );
            }
        }

        div()
            .flex()
            .items_center()
            .justify_between()
            .gap(Space::Sm.px())
            .flex_wrap()
            .child(tier_tag(
                match language {
                    Language::English => format!("TIER 02 · ALL NODES ({visible_count})"),
                    Language::SimplifiedChinese => format!("TIER 02 · 全部节点（{visible_count}）"),
                },
                theme,
            ))
            .child(filters)
    }
}

fn build_nodes_tree_edges(
    policies: &[TreePolicyData],
    visible_nodes: &[&TreeNodeData],
    focus: Option<&NodesTreeFocus>,
    theme: Theme,
) -> Vec<BezierEdge> {
    let visible_names: BTreeSet<&str> = visible_nodes.iter().map(|n| n.name.as_str()).collect();
    let mut edges = Vec::new();

    for policy in policies {
        for (_, member_name) in &policy.members {
            if !visible_names.contains(member_name.as_str()) {
                continue;
            }
            let is_active_exit = policy.current_exit.as_deref() == Some(member_name.as_str());
            let highlighted = match focus {
                Some(NodesTreeFocus::Policy(id)) => *id == policy.id,
                Some(NodesTreeFocus::Node(name)) => name == member_name,
                None => false,
            };

            let (color, width, dashed, layer) = if highlighted {
                if is_active_exit {
                    (theme.status_success, 2.75, false, 2)
                } else {
                    (with_alpha(theme.action_primary, 0.92), 2.1, false, 1)
                }
            } else if focus.is_some() {
                (with_alpha(theme.outline_strong, 0.06), 1.0, false, 0)
            } else if is_active_exit {
                (with_alpha(theme.status_success, 0.36), 1.6, false, 1)
            } else {
                (with_alpha(theme.outline_strong, 0.11), 1.0, false, 0)
            };

            edges.push(BezierEdge {
                from_key: format!("p:{}", policy.id),
                to_key: format!("n:{member_name}"),
                orientation: EdgeOrientation::Vertical,
                color,
                width,
                dashed,
                layer,
            });
        }
    }

    edges
}

fn tier_tag(label: String, theme: Theme) -> Div {
    div()
        .px(px(8.0))
        .py(px(2.0))
        .rounded(Radius::Badge.px())
        .bg(theme.surface_low)
        .border_1()
        .border_color(theme.outline_subtle)
        .text_size(px(11.0))
        .line_height(px(16.0))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(theme.text_tertiary)
        .child(label)
}
