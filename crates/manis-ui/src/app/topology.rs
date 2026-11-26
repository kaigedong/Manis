use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    AnyElement, Bounds, ContentMask, Context, Div, ElementId, FontWeight, InteractiveElement,
    IntoElement, ParentElement, PathBuilder, Pixels, Point, Rgba, SharedString,
    StatefulInteractiveElement, Styled, Task, canvas, div, point, px,
};
use manis_core::{NodeIdentity, PrimaryWorkspace};

use super::ManisApp;
use crate::theme::{Radius, Theme};

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

/// Add/remove operation armed via the left-edge `+` / `-` button on a downstream tree card.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::app) enum PolicyMemberEditOp {
    Add,
    Remove,
}

/// Armed two-step confirmation for adding or removing a candidate from a pinned policy group.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::app) struct PendingPolicyMemberEdit {
    pub(in crate::app) parent_policy_id: String,
    pub(in crate::app) parent_policy_name: String,
    pub(in crate::app) target_key: String,
    pub(in crate::app) target_label: String,
    pub(in crate::app) member_identity: NodeIdentity,
    pub(in crate::app) op: PolicyMemberEditOp,
    pub(in crate::app) animation_nonce: u64,
}

/// Interactive view-mode and hover/pin state shared by the desktop topology workspaces.
#[derive(Default)]
pub(in crate::app) struct TopologyState {
    pub(in crate::app) nodes_view: WorkspaceViewMode,
    pub(in crate::app) rules_view: WorkspaceViewMode,
    pub(in crate::app) node_source_filter: Option<String>,
    pub(in crate::app) hovered_nodes: Option<NodesTreeFocus>,
    pub(in crate::app) pinned_nodes: Option<NodesTreeFocus>,
    pub(in crate::app) hovered_member_target: Option<String>,
    pub(in crate::app) pending_member_edit: Option<PendingPolicyMemberEdit>,
    pub(in crate::app) member_edit_nonce: u64,
    pub(in crate::app) hovered_rules: Option<RulesTreeFocus>,
    pub(in crate::app) pinned_rules: Option<RulesTreeFocus>,
    pub(in crate::app) nodes_policy_col_scrolls: RefCell<Vec<gpui::ScrollHandle>>,
    pub(in crate::app) nodes_leaf_col_scroll: gpui::ScrollHandle,
    pub(in crate::app) nodes_horizontal_scroll: gpui::ScrollHandle,
    pub(in crate::app) rules_col_scrolls: [gpui::ScrollHandle; 3],
    nodes_layout: RefCell<LayoutRegistry>,
    rules_layout: RefCell<LayoutRegistry>,
    auto_scroll_generation: u64,
    auto_scroll_task: Option<Task<()>>,
    pub(in crate::app) rule_group_drag_preview: Option<RuleGroupDragPreview>,
    pub(in crate::app) rule_group_row_bounds: PortRegistry,
}

impl TopologyState {
    pub(in crate::app) fn nodes_policy_col_scroll(&self, layer_idx: usize) -> gpui::ScrollHandle {
        let mut scrolls = self.nodes_policy_col_scrolls.borrow_mut();
        while scrolls.len() <= layer_idx {
            scrolls.push(gpui::ScrollHandle::new());
        }
        scrolls[layer_idx].clone()
    }

    pub(super) fn replace_nodes_layout(&self, layout: LayoutRegistry) {
        *self.nodes_layout.borrow_mut() = layout;
    }

    pub(super) fn nodes_layout(&self) -> LayoutRegistry {
        self.nodes_layout.borrow().clone()
    }

    pub(super) fn replace_rules_layout(&self, layout: LayoutRegistry) {
        *self.rules_layout.borrow_mut() = layout;
    }

    pub(super) fn rules_layout(&self) -> LayoutRegistry {
        self.rules_layout.borrow().clone()
    }

    pub(super) fn cancel_auto_scroll(&mut self) {
        self.auto_scroll_generation = self.auto_scroll_generation.wrapping_add(1);
        self.auto_scroll_task = None;
    }

    pub(in crate::app) fn active_nodes_focus(&self) -> Option<&NodesTreeFocus> {
        self.pinned_nodes.as_ref().or(self.hovered_nodes.as_ref())
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
                self.hovered_member_target = None;
                self.pending_member_edit = None;
                self.cancel_auto_scroll();
                true
            }
            PrimaryWorkspace::RoutingRules => {
                if self.rules_view == mode {
                    return false;
                }
                self.rules_view = mode;
                self.hovered_rules = None;
                self.cancel_auto_scroll();
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
        let had = self.pinned_nodes.is_some()
            || self.pinned_rules.is_some()
            || self.pending_member_edit.is_some();
        self.pinned_nodes = None;
        self.pinned_rules = None;
        self.hovered_member_target = None;
        self.pending_member_edit = None;
        self.cancel_auto_scroll();
        had
    }
}

/// Frame-local registry mapping logical row keys to their laid-out window bounds.
pub(super) type PortRegistry = Rc<RefCell<HashMap<String, Bounds<Pixels>>>>;

/// Frame-local topology layout tracking each card's window bounds alongside its owning column's
/// visible scroll viewport so Bézier edges stay anchored during per-column scrolling and smoothly
/// fade out when a card scrolls past its column viewport.
///
/// GPUI runs every element's `prepaint` pass before any element's `paint` pass, so columns and
/// cards record their exact laid-out rectangles during `prepaint` and the Bézier canvas reads them
/// during `paint` with zero one-frame lag.
#[derive(Default)]
pub(super) struct TopologyLayout {
    ports: HashMap<String, (Bounds<Pixels>, String)>,
    viewports: HashMap<String, Bounds<Pixels>>,
}

impl TopologyLayout {
    pub(super) fn viewport(&self, column_key: &str) -> Option<Bounds<Pixels>> {
        self.viewports.get(column_key).copied()
    }

    pub(super) fn port_bounds(&self, port_key: &str) -> Option<Bounds<Pixels>> {
        self.ports.get(port_key).map(|(bounds, _)| *bounds)
    }
}

pub(super) type LayoutRegistry = Rc<RefCell<TopologyLayout>>;

const AUTO_SCROLL_FRAME_MS: u16 = 16;
const AUTO_SCROLL_DWELL_MS: u16 = 200;
const AUTO_SCROLL_VIEWPORT_PAD: f32 = 16.0;

#[derive(Clone, Copy)]
struct ScrollSegment {
    from_y: f32,
    to_y: f32,
    duration_ms: u16,
}

/// Smooth auto-scroll trajectory for one overflowing tree column so connected items outside the
/// viewport glide smoothly into view (sweeping from the first connected item to the last when
/// their span exceeds one viewport height).
pub(super) struct ColumnAutoScrollTrack {
    scroll_handle: gpui::ScrollHandle,
    segments: Vec<ScrollSegment>,
    total_ms: u16,
    last_applied_y: Option<f32>,
    cancelled: bool,
}

impl ColumnAutoScrollTrack {
    fn sample(&self, elapsed_ms: u16) -> f32 {
        let mut cursor = 0u16;
        for segment in &self.segments {
            if segment.duration_ms == 0 {
                continue;
            }
            let end = cursor.saturating_add(segment.duration_ms);
            if elapsed_ms < end {
                let raw = f32::from(elapsed_ms - cursor) / f32::from(segment.duration_ms);
                let eased = ease_in_out_cubic(raw);
                return segment.from_y + (segment.to_y - segment.from_y) * eased;
            }
            cursor = end;
        }
        self.segments.last().map_or(0.0, |segment| segment.to_y)
    }
}

fn ease_in_out_cubic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
    }
}

fn scroll_duration_ms(distance: f32, min_ms: u16, max_ms: u16, px_per_sec: f32) -> u16 {
    let clamped_ms =
        ((distance.max(0.0) / px_per_sec) * 1000.0).clamp(f32::from(min_ms), f32::from(max_ms));
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "clamped_ms is bounded within [min_ms, max_ms] where both are u16"
    )]
    {
        clamped_ms.round() as u16
    }
}

pub(super) fn build_column_auto_scroll_track(
    scroll_handle: gpui::ScrollHandle,
    viewport: Bounds<Pixels>,
    active_bounds: &[Bounds<Pixels>],
) -> Option<ColumnAutoScrollTrack> {
    let max_y = scroll_handle.max_offset().y.as_f32();
    let vp_h = viewport.size.height.as_f32();
    if max_y <= 1.0 || vp_h <= 1.0 || active_bounds.is_empty() {
        return None;
    }

    let cur_offset_y = scroll_handle.offset().y.as_f32();
    let cur_y = (-cur_offset_y).clamp(0.0, max_y);

    let mut first_top = f32::INFINITY;
    let mut last_bottom = f32::NEG_INFINITY;
    for bounds in active_bounds {
        let top = (bounds.top() - viewport.top()).as_f32() - cur_offset_y;
        let bottom = (bounds.bottom() - viewport.top()).as_f32() - cur_offset_y;
        first_top = first_top.min(top);
        last_bottom = last_bottom.max(bottom);
    }
    if !first_top.is_finite() || !last_bottom.is_finite() {
        return None;
    }

    // All connected items are already completely inside the visible viewport.
    if first_top >= cur_y - 1.0 && last_bottom <= cur_y + vp_h + 1.0 {
        return None;
    }

    let first_y = (first_top - AUTO_SCROLL_VIEWPORT_PAD).clamp(0.0, max_y);
    let last_y = (last_bottom + AUTO_SCROLL_VIEWPORT_PAD - vp_h).clamp(0.0, max_y);

    let mut segments = Vec::new();
    if last_y > first_y + 1.0 {
        // Active items exceed a single viewport: glide to the first active item (if needed),
        // hold briefly so its connection registers, then sweep smoothly to the last active item.
        if (cur_y - first_y).abs() > 2.0 {
            segments.push(ScrollSegment {
                from_y: cur_y,
                to_y: first_y,
                duration_ms: scroll_duration_ms((first_y - cur_y).abs(), 220, 560, 1200.0),
            });
        }
        segments.push(ScrollSegment {
            from_y: first_y,
            to_y: first_y,
            duration_ms: AUTO_SCROLL_DWELL_MS,
        });
        segments.push(ScrollSegment {
            from_y: first_y,
            to_y: last_y,
            duration_ms: scroll_duration_ms(last_y - first_y, 700, 2400, 480.0),
        });
    } else {
        // All active items fit within a single viewport, but at least one is currently off-screen:
        // glide smoothly so the active span sits centered in the column viewport.
        let target_y = (f32::midpoint(first_top, last_bottom) - vp_h * 0.5).clamp(0.0, max_y);
        if (target_y - cur_y).abs() <= 1.0 {
            return None;
        }
        segments.push(ScrollSegment {
            from_y: cur_y,
            to_y: target_y,
            duration_ms: scroll_duration_ms((target_y - cur_y).abs(), 260, 680, 950.0),
        });
    }

    let total_ms = segments
        .iter()
        .fold(0u16, |acc, seg| acc.saturating_add(seg.duration_ms));
    (total_ms > 0).then_some(ColumnAutoScrollTrack {
        scroll_handle,
        segments,
        total_ms,
        last_applied_y: Some(cur_y),
        cancelled: false,
    })
}

impl ManisApp {
    pub(super) fn start_topology_auto_scroll(
        &mut self,
        mut tracks: Vec<ColumnAutoScrollTrack>,
        cx: &mut Context<Self>,
    ) {
        self.topology.cancel_auto_scroll();
        if tracks.is_empty() {
            return;
        }
        let generation = self.topology.auto_scroll_generation;
        let max_total_ms = tracks.iter().map(|track| track.total_ms).max().unwrap_or(0);
        if max_total_ms == 0 {
            return;
        }

        let task = cx.spawn(async move |this, cx| {
            let mut elapsed_ms = 0u16;
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(u64::from(AUTO_SCROLL_FRAME_MS)))
                    .await;
                elapsed_ms = elapsed_ms.saturating_add(AUTO_SCROLL_FRAME_MS);

                let Some(entity) = this.upgrade() else {
                    break;
                };
                let keep_going = entity.update(cx, |this, cx| {
                    if this.topology.auto_scroll_generation != generation {
                        return false;
                    }
                    let mut any_changed = false;
                    let mut any_active = false;
                    for track in &mut tracks {
                        if track.cancelled {
                            continue;
                        }
                        let current_max = track.scroll_handle.max_offset().y.as_f32().max(0.0);
                        let current_offset = track.scroll_handle.offset();
                        let actual_y = -current_offset.y.as_f32();
                        if let Some(expected_y) = track.last_applied_y {
                            let clamped_expected = expected_y.clamp(0.0, current_max);
                            if (actual_y - clamped_expected).abs() > 1.5 {
                                // Yield immediately when the user manually scrolls this column.
                                track.cancelled = true;
                                continue;
                            }
                        }

                        let next_y = track.sample(elapsed_ms).clamp(0.0, current_max);
                        if (actual_y - next_y).abs() > 0.25 {
                            track
                                .scroll_handle
                                .set_offset(point(current_offset.x, px(-next_y)));
                            any_changed = true;
                        }
                        track.last_applied_y = Some(next_y);
                        if elapsed_ms < track.total_ms {
                            any_active = true;
                        }
                    }

                    if any_changed {
                        cx.notify();
                    }
                    if !any_active || elapsed_ms >= max_total_ms {
                        if this.topology.auto_scroll_generation == generation {
                            this.topology.auto_scroll_task = None;
                        }
                        return false;
                    }
                    true
                });

                if !keep_going {
                    break;
                }
            }
        });
        self.topology.auto_scroll_task = Some(task);
    }
}

pub(super) fn new_layout_registry() -> LayoutRegistry {
    Rc::new(RefCell::new(TopologyLayout::default()))
}

pub(super) fn column_viewport_tracker(registry: LayoutRegistry, column_key: String) -> AnyElement {
    canvas(
        move |bounds, _, _| {
            registry.borrow_mut().viewports.insert(column_key, bounds);
        },
        |_, (), _, _| {},
    )
    .absolute()
    .inset_0()
    .into_any_element()
}

pub(super) fn card_port_tracker(
    registry: LayoutRegistry,
    port_key: String,
    column_key: String,
) -> AnyElement {
    canvas(
        move |bounds, _, _| {
            registry
                .borrow_mut()
                .ports
                .insert(port_key, (bounds, column_key));
        },
        |_, (), _, _| {},
    )
    .absolute()
    .inset_0()
    .into_any_element()
}

#[derive(Clone, Copy)]
pub(super) enum EdgeOrientation {
    /// Connects right-center of `from` to left-center of `to` (left-to-right tree flow).
    Horizontal,
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

fn endpoint_visibility(y: Pixels, viewport: Bounds<Pixels>) -> f32 {
    let top = viewport.top().as_f32();
    let bottom = viewport.bottom().as_f32();
    let y = y.as_f32();
    if y <= top || y >= bottom {
        return 0.0;
    }
    let dist = (y - top).min(bottom - y);
    (dist / 10.0).clamp(0.0, 1.0)
}

pub(super) fn paint_edges_canvas(
    registry: LayoutRegistry,
    mut edges: Vec<BezierEdge>,
) -> AnyElement {
    edges.sort_by_key(|edge| edge.layer);
    canvas(
        |_, _, _| (),
        move |_, (), window, _| {
            let layout = registry.borrow();
            let clip_mask = layout
                .viewports
                .values()
                .copied()
                .reduce(|acc, next| acc.union(&next))
                .map(|bounds| ContentMask { bounds });
            window.with_content_mask(clip_mask, |window| {
                for edge in &edges {
                    let (Some((from, from_col)), Some((to, to_col))) = (
                        layout.ports.get(&edge.from_key),
                        layout.ports.get(&edge.to_key),
                    ) else {
                        continue;
                    };
                    let (p1, p2, c1, c2) = bezier_anchors(*from, *to, edge.orientation);
                    let vis_from = layout
                        .viewports
                        .get(from_col)
                        .map_or(1.0, |vp| endpoint_visibility(p1.y, *vp));
                    let vis_to = layout
                        .viewports
                        .get(to_col)
                        .map_or(1.0, |vp| endpoint_visibility(p2.y, *vp));
                    let visibility = vis_from.min(vis_to);
                    if visibility <= 0.01 {
                        continue;
                    }
                    let mut builder = PathBuilder::stroke(px(edge.width));
                    if edge.dashed {
                        builder = builder.dash_array(&[px(5.0), px(4.0)]);
                    }
                    builder.move_to(p1);
                    builder.cubic_bezier_to(p2, c1, c2);
                    if let Ok(path) = builder.build() {
                        window.paint_path(path, with_alpha(edge.color, edge.color.a * visibility));
                    }
                }
            });
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
        EdgeOrientation::Horizontal => {
            let p1 = point(from.right(), from.center().y);
            let p2 = point(to.left(), to.center().y);
            let dx = px(((p2.x - p1.x).as_f32() * 0.48).max(32.0));
            (p1, p2, point(p1.x + dx, p1.y), point(p2.x - dx, p2.y))
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum PortSide {
    Left,
    Right,
}

/// Visual anchor dot mounted on the left or right edge of a horizontal tree card.
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
        PortSide::Left => dot.left(px(offset)).top(gpui::relative(0.5)).mt(px(offset)),
        PortSide::Right => dot
            .right(px(offset))
            .top(gpui::relative(0.5))
            .mt(px(offset)),
    }
}

/// Colored column-tier badge rendered in each horizontal tree column header.
pub(super) fn col_tag(label: impl Into<SharedString>, fg: Rgba, bg: Rgba) -> Div {
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
        .child(label.into())
}

pub(super) const TREE_COL_LEFT_PAD: f32 = 12.0;
pub(super) const TREE_COL_RIGHT_PAD: f32 = 8.0;

/// A full-height horizontal tree column with a pinned header and an independently scrollable
/// vertical card list (without scrollbar chrome) whose padding preserves left/right ports and
/// left-edge membership `+`/`-` badges.
#[expect(
    clippy::too_many_arguments,
    reason = "Shared independently-scrollable tree column shell"
)]
pub(super) fn scrollable_tree_column(
    scroll_id: impl Into<ElementId>,
    debug_selector: String,
    column_key: String,
    header: AnyElement,
    cards: Vec<AnyElement>,
    card_gap: Pixels,
    scroll_handle: &gpui::ScrollHandle,
    registry: LayoutRegistry,
    min_width: Option<Pixels>,
    card_horizontal_padding: bool,
    theme: Theme,
) -> Div {
    let left_pad = px(TREE_COL_LEFT_PAD);
    let right_pad = px(TREE_COL_RIGHT_PAD);
    div()
        .relative()
        .flex_1()
        .min_w(min_width.unwrap_or(px(0.0)))
        .h_full()
        .min_h_0()
        .flex()
        .flex_col()
        .child(
            div()
                .flex_shrink_0()
                .pl(left_pad)
                .pr(right_pad)
                .pb(px(8.0))
                .border_b_1()
                .border_color(theme.outline_subtle)
                .flex()
                .flex_col()
                .child(header),
        )
        .child(
            div()
                .relative()
                .flex_1()
                .min_h_0()
                .w_full()
                .child(column_viewport_tracker(registry, column_key))
                .child(
                    div()
                        .id(scroll_id)
                        .debug_selector(move || debug_selector)
                        .size_full()
                        .overflow_y_scroll()
                        .track_scroll(scroll_handle)
                        .child(
                            div()
                                .w_full()
                                .pl(if card_horizontal_padding {
                                    left_pad
                                } else {
                                    px(0.0)
                                })
                                .pr(if card_horizontal_padding {
                                    right_pad
                                } else {
                                    px(0.0)
                                })
                                .py(px(6.0))
                                .flex()
                                .flex_col()
                                .gap(card_gap)
                                .children(cards),
                        ),
                ),
        )
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
            .expect("policy card in Column 1");
        let node_bounds = cx
            .debug_bounds("tree-node-0")
            .expect("node card in Column 2");
        assert!(
            policy_bounds.right() + gpui::px(32.0) <= node_bounds.origin.x,
            "unobstructed horizontal Bézier bay separates Policy column from Nodes column"
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
        let policy0_before = cx
            .debug_bounds("tree-policy-0")
            .expect("1st policy card visible");
        let last_policy = cx
            .debug_bounds("tree-policy-4")
            .expect("5th policy card visible");
        let node0_before = cx
            .debug_bounds("tree-node-0")
            .expect("1st node card visible");
        let last_node = cx
            .debug_bounds("tree-node-53")
            .expect("54th node card laid out");
        assert!(
            last_policy.right() <= canvas.right(),
            "all 5 policy groups fit within the canvas width"
        );
        assert!(
            last_policy.right() < node0_before.origin.x,
            "policies column sits to the left of the nodes column"
        );
        assert!(
            last_node.right() <= canvas.right(),
            "all 54 node cards fit within the nodes column width"
        );
        let nodes_col_max =
            app.read_with(cx, |app, _| app.topology.nodes_leaf_col_scroll.max_offset());
        assert!(
            nodes_col_max.y > gpui::px(0.0),
            "54-node column must scroll vertically on its own"
        );
        // Mouse-wheel scrolling over the nodes column must move only node cards and leave the
        // policy column completely stationary.
        cx.simulate_event(gpui::ScrollEvent {
            position: node0_before.center(),
            delta: gpui::ScrollDelta::Pixels(gpui::point(gpui::px(0.0), gpui::px(-180.0))),
            ..Default::default()
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let policy0_after = cx
            .debug_bounds("tree-policy-0")
            .expect("1st policy card after nodes scroll");
        let node0_after = cx
            .debug_bounds("tree-node-0")
            .expect("1st node card after nodes scroll");
        assert!(
            node0_after.origin.y < policy0_before.origin.y
                && node0_after.origin.y < node0_before.origin.y,
            "scrolling the nodes column moves node cards vertically"
        );
        assert_eq!(
            policy0_after.origin.y, policy0_before.origin.y,
            "scrolling the nodes column must not move the policy column"
        );

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
            "4 clustered rule cards fit comfortably inside rules-tree-canvas"
        );
        assert!(
            cx.debug_bounds("tree-rule-4").is_none(),
            "15 manual rules across 3 targets cluster into 3 cards plus FINAL"
        );

        // Add 12 local rule groups so Column 1 exceeds viewport height and verify scrolling
        // Column 1 moves only rule cards while Columns 2 and 3 remain stationary.
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
        let rule0_before = cx
            .debug_bounds("tree-rule-0")
            .expect("rule 0 before scroll");
        let rule_policy0_before = cx
            .debug_bounds("tree-rule-policy-0")
            .expect("rule policy 0 before scroll");
        let rule_exit0_before = cx
            .debug_bounds("tree-rule-exit-0")
            .expect("rule exit 0 before scroll");
        assert!(
            cx.debug_bounds("tree-rule-15").is_some(),
            "FINAL card after 12 local groups + 3 manual clusters is laid out"
        );
        let rules_col0_max =
            app.read_with(cx, |app, _| app.topology.rules_col_scrolls[0].max_offset());
        assert!(
            rules_col0_max.y > gpui::px(0.0),
            "rules column must be independently scrollable when rule cards exceed viewport height"
        );
        cx.simulate_event(gpui::ScrollEvent {
            position: rule0_before.center(),
            delta: gpui::ScrollDelta::Pixels(gpui::point(gpui::px(0.0), gpui::px(-160.0))),
            ..Default::default()
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let rule0_after = cx.debug_bounds("tree-rule-0").expect("rule 0 after scroll");
        let rule_policy0_after = cx
            .debug_bounds("tree-rule-policy-0")
            .expect("rule policy 0 after scroll");
        let rule_exit0_after = cx
            .debug_bounds("tree-rule-exit-0")
            .expect("rule exit 0 after scroll");
        assert!(
            rule0_after.origin.y < rule0_before.origin.y,
            "scrolling the rules column moves rule cards vertically"
        );
        assert_eq!(
            rule_policy0_after.origin.y, rule_policy0_before.origin.y,
            "scrolling the rules column must not move the policy groups column"
        );
        assert_eq!(
            rule_exit0_after.origin.y, rule_exit0_before.origin.y,
            "scrolling the rules column must not move the active exits column"
        );
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
        let node0 = cx
            .debug_bounds("tree-node-0")
            .expect("leaf node renders in rightmost column");
        assert!(
            airport.right() < hk_policy.left() && airport.right() < jp_policy.left(),
            "composite policy group sits in a column to the left of its child policy groups"
        );
        assert!(
            hk_policy.right() < node0.left() && jp_policy.right() < node0.left(),
            "child policy groups sit to the left of the physical nodes column"
        );
        // Only physical/leaf exits (HK 01, JP 01, DIRECT) appear in the nodes tier — never 香港 or JP.
        assert!(cx.debug_bounds("tree-node-1").is_some());
        assert!(cx.debug_bounds("tree-node-2").is_some());
        assert!(
            cx.debug_bounds("tree-node-3").is_none(),
            "referenced policy groups must not be duplicated into the nodes tier"
        );
    }

    #[gpui::test]
    #[expect(
        clippy::too_many_lines,
        reason = "Verifies pinned tier-1 hover +/-, two-step confirmation, cancel, commit, and acyclic guard"
    )]
    fn pinned_policy_offers_two_step_confirmed_add_and_remove_without_cycles(
        cx: &mut gpui::TestAppContext,
    ) {
        use std::collections::BTreeSet;

        use manis_core::{NodeIdentity, PolicyCandidateMatcher};

        cx.update(crate::init);
        let root = std::env::temp_dir().join(format!(
            "manis-tree-member-edit-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let store = root.join("subscriptions");
        let mut final_group = ManagedPolicyGroup::new("p-final", "Final").expect("final policy");
        final_group
            .set_matcher(PolicyCandidateMatcher::Explicit(BTreeSet::from([
                NodeIdentity::new("policy:p-proxy", "Proxy").expect("proxy ref"),
                NodeIdentity::new("builtin", "DIRECT").expect("direct ref"),
            ])))
            .expect("set final matcher");
        let mut proxy_group = ManagedPolicyGroup::new("p-proxy", "Proxy").expect("proxy policy");
        proxy_group
            .set_matcher(PolicyCandidateMatcher::Explicit(BTreeSet::from([
                NodeIdentity::new("mihomo:0", "HK 01").expect("hk ref"),
            ])))
            .expect("set proxy matcher");
        let mut jp_group = ManagedPolicyGroup::new("p-jp", "日本").expect("jp policy");
        jp_group
            .set_matcher(PolicyCandidateMatcher::Explicit(BTreeSet::from([
                NodeIdentity::new("mihomo:0", "JP 01").expect("jp ref"),
            ])))
            .expect("set jp matcher");

        let mut app = None;
        let (_, window) = cx.add_window_view(|window, cx| {
            let entity = cx.new(|_| {
                let mut fixture = ManisApp::with_fixture_controller_and_subscription_store(
                    "http://127.0.0.1:1",
                    store.clone(),
                );
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
                fixture.managed_policies.groups =
                    vec![final_group.clone(), proxy_group.clone(), jp_group.clone()];
                fixture
            });
            app = Some(entity.clone());
            crate::root(entity, window, cx)
        });
        let app = app.expect("fixture app");
        window.update(|window, cx| window.draw(cx).clear(cx));

        // Tier 1: Final (policy 0); Tier 2: Proxy (policy 1), 日本 (policy 2); Tier 3: nodes.
        let final_bounds = window
            .debug_bounds("tree-policy-0")
            .expect("tier-1 Final renders");
        let proxy_bounds = window
            .debug_bounds("tree-policy-1")
            .expect("tier-2 Proxy renders");
        let jp_bounds = window
            .debug_bounds("tree-policy-2")
            .expect("tier-2 日本 renders");
        let jp_node_bounds = window
            .debug_bounds("tree-node-1")
            .expect("tier-3 JP 01 renders");
        let direct_bounds = window
            .debug_bounds("tree-node-2")
            .expect("tier-3 DIRECT renders");
        assert!(
            final_bounds.right() < proxy_bounds.left() && final_bounds.right() < jp_bounds.left(),
            "Final sits in column 1 to the left of tier-2 policies"
        );

        // Pin Final.
        window.simulate_click(final_bounds.center(), Modifiers::none());
        window.update(|window, cx| window.draw(cx).clear(cx));
        assert_eq!(
            app.read_with(window, |app, _| app.topology.pinned_nodes.clone()),
            Some(NodesTreeFocus::Policy("p-final".to_owned()))
        );

        // Hover tier-2 日本 (not in Final) -> left-edge `+` trigger appears; no mutation yet.
        window.simulate_mouse_move(jp_bounds.center(), None, Modifiers::none());
        window.update(|window, cx| window.draw(cx).clear(cx));
        let add_trigger = window
            .debug_bounds("tree-member-trigger-policy-2")
            .expect("hovering unincluded tier-2 policy shows left-edge + button");
        assert!(
            add_trigger.center().x <= jp_bounds.left() + gpui::px(4.0),
            "+ button sits on the left edge of the card"
        );
        assert!(
            app.read_with(window, |app, _| app.topology.pending_member_edit.is_none()),
            "hover alone does not arm confirmation"
        );

        // First click on `+` arms two-step confirmation without mutating Final yet.
        window.simulate_click(add_trigger.center(), Modifiers::none());
        window.update(|window, cx| window.draw(cx).clear(cx));
        let armed = app
            .read_with(window, |app, _| app.topology.pending_member_edit.clone())
            .expect("first click arms two-step confirmation");
        assert_eq!(armed.op, super::PolicyMemberEditOp::Add);
        assert_eq!(armed.parent_policy_id, "p-final");
        assert_eq!(armed.target_key, "p:p-jp");
        let cancel_btn = window
            .debug_bounds("tree-member-cancel-policy-2")
            .expect("two-step cancel button appears");
        let _confirm_btn = window
            .debug_bounds("tree-member-confirm-policy-2")
            .expect("two-step confirm button appears");

        // Cancel clears armed confirmation without mutating.
        window.simulate_click(cancel_btn.center(), Modifiers::none());
        window.update(|window, cx| window.draw(cx).clear(cx));
        assert!(
            app.read_with(window, |app, _| app.topology.pending_member_edit.is_none()),
            "cancel disarms pending member edit"
        );

        // Hover tier-2 Proxy (already in Final) -> left-edge `-` trigger arms removal.
        window.simulate_mouse_move(proxy_bounds.center(), None, Modifiers::none());
        window.update(|window, cx| window.draw(cx).clear(cx));
        let remove_trigger = window
            .debug_bounds("tree-member-trigger-policy-1")
            .expect("hovering existing tier-2 member shows left-edge - button");
        window.simulate_click(remove_trigger.center(), Modifiers::none());
        window.update(|window, cx| window.draw(cx).clear(cx));
        let armed_remove = app
            .read_with(window, |app, _| app.topology.pending_member_edit.clone())
            .expect("first click on - arms removal confirmation");
        assert_eq!(armed_remove.op, super::PolicyMemberEditOp::Remove);
        assert_eq!(armed_remove.target_key, "p:p-proxy");

        // Hover tier-3 JP 01 (not in Final) and DIRECT (in Final) -> both offer +/- triggers.
        window.simulate_mouse_move(jp_node_bounds.center(), None, Modifiers::none());
        window.update(|window, cx| window.draw(cx).clear(cx));
        let node_add_trigger = window
            .debug_bounds("tree-member-trigger-node-1")
            .expect("hovering unincluded node shows left-edge + button");
        window.simulate_click(node_add_trigger.center(), Modifiers::none());
        window.update(|window, cx| window.draw(cx).clear(cx));
        assert_eq!(
            app.read_with(window, |app, _| app
                .topology
                .pending_member_edit
                .as_ref()
                .map(|p| (p.op, p.target_key.clone()))),
            Some((super::PolicyMemberEditOp::Add, "n:JP 01".to_owned()))
        );
        let node_confirm_btn = window
            .debug_bounds("tree-member-confirm-node-1")
            .expect("confirm button renders for armed node addition");
        window.simulate_click(node_confirm_btn.center(), Modifiers::none());
        window.run_until_parked();
        assert!(
            app.read_with(window, |app, _| app.topology.pending_member_edit.is_none()),
            "second click consumes confirmation and triggers persistence"
        );
        assert!(
            app.read_with(window, |app, _| !app.status.is_empty()),
            "confirmed edit ran through the persistence transaction"
        );

        window.simulate_mouse_move(direct_bounds.center(), None, Modifiers::none());
        window.update(|window, cx| window.draw(cx).clear(cx));
        let direct_remove_trigger = window
            .debug_bounds("tree-member-trigger-node-2")
            .expect("hovering included DIRECT node shows left-edge - button");
        window.simulate_click(direct_remove_trigger.center(), Modifiers::none());
        window.update(|window, cx| window.draw(cx).clear(cx));
        assert_eq!(
            app.read_with(window, |app, _| app
                .topology
                .pending_member_edit
                .as_ref()
                .map(|p| (p.op, p.target_key.clone()))),
            Some((super::PolicyMemberEditOp::Remove, "n:DIRECT".to_owned()))
        );

        // Acyclic guard: pin tier-2 日本 and hover tier-1 Final -> no + / - trigger allowed.
        app.update(window, |app, cx| {
            app.topology.pending_member_edit = None;
            app.topology.pinned_nodes = Some(NodesTreeFocus::Policy("p-jp".to_owned()));
            app.topology.hovered_member_target = None;
            cx.notify();
        });
        window.update(|window, cx| window.draw(cx).clear(cx));
        window.simulate_mouse_move(final_bounds.center(), None, Modifiers::none());
        window.update(|window, cx| window.draw(cx).clear(cx));
        assert!(
            window
                .debug_bounds("tree-member-trigger-policy-0")
                .is_none(),
            "upstream tier-1 policy must never offer + when a downstream tier-2 policy is pinned"
        );

        let _ = std::fs::remove_dir_all(root);
    }

    #[gpui::test]
    #[expect(
        clippy::too_many_lines,
        reason = "Verifies off-screen reveal, multi-screen first-to-last sweep, manual scroll yield, and rules-tree auto-scroll"
    )]
    fn pinning_policy_auto_scrolls_overflowing_columns_and_yields_to_manual_scroll(
        cx: &mut gpui::TestAppContext,
    ) {
        use std::collections::BTreeSet;
        use std::time::Duration;

        use manis_core::{NodeIdentity, PolicyCandidateMatcher};

        cx.update(crate::init);
        let mut app = None;
        let (_, window) = cx.add_window_view(|window, cx| {
            let entity = cx.new(|_| {
                let mut fixture = ManisApp::with_fixture_controller("http://127.0.0.1:1");
                fixture.source_providers = vec![LoadedProvider {
                    name: "BigAirport".to_owned(),
                    vehicle_type: Some("HTTP".to_owned()),
                    nodes: (0..54)
                        .map(|idx| LoadedProviderNode {
                            name: format!("Node-{idx:02}"),
                            protocol: "VLESS".to_owned(),
                            latency_label: Some(format!("{} ms", 30 + idx)),
                            alive: Some(true),
                        })
                        .collect(),
                }];

                let mut far_policy = ManagedPolicyGroup::new("p-far", "Far Nodes").expect("far");
                far_policy
                    .set_matcher(PolicyCandidateMatcher::Explicit(BTreeSet::from([
                        NodeIdentity::new("mihomo:0", "Node-40").expect("node 40"),
                        NodeIdentity::new("mihomo:0", "Node-41").expect("node 41"),
                        NodeIdentity::new("mihomo:0", "Node-42").expect("node 42"),
                    ])))
                    .expect("far matcher");

                let all_policy = ManagedPolicyGroup::new("p-all", "All Nodes").expect("all");

                fixture.managed_policies.groups = vec![far_policy, all_policy];
                fixture.local_rule_groups = (0..18)
                    .map(|idx| {
                        let target = if idx == 15 { "Far Nodes" } else { "All Nodes" };
                        LocalRoutingRuleGroup {
                            id: format!("rule-group-{idx:02}"),
                            name: format!("Rule Group {idx:02}"),
                            target_policy: manis_profile::Name::parse(target).expect("target"),
                            enabled: true,
                            rules: vec![
                                ManualRule::parse(
                                    ManualRuleKind::HostSuffix,
                                    &format!("r{idx}.example.com"),
                                    target,
                                )
                                .expect("rule"),
                            ],
                        }
                    })
                    .collect();
                fixture
            });
            app = Some(entity.clone());
            crate::root(entity, window, cx)
        });
        let app = app.expect("fixture app");
        window.update(|window, cx| window.draw(cx).clear(cx));

        let nodes_viewport = window
            .debug_bounds("nodes-tree-col-nodes-scroll")
            .expect("nodes column viewport");
        let policy_far = window
            .debug_bounds("tree-policy-0")
            .expect("Far Nodes policy card");
        let policy_all = window
            .debug_bounds("tree-policy-1")
            .expect("All Nodes policy card");
        let node40_initial = window
            .debug_bounds("tree-node-40")
            .expect("Node-40 laid out below fold");
        assert!(
            node40_initial.top() > nodes_viewport.bottom(),
            "Node-40 starts off-screen below the nodes viewport"
        );

        // 1. Pin "Far Nodes" (contains only off-screen Node-40..42): nodes column must auto-scroll
        // smoothly to bring Node-40..42 inside the viewport while the policy column stays fixed.
        window.simulate_click(policy_far.center(), Modifiers::none());
        window.update(|window, cx| window.draw(cx).clear(cx));
        for _ in 0..60 {
            window.executor().advance_clock(Duration::from_millis(16));
        }
        window.update(|window, cx| window.draw(cx).clear(cx));
        let policy_far_after = window
            .debug_bounds("tree-policy-0")
            .expect("Far Nodes policy card after scroll");
        let node40_after = window
            .debug_bounds("tree-node-40")
            .expect("Node-40 after auto-scroll");
        let node42_after = window
            .debug_bounds("tree-node-42")
            .expect("Node-42 after auto-scroll");
        assert_eq!(
            policy_far_after.origin.y, policy_far.origin.y,
            "pinning a policy must not scroll its own column"
        );
        assert!(
            node40_after.top() >= nodes_viewport.top()
                && node42_after.bottom() <= nodes_viewport.bottom(),
            "off-screen active nodes (Node-40..42) auto-scroll cleanly inside the viewport"
        );

        // 2. Pin "All Nodes" (spans Node-00..Node-53, exceeding viewport height): column sweeps
        // from the first active node down to the last active node (Node-53).
        window.simulate_click(policy_all.center(), Modifiers::none());
        window.update(|window, cx| window.draw(cx).clear(cx));
        // Advance through approach back toward Node-00 (first active node).
        for _ in 0..38 {
            window.executor().advance_clock(Duration::from_millis(16));
        }
        window.update(|window, cx| window.draw(cx).clear(cx));
        let node0_during = window
            .debug_bounds("tree-node-0")
            .expect("Node-00 at start of sweep");
        assert!(
            node0_during.top() >= nodes_viewport.top() - gpui::px(2.0)
                && node0_during.bottom() <= nodes_viewport.bottom(),
            "sweep visits the first active node (Node-00) before panning to the last"
        );
        // Advance through dwell + full sweep to the last active node (Node-53).
        for _ in 0..190 {
            window.executor().advance_clock(Duration::from_millis(16));
        }
        window.update(|window, cx| window.draw(cx).clear(cx));
        let node53_after = window
            .debug_bounds("tree-node-53")
            .expect("Node-53 at end of sweep");
        assert!(
            node53_after.top() >= nodes_viewport.top()
                && node53_after.bottom() <= nodes_viewport.bottom(),
            "multi-screen sweep finishes with the last active node (Node-53) inside the viewport"
        );

        // 3. Manual scroll interruption: re-pin "Far Nodes" then scroll manually mid-flight;
        // auto-scroll must yield immediately and leave the manual scroll offset intact.
        window.simulate_click(policy_far.center(), Modifiers::none());
        window.update(|window, cx| window.draw(cx).clear(cx));
        for _ in 0..5 {
            window.executor().advance_clock(Duration::from_millis(16));
        }
        window.simulate_event(gpui::ScrollEvent {
            position: nodes_viewport.center(),
            delta: gpui::ScrollDelta::Pixels(gpui::point(gpui::px(0.0), gpui::px(220.0))),
            ..Default::default()
        });
        window.update(|window, cx| window.draw(cx).clear(cx));
        let manual_offset = app.read_with(window, |app, _| {
            app.topology.nodes_leaf_col_scroll.offset().y
        });
        for _ in 0..60 {
            window.executor().advance_clock(Duration::from_millis(16));
        }
        window.update(|window, cx| window.draw(cx).clear(cx));
        let offset_after_wait = app.read_with(window, |app, _| {
            app.topology.nodes_leaf_col_scroll.offset().y
        });
        assert_eq!(
            offset_after_wait, manual_offset,
            "manual scroll wheel must immediately cancel column auto-scroll"
        );

        // 4. Rules tree: pinning "Far Nodes" in Column 2 must auto-scroll overflowing Column 1
        // so its matching rule (Rule Group 15) glides into view.
        window.update(|window, cx| {
            app.update(cx, |app, cx| {
                app.primary_workspace = PrimaryWorkspace::RoutingRules;
                cx.notify();
            });
            window.draw(cx).clear(cx);
        });
        let rules_viewport = window
            .debug_bounds("rules-tree-col-rules-scroll")
            .expect("rules column viewport");
        let rule15_before = window
            .debug_bounds("tree-rule-15")
            .expect("Rule Group 15 laid out below fold");
        assert!(
            rule15_before.top() > rules_viewport.bottom(),
            "Rule Group 15 starts off-screen below the rules column viewport"
        );
        let far_policy_in_rules = window
            .debug_bounds("tree-rule-policy-0")
            .expect("Far Nodes policy card in rules tree");
        window.simulate_click(far_policy_in_rules.center(), Modifiers::none());
        window.update(|window, cx| window.draw(cx).clear(cx));
        for _ in 0..60 {
            window.executor().advance_clock(Duration::from_millis(16));
        }
        window.update(|window, cx| window.draw(cx).clear(cx));
        let rule15_after = window
            .debug_bounds("tree-rule-15")
            .expect("Rule Group 15 after auto-scroll");
        assert!(
            rule15_after.top() >= rules_viewport.top()
                && rule15_after.bottom() <= rules_viewport.bottom(),
            "pinning a policy in rules tree auto-scrolls its off-screen rules into view"
        );
    }
}
