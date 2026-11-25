use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use gpui::{
    AnyElement, Bounds, Div, IntoElement, ParentElement, PathBuilder, Pixels, Point, Rgba, Styled,
    canvas, div, point, px,
};
use manis_core::PrimaryWorkspace;

mod nodes_tree;
mod rules_tree;

/// How a workspace lays out its policies, nodes, and routing rules.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(in crate::app) enum WorkspaceViewMode {
    #[default]
    Tree,
    List,
}

impl WorkspaceViewMode {
    pub(in crate::app) const fn toggled(self) -> Self {
        match self {
            Self::Tree => Self::List,
            Self::List => Self::Tree,
        }
    }
}

/// Active focus target in the policy-to-nodes topology tree.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::app) enum NodesTreeFocus {
    Policy(String),
    Node(String),
}

/// Active focus target in the `rule -> policy group -> active exit` topology tree.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::app) enum RulesTreeFocus {
    Rule(String),
    Policy(String),
    Exit(String),
}

/// Live in-flight drag preview for routing rule group priority reordering.
#[derive(Clone, Debug)]
pub(in crate::app) struct RuleGroupDragPreview {
    pub(in crate::app) dragged_id: String,
    pub(in crate::app) order: Vec<String>,
    pub(in crate::app) slot_height: Pixels,
}

/// Interactive view-mode and hover/pin state shared by the desktop topology workspaces.
#[derive(Debug, Default)]
pub(in crate::app) struct TopologyState {
    pub(in crate::app) nodes_view: WorkspaceViewMode,
    pub(in crate::app) rules_view: WorkspaceViewMode,
    pub(in crate::app) node_source_filter: Option<String>,
    pub(in crate::app) hovered_nodes: Option<NodesTreeFocus>,
    pub(in crate::app) pinned_nodes: Option<NodesTreeFocus>,
    pub(in crate::app) hovered_rules: Option<RulesTreeFocus>,
    pub(in crate::app) pinned_rules: Option<RulesTreeFocus>,
    pub(in crate::app) nodes_scroll: gpui::ScrollHandle,
    pub(in crate::app) rules_scroll: gpui::ScrollHandle,
    pub(in crate::app) rule_group_drag_preview: Option<RuleGroupDragPreview>,
    pub(in crate::app) rule_group_row_bounds: PortRegistry,
}

impl TopologyState {
    pub(in crate::app) fn active_nodes_focus(&self) -> Option<&NodesTreeFocus> {
        self.hovered_nodes.as_ref().or(self.pinned_nodes.as_ref())
    }

    pub(in crate::app) fn active_rules_focus(&self) -> Option<&RulesTreeFocus> {
        self.hovered_rules.as_ref().or(self.pinned_rules.as_ref())
    }

    pub(in crate::app) fn view_for(
        &self,
        workspace: PrimaryWorkspace,
    ) -> Option<WorkspaceViewMode> {
        match workspace {
            PrimaryWorkspace::Nodes => Some(self.nodes_view),
            PrimaryWorkspace::RoutingRules => Some(self.rules_view),
            PrimaryWorkspace::Activity
            | PrimaryWorkspace::Logs
            | PrimaryWorkspace::Configuration => None,
        }
    }

    pub(in crate::app) fn set_view_for(
        &mut self,
        workspace: PrimaryWorkspace,
        mode: WorkspaceViewMode,
    ) -> bool {
        match workspace {
            PrimaryWorkspace::Nodes => {
                if self.nodes_view == mode {
                    return false;
                }
                self.nodes_view = mode;
                self.hovered_nodes = None;
                true
            }
            PrimaryWorkspace::RoutingRules => {
                if self.rules_view == mode {
                    return false;
                }
                self.rules_view = mode;
                self.hovered_rules = None;
                true
            }
            PrimaryWorkspace::Activity
            | PrimaryWorkspace::Logs
            | PrimaryWorkspace::Configuration => false,
        }
    }

    pub(in crate::app) fn toggle_view_for(
        &mut self,
        workspace: PrimaryWorkspace,
    ) -> Option<WorkspaceViewMode> {
        let current = self.view_for(workspace)?;
        let next = current.toggled();
        self.set_view_for(workspace, next);
        Some(next)
    }

    pub(in crate::app) fn clear_pins(&mut self) -> bool {
        let had = self.pinned_nodes.is_some() || self.pinned_rules.is_some();
        self.pinned_nodes = None;
        self.pinned_rules = None;
        had
    }
}

/// Frame-local registry mapping logical card keys to their laid-out window bounds.
///
/// GPUI runs every element's `prepaint` pass before any element's `paint` pass, so cards record
/// their exact laid-out rectangles during `prepaint` and the background Bézier canvas reads them
/// during `paint` with zero one-frame lag.
pub(super) type PortRegistry = Rc<RefCell<HashMap<String, Bounds<Pixels>>>>;

pub(super) fn new_port_registry() -> PortRegistry {
    Rc::new(RefCell::new(HashMap::new()))
}

pub(super) fn port_tracker(registry: PortRegistry, key: String) -> AnyElement {
    canvas(
        move |bounds, _, _| {
            registry.borrow_mut().insert(key, bounds);
        },
        |_, (), _, _| {},
    )
    .absolute()
    .inset_0()
    .into_any_element()
}

#[derive(Clone, Copy)]
pub(super) enum EdgeOrientation {
    /// Connects bottom-center of `from` to top-center of `to` (downward tier-to-tier).
    Vertical,
    /// Connects right-center of `from` to left-center of `to` (Rule -> Policy -> Exit flow).
    Horizontal,
    /// Connects peers in the same row with a smooth arch above (left->right) or below (right->left).
    SameRowArc,
    /// Connects a lower policy back up to an upper policy along the outer edge (cyclic mesh link).
    UpwardLoop,
}

pub(super) struct BezierEdge {
    pub(super) from_key: String,
    pub(super) to_key: String,
    pub(super) orientation: EdgeOrientation,
    pub(super) color: Rgba,
    pub(super) width: f32,
    pub(super) dashed: bool,
    pub(super) layer: u8,
}

pub(super) fn with_alpha(color: Rgba, alpha: f32) -> Rgba {
    Rgba {
        a: alpha.clamp(0.0, 1.0),
        ..color
    }
}

pub(super) fn paint_edges_canvas(registry: PortRegistry, mut edges: Vec<BezierEdge>) -> AnyElement {
    edges.sort_by_key(|edge| edge.layer);
    canvas(
        |_, _, _| (),
        move |_, (), window, _| {
            let ports = registry.borrow();
            for edge in &edges {
                let (Some(from), Some(to)) = (ports.get(&edge.from_key), ports.get(&edge.to_key))
                else {
                    continue;
                };
                let (p1, p2, c1, c2) = bezier_anchors(*from, *to, edge.orientation);
                let mut builder = PathBuilder::stroke(px(edge.width));
                if edge.dashed {
                    builder = builder.dash_array(&[px(5.0), px(4.0)]);
                }
                builder.move_to(p1);
                builder.cubic_bezier_to(p2, c1, c2);
                if let Ok(path) = builder.build() {
                    window.paint_path(path, edge.color);
                }
            }
        },
    )
    .absolute()
    .inset_0()
    .into_any_element()
}

fn bezier_anchors(
    from: Bounds<Pixels>,
    to: Bounds<Pixels>,
    orientation: EdgeOrientation,
) -> (Point<Pixels>, Point<Pixels>, Point<Pixels>, Point<Pixels>) {
    match orientation {
        EdgeOrientation::Vertical => {
            let p1 = point(from.center().x, from.bottom());
            let p2 = point(to.center().x, to.top());
            let dy = px(((p2.y - p1.y).as_f32() * 0.54).max(34.0));
            (p1, p2, point(p1.x, p1.y + dy), point(p2.x, p2.y - dy))
        }
        EdgeOrientation::Horizontal => {
            let p1 = point(from.right(), from.center().y);
            let p2 = point(to.left(), to.center().y);
            let dx = px(((p2.x - p1.x).as_f32() * 0.48).max(36.0));
            (p1, p2, point(p1.x + dx, p1.y), point(p2.x - dx, p2.y))
        }
        EdgeOrientation::SameRowArc => {
            let left_to_right = from.center().x <= to.center().x;
            let span = (to.center().x - from.center().x).as_f32().abs();
            let lift = px((26.0 + span * 0.08).min(58.0));
            if left_to_right {
                let p1 = point(from.center().x, from.top());
                let p2 = point(to.center().x, to.top());
                (p1, p2, point(p1.x, p1.y - lift), point(p2.x, p2.y - lift))
            } else {
                let p1 = point(from.center().x, from.bottom());
                let p2 = point(to.center().x, to.bottom());
                (p1, p2, point(p1.x, p1.y + lift), point(p2.x, p2.y + lift))
            }
        }
        EdgeOrientation::UpwardLoop => {
            let go_right = from.center().x <= to.center().x;
            let p1 = if go_right {
                point(from.right(), from.center().y)
            } else {
                point(from.left(), from.center().y)
            };
            let p2 = if go_right {
                point(to.right(), to.center().y)
            } else {
                point(to.left(), to.center().y)
            };
            let bulge = px(if go_right { 48.0 } else { -48.0 });
            (p1, p2, point(p1.x + bulge, p1.y), point(p2.x + bulge, p2.y))
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum PortSide {
    Top,
    Bottom,
    Left,
    Right,
}

/// Visual anchor dot mounted on the edge of a tree card.
pub(super) fn port_dot(side: PortSide, fill: Rgba, border: Rgba, emphasized: bool) -> Div {
    let diameter = if emphasized { 10.0 } else { 8.0 };
    let offset = -(diameter / 2.0);
    let dot = div()
        .absolute()
        .size(px(diameter))
        .rounded_full()
        .bg(fill)
        .border_2()
        .border_color(border);
    match side {
        PortSide::Top => dot.top(px(offset)).left(gpui::relative(0.5)).ml(px(offset)),
        PortSide::Bottom => dot
            .bottom(px(offset))
            .left(gpui::relative(0.5))
            .ml(px(offset)),
        PortSide::Left => dot.left(px(offset)).top(gpui::relative(0.5)).mt(px(offset)),
        PortSide::Right => dot
            .right(px(offset))
            .top(gpui::relative(0.5))
            .mt(px(offset)),
    }
}

/// Small solid line swatch used in the inspector legend.
pub(super) fn legend_entry(color: Rgba, label: &'static str, text_color: Rgba) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(6.0))
        .flex_shrink_0()
        .child(div().w(px(16.0)).h(px(2.5)).rounded_full().bg(color))
        .child(
            div()
                .text_size(px(11.5))
                .line_height(px(16.0))
                .text_color(text_color)
                .child(label),
        )
}

/// Short protocol tag that fits cleanly inside compact node tiles.
pub(super) fn compact_protocol_label(protocol: &str) -> String {
    let trimmed = protocol.trim();
    if trimmed.eq_ignore_ascii_case("hysteria2") {
        "Hy2".to_owned()
    } else if trimmed.eq_ignore_ascii_case("shadowsocks") {
        "SS".to_owned()
    } else if trimmed.eq_ignore_ascii_case("wireguard") {
        "WG".to_owned()
    } else if trimmed.eq_ignore_ascii_case("reality") {
        "REAL".to_owned()
    } else if trimmed.len() > 6 {
        trimmed.chars().take(6).collect()
    } else {
        trimmed.to_owned()
    }
}

#[cfg(all(test, not(windows)))]
mod tests {
    use gpui::{AppContext as _, Modifiers};
    use manis_core::{ManagedPolicyGroup, ManagedPolicyStrategy, PrimaryWorkspace};

    use super::{NodesTreeFocus, WorkspaceViewMode};
    use crate::{
        app::ManisApp,
        manual_rule::{ManualRule, ManualRuleKind},
        mihomo::{LoadedProvider, LoadedProviderNode, LocalRoutingRuleGroup},
    };

    fn tree_fixture() -> ManisApp {
        let mut app = ManisApp::with_fixture_controller("http://127.0.0.1:1");
        app.source_providers = vec![LoadedProvider {
            name: "CloudEdge".to_owned(),
            vehicle_type: Some("HTTP".to_owned()),
            nodes: vec![
                LoadedProviderNode {
                    name: "香港 HK-01".to_owned(),
                    protocol: "Hysteria2".to_owned(),
                    latency_label: Some("38 ms".to_owned()),
                    alive: Some(true),
                },
                LoadedProviderNode {
                    name: "新加坡 SG-02".to_owned(),
                    protocol: "VLESS".to_owned(),
                    latency_label: Some("54 ms".to_owned()),
                    alive: Some(true),
                },
                LoadedProviderNode {
                    name: "日本 JP-03".to_owned(),
                    protocol: "Trojan".to_owned(),
                    latency_label: Some("67 ms".to_owned()),
                    alive: Some(true),
                },
            ],
        }];
        let mut g1 = ManagedPolicyGroup::new("ai", "AI 自动选择").expect("policy");
        g1.strategy = ManagedPolicyStrategy::Manual;
        let mut g2 = ManagedPolicyGroup::new("media", "流媒体与视频").expect("policy");
        g2.strategy = ManagedPolicyStrategy::LowestLatency;
        app.managed_policies.groups = vec![g1, g2];
        app.managed_policies
            .node_selections
            .set_policy_target("AI 自动选择", "新加坡 SG-02")
            .expect("selection");
        app.managed_policies
            .node_selections
            .set_policy_target("流媒体与视频", "香港 HK-01")
            .expect("selection");
        app.local_rule_groups = vec![LocalRoutingRuleGroup {
            id: "ai-rules".to_owned(),
            name: "AI 与模型接口".to_owned(),
            target_policy: manis_profile::Name::parse("AI 自动选择").expect("target"),
            enabled: true,
            rules: vec![
                ManualRule::parse(ManualRuleKind::HostSuffix, "openai.com", "AI 自动选择")
                    .expect("rule"),
            ],
        }];
        app.manual_rules =
            vec![ManualRule::parse(ManualRuleKind::GeoIp, "CN", "DIRECT").expect("direct rule")];
        app
    }

    #[gpui::test]
    fn nodes_tree_renders_two_tiers_switches_views_and_pins_policy_exit(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(crate::init);
        let mut app = None;
        let (_, cx) = cx.add_window_view(|window, cx| {
            let entity = cx.new(|_| tree_fixture());
            app = Some(entity.clone());
            crate::root(entity, window, cx)
        });
        let app = app.expect("fixture app");
        cx.update(|window, cx| window.draw(cx).clear(cx));

        // Default view is the interactive policy-to-node tree.
        assert!(cx.debug_bounds("workspace-view-switch").is_some());
        let policy_bounds = cx
            .debug_bounds("tree-policy-0")
            .expect("policy card in Tier 01");
        let node_bounds = cx
            .debug_bounds("tree-node-0")
            .expect("node card in Tier 02");
        assert!(
            policy_bounds.bottom() + gpui::px(120.0) <= node_bounds.origin.y,
            "unobstructed Bézier bay separates Tier 01 policies from Tier 02 nodes"
        );

        // Pin policy 0 ("AI 自动选择"), then click node 0 ("香港 HK-01") to switch its exit.
        cx.simulate_click(policy_bounds.center(), Modifiers::none());
        cx.update(|window, cx| window.draw(cx).clear(cx));
        app.read_with(cx, |app, _| {
            assert_eq!(
                app.topology.pinned_nodes,
                Some(NodesTreeFocus::Policy("ai".to_owned()))
            );
        });
        let node_click_target = cx.debug_bounds("tree-node-0").expect("node 0").center();
        cx.simulate_click(node_click_target, Modifiers::none());
        cx.update(|window, cx| window.draw(cx).clear(cx));
        app.read_with(cx, |app, _| {
            assert_eq!(
                app.managed_policies
                    .node_selections
                    .policy_target("AI 自动选择"),
                Some("香港 HK-01")
            );
        });

        // One-click icon switches to preserved List view and back to Tree view.
        let list_btn = cx
            .debug_bounds("view-mode-list")
            .expect("list view icon button");
        cx.simulate_click(list_btn.center(), Modifiers::none());
        cx.update(|window, cx| window.draw(cx).clear(cx));
        app.read_with(cx, |app, _| {
            assert_eq!(app.topology.nodes_view, WorkspaceViewMode::List);
        });
        assert!(cx.debug_bounds("node-policies-section").is_some());
        assert!(cx.debug_bounds("nodes-tree-canvas").is_none());

        let tree_btn = cx
            .debug_bounds("view-mode-tree")
            .expect("tree view icon button");
        cx.simulate_click(tree_btn.center(), Modifiers::none());
        cx.update(|window, cx| window.draw(cx).clear(cx));
        app.read_with(cx, |app, _| {
            assert_eq!(app.topology.nodes_view, WorkspaceViewMode::Tree);
        });
        assert!(cx.debug_bounds("nodes-tree-canvas").is_some());
    }

    #[gpui::test]
    fn rules_tree_renders_three_columns_and_toggles_back_to_pipeline(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(crate::init);
        let mut app = None;
        let (_, cx) = cx.add_window_view(|window, cx| {
            let entity = cx.new(|_| {
                let mut fixture = tree_fixture();
                fixture.primary_workspace = PrimaryWorkspace::RoutingRules;
                fixture
            });
            app = Some(entity.clone());
            crate::root(entity, window, cx)
        });
        let app = app.expect("fixture app");
        cx.update(|window, cx| window.draw(cx).clear(cx));

        let rule_bounds = cx
            .debug_bounds("tree-rule-0")
            .expect("rule card in Column 1");
        let policy_bounds = cx
            .debug_bounds("tree-rule-policy-0")
            .expect("policy card in Column 2");
        let exit_bounds = cx
            .debug_bounds("tree-rule-exit-0")
            .expect("active exit card in Column 3");
        assert!(
            rule_bounds.right() < policy_bounds.origin.x
                && policy_bounds.right() < exit_bounds.origin.x,
            "rules flow strictly left-to-right across three columns"
        );

        let list_btn = cx
            .debug_bounds("view-mode-list")
            .expect("list view icon button");
        cx.simulate_click(list_btn.center(), Modifiers::none());
        cx.update(|window, cx| window.draw(cx).clear(cx));
        app.read_with(cx, |app, _| {
            assert_eq!(app.topology.rules_view, WorkspaceViewMode::List);
        });
        assert!(cx.debug_bounds("routing-rule-fallback").is_some());
        assert!(cx.debug_bounds("rules-tree-canvas").is_none());
    }

    #[gpui::test]
    #[expect(
        clippy::too_many_lines,
        reason = "Verifies horizontal fit, canvas containment, and vertical scrolling for both dense nodes and tall rules trees"
    )]
    fn dense_54_nodes_and_15_manual_rules_fit_without_horizontal_overflow(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(crate::init);
        let mut app = None;
        let (_, cx) = cx.add_window_view(|window, cx| {
            let entity = cx.new(|_| {
                let mut fixture = ManisApp::with_fixture_controller("http://127.0.0.1:1");
                fixture.source_providers = vec![LoadedProvider {
                    name: "CloudEdge".to_owned(),
                    vehicle_type: Some("HTTP".to_owned()),
                    nodes: (1..=54)
                        .map(|index| LoadedProviderNode {
                            name: format!("HK {index:02}"),
                            protocol: "Trojan".to_owned(),
                            latency_label: Some(format!("{} ms", 40 + (index % 80))),
                            alive: Some(true),
                        })
                        .collect(),
                }];
                fixture.managed_policies.groups = ["Sozenth", "机场专线", "香港", "JP", "US"]
                    .into_iter()
                    .enumerate()
                    .map(|(idx, name)| {
                        ManagedPolicyGroup::new(&format!("p{idx}"), name).expect("policy")
                    })
                    .collect();
                fixture.manual_rules = (0..15)
                    .map(|index| {
                        let target = match index % 3 {
                            0 => "DIRECT",
                            1 => "香港",
                            _ => "JP",
                        };
                        ManualRule::parse(
                            ManualRuleKind::HostSuffix,
                            &format!("service-{index}.example.com"),
                            target,
                        )
                        .expect("manual rule")
                    })
                    .collect();
                fixture
            });
            app = Some(entity.clone());
            crate::root(entity, window, cx)
        });
        let app = app.expect("fixture app");
        cx.update(|window, cx| window.draw(cx).clear(cx));

        let canvas = cx
            .debug_bounds("nodes-tree-canvas")
            .expect("nodes tree canvas");
        let last_policy = cx
            .debug_bounds("tree-policy-4")
            .expect("5th policy card visible");
        let last_node = cx
            .debug_bounds("tree-node-53")
            .expect("54th node chip visible");
        assert!(
            last_policy.right() <= canvas.right(),
            "all 5 policy groups fit within the viewport width"
        );
        assert!(
            last_node.right() <= canvas.right(),
            "all 54 dense node chips wrap inside the canvas width"
        );
        assert!(
            last_node.bottom() <= canvas.bottom(),
            "nodes-tree-canvas must expand to enclose all 54 nodes instead of clipping early"
        );
        let nodes_max_scroll = app.read_with(cx, |app, _| app.topology.nodes_scroll.max_offset());
        let nodes_scroll_bounds = cx
            .debug_bounds("nodes-tree-scroll")
            .expect("nodes tree scroll viewport");
        if canvas.size.height > nodes_scroll_bounds.size.height {
            assert!(
                nodes_max_scroll.y > gpui::px(0.0),
                "nodes-tree-scroll must be scrollable when canvas exceeds viewport height"
            );
        }

        // Switch to RoutingRules: 15 manual rules cluster into 3 target groups + FINAL = 4 cards.
        cx.update(|window, cx| {
            app.update(cx, |app, cx| {
                app.primary_workspace = PrimaryWorkspace::RoutingRules;
                cx.notify();
            });
            window.draw(cx).clear(cx);
        });
        assert!(cx.debug_bounds("tree-rule-0").is_some());
        let final_rule = cx.debug_bounds("tree-rule-3").expect("FINAL rule visible");
        let rules_canvas = cx
            .debug_bounds("rules-tree-canvas")
            .expect("rules tree canvas");
        assert!(
            final_rule.bottom() <= rules_canvas.bottom(),
            "rules-tree-canvas must enclose FINAL without overflowing"
        );
        assert!(
            cx.debug_bounds("tree-rule-4").is_none(),
            "15 manual rules across 3 targets cluster into 3 cards plus FINAL"
        );

        // Add 12 local rule groups so the rules tree exceeds a standard desktop window height and
        // verify rules-tree-canvas still encloses FINAL while rules-tree-scroll scrolls vertically.
        cx.update(|window, cx| {
            app.update(cx, |app, cx| {
                app.local_rule_groups = (0..12)
                    .map(|idx| LocalRoutingRuleGroup {
                        id: format!("local-{idx}"),
                        name: format!("Rule Group {idx:02}"),
                        target_policy: manis_profile::Name::parse("香港").expect("target"),
                        enabled: true,
                        rules: vec![
                            ManualRule::parse(
                                ManualRuleKind::HostSuffix,
                                &format!("group-{idx}.example.com"),
                                "香港",
                            )
                            .expect("local group rule"),
                        ],
                    })
                    .collect();
                cx.notify();
            });
            window.draw(cx).clear(cx);
        });
        let tall_rules_canvas = cx
            .debug_bounds("rules-tree-canvas")
            .expect("tall rules tree canvas");
        let tall_final_rule = cx
            .debug_bounds("tree-rule-15")
            .expect("FINAL card after 12 local groups + 3 manual clusters");
        assert!(
            tall_final_rule.bottom() <= tall_rules_canvas.bottom(),
            "tall rules-tree-canvas must wrap every rule card down to FINAL"
        );
        let rules_max_scroll = app.read_with(cx, |app, _| app.topology.rules_scroll.max_offset());
        let rules_scroll_bounds = cx
            .debug_bounds("rules-tree-scroll")
            .expect("rules tree scroll viewport");
        if tall_rules_canvas.size.height > rules_scroll_bounds.size.height {
            assert!(
                rules_max_scroll.y > gpui::px(0.0),
                "rules-tree-scroll must be scrollable when rules exceed viewport height"
            );
        }
    }

    #[gpui::test]
    #[expect(
        clippy::too_many_lines,
        reason = "Constructs a realistic nested and cyclic policy catalog fixture"
    )]
    fn nested_and_mesh_policy_groups_layer_without_duplicating_into_nodes_tier(
        cx: &mut gpui::TestAppContext,
    ) {
        use manis_core::{
            PolicyCandidateKind, PolicyCatalog, PolicyGroup, PolicyGroupId, PolicyGroupKind,
            PolicyNode, ProxyId,
        };

        cx.update(crate::init);
        let (_, cx) = cx.add_window_view(|window, cx| {
            let entity = cx.new(|_| {
                let mut fixture = ManisApp::with_fixture_controller("http://127.0.0.1:1");
                fixture.source_providers = vec![LoadedProvider {
                    name: "CloudEdge".to_owned(),
                    vehicle_type: Some("HTTP".to_owned()),
                    nodes: vec![
                        LoadedProviderNode {
                            name: "HK 01".to_owned(),
                            protocol: "Trojan".to_owned(),
                            latency_label: Some("42 ms".to_owned()),
                            alive: Some(true),
                        },
                        LoadedProviderNode {
                            name: "JP 01".to_owned(),
                            protocol: "Trojan".to_owned(),
                            latency_label: Some("68 ms".to_owned()),
                            alive: Some(true),
                        },
                    ],
                }];
                let make_node =
                    |id: &str, name: &str, kind: PolicyCandidateKind, detail: &str, ms: u16| {
                        PolicyNode {
                            id: ProxyId::new(id),
                            name: name.to_owned(),
                            kind,
                            provider: None,
                            detail: detail.to_owned(),
                            latency_ms: Some(ms),
                            alive: Some(true),
                        }
                    };
                let groups = vec![
                    PolicyGroup {
                        id: PolicyGroupId::new("airport"),
                        name: "机场专线".to_owned(),
                        kind: PolicyGroupKind::Selector,
                        target: Some("香港".to_owned()),
                        nodes: vec![
                            make_node(
                                "hk",
                                "香港",
                                PolicyCandidateKind::PolicyGroup,
                                "Selector",
                                42,
                            ),
                            make_node("jp", "JP", PolicyCandidateKind::PolicyGroup, "URLTest", 68),
                            make_node("DIRECT", "DIRECT", PolicyCandidateKind::Node, "Direct", 0),
                        ],
                        rules: Vec::new(),
                        rules_total: 0,
                    },
                    PolicyGroup {
                        id: PolicyGroupId::new("hk"),
                        name: "香港".to_owned(),
                        kind: PolicyGroupKind::Selector,
                        target: Some("HK 01".to_owned()),
                        nodes: vec![
                            make_node("HK 01", "HK 01", PolicyCandidateKind::Node, "Trojan", 42),
                            make_node("jp", "JP", PolicyCandidateKind::PolicyGroup, "URLTest", 68),
                        ],
                        rules: Vec::new(),
                        rules_total: 0,
                    },
                    PolicyGroup {
                        id: PolicyGroupId::new("jp"),
                        name: "JP".to_owned(),
                        kind: PolicyGroupKind::UrlTest,
                        target: Some("JP 01".to_owned()),
                        nodes: vec![
                            make_node("JP 01", "JP 01", PolicyCandidateKind::Node, "Trojan", 68),
                            make_node(
                                "hk",
                                "香港",
                                PolicyCandidateKind::PolicyGroup,
                                "Selector",
                                42,
                            ),
                        ],
                        rules: Vec::new(),
                        rules_total: 0,
                    },
                ];
                fixture.catalog = Some(PolicyCatalog::try_new(groups).expect("nested catalog"));
                fixture
            });
            crate::root(entity, window, cx)
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));

        let airport = cx
            .debug_bounds("tree-policy-0")
            .expect("composite policy 机场专线 renders");
        let hk_policy = cx
            .debug_bounds("tree-policy-1")
            .expect("child policy 香港 renders");
        let jp_policy = cx
            .debug_bounds("tree-policy-2")
            .expect("child policy JP renders");
        assert!(
            airport.bottom() < hk_policy.top() && airport.bottom() < jp_policy.top(),
            "composite policy group layers above its child policy groups"
        );
        // Only physical/leaf exits (HK 01, JP 01, DIRECT) appear in the nodes tier — never 香港 or JP.
        assert!(cx.debug_bounds("tree-node-0").is_some());
        assert!(cx.debug_bounds("tree-node-1").is_some());
        assert!(cx.debug_bounds("tree-node-2").is_some());
        assert!(
            cx.debug_bounds("tree-node-3").is_none(),
            "referenced policy groups must not be duplicated into the nodes tier"
        );
    }
}
