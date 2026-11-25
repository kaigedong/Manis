use gpui::AppContext as _;
use gpui_component::{WindowExt as _, checkbox::Checkbox};

use super::{
    ActionRole, AnyElement, Button, Context, ControlSize, Dialog, Disableable, FluentBuilder,
    Focusable, InteractiveElement, IntoElement, Language, LocalRuleGroupEditorState, LogLevel,
    ManisApp, Message, ParentElement, Space, StatefulInteractiveElement, Styled,
    SubscriptionTextInput, TextInputSpec, TextRole, Theme, Window, copy, dialog_footer_surface,
    dialog_header_surface, div, field_label, px, record_event, style_action_button, surface_dialog,
};

/// The local rule group editor: a name, one target policy and the group's enabled state.
impl ManisApp {
    pub(in crate::app) fn ensure_local_rule_group_input(
        &mut self,
        theme: Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let language = self.language();
        let placeholder =
            language.localized(copy::configuration::LOCAL_RULE_GROUP_NAME_PLACEHOLDER);
        if let Some(input) = self.local_rule_group_draft.name_input.as_ref() {
            input.update(cx, |input, cx| {
                input.set_theme(theme, self.dark, cx);
                input.set_placeholder(placeholder, cx);
            });
            return;
        }
        let input = cx.new(|cx| {
            SubscriptionTextInput::new_field(
                TextInputSpec::new(
                    "local-rule-group-name-input",
                    placeholder,
                    96,
                    theme,
                    self.dark,
                ),
                window,
                cx,
            )
        });
        self.local_rule_group_draft.name_input = Some(input);
    }

    pub(in crate::app) fn open_local_rule_group_editor_new(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if crate::app::windows::activate(
            cx.entity().entity_id(),
            crate::app::windows::AuxiliaryKind::LocalRuleGroup,
            cx,
        ) {
            return;
        }
        if self.local_rule_group_editor.is_open() {
            return;
        }
        self.local_rule_group_editor = LocalRuleGroupEditorState::Creating;
        self.local_rule_group_draft.error = None;
        self.local_rule_group_draft.target_popover_open = false;
        self.local_rule_group_draft.enabled = true;
        if let Some(input) = self.local_rule_group_draft.name_input.as_ref() {
            input.update(cx, SubscriptionTextInput::clear_without_event);
        }
        if let Some(target) = self.local_rule_group_targets().first() {
            self.local_rule_group_draft.target.clone_from(target);
        }
        self.open_local_rule_group_dialog(window, cx);
    }

    pub(in crate::app) fn open_local_rule_group_editor_for_edit(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if crate::app::windows::activate(
            cx.entity().entity_id(),
            crate::app::windows::AuxiliaryKind::LocalRuleGroup,
            cx,
        ) {
            return;
        }
        if self.local_rule_group_editor.is_open() {
            return;
        }
        let Some(group) = self.local_rule_groups.get(index) else {
            return;
        };
        let (name, target, enabled) = (
            group.name.clone(),
            group.target_policy.as_str().to_owned(),
            group.enabled,
        );
        self.local_rule_group_editor = LocalRuleGroupEditorState::Editing(index);
        self.local_rule_group_draft.error = None;
        self.local_rule_group_draft.target_popover_open = false;
        self.local_rule_group_draft.enabled = enabled;
        self.local_rule_group_draft.target = target;
        if let Some(input) = self.local_rule_group_draft.name_input.as_ref() {
            input.update(cx, |input, cx| input.set_value_without_event(name, cx));
        }
        self.open_local_rule_group_dialog(window, cx);
    }

    fn open_local_rule_group_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.runtime.is_fixture() {
            crate::app::windows::open(
                cx.entity(),
                crate::app::windows::AuxiliaryKind::LocalRuleGroup,
                cx,
            );
            return;
        }
        let app = cx.entity();
        window.open_dialog(cx, move |dialog, window, cx| {
            app.update(cx, |this, cx| {
                let theme = this.theme();
                this.ensure_local_rule_group_input(theme, window, cx);
                this.local_rule_group_editor_modal(dialog, theme, this.language(), window, cx)
            })
        });
        if let Some(input) = self.local_rule_group_draft.name_input.as_ref() {
            input.focus_handle(cx).focus(window, cx);
        }
        cx.notify();
    }

    fn local_rule_group_editor_modal(
        &self,
        dialog: Dialog,
        theme: Theme,
        language: Language,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Dialog {
        let sections = self.local_rule_group_editor_sections(theme, language, cx);
        let viewport = window.viewport_size();
        let app = cx.entity();
        surface_dialog(dialog, theme, window)
            .width(px((viewport.width.as_f32() - 32.0).clamp(280.0, 560.0)))
            .margin_top(px(((viewport.height.as_f32() - 420.0) / 2.0).max(16.0)))
            .overlay(true)
            .overlay_closable(true)
            .keyboard(true)
            .close_button(false)
            .title(sections.header)
            .child(sections.body)
            .footer(sections.footer)
            .on_close(move |_, _, cx| {
                app.update(cx, ManisApp::close_local_rule_group_editor);
            })
    }

    pub(in crate::app) fn close_local_rule_group_editor(&mut self, cx: &mut Context<Self>) {
        self.local_rule_group_editor = LocalRuleGroupEditorState::Closed;
        self.local_rule_group_draft.error = None;
        self.local_rule_group_draft.target_popover_open = false;
        cx.notify();
    }

    pub(in crate::app) fn local_rule_group_targets(&self) -> Vec<String> {
        self.manual_rule_targets()
    }

    pub(in crate::app) fn set_local_rule_group_target(
        &mut self,
        target: String,
        cx: &mut Context<Self>,
    ) {
        self.local_rule_group_draft.target = target;
        self.local_rule_group_draft.error = None;
        self.local_rule_group_draft.target_popover_open = false;
        cx.notify();
    }

    pub(in crate::app) fn submit_local_rule_group(&mut self, cx: &mut Context<Self>) -> bool {
        if self.configuration_transfer.active || !self.local_rule_group_editor.is_open() {
            return false;
        }
        let editing = self.local_rule_group_editor.editing_index();
        let name = self
            .local_rule_group_draft
            .name_input
            .as_ref()
            .map(|input| input.read(cx).value().trim().to_owned())
            .unwrap_or_default();
        let target = self.local_rule_group_draft.target.clone();
        let enabled = self.local_rule_group_draft.enabled;
        let previous = self.routing_rollback_snapshot();
        let mut group = if let Some(index) = editing {
            let Some(group) = self.local_rule_groups.get(index).cloned() else {
                self.local_rule_group_draft.error =
                    Some(copy::configuration::LOCAL_RULE_GROUP_SAVE_FAILED);
                cx.notify();
                return false;
            };
            group
        } else {
            let Ok(group) = crate::mihomo::LocalRoutingRuleGroup::new(&name, &target) else {
                self.local_rule_group_draft.error =
                    Some(copy::configuration::LOCAL_RULE_GROUP_NAME_REQUIRED);
                cx.notify();
                return false;
            };
            group
        };
        if group.rename(&name).is_err() {
            self.local_rule_group_draft.error =
                Some(copy::configuration::LOCAL_RULE_GROUP_NAME_REQUIRED);
            cx.notify();
            return false;
        }
        if group.retarget(&target).is_err() {
            self.local_rule_group_draft.error =
                Some(copy::configuration::LOCAL_RULE_GROUP_TARGET_REQUIRED);
            cx.notify();
            return false;
        }
        group.enabled = enabled;
        let group_name = group.name.clone();
        match editing {
            Some(index) => self.local_rule_groups[index] = group,
            None => self.local_rule_groups.push(group),
        }
        let completion = copy::configuration::local_rule_group_saved(self.language(), &group_name);
        if !self.persist_routing_rules(completion, previous, cx) {
            return false;
        }
        self.local_rule_group_editor = LocalRuleGroupEditorState::Closed;
        self.local_rule_group_draft.error = None;
        record_event(
            LogLevel::Info,
            "routing.local_group.saved",
            format!("editing={} name={group_name}", editing.is_some()),
        );
        true
    }

    pub(in crate::app) fn set_local_rule_group_enabled(
        &mut self,
        index: usize,
        enabled: bool,
        cx: &mut Context<Self>,
    ) {
        if self.configuration_transfer.active {
            return;
        }
        let Some(group) = self.local_rule_groups.get_mut(index) else {
            return;
        };
        if group.enabled == enabled {
            return;
        }
        let previous = self.routing_rollback_snapshot();
        let Some(group) = self.local_rule_groups.get_mut(index) else {
            return;
        };
        group.enabled = enabled;
        let name = group.name.clone();
        let completion =
            copy::configuration::local_rule_group_enabled(self.language(), &name, enabled);
        if !self.persist_routing_rules(completion, previous, cx) {
            return;
        }
        record_event(
            LogLevel::Info,
            "routing.local_group.enabled",
            format!("index={index} enabled={enabled}"),
        );
        cx.notify();
    }

    /// Deleting a group takes its rules with it, so the confirmation states the count.
    pub(in crate::app) fn open_local_rule_group_delete(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.configuration_transfer.active || self.routing_apply_state.is_busy() {
            return;
        }
        if self.local_rule_groups.get(index).is_none() {
            return;
        }
        self.local_rule_group_delete = Some(index);
        let app = cx.entity();
        window.open_dialog(cx, move |dialog, window, cx| {
            app.update(cx, |this, cx| {
                this.local_rule_group_delete_dialog(
                    dialog,
                    this.theme(),
                    this.language(),
                    window,
                    cx,
                )
            })
        });
        cx.notify();
    }

    fn local_rule_group_delete_dialog(
        &self,
        dialog: Dialog,
        theme: Theme,
        language: Language,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Dialog {
        let Some(index) = self.local_rule_group_delete else {
            return surface_dialog(dialog, theme, window);
        };
        let Some(group) = self.local_rule_groups.get(index) else {
            return surface_dialog(dialog, theme, window);
        };
        let name = group.name.clone();
        let rules = group.rules.len();
        let app = cx.entity();
        surface_dialog(dialog, theme, window)
            .width(px(
                (window.viewport_size().width.as_f32() - 32.0).clamp(280.0, 480.0)
            ))
            .overlay(true)
            .overlay_closable(true)
            .keyboard(true)
            .close_button(false)
            .title(
                dialog_header_surface(theme).child(
                    div()
                        .text_size(px(17.0))
                        .font_weight(TextRole::SectionTitle.weight())
                        .child(language.localized(copy::configuration::DELETE_LOCAL_RULE_GROUP)),
                ),
            )
            .child(
                div()
                    .px_5()
                    .py_4()
                    .text_size(TextRole::Body.size())
                    .line_height(TextRole::Body.line_height())
                    .text_color(theme.text_secondary)
                    .child(copy::configuration::local_rule_group_delete_notice(
                        language, &name, rules,
                    )),
            )
            .footer(
                dialog_footer_surface(theme)
                    .child(
                        style_action_button(
                            Button::new("cancel-local-rule-group-delete")
                                .label(language.message(Message::Cancel)),
                            ActionRole::Secondary,
                            ControlSize::Standard,
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.local_rule_group_delete = None;
                            crate::app::windows::close_host(window, cx);
                        })),
                    )
                    .child(
                        style_action_button(
                            Button::new("confirm-local-rule-group-delete")
                                .label(language.message(Message::Delete)),
                            ActionRole::Danger,
                            ControlSize::Standard,
                        )
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                this.remove_local_rule_group(index, cx);
                                crate::app::windows::close_host(window, cx);
                            },
                        )),
                    ),
            )
            .on_close(move |_, _, cx| {
                app.update(cx, |this, _cx| this.local_rule_group_delete = None);
            })
    }

    pub(in crate::app) fn remove_local_rule_group(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.configuration_transfer.active {
            return;
        }
        let previous = self.routing_rollback_snapshot();
        if index >= self.local_rule_groups.len() {
            return;
        }
        let removed = self.local_rule_groups.remove(index);
        self.local_rule_group_delete = None;
        if self.local_rule_group_editor.editing_index() == Some(index) {
            self.local_rule_group_editor = LocalRuleGroupEditorState::Closed;
        }
        let completion = copy::configuration::local_rule_group_removed(
            self.language(),
            &removed.name,
            removed.rules.len(),
        );
        if !self.persist_routing_rules(completion, previous, cx) {
            return;
        }
        record_event(
            LogLevel::Info,
            "routing.local_group.removed",
            format!("rules={}", removed.rules.len()),
        );
        cx.notify();
    }

    pub(in crate::app) fn local_rule_group_editor_sections(
        &self,
        theme: Theme,
        language: Language,
        cx: &mut Context<Self>,
    ) -> crate::app::windows::EditorSections {
        crate::app::windows::EditorSections::new(
            self.local_rule_group_editor_header(theme, language),
            self.local_rule_group_editor_body(theme, language, cx),
            self.local_rule_group_editor_footer(theme, language, cx),
        )
    }

    fn local_rule_group_editor_header(&self, theme: Theme, language: Language) -> AnyElement {
        let editing = self.local_rule_group_editor.editing_index().is_some();
        dialog_header_surface(theme)
            .child(
                div()
                    .text_size(px(17.0))
                    .font_weight(TextRole::SectionTitle.weight())
                    .child(if editing {
                        language.localized(copy::configuration::EDIT_LOCAL_RULE_GROUP)
                    } else {
                        language.localized(copy::configuration::ADD_LOCAL_RULE_GROUP)
                    }),
            )
            .child(
                div()
                    .mt_1()
                    .text_size(TextRole::Metadata.size())
                    .text_color(theme.text_secondary)
                    .child(language.localized(
                        copy::configuration::THE_TARGET_POLICY_IS_USED_BY_EVERY_RULE_IN_THIS,
                    )),
            )
            .into_any_element()
    }

    fn local_rule_group_editor_body(
        &self,
        theme: Theme,
        language: Language,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let name_input = self
            .local_rule_group_draft
            .name_input
            .as_ref()
            .expect("local rule group name input is initialized before rendering")
            .clone();
        let draft = &self.local_rule_group_draft;
        div()
            .id("local-rule-group-modal-body")
            .flex_1()
            .min_h(px(0.0))
            .overflow_y_scroll()
            .px_5()
            .py_4()
            .child(field_label(
                language.localized(copy::configuration::LOCAL_RULE_GROUP_NAME),
                theme,
            ))
            .child(name_input)
            .child(
                field_label(
                    language.localized(copy::configuration::TARGET_POLICY),
                    theme,
                )
                .mt_4(),
            )
            .child(self.local_rule_group_target_select(theme, language, cx))
            .child(
                div()
                    .mt_4()
                    .child(
                        Checkbox::new("local-rule-group-enabled")
                            .label(language.localized(copy::configuration::LOCAL_RULE_GROUP_ENABLED))
                            .map(crate::components::primary_button_interaction)
                            .checked(draft.enabled)
                            .tab_stop(true)
                            .cursor_pointer()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.local_rule_group_draft.enabled =
                                    !this.local_rule_group_draft.enabled;
                                cx.notify();
                            })),
                    )
                    .child(
                        div()
                            .mt_1()
                            .text_size(TextRole::Metadata.size())
                            .line_height(TextRole::Metadata.line_height())
                            .text_color(theme.text_tertiary)
                            .child(language.localized(
                                copy::configuration::THE_TARGET_POLICY_IS_USED_BY_EVERY_RULE_IN_THIS,
                            )),
                    ),
            )
            .when_some(draft.error, |body, error| {
                body.child(
                    div()
                        .mt_3()
                        .p_3()
                        .rounded_md()
                        .bg(theme.surface_low)
                        .text_size(TextRole::Body.size())
                        .line_height(TextRole::Body.line_height())
                        .text_color(theme.status_error)
                        .child(language.localized(error)),
                )
            })
            .into_any_element()
    }

    fn local_rule_group_editor_footer(
        &self,
        theme: Theme,
        language: Language,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let editing = self.local_rule_group_editor.editing_index().is_some();
        let busy = self.routing_apply_state.is_busy();
        dialog_footer_surface(theme)
            .child(
                style_action_button(
                    Button::new("cancel-local-rule-group")
                        .debug_selector(|| "cancel-local-rule-group".into())
                        .label(language.message(Message::Cancel)),
                    ActionRole::Secondary,
                    ControlSize::Standard,
                )
                .on_click(cx.listener(|this, _, window, cx| {
                    this.close_local_rule_group_editor(cx);
                    crate::app::windows::close_host(window, cx);
                })),
            )
            .child(
                style_action_button(
                    Button::new("save-local-rule-group")
                        .debug_selector(|| "save-local-rule-group".into())
                        .label(if editing {
                            language.message(Message::SaveChanges)
                        } else {
                            language.localized(copy::configuration::ADD_LOCAL_RULE_GROUP)
                        })
                        .disabled(busy),
                    ActionRole::Primary,
                    ControlSize::Standard,
                )
                .when(busy, gpui::Styled::cursor_default)
                .on_click(cx.listener(|this, _, window, cx| {
                    if this.submit_local_rule_group(cx) {
                        crate::app::windows::close_host(window, cx);
                    }
                })),
            )
            .into_any_element()
    }

    fn local_rule_group_target_select(
        &self,
        theme: Theme,
        language: Language,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut menu = crate::components::popover_menu_list("local-rule-group-target-choices");
        for target in self.local_rule_group_targets() {
            let selected = self.local_rule_group_draft.target == target;
            let row_target = target.clone();
            let row_id = format!("local-rule-group-target-{target}");
            menu = menu.child(crate::components::popover_menu_option(
                row_id,
                target,
                None,
                selected,
                true,
                theme,
                cx.listener(move |this, _, _, cx| {
                    this.set_local_rule_group_target(row_target.clone(), cx);
                }),
            ));
        }
        let trigger = crate::components::style_select_trigger(
            Button::new("local-rule-group-target-select")
                .accessibility_label(language.localized(copy::configuration::CHOOSE_TARGET_POLICY))
                .dropdown_caret(true)
                .w_full()
                .child(
                    div()
                        .max_w(px(320.0))
                        .overflow_x_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_size(TextRole::Label.size())
                        .line_height(TextRole::Label.line_height())
                        .font_weight(TextRole::Label.weight())
                        .child(self.local_rule_group_draft.target.clone()),
                ),
            ControlSize::Standard,
        );
        let app = cx.entity();
        crate::components::anchored_popover(
            "local-rule-group-target-popover",
            trigger,
            menu,
            280.0,
            320.0,
        )
        .open(self.local_rule_group_draft.target_popover_open)
        .on_open_change(move |open, _, cx| {
            app.update(cx, |this, cx| {
                this.local_rule_group_draft.target_popover_open = *open;
                cx.notify();
            });
        })
        .into_any_element()
    }

    /// Adds a rule to one local group; the group keeps deciding its target.
    pub(in crate::app) fn local_rule_group_add_rule(
        &self,
        group_index: usize,
        language: Language,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .mt(Space::Sm.px())
            .child(
                style_action_button(
                    Button::new(format!("local-rule-group-add-rule-{group_index}"))
                        .label(language.localized(copy::configuration::ADD_RULE_TO_THIS_GROUP))
                        .accessibility_label(
                            language.localized(copy::configuration::ADD_RULE_TO_THIS_GROUP),
                        ),
                    ActionRole::Secondary,
                    ControlSize::Compact,
                )
                .debug_selector(move || format!("local-rule-group-add-rule-{group_index}"))
                .disabled(self.routing_apply_state.is_busy())
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.open_local_rule_editor_for_create(group_index, window, cx);
                })),
            )
            .into_any_element()
    }
}
