use std::{cell::Cell, rc::Rc};

use gpui::{
    App, AppContext as _, CursorStyle, DragMoveEvent, Pixels, Render, ScrollHandle, Stateful,
    Window, canvas,
};
use gpui_component::Icon;

use super::{
    AnyElement, Context, Div, FluentBuilder, InteractiveElement, IntoElement, Language, ManisApp,
    ParentElement, Radius, Role, StatefulInteractiveElement, Styled, Theme, div, mihomo, px,
};
use crate::app::topology::RuleGroupDragPreview;

/// Live metadata for an in-flight routing-rule group drag.
#[derive(Clone)]
pub(in crate::app) struct RuleGroupDrag {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) detail: String,
    pub(super) target: Option<String>,
    pub(super) width: Rc<Cell<Pixels>>,
    pub(super) height: Rc<Cell<Pixels>>,
    pub(super) live_position: Rc<Cell<usize>>,
    pub(super) compact: bool,
    pub(super) theme: Theme,
    scrolling: Rc<Cell<bool>>,
}

impl RuleGroupDrag {
    pub(in crate::app) fn new(
        id: &str,
        name: &str,
        detail: String,
        target: Option<String>,
        position: usize,
        compact: bool,
        theme: Theme,
    ) -> Self {
        Self {
            id: id.to_owned(),
            name: name.to_owned(),
            detail,
            target,
            width: Rc::new(Cell::new(px(560.0))),
            height: Rc::new(Cell::new(px(64.0))),
            live_position: Rc::new(Cell::new(position)),
            compact,
            theme,
            scrolling: Rc::new(Cell::new(false)),
        }
    }
}

impl Render for RuleGroupDrag {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let width = self.width.get().max(px(220.0));
        let height = self.height.get().max(px(52.0));
        let position = self.live_position.get();
        let theme = self.theme;
        div()
            .w(width)
            .min_h(height)
            .px(if self.compact { px(12.0) } else { px(14.0) })
            .py(px(12.0))
            .rounded(Radius::Card.px())
            .bg(theme.surface_elevated)
            .border_2()
            .border_color(theme.action_primary)
            .shadow_lg()
            .cursor(CursorStyle::ClosedHand)
            .flex()
            .items_center()
            .gap(px(10.0))
            .child(
                super::groups::order_badge(format!("{:02}", position + 1), theme)
                    .bg(theme.action_soft)
                    .text_color(theme.action_primary),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .size(px(32.0))
                    .rounded(Radius::Control.px())
                    .bg(theme.action_soft)
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(theme.action_primary)
                    .child(Icon::new(gpui_component_assets::IconName::GripVertical).size(px(16.0))),
            )
            .child(super::groups::rule_group_identity(
                &self.name,
                self.detail.clone(),
                self.target.clone(),
                theme,
            ))
    }
}

impl ManisApp {
    pub(in crate::app) fn normalized_rule_group_order(&self) -> Vec<String> {
        mihomo::normalized_routing_rule_group_order(
            &self.rule_sources.group_order,
            self.has_regular_manual_rules(),
            &self.rule_sources.sources,
            &self.local_rule_groups,
        )
    }

    pub(in crate::app) fn active_rule_group_drag_preview(
        &self,
        cx: &Context<Self>,
    ) -> Option<&RuleGroupDragPreview> {
        if !cx.has_active_drag() {
            return None;
        }
        self.topology.rule_group_drag_preview.as_ref()
    }

    pub(super) fn begin_rule_group_drag(
        &mut self,
        dragged_id: String,
        slot_height: Pixels,
        cx: &mut Context<Self>,
    ) {
        if self.routing_apply_state.is_busy() || self.configuration_transfer.active {
            return;
        }
        let order = self.normalized_rule_group_order();
        if order.len() <= 1 || !order.iter().any(|id| id == &dragged_id) {
            return;
        }
        self.topology.cancel_auto_scroll();
        self.topology.hovered_rules = None;
        self.topology.rule_group_drag_preview = Some(RuleGroupDragPreview {
            dragged_id,
            order,
            slot_height: slot_height.max(px(52.0)),
        });
        cx.notify();
    }

    pub(super) fn update_rule_group_drag_pointer(
        &mut self,
        drag: &RuleGroupDrag,
        pointer_y: Pixels,
        cx: &mut Context<Self>,
    ) {
        if self.routing_apply_state.is_busy() || self.configuration_transfer.active {
            return;
        }
        let Some(preview) = self.topology.rule_group_drag_preview.as_mut() else {
            return;
        };
        if preview.dragged_id != drag.id {
            return;
        }
        let Some(dragged_idx) = preview.order.iter().position(|id| id == &drag.id) else {
            return;
        };
        let row_bounds = self.topology.rule_group_row_bounds.borrow();
        let down_target = ((dragged_idx + 1)..preview.order.len()).rev().find(|&idx| {
            row_bounds
                .get(&preview.order[idx])
                .is_some_and(|bounds| pointer_y > bounds.center().y)
        });
        let up_target = (0..dragged_idx).find(|&idx| {
            row_bounds
                .get(&preview.order[idx])
                .is_some_and(|bounds| pointer_y < bounds.center().y)
        });
        let (target_id, after) = if let Some(idx) = down_target {
            (preview.order[idx].clone(), true)
        } else if let Some(idx) = up_target {
            (preview.order[idx].clone(), false)
        } else {
            return;
        };
        drop(row_bounds);
        if !mihomo::place_routing_rule_group(&mut preview.order, &drag.id, &target_id, after) {
            return;
        }
        if let Some(new_idx) = preview.order.iter().position(|id| id == &drag.id) {
            drag.live_position.set(new_idx);
        }
        cx.notify();
    }

    pub(crate) fn cancel_rule_group_drag(&mut self, cx: &mut Context<Self>) {
        if self.topology.rule_group_drag_preview.take().is_some() {
            cx.notify();
        }
    }

    pub(super) fn commit_rule_group_drag(&mut self, drag: &RuleGroupDrag, cx: &mut Context<Self>) {
        let Some(preview) = self.topology.rule_group_drag_preview.take() else {
            return;
        };
        if preview.dragged_id != drag.id {
            return;
        }
        self.apply_routing_rule_group_order(&preview.order, cx);
        cx.notify();
    }

    pub(in crate::app) fn rule_group_scroll_target(
        &self,
        scroll: Stateful<Div>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let handle = self.routing_rules_scroll.clone();
        Self::rule_group_drag_scroll_target(scroll.track_scroll(&handle), handle, cx)
    }

    pub(in crate::app) fn rule_group_drag_scroll_target(
        container: Stateful<Div>,
        handle: ScrollHandle,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        container
            .on_drag_move::<RuleGroupDrag>(cx.listener(
                move |this, event: &DragMoveEvent<RuleGroupDrag>, window, cx| {
                    let drag = event.drag(cx).clone();
                    if !drag.scrolling.replace(true) {
                        scroll_during_drag(handle.clone(), drag.scrolling.clone(), window, cx);
                    }
                    this.update_rule_group_drag_pointer(&drag, event.event.position.y, cx);
                },
            ))
            .on_drop(cx.listener(|this, drag: &RuleGroupDrag, _, cx| {
                cx.stop_propagation();
                this.commit_rule_group_drag(drag, cx);
            }))
    }

    pub(super) fn rule_group_drag_handle(
        &self,
        group_id: &str,
        group_name: &str,
        (position, group_count): (usize, usize),
        theme: Theme,
        language: Language,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.rule_group_drag_handle_sized(
            group_id,
            group_name,
            (position, group_count),
            px(32.0),
            px(16.0),
            theme,
            language,
            cx,
        )
    }

    pub(in crate::app) fn rule_group_compact_drag_handle(
        &self,
        group_id: &str,
        group_name: &str,
        (position, group_count): (usize, usize),
        theme: Theme,
        language: Language,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.rule_group_drag_handle_sized(
            group_id,
            group_name,
            (position, group_count),
            px(22.0),
            px(13.0),
            theme,
            language,
            cx,
        )
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "Shared drag handle builder parameterized by hit-target and icon size for list and tree cards"
    )]
    fn rule_group_drag_handle_sized(
        &self,
        group_id: &str,
        group_name: &str,
        (position, group_count): (usize, usize),
        button_size: Pixels,
        icon_size: Pixels,
        theme: Theme,
        language: Language,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let enabled = group_count > 1
            && !self.routing_apply_state.is_busy()
            && !self.configuration_transfer.active;
        let label = match language {
            Language::English => {
                format!("Drag to reorder {group_name}. Alt+Up/Down also moves it.")
            }
            Language::SimplifiedChinese => {
                format!("拖动调整{group_name}的顺序，也可按 Alt+上下方向键")
            }
        };
        let keyboard_id = group_id.to_owned();
        div()
            .id(format!("drag-rule-group-{group_id}"))
            .debug_selector({
                let id = group_id.to_owned();
                move || format!("drag-rule-group-{id}")
            })
            .flex_shrink_0()
            .size(button_size)
            .rounded(Radius::Control.px())
            .border_2()
            .border_color(gpui::rgba(0x0000_0000))
            .flex()
            .items_center()
            .justify_center()
            .text_color(theme.text_tertiary)
            .role(Role::Button)
            .aria_label(label.clone())
            .tab_stop(enabled)
            .focusable()
            .focus_visible(move |style| style.bg(theme.action_soft).border_color(theme.focus_ring))
            .tooltip(move |window, cx| {
                gpui_component::tooltip::Tooltip::new(label.clone()).build(window, cx)
            })
            .on_click(|_, _, cx| cx.stop_propagation())
            .when(enabled, |handle| {
                handle
                    .map(crate::components::primary_button_interaction)
                    .cursor(CursorStyle::OpenHand)
                    .hover(move |style| {
                        style.bg(theme.action_soft).text_color(theme.action_primary)
                    })
                    .on_key_down(cx.listener(move |this, event: &gpui::KeyDownEvent, _, cx| {
                        if !event.keystroke.modifiers.alt {
                            return;
                        }
                        let direction = match event.keystroke.key.as_str() {
                            "up" if position > 0 => mihomo::MoveDirection::Up,
                            "down" if position + 1 < group_count => mihomo::MoveDirection::Down,
                            _ => return,
                        };
                        cx.stop_propagation();
                        this.move_routing_rule_group(&keyboard_id, direction, cx);
                    }))
            })
            .child(Icon::new(gpui_component_assets::IconName::GripVertical).size(icon_size))
            .into_any_element()
    }

    pub(in crate::app) fn rule_group_drag_surface(
        header: AnyElement,
        drag: RuleGroupDrag,
        enabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let group_id = drag.id.clone();
        let width_tracker = drag.width.clone();
        let height_tracker = drag.height.clone();
        let app = cx.entity();
        div()
            .id(format!("rule-group-drag-surface-{group_id}"))
            .relative()
            .w_full()
            .child(
                canvas(
                    move |bounds, _, _| {
                        width_tracker.set(bounds.size.width);
                        height_tracker.set(bounds.size.height);
                    },
                    |_, (), _, _| {},
                )
                .absolute()
                .inset_0(),
            )
            .child(header)
            .when(enabled, |surface| {
                surface.on_drag(drag, move |drag, _, _, cx| {
                    cx.stop_propagation();
                    let dragged_id = drag.id.clone();
                    let slot_height = drag.height.get();
                    app.update(cx, |this, cx| {
                        this.begin_rule_group_drag(dragged_id, slot_height, cx);
                    });
                    cx.new(|_| drag.clone())
                })
            })
            .into_any_element()
    }

    pub(in crate::app) fn rule_group_drop_target(
        &self,
        card: Div,
        group_id: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let bounds_registry = self.topology.rule_group_row_bounds.clone();
        let bounds_key = group_id.to_owned();
        card.id(format!("rule-group-drop-{group_id}"))
            .debug_selector({
                let id = group_id.to_owned();
                move || format!("rule-group-drop-{id}")
            })
            .relative()
            .child(
                canvas(
                    move |bounds, _, _| {
                        bounds_registry.borrow_mut().insert(bounds_key, bounds);
                    },
                    |_, (), _, _| {},
                )
                .absolute()
                .inset_0(),
            )
            .when(
                !self.routing_apply_state.is_busy() && !self.configuration_transfer.active,
                |wrapper| {
                    wrapper.on_drop(cx.listener(|this, drag: &RuleGroupDrag, _, cx| {
                        cx.stop_propagation();
                        this.commit_rule_group_drag(drag, cx);
                    }))
                },
            )
            .into_any_element()
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "Shared dashed drag placeholder for list and tree cards with optional tree port tracker"
    )]
    pub(in crate::app) fn rule_group_drag_placeholder(
        group_id: &str,
        name: &str,
        position: usize,
        slot_height: Pixels,
        compact: bool,
        port_tracker: Option<AnyElement>,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = group_id.to_owned();
        div()
            .id(format!("rule-group-drop-{group_id}"))
            .debug_selector(move || format!("rule-group-drop-{id}"))
            .relative()
            .w_full()
            .min_h(slot_height.max(px(52.0)))
            .px(if compact { px(10.0) } else { px(14.0) })
            .py(if compact { px(8.0) } else { px(12.0) })
            .rounded(if compact { px(10.0) } else { Radius::Card.px() })
            .bg(theme.action_soft)
            .border_2()
            .border_dashed()
            .border_color(theme.action_primary)
            .flex()
            .items_center()
            .gap(px(10.0))
            .when_some(port_tracker, ParentElement::child)
            .on_drop(cx.listener(|this, drag: &RuleGroupDrag, _, cx| {
                cx.stop_propagation();
                this.commit_rule_group_drag(drag, cx);
            }))
            .child(
                super::groups::order_badge(format!("{:02}", position + 1), theme)
                    .bg(theme.surface_elevated)
                    .text_color(theme.action_primary),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .text_size(if compact { px(12.5) } else { px(13.0) })
                    .text_color(theme.action_primary)
                    .opacity(0.85)
                    .child(name.to_owned()),
            )
            .into_any_element()
    }
}

/// Continue scrolling while a held drag rests at a viewport edge. The loop ends on
/// drop/cancel, leaving the edge, or reaching the end of the list.
fn scroll_during_drag(
    handle: ScrollHandle,
    active: Rc<Cell<bool>>,
    window: &mut Window,
    cx: &mut App,
) {
    let bounds = handle.bounds();
    let pointer = window.mouse_position();
    let delta = if !cx.has_active_drag() || !bounds.contains(&pointer) {
        0.0
    } else if pointer.y < bounds.top() + px(40.0) {
        12.0
    } else if pointer.y > bounds.bottom() - px(40.0) {
        -12.0
    } else {
        0.0
    };
    let mut offset = handle.offset();
    let next = (offset.y + px(delta)).clamp(-handle.max_offset().y, px(0.0));
    if delta == 0.0 || next == offset.y {
        active.set(false);
        return;
    }
    offset.y = next;
    handle.set_offset(offset);
    window.refresh();
    window.on_next_frame(move |window, cx| scroll_during_drag(handle, active, window, cx));
}

#[cfg(all(test, not(windows)))]
mod tests {
    use super::*;
    use crate::manual_rule::{ManualRule, ManualRuleKind};
    use gpui::{Modifiers, MouseButton, point};

    #[gpui::test]
    #[expect(
        clippy::too_many_lines,
        reason = "Verifies live drag shift, drop rollback, escape cancel, and no-op reorder in one window"
    )]
    fn mouse_drag_live_reorders_and_rolls_back_failed_apply_without_toggling_group(
        cx: &mut gpui::TestAppContext,
    ) {
        let store = std::env::temp_dir().join(format!("manis-rule-drag-{}", std::process::id()));
        let remote = mihomo::save_qx_rule_source_in(
            &store,
            "https://rules.example.invalid/drag",
            "DIRECT",
            "DOMAIN,remote.example,DIRECT\n",
        )
        .unwrap()
        .into_source();
        let rules =
            vec![ManualRule::parse(ManualRuleKind::Host, "manual.example", "DIRECT").unwrap()];
        crate::manual_rule::save_manual_rules_in(&store, &rules).unwrap();
        let original = vec![
            mihomo::MANUAL_ROUTING_RULE_GROUP_ID.to_owned(),
            remote.id.clone(),
        ];
        let reordered = vec![
            remote.id.clone(),
            mihomo::MANUAL_ROUTING_RULE_GROUP_ID.to_owned(),
        ];
        mihomo::save_routing_rule_group_order_in(&store, &original).unwrap();
        cx.update(crate::init);
        let app = cx.new(|_| {
            let mut app = ManisApp::with_fixture_controller_and_subscription_store(
                "http://127.0.0.1:1",
                store.clone(),
            );
            app.primary_workspace = manis_core::PrimaryWorkspace::RoutingRules;
            app.topology.rules_view = crate::app::WorkspaceViewMode::List;
            app
        });
        let retained = app.clone();
        let (_, window) = cx.add_window_view(|window, cx| crate::root(retained, window, cx));
        window.update(|window, cx| window.draw(cx).clear(cx));
        let from_selector = Box::leak(
            format!("drag-rule-group-{}", mihomo::MANUAL_ROUTING_RULE_GROUP_ID).into_boxed_str(),
        );
        let target_selector = Box::leak(format!("rule-group-drop-{}", remote.id).into_boxed_str());
        let from = window.debug_bounds(from_selector).unwrap().center();
        let target = window.debug_bounds(target_selector).unwrap();
        let to = point(target.center().x, target.bottom() - px(4.0));
        let before = app.read_with(window, |app, _| app.node_workspace.clone());
        window.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
        window.simulate_mouse_move(
            from + point(px(10.0), px(10.0)),
            MouseButton::Left,
            Modifiers::none(),
        );
        window.update(|window, cx| window.draw(cx).clear(cx));
        assert!(
            window.update(|_, cx| cx.has_active_drag()),
            "handle starts a GPUI drag"
        );
        window.simulate_mouse_move(to, MouseButton::Left, Modifiers::none());
        window.update(|window, cx| window.draw(cx).clear(cx));
        assert_eq!(
            app.read_with(window, |app, _| {
                app.topology
                    .rule_group_drag_preview
                    .as_ref()
                    .map(|p| p.order.clone())
            }),
            Some(reordered.clone()),
            "moving across the target midpoint shifts the list live before drop"
        );
        window.simulate_mouse_up(to, MouseButton::Left, Modifiers::none());
        window.run_until_parked();
        assert!(
            app.read_with(window, |app, _| app
                .status
                .contains("test snapshots cannot write runtime configuration")),
            "the real drop reached the runtime transaction and rollback"
        );
        assert_eq!(
            app.read_with(window, |app, _| app.rule_sources.group_order.clone()),
            original
        );
        assert_eq!(
            mihomo::load_routing_rule_group_order_in(&store).unwrap(),
            original
        );
        app.read_with(window, |app, _| {
            assert_eq!(
                app.node_workspace
                    .is_group_collapsed(super::super::MANUAL_RULES_EXPANSION_KEY),
                before.is_group_collapsed(super::super::MANUAL_RULES_EXPANSION_KEY)
            );
            assert!(!app.manual_rule_editor_state.is_open());
        });
        // Escape cancels the GPUI drag and restores the pre-drag visual order without saving.
        let status = app.read_with(window, |app, _| app.status.clone());
        window.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
        window.simulate_mouse_move(
            from + point(px(10.0), px(10.0)),
            MouseButton::Left,
            Modifiers::none(),
        );
        window.update(|window, cx| window.draw(cx).clear(cx));
        assert!(window.update(|_, cx| cx.has_active_drag()));
        window.simulate_mouse_move(to, MouseButton::Left, Modifiers::none());
        window.update(|window, cx| window.draw(cx).clear(cx));
        window.simulate_keystrokes("escape");
        assert!(!window.update(|_, cx| cx.has_active_drag()));
        window.simulate_mouse_up(to, MouseButton::Left, Modifiers::none());
        assert_eq!(app.read_with(window, |app, _| app.status.clone()), status);
        // Repeating an unchanged drop does not start another apply.
        app.update(window, |app, cx| {
            app.reorder_routing_rule_group(
                mihomo::MANUAL_ROUTING_RULE_GROUP_ID,
                &remote.id,
                false,
                cx,
            );
        });
        assert!(!app.read_with(window, |app, _| app.routing_apply_state.is_busy()));
        std::fs::remove_dir_all(store).unwrap();
    }
}
