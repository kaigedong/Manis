use super::{
    ActionRole, Checkbox, Context, ControlSize, Disableable, Div, FluentBuilder,
    ImportedSubscriptionState, InteractiveElement, Language, ManisApp, ParentElement, Radius, Role,
    SourceMutation, Stateful, StatefulInteractiveElement, Styled, SubscriptionCardActivity,
    SubscriptionCardPresentation, SubscriptionFeedback, TextRole, Theme, copy, div, mihomo, px,
    refresh_interval_label, row_action_button, source_kind_label, source_update_label,
};

impl ManisApp {
    pub(in crate::app) fn imported_subscription_cards(
        &self,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        let language = self.language();
        let now = mihomo::current_unix_secs();
        div().children(self.imported_subscriptions.iter().map(|subscription| {
            let presentation = self.subscription_card_presentation(subscription, now, language);
            Self::imported_subscription_card(subscription, &presentation, language, theme, cx)
        }))
    }
    fn subscription_card_presentation(
        &self,
        subscription: &crate::app::ImportedSubscription,
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
                    .localized(copy::configuration::SOURCE_DISABLED_LABEL)
                    .to_owned(),
                SubscriptionCardActivity::Idle { healthy: true },
            ),
            ImportedSubscriptionState::Pending(_) | ImportedSubscriptionState::Refreshing(_) => (
                language
                    .localized(copy::configuration::UPDATE_STATUS)
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
        subscription: &crate::app::ImportedSubscription,
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
        subscription: &crate::app::ImportedSubscription,
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
        subscription: &crate::app::ImportedSubscription,
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

    pub(in crate::app) fn saved_single_node_cards(
        &self,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        let language = self.language();
        let controls_enabled = !matches!(
            self.proxy_source_editor.feedback,
            SubscriptionFeedback::Importing(_)
        ) && !self.source_refresh_busy();
        div().children(self.saved_single_nodes.iter().map(|saved| {
            Self::saved_single_node_card(saved, controls_enabled, language, theme, cx)
        }))
    }

    pub(in crate::app) fn saved_single_node_card(
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

    pub(in crate::app) fn saved_single_node_header(
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

    pub(in crate::app) fn saved_single_node_actions(
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

    pub(in crate::app) fn set_single_node_enabled(
        &mut self,
        id: String,
        enabled: bool,
        cx: &mut Context<Self>,
    ) {
        if self.configuration_transfer.active
            || self.source_refresh_busy()
            || self.routing_apply_state.is_busy()
        {
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
                    crate::app::mutate_saved_sources(&runtime, &store_dir, |store_dir| {
                        mihomo::update_single_node_source_enabled_in(store_dir, &id, enabled)
                    })
                })
                .await;
            this.update(cx, |this, cx| {
                this.routing_apply_state.finish();
                match result {
                    Ok(SourceMutation::Committed {
                        value: stored,
                        apply,
                    }) => {
                        let language = this.language();
                        apply.reconcile_proxy_mode(&mut this.proxy_mode);
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
                            apply.status_suffix(language)
                        );
                    }
                    Ok(SourceMutation::RollbackAttempted {
                        apply,
                        rollback_error,
                    }) => {
                        let language = this.language();
                        apply.reconcile_proxy_mode(&mut this.proxy_mode);
                        this.status = format!(
                            "{}{}",
                            language.localized(copy::configuration::COULD_NOT_UPDATE_SOURCE),
                            apply.status_suffix_after_rollback_attempt(
                                language,
                                rollback_error.as_ref(),
                            )
                        );
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

    pub(in crate::app) fn remove_saved_single_node(&mut self, id: String, cx: &mut Context<Self>) {
        if self.configuration_transfer.active
            || self.source_refresh_busy()
            || self.routing_apply_state.is_busy()
        {
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
                    crate::app::mutate_saved_sources(&runtime, &store_dir, |store_dir| {
                        mihomo::remove_single_node_source_in(store_dir, &id).map(|()| id.clone())
                    })
                })
                .await;
            this.update(cx, |this, cx| {
                this.routing_apply_state.finish();
                match result {
                    Ok(SourceMutation::Committed {
                        value: deleted_id,
                        apply,
                    }) => {
                        let language = this.language();
                        apply.reconcile_proxy_mode(&mut this.proxy_mode);
                        this.saved_single_nodes.retain(|node| node.id != deleted_id);
                        this.status = format!(
                            "{}{}",
                            language.localized(copy::configuration::SINGLE_NODE_SOURCE_REMOVED),
                            apply.status_suffix(language)
                        );
                    }
                    Ok(SourceMutation::RollbackAttempted {
                        apply,
                        rollback_error,
                    }) => {
                        let language = this.language();
                        apply.reconcile_proxy_mode(&mut this.proxy_mode);
                        this.status = format!(
                            "{}{}",
                            language.localized(copy::configuration::FAILED_TO_REMOVE_SOURCE),
                            apply.status_suffix_after_rollback_attempt(
                                language,
                                rollback_error.as_ref(),
                            )
                        );
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
}
