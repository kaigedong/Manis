use gpui_component::WindowExt as _;

use super::{
    ActionRole, AnyElement, Button, Context, ControlSize, Dialog, Disableable, Div, FluentBuilder,
    InteractiveElement, Language, ManisApp, ManualRulePopover, Message, ParentElement, Space,
    Stateful, StatefulInteractiveElement, Styled, TextRole, Theme, Window, copy,
    dialog_footer_surface, dialog_header_surface, div, field_label, manual_rule_error_label, px,
    style_action_button, surface_dialog,
};

impl ManisApp {
    pub(in crate::app) fn manual_rule_editor_modal(
        &self,
        dialog: Dialog,
        theme: Theme,
        language: Language,
        compact: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Dialog {
        let target_width = if compact { 260.0 } else { 240.0 };
        let target_open = self.manual_rule_popover == Some(ManualRulePopover::Target);
        let target_menu = self.manual_rule_target_menu(theme, cx);
        let app = cx.entity();
        let target = Self::manual_rule_select(
            "manual-rule-target-select",
            language.localized(copy::configuration::CHOOSE_TARGET_POLICY),
            self.manual_rule_target.clone(),
            target_menu,
            target_open,
            target_width,
            move |open, _, cx| {
                app.update(cx, |this, cx| {
                    this.manual_rule_popover = open.then_some(ManualRulePopover::Target);
                    cx.notify();
                });
            },
        );
        let editing = self.manual_rule_editor_state.editing_index().is_some();
        let final_selected = self
            .manual_rule_conditions
            .first()
            .map(|condition| condition.kind)
            == Some(crate::manual_rule::ManualRuleKind::Final);
        let conditions =
            self.manual_rule_editor_conditions(final_selected, compact, theme, language, cx);
        let body = self.manual_rule_editor_body(conditions, target, theme, language);
        let footer = self.manual_rule_editor_footer(editing, theme, language, cx);

        let viewport = window.viewport_size();
        let dialog_width = (viewport.width.as_f32() - 32.0).clamp(280.0, 720.0);
        let estimated_height = if compact {
            520.0
        } else {
            match self.manual_rule_condition_count {
                0 | 1 => 368.0,
                2 => 458.0,
                3 => 548.0,
                _ => 638.0,
            }
        };
        let margin_top = ((viewport.height.as_f32() - estimated_height) / 2.0).max(16.0);
        let app = cx.entity();

        surface_dialog(dialog, theme)
            .width(px(dialog_width))
            .max_h(px((viewport.height.as_f32() - 32.0).max(320.0)))
            .margin_top(px(margin_top))
            .overlay(true)
            .overlay_closable(true)
            .keyboard(true)
            .close_button(false)
            .title(Self::manual_rule_editor_title(
                editing,
                final_selected,
                theme,
                language,
            ))
            .child(body)
            .footer(footer)
            .on_close(move |_, _, cx| {
                app.update(cx, ManisApp::close_manual_rule_editor);
            })
    }

    pub(in crate::app) fn manual_rule_editor_conditions(
        &self,
        final_selected: bool,
        compact: bool,
        theme: Theme,
        language: Language,
        cx: &mut Context<Self>,
    ) -> Div {
        let mut conditions = div();
        for index in 0..self.manual_rule_condition_count {
            conditions = conditions.child(self.manual_rule_condition_editor(
                index,
                self.manual_rule_conditions[index].kind,
                theme,
                language,
                compact,
                cx,
            ));
        }
        if final_selected || self.manual_rule_condition_count >= crate::manual_rule::MAX_CONDITIONS
        {
            return conditions;
        }
        conditions.child(
            style_action_button(
                Button::new("add-manual-rule-condition")
                    .accessibility_label(
                        language.localized(copy::configuration::ADD_AN_AND_CONDITION),
                    )
                    .label(language.localized(copy::configuration::ADD_AND_CONDITION))
                    .mt(Space::Md.px()),
                ActionRole::Primary,
                ControlSize::Standard,
            )
            .on_click(cx.listener(|this, _, _, cx| this.add_manual_rule_condition(cx))),
        )
    }

    pub(in crate::app) fn manual_rule_editor_body(
        &self,
        conditions: Div,
        target: AnyElement,
        theme: Theme,
        language: Language,
    ) -> Stateful<Div> {
        div()
            .id("manual-rule-modal-body")
            .flex_1()
            .min_h(px(0.0))
            .overflow_y_scroll()
            .px_5()
            .py_4()
            .child(conditions)
            .child(
                div()
                    .mt_4()
                    .child(field_label(
                        language.localized(copy::configuration::POLICY_GROUP_AFTER_MATCH),
                        theme,
                    ))
                    .child(target),
            )
            .when_some(self.manual_rule_error, |body, error| {
                body.child(
                    div()
                        .mt_3()
                        .p_3()
                        .rounded_md()
                        .bg(theme.surface_low)
                        .text_size(TextRole::Body.size())
                        .line_height(TextRole::Body.line_height())
                        .text_color(theme.status_error)
                        .child(manual_rule_error_label(error, language)),
                )
            })
    }

    pub(in crate::app) fn manual_rule_editor_footer(
        &self,
        editing: bool,
        theme: Theme,
        language: Language,
        cx: &mut Context<Self>,
    ) -> Div {
        dialog_footer_surface(theme)
            .child(
                style_action_button(
                    Button::new("cancel-manual-rule").label(language.message(Message::Cancel)),
                    ActionRole::Secondary,
                    ControlSize::Standard,
                )
                .on_click(cx.listener(|this, _, window, cx| {
                    this.close_manual_rule_editor(cx);
                    window.close_dialog(cx);
                })),
            )
            .child(
                style_action_button(
                    Button::new("save-manual-rule")
                        .label(if editing {
                            language.message(Message::SaveChanges)
                        } else {
                            language.message(Message::AddRule)
                        })
                        .disabled(self.routing_apply_state.is_busy()),
                    ActionRole::Primary,
                    ControlSize::Standard,
                )
                .when(
                    self.routing_apply_state.is_busy(),
                    gpui::Styled::cursor_default,
                )
                .on_click(cx.listener(|this, _, window, cx| {
                    if this.submit_manual_rule(cx) {
                        window.close_dialog(cx);
                    }
                })),
            )
    }

    pub(in crate::app) fn manual_rule_editor_title(
        editing: bool,
        final_selected: bool,
        theme: Theme,
        language: Language,
    ) -> Stateful<Div> {
        dialog_header_surface(theme)
        .id("manual-rule-modal-header")
        .child(
            div()
                .text_size(px(17.0))
                .font_weight(TextRole::SectionTitle.weight())
                .child(if editing {
                    language.localized(copy::configuration::EDIT_ROUTING_RULE)
                } else {
                    language.localized(copy::configuration::ADD_ROUTING_RULE)
                }),
        )
        .child(
            div()
                .mt_1()
                .text_size(TextRole::Metadata.size())
                .text_color(theme.text_secondary)
                .child(if final_selected {
                    language.localized(copy::configuration::FINAL_IS_ALWAYS_EVALUATED_LAST_AND_HANDLES_UNMATCHED_TRAFFIC)
                } else {
                    language.localized(copy::configuration::ALL_CONDITIONS_MUST_MATCH_GROUP_ORDER_DETERMINES_RULE_PRIORITY)
                }),
        )
    }
}
