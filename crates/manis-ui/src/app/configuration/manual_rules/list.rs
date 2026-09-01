mod feedback;
mod groups;
use super::{
    ActionRole, AnyElement, Button, ButtonVariant, ButtonVariants, Collapsible, Context,
    ContextMenuExt, ControlSize, Disableable, Div, Entity, FluentBuilder, FontWeight, IconName,
    InteractiveElement, IntoElement, Language, MANUAL_RULES_EXPANSION_KEY, ManisApp,
    ManualRuleKeyboardAction, Message, ParentElement, PopupMenuItem, QxRuleImportFeedback,
    QxRuleKind, QxRuleList, Radius, Role, RuleGroupRenderContext, Space, Stateful,
    StatefulInteractiveElement, Styled, TextRole, Theme, copy, div, empty_state,
    manual_rule_keyboard_action, mihomo, px, rule_group_is_open, rule_source_expansion_key,
    source_update_label, style_action_button,
};
impl ManisApp {
    pub(in crate::app) fn manual_routing_rule_row(
        &self,
        order: usize,
        index: usize,
        rule: &crate::manual_rule::ManualRule,
        theme: Theme,
        language: Language,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let enabled = rule.is_enabled();
        let target = self.effective_rule_target(rule.target(), language);
        let matchers = Self::manual_rule_matchers(rule, enabled, theme, language);
        let edit_label = copy::configuration::manual_rule_accessibility(language, order);
        let row = div()
            .id(format!("manual-routing-rule-{index}"))
            .min_h(px(44.0))
            .px(Space::Md.px())
            .py(Space::Sm.px())
            .border_b_1()
            .border_color(theme.outline_subtle)
            .bg(if enabled {
                theme.surface_base
            } else {
                gpui::rgba(0x0000_0000)
            })
            .flex()
            .items_center()
            .gap(Space::Md.px())
            .hover(move |style| style.bg(theme.surface_low))
            .active(move |style| style.bg(theme.action_soft))
            .cursor_pointer()
            .role(Role::Button)
            .tab_stop(true)
            .focusable()
            .map(crate::components::primary_button_interaction)
            .aria_label(edit_label)
            .on_click(cx.listener(move |this, _, window, cx| {
                this.open_manual_rule_editor_for_edit(index, window, cx);
            }))
            .on_key_down(cx.listener(move |this, event, window, cx| {
                let Some(action) = manual_rule_keyboard_action(event) else {
                    return;
                };
                cx.stop_propagation();
                match action {
                    ManualRuleKeyboardAction::Edit => {
                        this.open_manual_rule_editor_for_edit(index, window, cx);
                    }
                    ManualRuleKeyboardAction::Toggle => {
                        this.set_manual_rule_enabled(index, !enabled, cx);
                    }
                    ManualRuleKeyboardAction::Delete => {
                        this.remove_manual_rule(index, cx);
                    }
                }
            }))
            .child(
                div()
                    .w(px(42.0))
                    .flex_shrink_0()
                    .text_size(TextRole::Metadata.size())
                    .line_height(TextRole::Metadata.line_height())
                    .text_color(theme.text_tertiary)
                    .child(format!("#{order:03}")),
            )
            .child(matchers)
            .child(
                div()
                    .flex_shrink_0()
                    .text_size(TextRole::Data.size())
                    .line_height(TextRole::Data.line_height())
                    .font_weight(TextRole::Data.weight())
                    .text_color(if enabled {
                        theme.action_primary
                    } else {
                        theme.text_tertiary
                    })
                    .child(target),
            )
            .when(!enabled, |row| {
                row.child(
                    div()
                        .flex_shrink_0()
                        .px_2()
                        .py_1()
                        .rounded(Radius::Control.px())
                        .bg(theme.surface_chrome)
                        .text_size(TextRole::Metadata.size())
                        .line_height(TextRole::Metadata.line_height())
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme.text_tertiary)
                        .child(language.localized(copy::configuration::DISABLED)),
                )
            });
        Self::manual_rule_context_menu(row, index, enabled, language, cx.entity())
    }

    pub(in crate::app) fn manual_rule_context_menu(
        row: Stateful<Div>,
        index: usize,
        enabled: bool,
        language: Language,
        app: Entity<Self>,
    ) -> AnyElement {
        let toggle_label = if enabled {
            language.message(Message::Disable)
        } else {
            language.message(Message::Enable)
        };
        row.context_menu(move |menu, _, _| {
            let toggle_app = app.clone();
            let delete_app = app.clone();
            menu.item(PopupMenuItem::new(toggle_label).on_click(move |_, _, cx| {
                toggle_app.update(cx, |this, cx| {
                    this.set_manual_rule_enabled(index, !enabled, cx);
                });
            }))
            .separator()
            .item(
                PopupMenuItem::new(language.message(Message::Delete)).on_click(move |_, _, cx| {
                    delete_app.update(cx, |this, cx| {
                        this.remove_manual_rule(index, cx);
                    });
                }),
            )
        })
        .into_any_element()
    }

    pub(in crate::app) fn manual_rule_matchers(
        rule: &crate::manual_rule::ManualRule,
        enabled: bool,
        theme: Theme,
        language: Language,
    ) -> Div {
        let primary_text = if enabled {
            theme.text_primary
        } else {
            theme.text_tertiary
        };
        let secondary_text = if enabled {
            theme.text_secondary
        } else {
            theme.text_tertiary
        };
        if rule.is_final() {
            return div()
                .flex_1()
                .min_w(px(0.0))
                .flex()
                .items_center()
                .gap(Space::Sm.px())
                .child(
                    div()
                        .px(Space::Sm.px())
                        .py(Space::Xs.px())
                        .rounded(Radius::Control.px())
                        .bg(theme.surface_high)
                        .text_size(TextRole::Label.size())
                        .line_height(TextRole::Label.line_height())
                        .font_weight(TextRole::Label.weight())
                        .text_color(primary_text)
                        .child("FINAL"),
                )
                .child(
                    div()
                        .text_size(TextRole::Metadata.size())
                        .line_height(TextRole::Metadata.line_height())
                        .text_color(secondary_text)
                        .child(language.localized(copy::configuration::FALLBACK_ALWAYS_LAST)),
                );
        }
        let mut matchers = div()
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .flex_wrap()
            .items_center()
            .gap_1();
        for (condition_index, condition) in rule.conditions().iter().enumerate() {
            if condition_index > 0 {
                matchers = matchers.child(
                    div()
                        .mx_1()
                        .text_size(TextRole::Metadata.size())
                        .line_height(TextRole::Metadata.line_height())
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme.text_tertiary)
                        .child(language.localized(copy::configuration::AND)),
                );
            }
            matchers = matchers.child(
                div()
                    .flex()
                    .items_center()
                    .gap(Space::Sm.px())
                    .px(Space::Sm.px())
                    .py(Space::Xs.px())
                    .rounded(Radius::Control.px())
                    .bg(theme.surface_high)
                    .child(
                        div()
                            .text_size(TextRole::Metadata.size())
                            .line_height(TextRole::Metadata.line_height())
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(secondary_text)
                            .child(condition.kind().display_label()),
                    )
                    .child(
                        div()
                            .text_size(TextRole::Data.size())
                            .line_height(TextRole::Data.line_height())
                            .text_color(primary_text)
                            .child(condition.parameter().to_owned()),
                    ),
            );
        }
        matchers
    }

    pub(in crate::app) fn rule_group_order_controls(
        &self,
        group_id: &str,
        group_name: &str,
        position: (usize, usize),
        _theme: Theme,
        language: Language,
        cx: &mut Context<Self>,
    ) -> Div {
        let (position, group_count) = position;
        let up_id = group_id.to_owned();
        let down_id = group_id.to_owned();
        div()
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap(Space::Xs.px())
            .child(
                style_action_button(
                    Button::new(format!("move-rule-group-up-{group_id}"))
                        .accessibility_label(copy::configuration::move_rule_group(
                            language, group_name, true,
                        ))
                        .icon(IconName::ArrowUp)
                        .disabled(position == 0 || self.routing_apply_state.is_busy()),
                    ActionRole::Secondary,
                    ControlSize::Icon,
                )
                .when(
                    position == 0 || self.routing_apply_state.is_busy(),
                    gpui::Styled::cursor_default,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.move_routing_rule_group(&up_id, mihomo::MoveDirection::Up, cx);
                })),
            )
            .child(
                style_action_button(
                    Button::new(format!("move-rule-group-down-{group_id}"))
                        .accessibility_label(copy::configuration::move_rule_group(
                            language, group_name, false,
                        ))
                        .icon(IconName::ArrowDown)
                        .disabled(
                            position + 1 >= group_count || self.routing_apply_state.is_busy(),
                        ),
                    ActionRole::Secondary,
                    ControlSize::Icon,
                )
                .when(
                    position + 1 >= group_count || self.routing_apply_state.is_busy(),
                    gpui::Styled::cursor_default,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.move_routing_rule_group(&down_id, mihomo::MoveDirection::Down, cx);
                })),
            )
    }
}
