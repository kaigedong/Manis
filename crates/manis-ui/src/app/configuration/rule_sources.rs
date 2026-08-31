impl ManisApp {
    fn rule_source_manager(
        &self,
        _input: Entity<SubscriptionTextInput>,
        busy: bool,
        theme: Theme,
        compact: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let language = self.language();
        let add_action = action_button(
            "configuration-add-rule-source",
            language.localized(copy::configuration::ADD_SOURCE),
            ActionRole::Primary,
            ControlSize::Compact,
        )
        .on_click(cx.listener(move |this, _, window, cx| {
            this.open_new_qx_rule_editor(cx);
            this.open_qx_rule_source_dialog(window, cx);
        }));
        let mut panel = panel_surface("configuration-rule-sources", compact, theme)
            .child(section_heading(
                language.localized(copy::configuration::RULE_SOURCES),
                language.count(CountNoun::Source, self.rule_sources.sources.len()),
                Some(add_action.into_any_element()),
                theme,
            ))
            .child(
                div()
                    .mt(Space::Lg.px())
                    .pt(Space::Md.px())
                    .border_t_1()
                    .border_color(theme.outline_subtle)
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(TextRole::Label.size())
                            .line_height(TextRole::Label.line_height())
                            .font_weight(TextRole::Label.weight())
                            .child(language.localized(copy::common::SAVED)),
                    ),
            );

        if self.rule_sources.sources.is_empty() {
            panel = panel.child(
                empty_state(
                    language.localized(copy::configuration::NO_RULE_SOURCES),
                    language.localized(copy::configuration::ADD_A_REMOTE_QX_RULE_SET),
                    Some(
                        action_button(
                            "configuration-empty-add-rule-source",
                            language.localized(copy::configuration::ADD_SOURCE),
                            ActionRole::Primary,
                            ControlSize::Compact,
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.open_new_qx_rule_editor(cx);
                            this.open_qx_rule_source_dialog(window, cx);
                        }))
                        .into_any_element(),
                    ),
                    theme,
                )
                .mt(Space::Md.px()),
            );
        }
        for (index, source) in self.rule_sources.sources.iter().enumerate() {
            panel = panel.child(self.rule_source_card(index, source, busy, theme, cx));
        }
        panel
    }

    fn open_new_qx_rule_editor(&mut self, cx: &mut Context<Self>) {
        self.rule_sources.editor_source_id = None;
        self.rule_sources.editor_refresh_interval = RemoteSourceRefreshInterval::Manual;
        self.rule_sources.editor_popover = super::QxRuleEditorPopover::None;
        self.rule_sources.feedback = QxRuleImportFeedback::Idle;
        if !self
            .qx_rule_targets()
            .contains(&self.rule_sources.target_policy)
            && let Some(target) = self.qx_rule_targets().into_iter().next()
        {
            self.rule_sources.target_policy = target;
        }
        if let Some(input) = self.inputs.qx_rule.as_ref() {
            input.update(cx, SubscriptionTextInput::clear_without_event);
        }
        if let Some(input) = self.inputs.qx_rule_name.as_ref() {
            input.update(cx, SubscriptionTextInput::clear_without_event);
        }
        cx.notify();
    }

    fn open_qx_rule_editor(&mut self, id: String, cx: &mut Context<Self>) {
        let Some((index, source)) = self
            .rule_sources
            .sources
            .iter()
            .enumerate()
            .find(|(_, source)| source.id == id)
        else {
            return;
        };
        let url = source.source.expose_to(str::to_owned);
        let name = Self::qx_rule_source_name(source, index, self.language());
        let target = self.effective_rule_target(source.target_policy.as_str(), self.language());
        self.rule_sources.editor_source_id = Some(id);
        self.rule_sources.editor_refresh_interval = source.refresh_interval;
        self.rule_sources.editor_popover = super::QxRuleEditorPopover::None;
        self.rule_sources.target_policy = target;
        self.rule_sources.feedback = QxRuleImportFeedback::Idle;
        if let Some(input) = self.inputs.qx_rule.as_ref() {
            input.update(cx, |input, cx| input.set_value_without_event(url, cx));
        }
        if let Some(input) = self.inputs.qx_rule_name.as_ref() {
            input.update(cx, |input, cx| input.set_value_without_event(name, cx));
        }
        cx.notify();
    }

    fn open_qx_rule_source_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let app = cx.entity();
        window.open_dialog(cx, move |dialog, window, cx| {
            app.update(cx, |this, cx| {
                this.qx_rule_source_editor_modal(dialog, this.theme(), this.language(), window, cx)
            })
        });
        if let Some(input) = self.inputs.qx_rule_name.as_ref() {
            input.focus_handle(cx).focus(window, cx);
        }
        cx.notify();
    }

    fn close_qx_rule_editor(&mut self, cx: &mut Context<Self>) {
        self.rule_sources.editor_source_id = None;
        self.rule_sources.editor_popover = super::QxRuleEditorPopover::None;
        self.rule_sources.feedback = QxRuleImportFeedback::Idle;
        if let Some(input) = self.inputs.qx_rule.as_ref() {
            input.update(cx, SubscriptionTextInput::clear_without_event);
        }
        if let Some(input) = self.inputs.qx_rule_name.as_ref() {
            input.update(cx, SubscriptionTextInput::clear_without_event);
        }
        cx.notify();
    }

    fn qx_rule_source_editor_modal(
        &self,
        dialog: Dialog,
        theme: Theme,
        language: Language,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Dialog {
        let input = self
            .inputs
            .qx_rule
            .as_ref()
            .expect("QX rule input is initialized before rendering")
            .clone();
        let viewport = window.viewport_size();
        let view = QxRuleEditorView {
            editing: self.rule_sources.editor_source_id.is_some(),
            busy: self.rule_sources.feedback == QxRuleImportFeedback::Importing,
            dialog_width: (viewport.width.as_f32() - 32.0).clamp(300.0, 620.0),
        };
        let body = div()
            .id("qx-rule-source-modal-body")
            .flex_1()
            .min_h(px(0.0))
            .overflow_y_scroll()
            .px_5()
            .py_4()
            .child(field_label(
                language.localized(copy::configuration::SOURCE_NAME),
                theme,
            ))
            .child(
                self.inputs
                    .qx_rule_name
                    .as_ref()
                    .expect("QX rule name input is initialized before rendering")
                    .clone(),
            )
            .child(field_label(language.localized(copy::configuration::RULE_URL), theme).mt_4())
            .child(input.clone())
            .child(
                field_label(
                    language.localized(copy::configuration::TARGET_POLICY),
                    theme,
                )
                .mt_4(),
            )
            .child(self.qx_rule_target_select(view, language, theme, cx))
            .child(
                field_label(
                    language.localized(copy::configuration::UPDATE_INTERVAL),
                    theme,
                )
                .mt_4(),
            )
            .child(self.qx_rule_interval_select(view, language, theme, cx))
            .child(self.qx_rule_import_feedback(theme, language));
        let app = cx.entity();
        surface_dialog(dialog, theme)
            .width(px(view.dialog_width))
            .max_h(px((viewport.height.as_f32() - 32.0).max(320.0)))
            .margin_top(px(((viewport.height.as_f32() - 520.0) / 2.0).max(16.0)))
            .overlay(true)
            .overlay_closable(true)
            .keyboard(true)
            .close_button(false)
            .title(Self::qx_rule_editor_title(view.editing, language, theme))
            .child(body)
            .footer(Self::qx_rule_editor_footer(
                input, view, language, theme, cx,
            ))
            .on_close(move |_, _, cx| {
                app.update(cx, ManisApp::close_qx_rule_editor);
            })
    }

    fn qx_rule_target_select(
        &self,
        view: QxRuleEditorView,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut menu = div().p_1();
        for target in self.qx_rule_targets() {
            let selected = target == self.rule_sources.target_policy;
            menu = menu.child(Self::qx_rule_editor_option(
                format!("qx-rule-editor-target-{target}"),
                target.clone(),
                selected,
                theme,
                cx.listener(move |this, _, _, cx| {
                    this.rule_sources.target_policy.clone_from(&target);
                    this.rule_sources.editor_popover = super::QxRuleEditorPopover::None;
                    this.rule_sources.feedback = QxRuleImportFeedback::Idle;
                    cx.notify();
                }),
            ));
        }
        let trigger = Button::new("qx-rule-editor-target")
            .accessibility_label(language.localized(copy::configuration::CHOOSE_TARGET_POLICY))
            .dropdown_caret(true)
            .w_full()
            .child(self.rule_sources.target_policy.clone())
            .disabled(view.busy);
        let trigger = style_action_button(trigger, ActionRole::Secondary, ControlSize::Standard)
            .when(view.busy, gpui::Styled::cursor_default);
        let app = cx.entity();
        crate::components::anchored_popover(
            "qx-rule-editor-target-popover",
            trigger,
            menu,
            (view.dialog_width - 40.0).max(240.0),
            320.0,
        )
        .open(self.rule_sources.editor_popover == super::QxRuleEditorPopover::Target)
        .on_open_change(move |open, _, cx| {
            app.update(cx, |this, cx| {
                this.rule_sources.editor_popover = if *open {
                    super::QxRuleEditorPopover::Target
                } else {
                    super::QxRuleEditorPopover::None
                };
                cx.notify();
            });
        })
        .into_any_element()
    }

    fn qx_rule_interval_select(
        &self,
        view: QxRuleEditorView,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut menu = div().p_1();
        for interval in [
            RemoteSourceRefreshInterval::Manual,
            RemoteSourceRefreshInterval::Hourly,
            RemoteSourceRefreshInterval::SixHours,
            RemoteSourceRefreshInterval::TwelveHours,
            RemoteSourceRefreshInterval::Daily,
        ] {
            let label = refresh_interval_label(interval, language);
            menu = menu.child(Self::qx_rule_editor_option(
                format!("qx-rule-editor-interval-{interval:?}"),
                label.to_owned(),
                interval == self.rule_sources.editor_refresh_interval,
                theme,
                cx.listener(move |this, _, _, cx| {
                    this.rule_sources.editor_refresh_interval = interval;
                    this.rule_sources.editor_popover = super::QxRuleEditorPopover::None;
                    cx.notify();
                }),
            ));
        }
        let trigger = Button::new("qx-rule-editor-refresh-interval")
            .accessibility_label(
                language.localized(copy::configuration::CHOOSE_RULE_UPDATE_INTERVAL),
            )
            .dropdown_caret(true)
            .w_full()
            .child(refresh_interval_label(
                self.rule_sources.editor_refresh_interval,
                language,
            ))
            .disabled(view.busy);
        let trigger = style_action_button(trigger, ActionRole::Secondary, ControlSize::Standard)
            .when(view.busy, gpui::Styled::cursor_default);
        let app = cx.entity();
        crate::components::anchored_popover(
            "qx-rule-editor-refresh-popover",
            trigger,
            menu,
            (view.dialog_width - 40.0).max(240.0),
            280.0,
        )
        .open(self.rule_sources.editor_popover == super::QxRuleEditorPopover::Interval)
        .on_open_change(move |open, _, cx| {
            app.update(cx, |this, cx| {
                this.rule_sources.editor_popover = if *open {
                    super::QxRuleEditorPopover::Interval
                } else {
                    super::QxRuleEditorPopover::None
                };
                cx.notify();
            });
        })
        .into_any_element()
    }

    fn qx_rule_editor_option(
        id: impl Into<gpui::ElementId>,
        label: String,
        selected: bool,
        theme: Theme,
        listener: impl Fn(&gpui::ClickEvent, &mut Window, &mut gpui::App) + 'static,
    ) -> Stateful<Div> {
        div()
            .id(id)
            .role(Role::Button)
            .aria_label(label.clone())
            .tab_stop(true)
            .focusable()
            .map(crate::components::primary_button_interaction)
            .cursor_pointer()
            .px_3()
            .py_2()
            .rounded(Radius::Control.px())
            .bg(if selected {
                theme.action_soft
            } else {
                gpui::rgba(0x0000_0000)
            })
            .hover(move |style| {
                if selected {
                    style.bg(theme.action_soft)
                } else {
                    style.bg(theme.button_hover)
                }
            })
            .active(move |style| {
                if selected {
                    style.bg(theme.action_soft)
                } else {
                    style.bg(theme.button_active)
                }
            })
            .font_weight(if selected {
                FontWeight::SEMIBOLD
            } else {
                FontWeight::NORMAL
            })
            .text_color(if selected {
                theme.action_primary
            } else {
                theme.text_primary
            })
            .child(label)
            .on_click(listener)
    }

    fn qx_rule_editor_footer(
        input: Entity<SubscriptionTextInput>,
        view: QxRuleEditorView,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        dialog_footer_surface(theme)
            .child(
                style_action_button(
                    Button::new("cancel-qx-rule-source").label(language.message(Message::Cancel)),
                    ActionRole::Secondary,
                    ControlSize::Standard,
                )
                .on_click(cx.listener(|this, _, window, cx| {
                    this.close_qx_rule_editor(cx);
                    window.close_dialog(cx);
                })),
            )
            .child(
                style_action_button(
                    Button::new("save-qx-rule-source")
                        .label(if view.busy {
                            language.localized(copy::configuration::PROCESSING)
                        } else if view.editing {
                            language.message(Message::SaveChanges)
                        } else {
                            language.localized(copy::configuration::ADD_SOURCE)
                        })
                        .loading(view.busy),
                    ActionRole::Primary,
                    ControlSize::Standard,
                )
                .when(view.busy, gpui::Styled::cursor_default)
                .on_click(cx.listener(move |this, _, window, cx| {
                    if !view.busy && this.submit_qx_rule_import(&input, cx) {
                        window.close_dialog(cx);
                    }
                })),
            )
    }

    fn qx_rule_editor_title(editing: bool, language: Language, theme: Theme) -> Div {
        dialog_header_surface(theme)
            .child(
                div()
                    .text_size(px(17.0))
                    .font_weight(TextRole::SectionTitle.weight())
                    .child(if editing {
                        language.localized(copy::configuration::EDIT_RULE_SOURCE)
                    } else {
                        language.localized(copy::configuration::ADD_RULE_SOURCE)
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
    }

    fn rule_source_card(
        &self,
        index: usize,
        source: &mihomo::StoredQxRuleSource,
        busy: bool,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let language = self.language();
        let presentation = self.rule_source_card_presentation(index, source, busy, language);
        let edit_id = source.id.clone();
        let controls_enabled = presentation.controls_enabled;
        let toggle_id = source.id.clone();
        let enabled = source.enabled;
        div()
            .id(format!("qx-rule-source-card-{}", source.id))
            .role(Role::Button)
            .aria_label(language.localized(copy::configuration::EDIT_THIS_RULE_SOURCE))
            .tab_stop(controls_enabled)
            .focusable()
            .map(crate::components::primary_button_interaction)
            .when(controls_enabled, gpui::Styled::cursor_pointer)
            .mt(Space::Sm.px())
            .p(Space::Md.px())
            .rounded(Radius::Row.px())
            .border_1()
            .border_color(if presentation.duplicate {
                theme.status_warning
            } else {
                theme.outline_subtle
            })
            .bg(theme.surface_low)
            .flex()
            .items_center()
            .gap_2()
            .when(controls_enabled, |card| {
                card.hover(move |card| card.bg(theme.action_soft))
            })
            .child(
                Checkbox::new(format!("qx-rule-enabled-{toggle_id}"))
                    .block_mouse_except_scroll()
                    .aria_label(presentation.name.clone())
                    .flex_shrink_0()
                    .map(crate::components::primary_button_interaction)
                    .checked(enabled)
                    .disabled(!controls_enabled)
                    .tab_stop(controls_enabled)
                    .when(controls_enabled, gpui::Styled::cursor_pointer)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        if controls_enabled {
                            this.set_qx_rule_source_enabled(toggle_id.clone(), !enabled, cx);
                        }
                    })),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .child(Self::rule_source_card_header(
                        source,
                        &presentation,
                        language,
                        theme,
                    ))
                    .child(
                        div()
                            .mt_1()
                            .overflow_x_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(TextRole::Metadata.size())
                            .text_color(theme.text_tertiary)
                            .child(source.source.expose_to(str::to_owned)),
                    )
                    .when_some(
                        Self::rule_source_refresh_error(&presentation),
                        |card, error| card.child(Self::rule_source_error(error, language, theme)),
                    )
                    .child(Self::rule_source_card_actions(
                        index,
                        source,
                        &presentation,
                        language,
                        theme,
                        cx,
                    )),
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                if controls_enabled {
                    this.open_qx_rule_editor(edit_id.clone(), cx);
                    this.open_qx_rule_source_dialog(window, cx);
                }
            }))
    }

    fn rule_source_card_presentation(
        &self,
        index: usize,
        source: &mihomo::StoredQxRuleSource,
        busy: bool,
        language: Language,
    ) -> RuleSourceCardPresentation {
        let refresh = match self.rule_sources.refreshes.get(&source.id) {
            Some(QxRuleSourceRefreshState::Refreshing { .. }) => {
                RuleSourceRefreshPresentation::Refreshing
            }
            Some(QxRuleSourceRefreshState::Failed { message, .. }) => {
                RuleSourceRefreshPresentation::Failed(message.clone())
            }
            None => RuleSourceRefreshPresentation::Idle,
        };
        RuleSourceCardPresentation {
            name: Self::qx_rule_source_name(source, index, language),
            refresh,
            duplicate: matches!(
                &self.rule_sources.feedback,
                QxRuleImportFeedback::AlreadyExists { source_id, .. } if source_id == &source.id
            ),
            controls_enabled: !busy && !self.source_refresh_busy(),
            target_policy: self.effective_rule_target(source.target_policy.as_str(), language),
            last_update: source_update_label(
                source.last_successful_update_unix_secs,
                mihomo::current_unix_secs(),
                language,
            ),
        }
    }

    pub(super) fn qx_rule_source_name(
        source: &mihomo::StoredQxRuleSource,
        index: usize,
        language: Language,
    ) -> String {
        source
            .name
            .clone()
            .or_else(|| source.source.subscription_name())
            .unwrap_or_else(|| copy::configuration::numbered_rule_source(language, index + 1))
    }

    fn rule_source_card_header(
        source: &mihomo::StoredQxRuleSource,
        presentation: &RuleSourceCardPresentation,
        language: Language,
        theme: Theme,
    ) -> Div {
        let enabled = source.enabled;
        div()
            .flex()
            .items_center()
            .gap_2()
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .overflow_x_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_size(TextRole::Label.size())
                    .font_weight(TextRole::Label.weight())
                    .text_color(if enabled {
                        theme.text_primary
                    } else {
                        theme.text_secondary
                    })
                    .child(presentation.name.clone()),
            )
            .when(presentation.refresh.is_refreshing(), |header| {
                header.child(Self::benchmark_latency_spinner(
                    format!("qx-rule-refresh-{}", source.id),
                    theme,
                ))
            })
            .when(presentation.duplicate, |header| {
                header.child(Self::rule_source_state_label(
                    language.localized(copy::configuration::ALREADY_ADDED),
                    theme.status_warning,
                ))
            })
            .when(!enabled, |header| {
                header.child(Self::rule_source_state_label(
                    language.localized(copy::configuration::DISABLED_2),
                    theme.text_tertiary,
                ))
            })
    }

    fn rule_source_state_label(label: &'static str, color: gpui::Rgba) -> Div {
        div()
            .flex_shrink_0()
            .text_size(TextRole::Metadata.size())
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(color)
            .child(label)
    }

    fn rule_source_refresh_error(presentation: &RuleSourceCardPresentation) -> Option<&str> {
        match &presentation.refresh {
            RuleSourceRefreshPresentation::Failed(message) => Some(message),
            RuleSourceRefreshPresentation::Idle | RuleSourceRefreshPresentation::Refreshing => None,
        }
    }

    fn rule_source_error(error: &str, language: Language, theme: Theme) -> Div {
        div()
            .mt_1()
            .text_size(TextRole::Metadata.size())
            .line_height(TextRole::Metadata.line_height())
            .text_color(theme.route_trace)
            .child(format!(
                "{}: {error}",
                language.localized(copy::configuration::LAST_UPDATE_FAILED)
            ))
    }

    fn rule_source_card_actions(
        index: usize,
        source: &mihomo::StoredQxRuleSource,
        presentation: &RuleSourceCardPresentation,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        let refresh_id = source.id.clone();
        let remove_id = source.id.clone();
        let refreshing = presentation.refresh.is_refreshing();
        let refresh_enabled = presentation.controls_enabled && source.enabled;
        let controls_enabled = presentation.controls_enabled;
        div()
            .mt_1()
            .flex()
            .items_center()
            .gap_2()
            .text_size(TextRole::Metadata.size())
            .text_color(theme.text_secondary)
            .child(copy::configuration::rule_source_counts(
                language,
                source.rule_count,
                source.diagnostic_count,
            ))
            .child("·")
            .child(format!(
                "{} {}",
                language.localized(copy::configuration::TARGET),
                presentation.target_policy
            ))
            .child("·")
            .child(refresh_interval_label(source.refresh_interval, language))
            .child("·")
            .child(presentation.last_update.clone())
            .child(div().flex_1())
            .child(Self::rule_source_refresh_button(
                refresh_id,
                refreshing,
                refresh_enabled,
                language,
                theme,
                cx,
            ))
            .child(
                row_action_button(
                    format!("qx-rule-remove-{index}"),
                    language.localized(copy::configuration::REMOVE),
                    ActionRole::Danger,
                    ControlSize::Compact,
                )
                .disabled(!controls_enabled)
                .when(!controls_enabled, gpui::Styled::cursor_default)
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    if controls_enabled {
                        this.remove_qx_rule_source(remove_id.clone(), cx);
                    }
                })),
            )
    }

    fn rule_source_refresh_button(
        id: String,
        refreshing: bool,
        enabled: bool,
        language: Language,
        _theme: Theme,
        cx: &mut Context<Self>,
    ) -> Button {
        row_action_button(
            format!("qx-rule-refresh-{id}"),
            if refreshing {
                language.localized(copy::configuration::UPDATING)
            } else {
                language.localized(copy::configuration::UPDATE_NOW)
            },
            ActionRole::Secondary,
            ControlSize::Compact,
        )
        .disabled(!enabled)
        .loading(refreshing)
        .when(!enabled || refreshing, gpui::Styled::cursor_default)
        .on_click(cx.listener(move |this, _, _, cx| {
            cx.stop_propagation();
            if enabled {
                this.refresh_qx_rule_source(id.clone(), cx);
            }
        }))
    }

    fn qx_rule_source_target_menu(
        &self,
        source_id: &str,
        selected_target: &str,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        let mut choices = div();
        for target in self.qx_rule_targets() {
            let selected = target == selected_target;
            let target_id = target.clone();
            let source_id = source_id.to_owned();
            choices = choices.child(
                Button::new(format!("qx-rule-source-target-{source_id}-{target_id}"))
                    .map(crate::components::primary_button_interaction)
                    .accessibility_label(format!("Target {target}"))
                    .selected(selected)
                    .with_variant(ButtonVariant::Text)
                    .with_size(gpui_component::Size::Small)
                    .w_full()
                    .min_h(ControlSize::Standard.min_pointer_target())
                    .px(Space::Md.px())
                    .py(Space::Sm.px())
                    .border_b_1()
                    .border_color(theme.outline_subtle)
                    .bg(if selected {
                        theme.action_soft
                    } else {
                        theme.surface_high
                    })
                    .text_color(if selected {
                        theme.action_primary
                    } else {
                        theme.text_primary
                    })
                    .child(
                        div()
                            .w_full()
                            .text_size(TextRole::Label.size())
                            .line_height(TextRole::Label.line_height())
                            .font_weight(TextRole::Label.weight())
                            .child(target),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.update_qx_rule_source_target(source_id.clone(), target_id.clone(), cx);
                    })),
            );
        }
        choices
    }

    fn qx_rule_source_target_select(
        &self,
        source: &crate::mihomo::StoredQxRuleSource,
        enabled: bool,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let language = self.language();
        let source_id = source.id.clone();
        let selected_target = self.effective_rule_target(source.target_policy.as_str(), language);
        let open = self.rule_sources.target_popover.as_deref() == Some(source.id.as_str());
        let updating = self.rule_sources.target_updates.contains_key(&source.id);
        let menu = self.qx_rule_source_target_menu(&source.id, &selected_target, theme, cx);
        let display_value = if updating {
            language.localized(copy::configuration::SAVING).to_owned()
        } else {
            format!(
                "{} · {selected_target}",
                language.message(Message::PolicyGroup)
            )
        };
        let trigger = style_action_button(
            Button::new(format!("qx-rule-target-select-{}", source.id))
                .accessibility_label(
                    language
                        .localized(copy::configuration::CHANGE_TARGET_POLICY_FOR_THIS_RULE_SOURCE),
                )
                .dropdown_caret(true)
                .w_full()
                .disabled(!enabled)
                .child(
                    div()
                        .min_w(px(0.0))
                        .overflow_x_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_size(TextRole::Label.size())
                        .line_height(TextRole::Label.line_height())
                        .font_weight(TextRole::Label.weight())
                        .child(display_value),
                ),
            ActionRole::Secondary,
            ControlSize::Compact,
        )
        .when(!enabled, gpui::Styled::cursor_default);
        let app = cx.entity();
        crate::components::anchored_popover(
            format!("qx-rule-target-popover-{}", source.id),
            trigger,
            menu,
            240.0,
            320.0,
        )
        .open(open)
        .on_open_change(move |open, _, cx| {
            app.update(cx, |this, cx| {
                this.rule_sources.target_popover = open.then(|| source_id.clone());
                cx.notify();
            });
        })
        .into_any_element()
    }
}
