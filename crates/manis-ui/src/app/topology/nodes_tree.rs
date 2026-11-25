use std::collections::{BTreeMap, BTreeSet};

use gpui::{
    AnyElement, Context, Div, FontWeight, InteractiveElement, IntoElement, ParentElement, Stateful,
    StatefulInteractiveElement, Styled, div, prelude::FluentBuilder as _, px,
};
use gpui_component::{Icon, scroll::ScrollableElement as _};
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
        let policy_graph = build_policy_graph(&policies);
        let highlight = compute_nodes_tree_highlight(&policies, &policy_graph, focus.as_ref());
        let dense_mode = visible_nodes.len() > 10;
        let (bg_edges, fg_edges) = build_nodes_tree_edges(
            &policies,
            &policy_graph,
            &visible_nodes,
            focus.as_ref(),
            &highlight,
            dense_mode,
            theme,
        );

        let node_cards = visible_nodes
            .iter()
            .enumerate()
            .map(|(index, node)| {
                Self::nodes_tree_node_card(
                    index,
                    node,
                    &policies,
                    focus.as_ref(),
                    &highlight,
                    pinned_policy_id.as_deref(),
                    dense_mode,
                    registry.clone(),
                    language,
                    theme,
                    cx,
                )
            })
            .collect::<Vec<_>>();

        // Keep the entire tree fitted cleanly to the viewport width: policy layers stack in
        // topological order (composite groups above leaf groups, with same-row/cyclic arcs for
        // mesh links), and the physical nodes tier switches to a wrapped chip matrix when > 10.
        let policy_layer_count = policy_graph.layers.len().max(1);
        let node_bay_height = if policy_layer_count > 1 {
            if dense_mode { px(74.0) } else { px(124.0) }
        } else if dense_mode {
            px(96.0)
        } else {
            px(152.0)
        };
        let inter_policy_bay_height = px(58.0);

        let mut policy_tiers_stack = div().relative().w_full().flex().flex_col();
        if policies.is_empty() {
            policy_tiers_stack = policy_tiers_stack.child(
                div()
                    .py(Space::Lg.px())
                    .child(self.empty_policy_content(language, theme)),
            );
        } else {
            for (layer_idx, policy_indices) in policy_graph.layers.iter().enumerate() {
                if layer_idx > 0 {
                    policy_tiers_stack =
                        policy_tiers_stack.child(div().h(inter_policy_bay_height).flex_shrink_0());
                }
                let layer_has_same_row = policy_graph
                    .layer_has_same_row_edge
                    .get(layer_idx)
                    .copied()
                    .unwrap_or(false);
                let tier_number = layer_idx + 1;
                let tier_label = if policy_layer_count == 1 {
                    match language {
                        Language::English => {
                            format!("TIER 01 · POLICY GROUPS ({})", policies.len())
                        }
                        Language::SimplifiedChinese => {
                            format!("TIER 01 · 策略组（{}）", policies.len())
                        }
                    }
                } else if layer_idx + 1 < policy_layer_count {
                    match language {
                        Language::English => format!(
                            "TIER {:02} · COMPOSITE POLICIES ({})",
                            tier_number,
                            policy_indices.len()
                        ),
                        Language::SimplifiedChinese => format!(
                            "TIER {:02} · 复合策略组（{}）",
                            tier_number,
                            policy_indices.len()
                        ),
                    }
                } else {
                    match language {
                        Language::English => format!(
                            "TIER {:02} · POLICY GROUPS ({})",
                            tier_number,
                            policy_indices.len()
                        ),
                        Language::SimplifiedChinese => format!(
                            "TIER {:02} · 策略组（{}）",
                            tier_number,
                            policy_indices.len()
                        ),
                    }
                };

                let layer_cards = policy_indices
                    .iter()
                    .map(|&policy_idx| {
                        Self::nodes_tree_policy_card(
                            policy_idx,
                            &policies[policy_idx],
                            &policies,
                            &policy_graph,
                            focus.as_ref(),
                            &highlight,
                            pinned_policy_id.as_deref(),
                            registry.clone(),
                            language,
                            theme,
                            cx,
                        )
                    })
                    .collect::<Vec<_>>();

                let mut header = div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(Space::Sm.px())
                    .child(tier_tag(tier_label, theme));
                if layer_idx == 0 {
                    header = header.child(
                        div()
                            .text_size(px(11.5))
                            .line_height(px(16.0))
                            .text_color(theme.text_tertiary)
                            .child(match language {
                                Language::English => {
                                    "Hover a policy group to trace member groups & nodes · Click to pin & switch exit"
                                }
                                Language::SimplifiedChinese => {
                                    "悬停任意策略组高亮全部成员与出口链路 · 点击锁定后可直接点下级策略组或节点切换"
                                }
                            }),
                    );
                }

                let row = div()
                    .w_full()
                    .when(layer_has_same_row, |el| el.pt(px(26.0)))
                    .flex()
                    .flex_wrap()
                    .items_stretch()
                    .when(policy_indices.len() < 3, Styled::justify_center)
                    .gap(px(10.0))
                    .children(layer_cards);

                policy_tiers_stack = policy_tiers_stack.child(
                    div()
                        .relative()
                        .w_full()
                        .flex()
                        .flex_col()
                        .gap(px(8.0))
                        .child(header)
                        .child(row),
                );
            }
        }

        let node_grid = if node_cards.is_empty() {
            div()
                .py(Space::Lg.px())
                .child(Self::node_empty_state(compact, language, theme, cx))
        } else if dense_mode {
            div()
                .w_full()
                .flex()
                .flex_wrap()
                .items_stretch()
                .gap_x(px(6.0))
                .gap_y(px(8.0))
                .children(node_cards)
        } else {
            div()
                .w_full()
                .flex()
                .items_stretch()
                .gap(px(8.0))
                .children(node_cards)
        };

        let nodes_tier_number = policy_layer_count + 1;
        let canvas_inner = div()
            .relative()
            .w_full()
            .flex()
            .flex_col()
            // Passive background edges render behind cards.
            .child(paint_edges_canvas(registry.clone(), bg_edges))
            // Layered policy groups (supports nested hierarchies and cyclic mesh references).
            .child(policy_tiers_stack)
            // Unobstructed Bézier curve bay between policies and physical nodes
            .child(div().h(node_bay_height).flex_shrink_0())
            // Physical node cards + footer filter row strictly below cards
            .child(
                div()
                    .relative()
                    .w_full()
                    .flex()
                    .flex_col()
                    .gap(px(10.0))
                    .child(node_grid)
                    .child(Self::nodes_tree_tier2_footer(
                        nodes_tier_number,
                        visible_nodes.len(),
                        all_nodes.len(),
                        &source_chips,
                        active_filter,
                        language,
                        theme,
                        cx,
                    )),
            )
            // Highlighted focus edges and nested policy active links render in a click-through
            // overlay so multi-row traces stay vivid all the way to every target port.
            .when(!fg_edges.is_empty(), |inner| {
                inner.child(paint_edges_canvas(registry, fg_edges))
            });

        let canvas_card = card_surface(theme)
            .id("nodes-tree-canvas")
            .debug_selector(|| "nodes-tree-canvas".to_owned())
            .w_full()
            .flex_shrink_0()
            .min_h(px(360.0))
            .p(px(16.0))
            .on_click(cx.listener(|this, _, _, cx| {
                if this.topology.pinned_nodes.take().is_some() {
                    cx.notify();
                }
            }))
            .child(canvas_inner);

        let horizontal_pad = if compact { px(12.0) } else { px(24.0) };
        div()
            .id("nodes-tree-workspace")
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
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .child(
                        div()
                            .id("nodes-tree-scroll")
                            .debug_selector(|| "nodes-tree-scroll".to_owned())
                            .size_full()
                            .overflow_y_scroll()
                            .track_scroll(&self.topology.nodes_scroll)
                            .child(
                                div()
                                    .w_full()
                                    .px(horizontal_pad)
                                    .pb(Space::Lg.px())
                                    .flex()
                                    .flex_col()
                                    .child(canvas_card),
                            ),
                    )
                    .vertical_scrollbar(&self.topology.nodes_scroll),
            )
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
        // every leaf exit referenced by a policy group (such as DIRECT / REJECT or runtime-only
        // proxies) exists in the physical nodes tier — never duplicating other policy groups there.
        let policy_names: BTreeSet<&str> = policies.iter().map(|p| p.name.as_str()).collect();
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
                if policy_names.contains(member_name.as_str()) {
                    continue;
                }
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
                    let policy_name_map: BTreeMap<String, usize> = policies
                        .iter()
                        .enumerate()
                        .map(|(idx, p)| (p.name.clone(), idx))
                        .collect();
                    let member_summary = policy_member_summary(policy, &policy_name_map, language);
                    summary = summary
                        .child(route_segment(
                            policy.name.clone(),
                            RouteSegmentKind::Group,
                            None,
                            theme,
                        ))
                        .child(status_badge(
                            format!("{} · {member_summary}", policy.strategy),
                            StatusTone::Neutral,
                            theme,
                        ));

                    // Walk active exit chain (cycle-safe) so nested policy groups display
                    // every hop down to the final exit node.
                    let mut visited_ids = BTreeSet::from([policy.id.clone()]);
                    let mut curr = policy;
                    let mut had_hop = false;
                    while let Some(exit_name) = curr.current_exit.as_deref() {
                        had_hop = true;
                        summary = summary.child(
                            div()
                                .text_size(px(11.0))
                                .text_color(theme.text_tertiary)
                                .child("→"),
                        );
                        if let Some(next_policy) = policies.iter().find(|p| p.name == exit_name) {
                            summary = summary.child(route_segment(
                                next_policy.name.clone(),
                                RouteSegmentKind::Group,
                                None,
                                theme,
                            ));
                            if !visited_ids.insert(next_policy.id.clone()) {
                                break;
                            }
                            curr = next_policy;
                        } else {
                            let display = Self::policy_candidate_display_name(exit_name);
                            let exit_label = match curr.current_latency_ms {
                                Some(ms) => format!("{display} · {ms} ms"),
                                None => display.to_owned(),
                            };
                            summary = summary.child(route_segment(
                                exit_label,
                                RouteSegmentKind::Exit,
                                curr.current_latency_ms,
                                theme,
                            ));
                            break;
                        }
                    }
                    if !had_hop {
                        summary = summary
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(theme.text_tertiary)
                                    .child("→"),
                            )
                            .child(route_segment(
                                match language {
                                    Language::English => "No exit",
                                    Language::SimplifiedChinese => "无可用出口",
                                },
                                RouteSegmentKind::Exit,
                                None,
                                theme,
                            ));
                    }
                    if pinned_policy_id == Some(policy.id.as_str()) {
                        let pin_hint = if policy.allows_manual_selection {
                            match language {
                                Language::English => {
                                    "Pinned · Click a highlighted group or node to switch"
                                }
                                Language::SimplifiedChinese => {
                                    "已锁定 · 点击下方高亮策略组或节点即可切换出口"
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
        reason = "Single GPUI card builder with hover, pin, sub-policy switch, and dual ports"
    )]
    fn nodes_tree_policy_card(
        index: usize,
        policy: &TreePolicyData,
        policies: &[TreePolicyData],
        policy_graph: &PolicyGraph,
        focus: Option<&NodesTreeFocus>,
        highlight: &NodesTreeHighlight,
        pinned_policy_id: Option<&str>,
        registry: PortRegistry,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let is_pinned = pinned_policy_id == Some(policy.id.as_str());
        let role = highlight.policy_roles.get(&policy.id).copied();
        let highlighted = role.is_some();
        let dimmed = focus.is_some() && !highlighted;

        // If a manual parent policy is currently pinned and this policy group is one of its
        // selectable members, clicking this card switches the parent's exit to this group.
        let pinned_parent_switch: Option<PolicySelectionRequest> =
            pinned_policy_id.and_then(|pid| {
                if pid == policy.id {
                    return None;
                }
                let parent = policies.iter().find(|p| p.id == pid)?;
                if !parent.allows_manual_selection {
                    return None;
                }
                let (proxy_id, _) = parent
                    .members
                    .iter()
                    .find(|(_, name)| name == &policy.name)?;
                Some(PolicySelectionRequest {
                    group_id: parent.group_id.clone(),
                    group_name: parent.name.clone(),
                    node_id: proxy_id.clone(),
                    node_name: policy.name.clone(),
                })
            });
        let keep_pinned_on_hover = pinned_policy_id.is_some_and(|pid| {
            pid != policy.id
                && policies
                    .iter()
                    .find(|p| p.id == pid)
                    .is_some_and(|p| p.members.iter().any(|(_, name)| name == &policy.name))
        });

        let (card_bg, card_border, port_accent) = match role {
            Some(PolicyHighlightRole::ActiveChild | PolicyHighlightRole::ActiveDescendant) => (
                theme.status_success_soft,
                theme.status_success,
                theme.status_success,
            ),
            Some(PolicyHighlightRole::Parent) => {
                (theme.route_soft, theme.route_trace, theme.route_trace)
            }
            Some(PolicyHighlightRole::Focused | PolicyHighlightRole::CandidateChild) => (
                theme.action_soft,
                theme.action_primary,
                theme.action_primary,
            ),
            None => (
                theme.surface_elevated,
                theme.outline_subtle,
                theme.outline_strong,
            ),
        };

        let role_badge: Option<(&'static str, gpui::Rgba, gpui::Rgba)> = if is_pinned {
            None
        } else {
            match role {
                Some(PolicyHighlightRole::ActiveChild) => Some((
                    match language {
                        Language::English => "Active",
                        Language::SimplifiedChinese => "当前出口",
                    },
                    theme.action_on_primary,
                    theme.status_success,
                )),
                Some(PolicyHighlightRole::CandidateChild) if pinned_parent_switch.is_some() => {
                    Some((
                        match language {
                            Language::English => "Switch",
                            Language::SimplifiedChinese => "点击切换",
                        },
                        theme.action_on_primary,
                        theme.action_primary,
                    ))
                }
                Some(PolicyHighlightRole::Parent) => Some((
                    match language {
                        Language::English => "Parent",
                        Language::SimplifiedChinese => "上级策略组",
                    },
                    theme.route_trace,
                    theme.surface_elevated,
                )),
                _ => None,
            }
        };

        let exit_display = policy.current_exit.as_deref().map_or_else(
            || "—".to_owned(),
            |name| Self::policy_candidate_display_name(name).to_owned(),
        );
        let latency_color = theme.latency_color(policy.current_latency_ms);
        let latency_label = policy
            .current_latency_ms
            .map_or_else(|| "—".to_owned(), |ms| format!("{ms}ms"));

        let member_summary = policy_member_summary(policy, &policy_graph.name_to_idx, language);
        let show_top_port =
            policy_graph.has_incoming_policy[index] || policy_graph.has_same_row_edge[index];
        let show_bottom_port = !policy.members.is_empty() || policy_graph.has_same_row_edge[index];

        let hover_id = policy.id.clone();
        let click_id = policy.id.clone();
        let selector = format!("tree-policy-{index}");

        let card = div()
            .id(format!("tree-policy-card-{index}"))
            .debug_selector(move || selector)
            .relative()
            .flex_1()
            .min_w(px(162.0))
            .max_w(px(300.0))
            .p(px(10.0))
            .rounded(Radius::Card.px())
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
                    if keep_pinned_on_hover {
                        return;
                    }
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
                if let Some(request) = pinned_parent_switch.clone() {
                    this.select_policy_node(request, cx);
                } else if matches!(
                    this.topology.pinned_nodes.as_ref(),
                    Some(NodesTreeFocus::Policy(current)) if current == &click_id
                ) {
                    this.topology.pinned_nodes = None;
                    cx.notify();
                } else {
                    this.topology.pinned_nodes = Some(NodesTreeFocus::Policy(click_id.clone()));
                    cx.notify();
                }
            }))
            .child(port_tracker(registry, format!("p:{}", policy.id)))
            // Top anchor port when referenced by another policy group
            .when(show_top_port, |el| {
                el.child(port_dot(
                    PortSide::Top,
                    if highlighted {
                        port_accent
                    } else {
                        theme.surface_elevated
                    },
                    port_accent,
                    highlighted,
                ))
            })
            // Title + optional pinned / role badge
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
                    })
                    .when_some(role_badge, |row, (label, fg, bg)| {
                        row.child(
                            div()
                                .flex_shrink_0()
                                .px(px(6.0))
                                .h(px(18.0))
                                .rounded_full()
                                .bg(bg)
                                .border_1()
                                .border_color(card_border)
                                .flex()
                                .items_center()
                                .text_size(px(10.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(fg)
                                .child(label),
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
                            .child(member_summary),
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
            .when(show_bottom_port, |el| {
                el.child(port_dot(
                    PortSide::Bottom,
                    if highlighted {
                        port_accent
                    } else {
                        theme.surface_elevated
                    },
                    port_accent,
                    highlighted,
                ))
            });

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
        highlight: &NodesTreeHighlight,
        pinned_policy_id: Option<&str>,
        dense: bool,
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
                    if policy.current_exit.as_deref() == Some(node.name.as_str())
                        || highlight.active_chain_nodes.contains(&node.name)
                    {
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
        let display_name = Self::policy_candidate_display_name(&node.name).to_owned();

        let base = div()
            .id(format!("tree-node-card-{index}"))
            .debug_selector(move || selector)
            .relative()
            .bg(card_bg)
            .border_1()
            .border_color(card_border)
            .when(dimmed, |el| el.opacity(if dense { 0.22 } else { 0.30 }))
            .cursor_pointer()
            .flex()
            .flex_col()
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
            ));

        if dense {
            return base
                .w(px(118.0))
                .flex_shrink_0()
                .pt(px(6.0))
                .pb(px(5.0))
                .px(px(7.0))
                .rounded(px(9.0))
                .gap(px(3.0))
                // Row 1: compact region code + node name + active indicator dot
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(4.0))
                        .min_w_0()
                        .child(
                            div()
                                .flex_shrink_0()
                                .px(px(3.5))
                                .h(px(15.0))
                                .rounded(px(4.0))
                                .bg(theme.surface_low)
                                .border_1()
                                .border_color(theme.outline_subtle)
                                .flex()
                                .items_center()
                                .justify_center()
                                .font_family(MONO_FAMILY)
                                .text_size(px(9.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(theme.text_secondary)
                                .child(node.region.clone()),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .overflow_hidden()
                                .text_ellipsis()
                                .whitespace_nowrap()
                                .text_size(px(11.0))
                                .line_height(px(15.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(theme.text_primary)
                                .child(display_name),
                        )
                        .child(div().size(px(5.5)).rounded_full().flex_shrink_0().bg(
                            if active_anywhere {
                                theme.status_success
                            } else {
                                with_alpha(theme.text_tertiary, 0.28)
                            },
                        )),
                )
                // Row 2: protocol (or role badge when highlighted) + latency
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap(px(4.0))
                        .child(if let Some(label) = role_label {
                            div()
                                .px(px(4.0))
                                .h(px(14.0))
                                .rounded(px(4.0))
                                .bg(if is_active_hl {
                                    theme.status_success
                                } else {
                                    theme.surface_elevated
                                })
                                .flex()
                                .items_center()
                                .text_size(px(9.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(if is_active_hl {
                                    theme.action_on_primary
                                } else {
                                    theme.action_primary
                                })
                                .child(label)
                        } else {
                            div()
                                .font_family(MONO_FAMILY)
                                .text_size(px(9.5))
                                .text_color(theme.text_tertiary)
                                .child(node.protocol.clone())
                        })
                        .child(
                            div()
                                .font_family(MONO_FAMILY)
                                .text_size(px(10.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(latency_color)
                                .child(latency_label),
                        ),
                )
                .child(div().h(px(2.0)).rounded_full().bg(source_bar_color))
                .into_any_element();
        }

        base.flex_1()
            .min_w(px(90.0))
            .pt(px(12.0))
            .pb(px(9.0))
            .px(px(9.0))
            .rounded(px(12.0))
            .gap(px(5.0))
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
                    .child(display_name),
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
        clippy::too_many_arguments,
        clippy::too_many_lines,
        reason = "Renders the physical nodes tier badge and source filter chips together"
    )]
    fn nodes_tree_tier2_footer(
        tier_number: usize,
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
                    Language::English => {
                        format!("TIER {tier_number:02} · ALL NODES ({visible_count})")
                    }
                    Language::SimplifiedChinese => {
                        format!("TIER {tier_number:02} · 全部节点（{visible_count}）")
                    }
                },
                theme,
            ))
            .child(filters)
    }
}

struct PolicyGraph {
    name_to_idx: BTreeMap<String, usize>,
    /// Directed policy -> policy edges `(from_idx, to_idx)`.
    policy_edges: Vec<(usize, usize)>,
    layer_of: Vec<usize>,
    layers: Vec<Vec<usize>>,
    has_incoming_policy: Vec<bool>,
    has_same_row_edge: Vec<bool>,
    layer_has_same_row_edge: Vec<bool>,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum PolicyHighlightRole {
    Focused,
    ActiveChild,
    CandidateChild,
    ActiveDescendant,
    Parent,
}

#[derive(Default)]
struct NodesTreeHighlight {
    policy_roles: BTreeMap<String, PolicyHighlightRole>,
    active_chain_policy_edges: BTreeSet<(usize, usize)>,
    active_chain_node_edges: BTreeSet<(usize, String)>,
    active_chain_nodes: BTreeSet<String>,
    highlighted_policy_edges: BTreeSet<(usize, usize)>,
}

fn policy_graph_height(idx: usize, children: &[Vec<usize>], visiting: &mut [bool]) -> usize {
    visiting[idx] = true;
    let mut max_child = 0usize;
    let mut has_descendant = false;
    for &child in &children[idx] {
        if !visiting[child] {
            has_descendant = true;
            max_child = max_child.max(policy_graph_height(child, children, visiting));
        }
    }
    visiting[idx] = false;
    if has_descendant { max_child + 1 } else { 0 }
}

fn build_policy_graph(policies: &[TreePolicyData]) -> PolicyGraph {
    let n = policies.len();
    let mut name_to_idx = BTreeMap::new();
    for (idx, policy) in policies.iter().enumerate() {
        name_to_idx.entry(policy.name.clone()).or_insert(idx);
    }

    let mut children: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut policy_edges = Vec::new();
    let mut has_incoming_policy = vec![false; n];

    for (from_idx, policy) in policies.iter().enumerate() {
        let mut seen_children = BTreeSet::new();
        for (_, member_name) in &policy.members {
            if let Some(&to_idx) = name_to_idx.get(member_name)
                && to_idx != from_idx
                && seen_children.insert(to_idx)
            {
                children[from_idx].push(to_idx);
                policy_edges.push((from_idx, to_idx));
                has_incoming_policy[to_idx] = true;
            }
        }
    }

    // Bottom-up cycle-safe longest path height: leaf policies get height 0, composite policies
    // that wrap them get height 1+, and symmetric cycles settle into the same layer gracefully.
    let mut visiting = vec![false; n];
    let heights: Vec<usize> = (0..n)
        .map(|idx| policy_graph_height(idx, &children, &mut visiting))
        .collect();
    let max_height = heights.iter().copied().max().unwrap_or(0);

    let layer_of: Vec<usize> = heights.iter().map(|&h| max_height - h).collect();
    let mut layers: Vec<Vec<usize>> = vec![Vec::new(); max_height + 1];
    for (idx, &layer) in layer_of.iter().enumerate() {
        layers[layer].push(idx);
    }
    // Drop any empty intermediate layer while keeping layer_of consistent.
    let non_empty_layers: Vec<Vec<usize>> = layers
        .into_iter()
        .filter(|layer| !layer.is_empty())
        .collect();
    let mut normalized_layer_of = vec![0usize; n];
    for (layer_idx, group) in non_empty_layers.iter().enumerate() {
        for &idx in group {
            normalized_layer_of[idx] = layer_idx;
        }
    }

    let mut has_same_row_edge = vec![false; n];
    let mut layer_has_same_row_edge = vec![false; non_empty_layers.len()];
    for &(from_idx, to_idx) in &policy_edges {
        if normalized_layer_of[from_idx] == normalized_layer_of[to_idx] {
            has_same_row_edge[from_idx] = true;
            has_same_row_edge[to_idx] = true;
            layer_has_same_row_edge[normalized_layer_of[from_idx]] = true;
        }
    }

    PolicyGraph {
        name_to_idx,
        policy_edges,
        layer_of: normalized_layer_of,
        layers: non_empty_layers,
        has_incoming_policy,
        has_same_row_edge,
        layer_has_same_row_edge,
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "Traces policy and node focus highlights across nested and cyclic policy graphs"
)]
fn compute_nodes_tree_highlight(
    policies: &[TreePolicyData],
    graph: &PolicyGraph,
    focus: Option<&NodesTreeFocus>,
) -> NodesTreeHighlight {
    let mut hl = NodesTreeHighlight::default();
    match focus {
        None => {}
        Some(NodesTreeFocus::Policy(focused_id)) => {
            let Some(focused_idx) = policies.iter().position(|p| p.id == *focused_id) else {
                return hl;
            };
            let focused = &policies[focused_idx];
            hl.policy_roles
                .insert(focused.id.clone(), PolicyHighlightRole::Focused);

            // Direct outgoing policy children
            for &(from_idx, to_idx) in &graph.policy_edges {
                if from_idx == focused_idx {
                    let child = &policies[to_idx];
                    let is_active = focused.current_exit.as_deref() == Some(child.name.as_str());
                    hl.highlighted_policy_edges.insert((from_idx, to_idx));
                    if is_active {
                        hl.policy_roles
                            .insert(child.id.clone(), PolicyHighlightRole::ActiveChild);
                        hl.active_chain_policy_edges.insert((from_idx, to_idx));
                    } else {
                        hl.policy_roles
                            .entry(child.id.clone())
                            .or_insert(PolicyHighlightRole::CandidateChild);
                    }
                } else if to_idx == focused_idx {
                    let parent = &policies[from_idx];
                    hl.policy_roles
                        .entry(parent.id.clone())
                        .or_insert(PolicyHighlightRole::Parent);
                    hl.highlighted_policy_edges.insert((from_idx, to_idx));
                }
            }

            // Follow active exit chain transitively down to the final physical node (cycle-safe).
            let mut visited = BTreeSet::from([focused_idx]);
            let mut curr_idx = focused_idx;
            while let Some(exit_name) = policies[curr_idx].current_exit.as_deref() {
                if let Some(&next_idx) = graph.name_to_idx.get(exit_name) {
                    hl.active_chain_policy_edges.insert((curr_idx, next_idx));
                    hl.highlighted_policy_edges.insert((curr_idx, next_idx));
                    if !visited.insert(next_idx) {
                        break;
                    }
                    hl.policy_roles
                        .entry(policies[next_idx].id.clone())
                        .or_insert(PolicyHighlightRole::ActiveDescendant);
                    curr_idx = next_idx;
                } else {
                    hl.active_chain_node_edges
                        .insert((curr_idx, exit_name.to_owned()));
                    hl.active_chain_nodes.insert(exit_name.to_owned());
                    break;
                }
            }
        }
        Some(NodesTreeFocus::Node(focused_node)) => {
            let mut active_owners = BTreeSet::new();
            let mut queue = Vec::new();
            let mut visited = BTreeSet::new();

            for (idx, policy) in policies.iter().enumerate() {
                if policy.members.iter().any(|(_, name)| name == focused_node) {
                    let is_active = policy.current_exit.as_deref() == Some(focused_node.as_str());
                    hl.policy_roles.insert(
                        policy.id.clone(),
                        if is_active {
                            PolicyHighlightRole::ActiveChild
                        } else {
                            PolicyHighlightRole::Focused
                        },
                    );
                    if is_active {
                        active_owners.insert(idx);
                    }
                    visited.insert(idx);
                    queue.push(idx);
                }
            }

            // Walk upstream through policy-to-policy edges so composite groups are highlighted too.
            let mut head = 0;
            while head < queue.len() {
                let child_idx = queue[head];
                head += 1;
                let child_name = policies[child_idx].name.as_str();
                let child_is_active = active_owners.contains(&child_idx);

                for &(parent_idx, target_idx) in &graph.policy_edges {
                    if target_idx != child_idx {
                        continue;
                    }
                    hl.highlighted_policy_edges.insert((parent_idx, child_idx));
                    let parent_points_here =
                        policies[parent_idx].current_exit.as_deref() == Some(child_name);
                    if child_is_active && parent_points_here {
                        hl.active_chain_policy_edges.insert((parent_idx, child_idx));
                        active_owners.insert(parent_idx);
                        hl.policy_roles.insert(
                            policies[parent_idx].id.clone(),
                            PolicyHighlightRole::ActiveDescendant,
                        );
                    } else {
                        hl.policy_roles
                            .entry(policies[parent_idx].id.clone())
                            .or_insert(PolicyHighlightRole::Parent);
                    }
                    if visited.insert(parent_idx) {
                        queue.push(parent_idx);
                    }
                }
            }
        }
    }
    hl
}

fn policy_member_summary(
    policy: &TreePolicyData,
    policy_names: &BTreeMap<String, usize>,
    language: Language,
) -> String {
    let sub_groups = policy
        .members
        .iter()
        .filter(|(_, name)| policy_names.contains_key(name))
        .count();
    let leaf_nodes = policy.members.len().saturating_sub(sub_groups);
    match (sub_groups, leaf_nodes) {
        (0, _) => language.count(CountNoun::Node, policy.members.len()),
        (groups, 0) => match language {
            Language::English => {
                if groups == 1 {
                    "1 group".to_owned()
                } else {
                    format!("{groups} groups")
                }
            }
            Language::SimplifiedChinese => format!("{groups} 个策略组"),
        },
        (groups, nodes) => match language {
            Language::English => format!("{groups} grp · {nodes} node"),
            Language::SimplifiedChinese => format!("{groups} 组 · {nodes} 节点"),
        },
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "Builds policy-to-policy mesh edges and policy-to-node edges in one coherent pass"
)]
fn build_nodes_tree_edges(
    policies: &[TreePolicyData],
    graph: &PolicyGraph,
    visible_nodes: &[&TreeNodeData],
    focus: Option<&NodesTreeFocus>,
    highlight: &NodesTreeHighlight,
    dense_mode: bool,
    theme: Theme,
) -> (Vec<BezierEdge>, Vec<BezierEdge>) {
    let visible_names: BTreeSet<&str> = visible_nodes.iter().map(|n| n.name.as_str()).collect();
    let mut bg_edges = Vec::new();
    let mut fg_edges = Vec::new();

    // 1. Policy -> Policy edges (supports downward hierarchy, same-row peer arcs, and cyclic loops).
    for &(from_idx, to_idx) in &graph.policy_edges {
        let from_policy = &policies[from_idx];
        let to_policy = &policies[to_idx];
        let orientation = match graph.layer_of[from_idx].cmp(&graph.layer_of[to_idx]) {
            std::cmp::Ordering::Less => EdgeOrientation::Vertical,
            std::cmp::Ordering::Equal => EdgeOrientation::SameRowArc,
            std::cmp::Ordering::Greater => EdgeOrientation::UpwardLoop,
        };
        let is_active_exit = from_policy.current_exit.as_deref() == Some(to_policy.name.as_str());
        let on_active_chain = highlight
            .active_chain_policy_edges
            .contains(&(from_idx, to_idx));
        let is_highlighted = highlight
            .highlighted_policy_edges
            .contains(&(from_idx, to_idx));

        if on_active_chain {
            fg_edges.push(BezierEdge {
                from_key: format!("p:{}", from_policy.id),
                to_key: format!("p:{}", to_policy.id),
                orientation,
                color: theme.status_success,
                width: 2.75,
                dashed: false,
                layer: 3,
            });
        } else if is_highlighted {
            let is_parent_incoming = matches!(
                highlight.policy_roles.get(&from_policy.id),
                Some(PolicyHighlightRole::Parent)
            );
            fg_edges.push(BezierEdge {
                from_key: format!("p:{}", from_policy.id),
                to_key: format!("p:{}", to_policy.id),
                orientation,
                color: if is_parent_incoming {
                    with_alpha(theme.route_trace, 0.85)
                } else {
                    with_alpha(theme.action_primary, 0.90)
                },
                width: 2.0,
                dashed: is_parent_incoming,
                layer: 2,
            });
        } else if focus.is_some() {
            bg_edges.push(BezierEdge {
                from_key: format!("p:{}", from_policy.id),
                to_key: format!("p:{}", to_policy.id),
                orientation,
                color: with_alpha(theme.outline_strong, 0.06),
                width: 1.0,
                dashed: false,
                layer: 0,
            });
        } else if is_active_exit {
            fg_edges.push(BezierEdge {
                from_key: format!("p:{}", from_policy.id),
                to_key: format!("p:{}", to_policy.id),
                orientation,
                color: with_alpha(theme.status_success, 0.62),
                width: 1.9,
                dashed: false,
                layer: 2,
            });
        } else {
            bg_edges.push(BezierEdge {
                from_key: format!("p:{}", from_policy.id),
                to_key: format!("p:{}", to_policy.id),
                orientation,
                color: with_alpha(theme.action_primary, 0.24),
                width: 1.25,
                dashed: false,
                layer: 1,
            });
        }
    }

    // 2. Policy -> Leaf Node edges (policy-group members were handled above).
    for (policy_idx, policy) in policies.iter().enumerate() {
        let visible_member_count = policy
            .members
            .iter()
            .filter(|(_, name)| {
                !graph.name_to_idx.contains_key(name) && visible_names.contains(name.as_str())
            })
            .count();

        for (_, member_name) in &policy.members {
            if graph.name_to_idx.contains_key(member_name)
                || !visible_names.contains(member_name.as_str())
            {
                continue;
            }
            let is_active_exit = policy.current_exit.as_deref() == Some(member_name.as_str());
            let on_active_chain = highlight
                .active_chain_node_edges
                .contains(&(policy_idx, member_name.clone()));
            let highlighted = match focus {
                Some(NodesTreeFocus::Policy(id)) => *id == policy.id,
                Some(NodesTreeFocus::Node(name)) => name == member_name,
                None => false,
            };

            if highlighted || on_active_chain {
                // Calibrate candidate stroke opacity when a policy fans out to many nodes so
                // dense constellations stay crisp while the active exit stays unmistakable.
                let (color, width, dashed, layer) = if is_active_exit || on_active_chain {
                    (theme.status_success, 2.75, !highlighted, 2)
                } else if visible_member_count > 16 {
                    (with_alpha(theme.action_primary, 0.42), 1.35, false, 1)
                } else if visible_member_count > 8 {
                    (with_alpha(theme.action_primary, 0.68), 1.7, false, 1)
                } else {
                    (with_alpha(theme.action_primary, 0.92), 2.1, false, 1)
                };
                let edge = BezierEdge {
                    from_key: format!("p:{}", policy.id),
                    to_key: format!("n:{member_name}"),
                    orientation: EdgeOrientation::Vertical,
                    color,
                    width,
                    dashed,
                    layer,
                };
                if dense_mode || on_active_chain {
                    fg_edges.push(edge);
                } else {
                    bg_edges.push(edge);
                }
            } else if focus.is_some() {
                if !dense_mode || is_active_exit {
                    bg_edges.push(BezierEdge {
                        from_key: format!("p:{}", policy.id),
                        to_key: format!("n:{member_name}"),
                        orientation: EdgeOrientation::Vertical,
                        color: with_alpha(theme.outline_strong, 0.06),
                        width: 1.0,
                        dashed: false,
                        layer: 0,
                    });
                }
            } else if is_active_exit {
                let edge = BezierEdge {
                    from_key: format!("p:{}", policy.id),
                    to_key: format!("n:{member_name}"),
                    orientation: EdgeOrientation::Vertical,
                    color: with_alpha(theme.status_success, if dense_mode { 0.44 } else { 0.36 }),
                    width: 1.6,
                    dashed: false,
                    layer: 1,
                };
                if dense_mode {
                    fg_edges.push(edge);
                } else {
                    bg_edges.push(edge);
                }
            } else if !dense_mode {
                bg_edges.push(BezierEdge {
                    from_key: format!("p:{}", policy.id),
                    to_key: format!("n:{member_name}"),
                    orientation: EdgeOrientation::Vertical,
                    color: with_alpha(theme.outline_strong, 0.11),
                    width: 1.0,
                    dashed: false,
                    layer: 0,
                });
            }
        }
    }

    (bg_edges, fg_edges)
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
