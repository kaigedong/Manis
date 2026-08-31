impl ManisApp {
    fn source_panel(&self, theme: Theme, compact: bool, cx: &mut Context<Self>) -> Div {
        let language = self.language();
        let saved_source_count = self.imported_subscriptions.len() + self.saved_single_nodes.len();
        let add_action = action_button(
            "configuration-add-proxy-source",
            language.localized(copy::configuration::ADD_SOURCE),
            ActionRole::Primary,
            ControlSize::Compact,
        )
        .on_click(cx.listener(|this, _, window, cx| {
            this.open_new_subscription_editor(cx);
            this.open_proxy_source_dialog(window, cx);
        }));

        let panel = panel_surface("configuration-source", compact, theme)
            .child(section_heading(
                language.localized(copy::configuration::PROXY_SOURCES),
                language.count(CountNoun::Source, saved_source_count),
                Some(add_action.into_any_element()),
                theme,
            ))
            .when_some(self.source_store_error, |panel, error| {
                panel.child(Self::subscription_error(
                    language.localized(copy::configuration::SOME_LOCAL_SOURCES_COULD_NOT_BE_RESTORED),
                    copy::configuration::subscription_store_error(language, error).to_owned(),
                    Some(language.localized(copy::configuration::OTHER_SAFELY_READABLE_SOURCES_ARE_KEPT_CHECK_THE_USER_DATA)),
                    theme,
                ))
            })
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
            )
            .when(saved_source_count == 0, |panel| {
                panel.child(
                    empty_state(
                        language.localized(copy::configuration::NO_PROXY_SOURCES),
                        language.localized(copy::configuration::ADD_A_SUBSCRIPTION_OR_A_SINGLE_NODE_SOURCE),
                        Some(
                        action_button(
                            "configuration-empty-add-proxy-source",
                            language.localized(copy::configuration::ADD_SOURCE),
                            ActionRole::Primary,
                            ControlSize::Compact,
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.open_new_subscription_editor(cx);
                            this.open_proxy_source_dialog(window, cx);
                        }))
                            .into_any_element(),
                        ),
                        theme,
                    )
                    .mt(Space::Md.px()),
                )
            })
            .child(self.imported_subscription_cards(theme, cx))
            .child(self.saved_single_node_cards(theme, cx));
        div().w_full().child(panel)
    }

    fn open_new_subscription_editor(&mut self, cx: &mut Context<Self>) {
        self.proxy_source_editor.subscription_source_id = None;
        self.proxy_source_editor.single_node_source_id = None;
        self.proxy_source_editor.kind = ProxySourceEditorKind::Subscription;
        self.proxy_source_editor.refresh_interval = RemoteSourceRefreshInterval::Manual;
        self.proxy_source_editor.interval_popover = false;
        self.proxy_source_editor.enabled = true;
        self.proxy_source_editor.error = None;
        self.proxy_source_editor.feedback = SubscriptionFeedback::Idle;
        if let Some(input) = self.proxy_source_editor.name_input.as_ref() {
            input.update(cx, SubscriptionTextInput::clear_without_event);
        }
        if let Some(input) = self.proxy_source_editor.input.as_ref() {
            input.update(cx, SubscriptionTextInput::clear_without_event);
        }
        cx.notify();
    }

    fn open_subscription_editor(&mut self, id: String, cx: &mut Context<Self>) {
        let Some(subscription) = self
            .imported_subscriptions
            .iter()
            .find(|subscription| subscription.id == id)
        else {
            return;
        };
        let name = subscription.name.clone();
        let url = subscription.source.expose_to(str::to_owned);
        self.proxy_source_editor.subscription_source_id = Some(id);
        self.proxy_source_editor.single_node_source_id = None;
        self.proxy_source_editor.kind = ProxySourceEditorKind::Subscription;
        self.proxy_source_editor.refresh_interval = subscription.refresh_interval;
        self.proxy_source_editor.interval_popover = false;
        self.proxy_source_editor.enabled = subscription.enabled;
        self.proxy_source_editor.error = None;
        self.proxy_source_editor.feedback = SubscriptionFeedback::Idle;
        if let Some(input) = self.proxy_source_editor.name_input.as_ref() {
            input.update(cx, |input, cx| input.set_value_without_event(name, cx));
        }
        if let Some(input) = self.proxy_source_editor.input.as_ref() {
            input.update(cx, |input, cx| input.set_value_without_event(url, cx));
        }
        cx.notify();
    }

    fn open_single_node_editor(&mut self, id: String, cx: &mut Context<Self>) {
        let Some(saved) = self.saved_single_nodes.iter().find(|saved| saved.id == id) else {
            return;
        };
        let name = saved.name.clone();
        let url = saved.source.expose_to(str::to_owned);
        self.proxy_source_editor.subscription_source_id = None;
        self.proxy_source_editor.single_node_source_id = Some(id);
        self.proxy_source_editor.kind = ProxySourceEditorKind::SingleNode;
        self.proxy_source_editor.refresh_interval = RemoteSourceRefreshInterval::Manual;
        self.proxy_source_editor.interval_popover = false;
        self.proxy_source_editor.enabled = saved.enabled;
        self.proxy_source_editor.error = None;
        self.proxy_source_editor.feedback = SubscriptionFeedback::Idle;
        if let Some(input) = self.proxy_source_editor.name_input.as_ref() {
            input.update(cx, |input, cx| input.set_value_without_event(name, cx));
        }
        if let Some(input) = self.proxy_source_editor.input.as_ref() {
            input.update(cx, |input, cx| input.set_value_without_event(url, cx));
        }
        cx.notify();
    }

    fn open_proxy_source_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let app = cx.entity();
        window.open_dialog(cx, move |dialog, window, cx| {
            app.update(cx, |this, cx| {
                let theme = this.theme();
                this.proxy_source_editor_modal(dialog, theme, this.language(), window, cx)
            })
        });
        if let Some(input) = self.proxy_source_editor.name_input.as_ref() {
            input.focus_handle(cx).focus(window, cx);
        }
        cx.notify();
    }

    fn close_subscription_editor(&mut self, cx: &mut Context<Self>) {
        self.configuration_add_section = None;
        self.proxy_source_editor.subscription_source_id = None;
        self.proxy_source_editor.single_node_source_id = None;
        self.proxy_source_editor.interval_popover = false;
        self.proxy_source_editor.error = None;
        self.proxy_source_editor.feedback = SubscriptionFeedback::Idle;
        if let Some(input) = self.proxy_source_editor.name_input.as_ref() {
            input.update(cx, SubscriptionTextInput::clear_without_event);
        }
        if let Some(input) = self.proxy_source_editor.input.as_ref() {
            input.update(cx, SubscriptionTextInput::clear_without_event);
        }
        cx.notify();
    }

    fn proxy_source_editor_modal(
        &self,
        dialog: Dialog,
        theme: Theme,
        language: Language,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Dialog {
        let input = self
            .proxy_source_editor
            .input
            .as_ref()
            .expect("subscription input is initialized before rendering")
            .clone();
        let name_input = self
            .proxy_source_editor
            .name_input
            .as_ref()
            .expect("subscription name input is initialized before rendering")
            .clone();
        let viewport = window.viewport_size();
        let view = ProxySourceEditorView {
            direct_input: self.proxy_source_editor.kind == ProxySourceEditorKind::SingleNode,
            editing: self.proxy_source_editor.subscription_source_id.is_some()
                || self.proxy_source_editor.single_node_source_id.is_some(),
            activity: if matches!(
                self.proxy_source_editor.feedback,
                SubscriptionFeedback::Importing(_)
            ) {
                ProxySourceEditorActivity::Busy
            } else {
                ProxySourceEditorActivity::Idle
            },
            enabled: self.proxy_source_editor.enabled,
            dialog_width: (viewport.width.as_f32() - 32.0).clamp(300.0, 620.0),
        };
        let interval_select = self.proxy_source_interval_select(view, language, theme, cx);
        let body = self.proxy_source_editor_body(
            ProxySourceEditorInputs {
                source: input.clone(),
                name: name_input,
                interval_select,
            },
            view,
            language,
            theme,
            cx,
        );
        let footer = Self::proxy_source_editor_footer(input, view, language, theme, cx);
        let app = cx.entity();
        surface_dialog(dialog, theme)
            .width(px(view.dialog_width))
            .max_h(px((viewport.height.as_f32() - 32.0).max(320.0)))
            .margin_top(px(((viewport.height.as_f32() - 480.0) / 2.0).max(16.0)))
            .overlay(true)
            .overlay_closable(true)
            .keyboard(true)
            .close_button(false)
            .title(Self::proxy_source_editor_title(view, language, theme))
            .child(body)
            .footer(footer)
            .on_close(move |_, _, cx| {
                app.update(cx, ManisApp::close_subscription_editor);
            })
    }

    fn proxy_source_interval_select(
        &self,
        view: ProxySourceEditorView,
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
            let selected = interval == self.proxy_source_editor.refresh_interval;
            menu = menu.child(
                div()
                    .id(format!("subscription-refresh-option-{interval:?}"))
                    .role(Role::Button)
                    .aria_label(refresh_interval_label(interval, language))
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
                        theme.surface_high
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
                    .child(refresh_interval_label(interval, language))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.proxy_source_editor.refresh_interval = interval;
                        this.proxy_source_editor.interval_popover = false;
                        cx.notify();
                    })),
            );
        }
        let trigger = Button::new("subscription-editor-refresh-interval")
            .accessibility_label(
                language.localized(copy::configuration::CHOOSE_SUBSCRIPTION_UPDATE_INTERVAL),
            )
            .dropdown_caret(true)
            .w_full()
            .child(refresh_interval_label(
                self.proxy_source_editor.refresh_interval,
                language,
            ))
            .disabled(view.busy());
        let trigger = style_action_button(trigger, ActionRole::Secondary, ControlSize::Standard)
            .when(view.busy(), gpui::Styled::cursor_default);
        let app = cx.entity();
        crate::components::anchored_popover(
            "subscription-editor-refresh-popover",
            trigger,
            menu,
            (view.dialog_width - 40.0).max(240.0),
            280.0,
        )
        .open(self.proxy_source_editor.interval_popover)
        .on_open_change(move |open, _, cx| {
            app.update(cx, |this, cx| {
                this.proxy_source_editor.interval_popover = *open;
                cx.notify();
            });
        })
        .into_any_element()
    }

    fn proxy_source_editor_body(
        &self,
        inputs: ProxySourceEditorInputs,
        view: ProxySourceEditorView,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        div()
            .id("proxy-source-modal-body")
            .flex_1()
            .min_h(px(0.0))
            .overflow_y_scroll()
            .px_5()
            .py_4()
            .when(!view.editing, |body| {
                body.child(field_label(
                    language.localized(copy::configuration::SOURCE_TYPE),
                    theme,
                ))
                .child(Self::proxy_source_kind_picker(view, language, theme, cx))
            })
            .child(field_label(
                if view.direct_input {
                    language.localized(copy::configuration::NODE_NAME)
                } else {
                    language.localized(copy::configuration::SOURCE_NAME)
                },
                theme,
            ))
            .child(inputs.name)
            .child(field_label(language.localized(copy::configuration::SOURCE_URL), theme).mt_4())
            .child(inputs.source)
            .when(!view.direct_input, |body| {
                body.child(
                    field_label(
                        language.localized(copy::configuration::UPDATE_INTERVAL),
                        theme,
                    )
                    .mt_4(),
                )
                .child(inputs.interval_select)
            })
            .child(
                Checkbox::new("proxy-source-editor-enabled")
                    .label(language.localized(copy::configuration::USE_THIS_SOURCE))
                    .map(crate::components::primary_button_interaction)
                    .checked(view.enabled)
                    .disabled(view.busy())
                    .tab_stop(!view.busy())
                    .cursor_pointer()
                    .mt_4()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if !view.busy() {
                            this.proxy_source_editor.enabled = !view.enabled;
                            cx.notify();
                        }
                    })),
            )
            .when_some(self.proxy_source_editor.error.clone(), |body, error| {
                body.child(
                    div()
                        .mt_3()
                        .text_size(TextRole::Metadata.size())
                        .text_color(theme.status_error)
                        .child(error),
                )
            })
    }

    fn proxy_source_kind_picker(
        view: ProxySourceEditorView,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        div()
            .mt_1()
            .flex()
            .p_1()
            .gap_1()
            .rounded(Radius::Control.px())
            .border_1()
            .border_color(theme.outline_subtle)
            .bg(theme.surface_base)
            .children(
                [
                    (
                        ProxySourceEditorKind::Subscription,
                        "proxy-source-kind-subscription",
                    ),
                    (
                        ProxySourceEditorKind::SingleNode,
                        "proxy-source-kind-single-node",
                    ),
                ]
                .map(|(kind, id)| {
                    let selected = (kind == ProxySourceEditorKind::SingleNode) == view.direct_input;
                    action_button(
                        id,
                        match kind {
                            ProxySourceEditorKind::Subscription => {
                                language.localized(copy::configuration::SUBSCRIPTION)
                            }
                            ProxySourceEditorKind::SingleNode => {
                                language.localized(copy::configuration::SINGLE_NODE_2)
                            }
                        },
                        if selected {
                            ActionRole::Primary
                        } else {
                            ActionRole::Secondary
                        },
                        ControlSize::Compact,
                    )
                    .selected(selected)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.proxy_source_editor.kind = kind;
                        this.proxy_source_editor.error = None;
                        cx.notify();
                    }))
                }),
            )
    }

    fn proxy_source_editor_footer(
        input: Entity<SubscriptionTextInput>,
        view: ProxySourceEditorView,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        dialog_footer_surface(theme)
            .child(
                style_action_button(
                    Button::new("cancel-proxy-source").label(language.message(Message::Cancel)),
                    ActionRole::Secondary,
                    ControlSize::Standard,
                )
                .on_click(cx.listener(|this, _, window, cx| {
                    this.close_subscription_editor(cx);
                    window.close_dialog(cx);
                })),
            )
            .child(
                style_action_button(
                    Button::new("save-proxy-source")
                        .label(if view.busy() {
                            language.localized(copy::configuration::PROCESSING)
                        } else if view.editing {
                            language.message(Message::SaveChanges)
                        } else {
                            language.localized(copy::configuration::ADD_SOURCE)
                        })
                        .loading(view.busy()),
                    ActionRole::Primary,
                    ControlSize::Standard,
                )
                .when(view.busy(), gpui::Styled::cursor_default)
                .on_click(cx.listener(move |this, _, window, cx| {
                    if !view.busy() && this.submit_source_import(&input, cx) {
                        window.close_dialog(cx);
                    }
                })),
            )
    }

    fn proxy_source_editor_title(
        view: ProxySourceEditorView,
        language: Language,
        theme: Theme,
    ) -> Div {
        dialog_header_surface(theme)
            .child(
                div()
                    .text_size(px(17.0))
                    .font_weight(TextRole::SectionTitle.weight())
                    .child(if view.editing {
                        language.localized(copy::configuration::EDIT_PROXY_SOURCE)
                    } else {
                        language.localized(copy::configuration::ADD_PROXY_SOURCE)
                    }),
            )
            .child(
                div()
                    .mt_1()
                    .text_size(TextRole::Metadata.size())
                    .text_color(theme.text_secondary)
                    .child(if view.direct_input {
                        language.localized(copy::configuration::A_SINGLE_NODE_SOURCE_DOES_NOT_NEED_AN_UPDATE_INTERVAL)
                    } else {
                        language.localized(copy::configuration::CHOOSE_A_SUBSCRIPTION_OR_A_SINGLE_NODE_SHARE_LINK)
                    }),
            )
    }

    fn submit_source_import(
        &mut self,
        input: &Entity<SubscriptionTextInput>,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.configuration_transfer.active {
            return false;
        }
        let name = self
            .proxy_source_editor
            .name_input
            .as_ref()
            .map(|input| input.read(cx).value().trim().to_owned())
            .unwrap_or_default();
        if name.is_empty() {
            self.proxy_source_editor.error = Some(
                self.language()
                    .localized(copy::configuration::ENTER_A_SOURCE_NAME)
                    .to_owned(),
            );
            cx.notify();
            return false;
        }
        self.proxy_source_editor.error = None;
        let (input_value, result) = {
            let input = input.read(cx);
            (
                input.value().to_owned(),
                match self.proxy_source_editor.kind {
                    ProxySourceEditorKind::Subscription => {
                        validate_subscription_preview(input.value())
                    }
                    ProxySourceEditorKind::SingleNode => {
                        validate_single_node_preview(input.value())
                    }
                },
            )
        };
        match result {
            Ok(preview) if preview.kind == SourceKind::SingleNode => {
                if self.proxy_source_editor.subscription_source_id.is_some() {
                    self.proxy_source_editor.error = Some(
                        self.language()
                            .localized(copy::configuration::AN_EXISTING_SUBSCRIPTION_MUST_KEEP_AN_HTTP_HTTPS_URL)
                            .to_owned(),
                    );
                    cx.notify();
                    return false;
                }
                self.import_single_node(input_value, name, preview, cx)
            }
            Ok(preview) => {
                if self.proxy_source_editor.single_node_source_id.is_some() {
                    self.proxy_source_editor.error = Some(
                        self.language()
                            .localized(copy::configuration::THIS_SOURCE_MUST_REMAIN_A_SINGLE_NODE_SHARE_LINK)
                            .to_owned(),
                    );
                    cx.notify();
                    return false;
                }
                trace_ui(UiEvent::SourceRecognitionSucceeded);
                self.import_remote_subscription(
                    super::SubscriptionImportRequest {
                        input: input_value,
                        name,
                        refresh_interval: self.proxy_source_editor.refresh_interval,
                        enabled: self.proxy_source_editor.enabled,
                        editing_id: self.proxy_source_editor.subscription_source_id.clone(),
                        kind: preview.kind,
                    },
                    cx,
                );
                true
            }
            Err(error) => {
                self.proxy_source_editor.feedback = SubscriptionFeedback::InvalidInput(error);
                self.status = format!(
                    "{}: {}",
                    self.language()
                        .localized(copy::configuration::SOURCE_RECOGNITION_FAILED),
                    copy::configuration::subscription_input_error(self.language(), error)
                );
                trace_ui(UiEvent::SourceRecognitionFailed);
                cx.notify();
                false
            }
        }
    }

    fn import_single_node(
        &mut self,
        input_value: String,
        name: String,
        preview: crate::subscription::SubscriptionPreview,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.configuration_transfer.active {
            return false;
        }
        let Some(store_dir) = self.subscription_store_dir.clone() else {
            self.proxy_source_editor.feedback =
                SubscriptionFeedback::StoreFailed(SubscriptionStoreError::DataDirectoryUnavailable);
            self.language()
                .localized(copy::configuration::COULD_NOT_DETERMINE_WHERE_TO_SAVE_THE_NODE)
                .clone_into(&mut self.status);
            trace_ui(UiEvent::SourceImportFailed);
            cx.notify();
            return false;
        };
        self.subscription_preview_generation = self.subscription_preview_generation.wrapping_add(1);
        let generation = self.subscription_preview_generation;
        self.proxy_source_editor.feedback = SubscriptionFeedback::Importing(SourceKind::SingleNode);
        self.language()
            .localized(copy::configuration::VALIDATING_AND_SAVING_SINGLE_NODE_SOURCE)
            .clone_into(&mut self.status);
        if let Some(input) = self.proxy_source_editor.input.as_ref() {
            input.update(cx, |input, cx| input.set_enabled(false, cx));
        }
        let runtime = self.runtime.clone();
        let editing_id = self.proxy_source_editor.single_node_source_id.clone();
        let enabled = self.proxy_source_editor.enabled;
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn(async move {
                    let providers = mihomo::preview_single_node(&input_value)?;
                    let transaction = super::mutate_saved_sources(&runtime, &store_dir, || {
                        if let Some(id) = editing_id {
                            mihomo::update_single_node_source_in(
                                &store_dir,
                                &id,
                                &input_value,
                                &name,
                                enabled,
                            )
                        } else {
                            mihomo::save_single_node_source_with_options_in(
                                &store_dir,
                                &input_value,
                                &name,
                                enabled,
                            )
                        }
                    })?;
                    Ok::<_, SubscriptionStoreError>((transaction, providers))
                })
                .await;
            this.update(cx, |this, cx| {
                this.finish_single_node_import(generation, preview, result, cx);
            })
            .ok();
        })
        .detach();
        cx.notify();
        true
    }

    fn finish_single_node_import(
        &mut self,
        generation: u64,
        preview: crate::subscription::SubscriptionPreview,
        result: super::SingleNodeImportResult,
        cx: &mut Context<Self>,
    ) {
        if self.subscription_preview_generation != generation {
            return;
        }
        if let Some(input) = self.proxy_source_editor.input.as_ref() {
            input.update(cx, |input, cx| input.set_enabled(true, cx));
        }
        match result {
            Ok((transaction, providers)) => {
                self.finish_saved_single_node(transaction, providers, preview, cx);
            }
            Err(error) => {
                self.proxy_source_editor.feedback = SubscriptionFeedback::StoreFailed(error);
                self.status = format!(
                    "{}: {}",
                    self.language()
                        .localized(copy::configuration::SINGLE_NODE_SOURCE_SAVE_FAILED),
                    copy::configuration::subscription_store_error(self.language(), error)
                );
                trace_ui(UiEvent::SourceImportFailed);
                cx.notify();
            }
        }
    }

    fn finish_saved_single_node(
        &mut self,
        mut transaction: super::SourceMutation<mihomo::StoredSingleNode>,
        providers: Vec<mihomo::LoadedProvider>,
        preview: crate::subscription::SubscriptionPreview,
        cx: &mut Context<Self>,
    ) {
        let language = self.language();
        transaction.apply.reconcile_proxy_mode(&mut self.proxy_mode);
        let Some(stored) = transaction.value.take() else {
            self.proxy_source_editor.feedback =
                SubscriptionFeedback::StoreFailed(SubscriptionStoreError::StoreUnavailable);
            self.status = format!(
                "{}{}",
                language.localized(copy::configuration::SINGLE_NODE_SOURCE_SAVE_FAILED),
                transaction
                    .apply
                    .status_suffix_after_source_rollback(language)
            );
            trace_ui(UiEvent::SourceImportFailed);
            cx.notify();
            return;
        };
        if let Some(existing) = self
            .saved_single_nodes
            .iter_mut()
            .find(|node| node.id == stored.id)
        {
            *existing = stored;
        } else {
            self.saved_single_nodes.push(stored);
        }
        self.subscription_preview_providers = providers;
        self.proxy_source_editor.feedback = SubscriptionFeedback::Valid(preview);
        if let Some(input) = self.proxy_source_editor.input.as_ref() {
            input.update(cx, SubscriptionTextInput::clear_without_event);
        }
        if let Some(input) = self.proxy_source_editor.name_input.as_ref() {
            input.update(cx, SubscriptionTextInput::clear_without_event);
        }
        self.configuration_add_section = None;
        self.status = copy::configuration::single_node_saved(
            language,
            &transaction.apply.status_suffix(language),
        );
        trace_ui(UiEvent::SourceImportSucceeded);
        cx.notify();
    }

    fn imported_subscription_cards(&self, theme: Theme, cx: &mut Context<Self>) -> Div {
        let language = self.language();
        let now = mihomo::current_unix_secs();
        div().children(self.imported_subscriptions.iter().map(|subscription| {
            let presentation = self.subscription_card_presentation(subscription, now, language);
            Self::imported_subscription_card(subscription, &presentation, language, theme, cx)
        }))
    }

    fn subscription_card_presentation(
        &self,
        subscription: &super::ImportedSubscription,
        now: u64,
        language: Language,
    ) -> SubscriptionCardPresentation {
        let node_count = subscription
            .providers
            .iter()
            .map(|provider| provider.nodes.len())
            .sum::<usize>();
        let (state, activity) = match &subscription.state {
            ImportedSubscriptionState::None => (
                language
                    .localized(copy::configuration::DISABLED_2)
                    .to_owned(),
                SubscriptionCardActivity::Idle { healthy: true },
            ),
            ImportedSubscriptionState::Pending(_) | ImportedSubscriptionState::Refreshing(_) => (
                language
                    .localized(copy::configuration::UPDATING_2)
                    .to_owned(),
                SubscriptionCardActivity::Busy,
            ),
            ImportedSubscriptionState::Ready(kind) => (
                copy::configuration::source_nodes(
                    language,
                    source_kind_label(*kind, language),
                    node_count,
                ),
                SubscriptionCardActivity::Idle { healthy: true },
            ),
            ImportedSubscriptionState::Unavailable(_, _)
            | ImportedSubscriptionState::StoreError(_) => (
                language
                    .localized(copy::configuration::UPDATE_FAILED)
                    .to_owned(),
                SubscriptionCardActivity::Idle { healthy: false },
            ),
            ImportedSubscriptionState::Removing(_) => (
                language.localized(copy::configuration::REMOVING).to_owned(),
                SubscriptionCardActivity::Busy,
            ),
        };
        let controls_enabled = !activity.is_busy()
            && !matches!(
                self.proxy_source_editor.feedback,
                SubscriptionFeedback::Importing(_)
            )
            && !self.source_refresh_busy();
        SubscriptionCardPresentation {
            state,
            activity,
            controls_enabled,
            updated: source_update_label(
                subscription.last_successful_update_unix_secs,
                now,
                language,
            ),
        }
    }

    fn imported_subscription_card(
        subscription: &super::ImportedSubscription,
        presentation: &SubscriptionCardPresentation,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let card_edit_id = subscription.id.clone();
        let controls_enabled = presentation.controls_enabled;
        let toggle_id = subscription.id.clone();
        let enabled = subscription.enabled;
        div()
            .id(format!("subscription-card-{card_edit_id}"))
            .role(Role::Button)
            .aria_label(language.localized(copy::configuration::EDIT_THIS_SUBSCRIPTION))
            .tab_stop(controls_enabled)
            .focusable()
            .map(crate::components::primary_button_interaction)
            .when(controls_enabled, gpui::Styled::cursor_pointer)
            .mt_2()
            .px_3()
            .py_2()
            .rounded(Radius::Row.px())
            .border_1()
            .border_color(theme.outline_subtle)
            .bg(theme.surface_low)
            .flex()
            .items_center()
            .gap_2()
            .when(controls_enabled, |card| {
                card.hover(move |card| card.bg(theme.action_soft))
            })
            .child(
                Checkbox::new(format!("subscription-enabled-{toggle_id}"))
                    .block_mouse_except_scroll()
                    .aria_label(subscription.name.clone())
                    .flex_shrink_0()
                    .map(crate::components::primary_button_interaction)
                    .checked(enabled)
                    .disabled(!controls_enabled)
                    .tab_stop(controls_enabled)
                    .when(controls_enabled, gpui::Styled::cursor_pointer)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        if controls_enabled {
                            this.set_subscription_enabled(&toggle_id, !enabled, cx);
                        }
                    })),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .child(Self::subscription_card_header(
                        subscription,
                        presentation,
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
                            .child(subscription.source.expose_to(str::to_owned)),
                    )
                    .child(Self::subscription_card_actions(
                        subscription,
                        presentation,
                        language,
                        theme,
                        cx,
                    )),
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                if controls_enabled {
                    this.open_subscription_editor(card_edit_id.clone(), cx);
                    this.open_proxy_source_dialog(window, cx);
                }
            }))
    }

    fn subscription_card_header(
        subscription: &super::ImportedSubscription,
        presentation: &SubscriptionCardPresentation,
        theme: Theme,
    ) -> Div {
        let enabled = subscription.enabled;
        div()
            .flex()
            .items_center()
            .gap_2()
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .min_w(px(0.0))
                            .overflow_x_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(TextRole::Label.size())
                            .font_weight(TextRole::Label.weight())
                            .text_color(if !enabled {
                                theme.text_secondary
                            } else if presentation.activity.is_healthy() {
                                theme.text_primary
                            } else {
                                theme.status_error
                            })
                            .child(subscription.name.clone()),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_size(TextRole::Metadata.size())
                            .text_color(theme.text_tertiary)
                            .child(presentation.state.clone()),
                    ),
            )
            .when(presentation.activity.is_busy(), |row| {
                row.child(Self::benchmark_latency_spinner(
                    format!("source-refresh-{}", subscription.id),
                    theme,
                ))
            })
    }

    fn subscription_card_actions(
        subscription: &super::ImportedSubscription,
        presentation: &SubscriptionCardPresentation,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        let refresh_id = subscription.id.clone();
        let remove_id = subscription.id.clone();
        let refresh_enabled = presentation.controls_enabled && subscription.enabled;
        let controls_enabled = presentation.controls_enabled;
        let busy = presentation.activity.is_busy();
        div()
            .mt_1()
            .flex()
            .items_center()
            .gap_2()
            .text_size(TextRole::Metadata.size())
            .text_color(theme.text_secondary)
            .child(refresh_interval_label(
                subscription.refresh_interval,
                language,
            ))
            .child("·")
            .child(presentation.updated.clone())
            .child(div().flex_1())
            .child(
                row_action_button(
                    format!("subscription-refresh-{refresh_id}"),
                    if busy {
                        language.localized(copy::configuration::UPDATING)
                    } else {
                        language.localized(copy::configuration::UPDATE_NOW)
                    },
                    ActionRole::Secondary,
                    ControlSize::Compact,
                )
                .accessibility_label(
                    language.localized(copy::configuration::UPDATE_THIS_SUBSCRIPTION_NOW),
                )
                .disabled(!refresh_enabled)
                .loading(busy)
                .when(!refresh_enabled || busy, gpui::Styled::cursor_default)
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    if refresh_enabled {
                        this.refresh_imported_subscription(refresh_id.clone(), cx);
                    }
                })),
            )
            .when(controls_enabled, |row| {
                row.child(
                    row_action_button(
                        format!("remove-{remove_id}"),
                        language.localized(copy::configuration::REMOVE),
                        ActionRole::Danger,
                        ControlSize::Compact,
                    )
                    .accessibility_label(
                        language.localized(copy::configuration::REMOVE_THIS_SUBSCRIPTION),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.remove_imported_subscription(remove_id.clone(), cx);
                    })),
                )
            })
    }

    fn saved_single_node_cards(&self, theme: Theme, cx: &mut Context<Self>) -> Div {
        let language = self.language();
        let controls_enabled = !matches!(
            self.proxy_source_editor.feedback,
            SubscriptionFeedback::Importing(_)
        ) && !self.source_refresh_busy();
        div().children(self.saved_single_nodes.iter().map(|saved| {
            Self::saved_single_node_card(saved, controls_enabled, language, theme, cx)
        }))
    }

    fn saved_single_node_card(
        saved: &mihomo::StoredSingleNode,
        controls_enabled: bool,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let card_edit_id = saved.id.clone();
        let node = saved.source.preview();
        let toggle_id = saved.id.clone();
        let enabled = saved.enabled;
        div()
            .id(format!("single-node-card-{card_edit_id}"))
            .role(Role::Button)
            .aria_label(language.localized(copy::configuration::EDIT_THIS_SINGLE_NODE_SOURCE))
            .tab_stop(controls_enabled)
            .focusable()
            .map(crate::components::primary_button_interaction)
            .when(controls_enabled, gpui::Styled::cursor_pointer)
            .mt_2()
            .px_3()
            .py_2()
            .rounded(Radius::Row.px())
            .border_1()
            .border_color(theme.outline_subtle)
            .bg(theme.surface_low)
            .flex()
            .items_center()
            .gap_2()
            .when(controls_enabled, |card| {
                card.hover(move |card| card.bg(theme.action_soft))
            })
            .child(
                Checkbox::new(format!("single-node-enabled-{toggle_id}"))
                    .block_mouse_except_scroll()
                    .aria_label(saved.name.clone())
                    .flex_shrink_0()
                    .map(crate::components::primary_button_interaction)
                    .checked(enabled)
                    .disabled(!controls_enabled)
                    .tab_stop(controls_enabled)
                    .when(controls_enabled, gpui::Styled::cursor_pointer)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        if controls_enabled {
                            this.set_single_node_enabled(toggle_id.clone(), !enabled, cx);
                        }
                    })),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .child(Self::saved_single_node_header(saved, node, theme))
                    .child(
                        div()
                            .mt_1()
                            .overflow_x_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(TextRole::Metadata.size())
                            .text_color(theme.text_tertiary)
                            .child(saved.source.expose_to(str::to_owned)),
                    )
                    .child(Self::saved_single_node_actions(
                        saved,
                        node,
                        controls_enabled,
                        language,
                        theme,
                        cx,
                    )),
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                if controls_enabled {
                    this.open_single_node_editor(card_edit_id.clone(), cx);
                    this.open_proxy_source_dialog(window, cx);
                }
            }))
    }

    fn saved_single_node_header(
        saved: &mihomo::StoredSingleNode,
        node: &crate::subscription::SourceNodePreview,
        theme: Theme,
    ) -> Div {
        let enabled = saved.enabled;
        div().flex().items_center().gap_2().child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div()
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
                        .child(saved.name.clone()),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .text_size(TextRole::Metadata.size())
                        .text_color(theme.text_tertiary)
                        .child(node.protocol),
                ),
        )
    }

    fn saved_single_node_actions(
        saved: &mihomo::StoredSingleNode,
        node: &crate::subscription::SourceNodePreview,
        controls_enabled: bool,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        let remove_id = saved.id.clone();
        div()
            .mt_1()
            .flex()
            .items_center()
            .gap_2()
            .text_size(TextRole::Metadata.size())
            .text_color(theme.text_secondary)
            .child(format!(
                "{} · {}",
                node.endpoint,
                copy::configuration::source_node_detail(language, node.detail)
            ))
            .child(div().flex_1())
            .when(controls_enabled, |row| {
                row.child(
                    row_action_button(
                        format!("remove-single-node-{remove_id}"),
                        language.localized(copy::configuration::REMOVE),
                        ActionRole::Danger,
                        ControlSize::Compact,
                    )
                    .accessibility_label(
                        language.localized(copy::configuration::REMOVE_SINGLE_NODE_SOURCE),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.remove_saved_single_node(remove_id.clone(), cx);
                    })),
                )
            })
    }

    fn set_single_node_enabled(&mut self, id: String, enabled: bool, cx: &mut Context<Self>) {
        if self.configuration_transfer.active {
            return;
        }
        let Some(store_dir) = self.subscription_store_dir.clone() else {
            return;
        };
        if !self.routing_apply_state.begin() {
            return;
        }
        let runtime = self.runtime.clone();
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn(async move {
                    super::mutate_saved_sources(&runtime, &store_dir, || {
                        mihomo::update_single_node_source_enabled_in(&store_dir, &id, enabled)
                    })
                })
                .await;
            this.update(cx, |this, cx| {
                this.routing_apply_state.finish();
                match result {
                    Ok(transaction) => {
                        let language = this.language();
                        transaction.apply.reconcile_proxy_mode(&mut this.proxy_mode);
                        if let Some(stored) = transaction.value {
                            if let Some(existing) = this
                                .saved_single_nodes
                                .iter_mut()
                                .find(|existing| existing.id == stored.id)
                            {
                                *existing = stored;
                            }
                            this.status = format!(
                                "{}{}",
                                language.localized(copy::configuration::SINGLE_NODE_SOURCE_UPDATED),
                                transaction.apply.status_suffix(language)
                            );
                        } else {
                            this.status = format!(
                                "{}{}",
                                language.localized(copy::configuration::COULD_NOT_UPDATE_SOURCE),
                                transaction.apply.status_suffix_after_rollback_attempt(
                                    language,
                                    transaction.rollback_error.as_ref(),
                                )
                            );
                        }
                    }
                    Err(error) => {
                        this.status = format!(
                            "{}: {}",
                            this.language()
                                .localized(copy::configuration::COULD_NOT_UPDATE_SOURCE),
                            copy::configuration::subscription_store_error(this.language(), error)
                        );
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn remove_saved_single_node(&mut self, id: String, cx: &mut Context<Self>) {
        if self.configuration_transfer.active {
            return;
        }
        let Some(store_dir) = self.subscription_store_dir.clone() else {
            return;
        };
        if !self.routing_apply_state.begin() {
            return;
        }
        let runtime = self.runtime.clone();
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn(async move {
                    super::mutate_saved_sources(&runtime, &store_dir, || {
                        mihomo::remove_single_node_source_in(&store_dir, &id).map(|()| id.clone())
                    })
                })
                .await;
            this.update(cx, |this, cx| {
                this.routing_apply_state.finish();
                match result {
                    Ok(transaction) => {
                        let language = this.language();
                        transaction.apply.reconcile_proxy_mode(&mut this.proxy_mode);
                        if let Some(deleted_id) = transaction.value {
                            this.saved_single_nodes.retain(|node| node.id != deleted_id);
                            this.status = format!(
                                "{}{}",
                                language.localized(copy::configuration::SINGLE_NODE_SOURCE_REMOVED),
                                transaction.apply.status_suffix(language)
                            );
                        } else {
                            this.status = format!(
                                "{}{}",
                                language.localized(copy::configuration::FAILED_TO_REMOVE_SOURCE),
                                transaction.apply.status_suffix_after_rollback_attempt(
                                    language,
                                    transaction.rollback_error.as_ref(),
                                )
                            );
                        }
                    }
                    Err(error) => {
                        this.status = format!(
                            "{}: {}",
                            this.language()
                                .localized(copy::configuration::FAILED_TO_REMOVE_SOURCE),
                            copy::configuration::subscription_store_error(this.language(), error)
                        );
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn subscription_error(
        title: &'static str,
        message: String,
        recovery: Option<&'static str>,
        theme: Theme,
    ) -> Div {
        div()
            .mt(Space::Md.px())
            .p(Space::Md.px())
            .rounded(Radius::Pane.px())
            .border_1()
            .border_color(theme.outline_strong)
            .bg(theme.surface_low)
            .child(
                div()
                    .text_size(TextRole::Label.size())
                    .line_height(TextRole::Label.line_height())
                    .font_weight(TextRole::Label.weight())
                    .child(title),
            )
            .child(
                div()
                    .mt(Space::Xs.px())
                    .text_size(TextRole::Metadata.size())
                    .line_height(TextRole::Metadata.line_height())
                    .text_color(theme.text_secondary)
                    .child(message),
            )
            .when_some(recovery, |card, recovery| {
                card.child(
                    div()
                        .mt(Space::Sm.px())
                        .text_size(TextRole::Metadata.size())
                        .line_height(TextRole::Metadata.line_height())
                        .text_color(theme.text_tertiary)
                        .child(recovery),
                )
            })
    }
}
