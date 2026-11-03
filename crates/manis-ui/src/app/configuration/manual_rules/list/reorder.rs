use std::{cell::Cell, rc::Rc};

use gpui::{
    App, AppContext as _, CursorStyle, DragMoveEvent, Render, ScrollHandle, Stateful, Window,
};
use gpui_component::Icon;

use super::{
    AnyElement, Context, Div, FluentBuilder, InteractiveElement, IntoElement, Language, ManisApp,
    ParentElement, Radius, Role, StatefulInteractiveElement, Styled, TextRole, Theme, div, mihomo,
    px,
};

#[derive(Clone)]
struct RuleGroupDrag {
    id: String,
    label: String,
    after: Rc<Cell<bool>>,
    theme: Theme,
    scrolling: Rc<Cell<bool>>,
}

impl Render for RuleGroupDrag {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_4()
            .py_3()
            .rounded(Radius::Control.px())
            .bg(self.theme.surface_overlay)
            .border_1()
            .border_color(self.theme.action_primary)
            .text_color(self.theme.text_primary)
            .text_size(TextRole::Body.size())
            .child(self.label.clone())
    }
}

impl ManisApp {
    pub(in crate::app) fn rule_group_scroll_target(&self, scroll: Stateful<Div>) -> Stateful<Div> {
        let handle = self.routing_rules_scroll.clone();
        scroll
            .track_scroll(&handle)
            .on_drag_move::<RuleGroupDrag>(move |event, window, cx| {
                let drag = event.drag(cx);
                if !drag.scrolling.replace(true) {
                    scroll_during_drag(handle.clone(), drag.scrolling.clone(), window, cx);
                }
            })
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
        let drag = RuleGroupDrag {
            id: group_id.to_owned(),
            label: group_name.to_owned(),
            after: Rc::new(Cell::new(false)),
            scrolling: Rc::new(Cell::new(false)),
            theme,
        };
        let keyboard_id = group_id.to_owned();
        div()
            .id(format!("drag-rule-group-{group_id}"))
            .debug_selector({
                let id = group_id.to_owned();
                move || format!("drag-rule-group-{id}")
            })
            .flex_shrink_0()
            .size(px(32.0))
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
                    .hover(move |style| style.bg(theme.action_soft))
                    .on_drag(drag, |drag, _, _, cx| {
                        cx.stop_propagation();
                        cx.new(|_| drag.clone())
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
            .child(Icon::new(gpui_component_assets::IconName::GripVertical).size(px(16.0)))
            .into_any_element()
    }

    pub(super) fn rule_group_drop_target(
        &self,
        title: Div,
        group_id: &str,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let anchor_id = group_id.to_owned();
        let hover_id = group_id.to_owned();
        title
            .id(format!("rule-group-drop-{group_id}"))
            .debug_selector({
                let id = group_id.to_owned();
                move || format!("rule-group-drop-{id}")
            })
            .when(
                !self.routing_apply_state.is_busy() && !self.configuration_transfer.active,
                |title| {
                    title
                        .on_drag_move::<RuleGroupDrag>(|event: &DragMoveEvent<_>, window, cx| {
                            if event.bounds.contains(&event.event.position) {
                                let after = event.event.position.y > event.bounds.center().y;
                                if event.drag(cx).after.replace(after) != after {
                                    window.refresh();
                                }
                            }
                        })
                        .drag_over::<RuleGroupDrag>(move |style, drag, _, _| {
                            if drag.id == hover_id {
                                return style;
                            }
                            let style = style.border_color(theme.action_primary);
                            if drag.after.get() {
                                style.border_b_2()
                            } else {
                                style.border_t_2()
                            }
                        })
                        .on_drop(cx.listener(move |this, drag: &RuleGroupDrag, _, cx| {
                            cx.stop_propagation();
                            this.reorder_routing_rule_group(
                                &drag.id,
                                &anchor_id,
                                drag.after.get(),
                                cx,
                            );
                        }))
                },
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
    fn mouse_drag_rolls_back_failed_apply_without_toggling_group(cx: &mut gpui::TestAppContext) {
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
        mihomo::save_routing_rule_group_order_in(&store, &original).unwrap();
        cx.update(crate::init);
        let app = cx.new(|_| {
            let mut app = ManisApp::with_fixture_controller_and_subscription_store(
                "http://127.0.0.1:1",
                store.clone(),
            );
            app.primary_workspace = manis_core::PrimaryWorkspace::RoutingRules;
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
        // Escape cancels the GPUI drag before any save or kernel apply.
        let status = app.read_with(window, |app, _| app.status.clone());
        window.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
        window.simulate_mouse_move(
            from + point(px(10.0), px(10.0)),
            MouseButton::Left,
            Modifiers::none(),
        );
        window.update(|window, cx| window.draw(cx).clear(cx));
        assert!(window.update(|_, cx| cx.has_active_drag()));
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
