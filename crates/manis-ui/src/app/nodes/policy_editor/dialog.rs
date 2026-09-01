use super::{
    ActionRole, Button, Context, ControlSize, Dialog, Disableable, Div, FluentBuilder, Focusable,
    IntoElement, Language, ManagedPolicyDraft, ManisApp, Message, ParentElement, Styled, TextRole,
    Theme, Window, WindowExt, WindowSizeClass, copy, dialog_footer_surface, dialog_header_surface,
    div, px, status_badge, style_action_button, surface_dialog,
};

impl ManisApp {
    pub(in crate::app) fn open_managed_policy_create(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.start_managed_policy_create(cx);
        Self::open_managed_policy_dialog(window, cx);
        if let Some(input) = self.inputs.policy_group_name.as_ref() {
            input.focus_handle(cx).focus(window, cx);
        }
    }

    pub(in crate::app) fn open_managed_policy_settings(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.start_managed_policy_edit(id, cx);
        if self.managed_policies.draft.is_none() {
            return;
        }
        Self::open_managed_policy_dialog(window, cx);
        if let Some(input) = self.inputs.policy_group_name.as_ref() {
            input.focus_handle(cx).focus(window, cx);
        }
    }

    pub(in crate::app) fn open_managed_policy_dialog(window: &mut Window, cx: &mut Context<Self>) {
        let app = cx.entity();
        window.open_dialog(cx, move |dialog, window, cx| {
            app.update(cx, |this, cx| {
                let theme = this.theme();
                this.ensure_policy_group_inputs(theme, window, cx);
                this.set_policy_group_editor_enabled(
                    !this.managed_policies.mutation_state.is_busy(),
                    cx,
                );
                let compact = WindowSizeClass::for_width(window.viewport_size().width.as_f32())
                    == WindowSizeClass::Compact;
                this.managed_policy_editor_modal(
                    dialog,
                    compact,
                    this.language(),
                    theme,
                    window,
                    cx,
                )
            })
        });
        cx.notify();
    }

    pub(in crate::app) fn set_policy_group_editor_enabled(
        &mut self,
        enabled: bool,
        cx: &mut Context<Self>,
    ) {
        for input in [
            self.inputs.policy_group_name.as_ref(),
            self.inputs.policy_group_filter.as_ref(),
        ]
        .into_iter()
        .flatten()
        {
            input.update(cx, |input, cx| input.set_enabled(enabled, cx));
        }
    }

    pub(in crate::app) fn close_managed_policy_editor(&mut self, cx: &mut Context<Self>) {
        self.managed_policies.draft = None;
        self.managed_policies.editor_popover = None;
        self.language()
            .localized(copy::nodes::POLICY_EDITING_CANCELLED)
            .clone_into(&mut self.status);
        cx.notify();
    }

    pub(in crate::app) fn managed_policy_editor_modal(
        &self,
        dialog: Dialog,
        compact: bool,
        language: Language,
        theme: Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Dialog {
        let viewport = window.viewport_size();
        let width = (viewport.width.as_f32() - 32.0).clamp(320.0, 780.0);
        let max_height = (viewport.height.as_f32() - 32.0).max(360.0);
        let margin_top = ((viewport.height.as_f32() - 640.0) / 2.0).max(16.0);
        let app = cx.entity();
        let busy = self.managed_policies.mutation_state.is_busy();
        let (title, body, footer) = if let Some(draft) = self.managed_policies.draft.as_ref() {
            (
                Self::managed_policy_editor_title(draft, language, theme),
                self.policy_editor_form(draft, compact, false, language, theme, cx)
                    .into_any_element(),
                self.managed_policy_editor_footer(draft, language, theme, cx),
            )
        } else {
            (
                Self::managed_policy_editor_completed_title(language, theme),
                div()
                    .p_5()
                    .text_size(TextRole::Body.size())
                    .line_height(TextRole::Body.line_height())
                    .text_color(theme.text_secondary)
                    .child(self.status.clone())
                    .into_any_element(),
                Self::managed_policy_editor_completed_footer(language, theme, cx),
            )
        };

        surface_dialog(dialog, theme)
            .width(px(width))
            .max_h(px(max_height))
            .margin_top(px(margin_top))
            .overlay(true)
            .overlay_closable(!busy)
            .keyboard(!busy)
            .close_button(false)
            .title(title)
            .child(body)
            .footer(footer)
            .on_close(move |_, _, cx| {
                app.update(cx, |this, cx| {
                    if !this.managed_policies.mutation_state.is_busy() {
                        this.managed_policies.draft = None;
                        this.managed_policies.editor_popover = None;
                        cx.notify();
                    }
                });
            })
    }

    pub(in crate::app) fn managed_policy_editor_title(
        draft: &ManagedPolicyDraft,
        language: Language,
        theme: Theme,
    ) -> Div {
        let title = if draft.editing_id.is_some() {
            language.localized(copy::common::EDIT_POLICY_GROUP)
        } else {
            language.localized(copy::nodes::NEW_POLICY_GROUP)
        };
        dialog_header_surface(theme).child(
            div()
                .text_size(TextRole::SectionTitle.size())
                .line_height(TextRole::SectionTitle.line_height())
                .font_weight(TextRole::SectionTitle.weight())
                .text_color(theme.text_primary)
                .child(title),
        )
    }

    pub(in crate::app) fn managed_policy_editor_completed_title(
        language: Language,
        theme: Theme,
    ) -> Div {
        dialog_header_surface(theme)
            .flex()
            .items_center()
            .justify_between()
            .child(
                div()
                    .text_size(TextRole::SectionTitle.size())
                    .line_height(TextRole::SectionTitle.line_height())
                    .font_weight(TextRole::SectionTitle.weight())
                    .child(language.localized(copy::nodes::GROUP_SAVED)),
            )
            .child(status_badge(
                language.localized(copy::nodes::DONE),
                crate::components::StatusTone::Success,
                theme,
            ))
    }

    pub(in crate::app) fn managed_policy_editor_footer(
        &self,
        draft: &ManagedPolicyDraft,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        let editing_id = draft.editing_id.clone();
        let busy = self.managed_policies.mutation_state.is_busy();
        dialog_footer_surface(theme)
            .flex_col()
            .items_stretch()
            .gap_3()
            .child(
                div()
                    .text_size(TextRole::Metadata.size())
                    .line_height(TextRole::Metadata.line_height())
                    .text_color(theme.text_secondary)
                    .child(self.status.clone()),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap_2()
                    .when_some(editing_id.clone(), |actions, remove_id| {
                        actions.child(
                            style_action_button(
                                Button::new("remove-managed-policy-dialog")
                                    .label(language.localized(copy::app::DELETE_POLICY_GROUP))
                                    .disabled(busy),
                                ActionRole::Danger,
                                ControlSize::Standard,
                            )
                            .when(busy, gpui::Styled::cursor_default)
                            .on_click(cx.listener(
                                move |this, _, window, cx| {
                                    if !busy {
                                        this.remove_managed_policy_from_dialog(
                                            &remove_id, window, cx,
                                        );
                                    }
                                },
                            )),
                        )
                    })
                    .child(
                        style_action_button(
                            Button::new("cancel-managed-policy-dialog")
                                .label(language.message(Message::Cancel))
                                .disabled(busy),
                            ActionRole::Secondary,
                            ControlSize::Standard,
                        )
                        .when(busy, gpui::Styled::cursor_default)
                        .on_click(cx.listener(|this, _, window, cx| {
                            if !this.managed_policies.mutation_state.is_busy() {
                                this.close_managed_policy_editor(cx);
                                window.close_dialog(cx);
                            }
                        })),
                    )
                    .child(
                        style_action_button(
                            Button::new("save-managed-policy-dialog")
                                .label(if busy {
                                    language.localized(copy::nodes::APPLYING_CHANGES)
                                } else if editing_id.is_some() {
                                    language.message(Message::SaveChanges)
                                } else {
                                    language.message(Message::AddPolicyGroup)
                                })
                                .loading(busy)
                                .disabled(busy),
                            ActionRole::Primary,
                            ControlSize::Standard,
                        )
                        .when(busy, gpui::Styled::cursor_default)
                        .on_click(cx.listener(|this, _, window, cx| {
                            if !this.managed_policies.mutation_state.is_busy() {
                                this.save_managed_policy_from_dialog(window, cx);
                            }
                        })),
                    ),
            )
    }

    pub(in crate::app) fn managed_policy_editor_completed_footer(
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        dialog_footer_surface(theme).child(
            style_action_button(
                Button::new("close-managed-policy-dialog")
                    .label(language.localized(copy::nodes::DONE)),
                ActionRole::Primary,
                ControlSize::Standard,
            )
            .on_click(cx.listener(|_, _, window, cx| {
                window.close_dialog(cx);
            })),
        )
    }
}
