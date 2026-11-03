use super::{
    Context, ImportSubscriptionError, ImportedSubscription, ImportedSubscriptionState,
    LoadedProvider, ManisApp, SourceKind, SourceLoadOutcome, SourceMutation, StoredSubscription,
    SubscriptionFeedback, SubscriptionImportRequest, SubscriptionImportResult,
    SubscriptionStoreError, SubscriptionTextInput, UiEvent, copy, mihomo, mutate_saved_sources,
    trace_ui,
};

impl ManisApp {
    pub(in crate::app) fn import_remote_subscription(
        &mut self,
        request: SubscriptionImportRequest,
        cx: &mut Context<Self>,
    ) {
        if self.configuration_transfer.active
            || self.source_refresh_busy()
            || self.routing_apply_state.is_busy()
        {
            return;
        }
        let SubscriptionImportRequest {
            input,
            name,
            refresh_interval,
            enabled,
            editing_id,
            kind,
        } = request;
        let language = self.language();
        let Some(store_dir) = self.subscription_store_dir.clone() else {
            self.proxy_source_editor.feedback =
                SubscriptionFeedback::StoreFailed(SubscriptionStoreError::DataDirectoryUnavailable);
            language
                .localized(copy::app::THE_SUBSCRIPTION_STORAGE_LOCATION_IS_UNAVAILABLE)
                .clone_into(&mut self.status);
            trace_ui(UiEvent::SourceImportFailed);
            cx.notify();
            return;
        };
        self.proxy_source_editor.import_generation =
            self.proxy_source_editor.import_generation.wrapping_add(1);
        let generation = self.proxy_source_editor.import_generation;
        self.proxy_source_editor.feedback = SubscriptionFeedback::Importing(kind);
        language
            .localized(copy::app::VALIDATING_NODES_AND_IMPORTING_SUBSCRIPTION)
            .clone_into(&mut self.status);
        trace_ui(UiEvent::SourceImportStarted);
        if let Some(input) = self.proxy_source_editor.input.as_ref() {
            input.update(cx, |input, cx| input.set_enabled(false, cx));
        }
        if let Some(input) = self.proxy_source_editor.name_input.as_ref() {
            input.update(cx, |input, cx| input.set_enabled(false, cx));
        }

        let executor = cx.background_executor().clone();
        let runtime = self.runtime.clone();
        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn(async move {
                    let providers = mihomo::preview_subscription(&input)
                        .map_err(ImportSubscriptionError::Preview)?;
                    let mutation = mutate_saved_sources(&runtime, &store_dir, |store_dir| {
                        let mut subscription = if let Some(id) = editing_id.as_deref() {
                            mihomo::update_subscription_source_in(
                                store_dir,
                                id,
                                &input,
                                &name,
                                refresh_interval,
                                enabled,
                            )
                        } else {
                            mihomo::save_subscription_source_with_options_in(
                                store_dir,
                                &input,
                                &name,
                                refresh_interval,
                                enabled,
                            )
                        }?;
                        let proxy_nameservers =
                            mihomo::discover_subscription_proxy_nameservers(&subscription.source);
                        if !proxy_nameservers.is_empty() {
                            subscription = mihomo::update_subscription_source_proxy_nameservers_in(
                                store_dir,
                                &subscription.id,
                                &proxy_nameservers,
                            )?;
                        }
                        Ok(subscription)
                    })
                    .map_err(ImportSubscriptionError::Store)?;
                    Ok::<_, ImportSubscriptionError>(SourceLoadOutcome {
                        providers,
                        mutation,
                    })
                })
                .await;
            this.update(cx, |this, cx| {
                this.finish_subscription_import(generation, kind, result, cx);
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    pub(in crate::app) fn finish_subscription_import(
        &mut self,
        generation: u64,
        kind: SourceKind,
        result: SubscriptionImportResult,
        cx: &mut Context<Self>,
    ) {
        if self.proxy_source_editor.import_generation != generation {
            return;
        }
        if let Some(input) = self.proxy_source_editor.input.as_ref() {
            input.update(cx, |input, cx| input.set_enabled(true, cx));
        }
        if let Some(input) = self.proxy_source_editor.name_input.as_ref() {
            input.update(cx, |input, cx| input.set_enabled(true, cx));
        }
        let language = self.language();
        match result {
            Ok(SourceLoadOutcome {
                providers,
                mutation:
                    SourceMutation::Committed {
                        value: subscription,
                        apply,
                    },
            }) => {
                let node_count: usize = providers.iter().map(|provider| provider.nodes.len()).sum();
                let provider_count = providers.len();
                self.subscription_action_generation =
                    self.subscription_action_generation.wrapping_add(1);
                self.merge_imported_subscription(
                    subscription,
                    &providers,
                    self.subscription_action_generation,
                    kind,
                );
                self.subscription_preview_providers = providers;
                self.proxy_source_editor.feedback = SubscriptionFeedback::Idle;
                if let Some(input) = self.proxy_source_editor.input.as_ref() {
                    input.update(cx, SubscriptionTextInput::clear_without_event);
                }
                if let Some(input) = self.proxy_source_editor.name_input.as_ref() {
                    input.update(cx, SubscriptionTextInput::clear_without_event);
                }
                self.configuration_add_section = None;
                self.proxy_source_editor.target.reset();
                self.proxy_source_editor.error = None;
                apply.reconcile_proxy_mode(&mut self.proxy_mode);
                self.status = copy::app::subscription_imported(
                    language,
                    self.imported_subscriptions.len(),
                    provider_count,
                    node_count,
                    &apply.status_suffix(language),
                );
                trace_ui(UiEvent::SourceImportSucceeded);
            }
            Ok(SourceLoadOutcome {
                mutation:
                    SourceMutation::RollbackAttempted {
                        apply,
                        rollback_error,
                    },
                ..
            }) => {
                self.proxy_source_editor.feedback =
                    SubscriptionFeedback::StoreFailed(SubscriptionStoreError::StoreUnavailable);
                self.status = format!(
                    "{}{}",
                    language.localized(copy::app::SUBSCRIPTION_SAVE_FAILED_TITLE),
                    apply.status_suffix_after_rollback_attempt(language, rollback_error.as_ref())
                );
                trace_ui(UiEvent::SourceImportFailed);
            }
            Err(ImportSubscriptionError::Preview(error)) => {
                self.proxy_source_editor.feedback = SubscriptionFeedback::PreviewFailed(error);
                self.status = format!(
                    "{}{}",
                    language.localized(copy::app::SUBSCRIPTION_IMPORT_FAILED),
                    copy::configuration::subscription_preview_error(language, error)
                );
                trace_ui(UiEvent::SourceImportFailed);
            }
            Err(ImportSubscriptionError::Store(error)) => {
                self.proxy_source_editor.feedback = SubscriptionFeedback::StoreFailed(error);
                self.status = format!(
                    "{}{}",
                    language.localized(copy::app::SUBSCRIPTION_SAVE_FAILED_PREFIX),
                    copy::configuration::subscription_store_error(language, error)
                );
                trace_ui(UiEvent::SourceImportFailed);
            }
        }
        cx.notify();
    }

    pub(in crate::app) fn merge_imported_subscription(
        &mut self,
        subscription: StoredSubscription,
        providers: &[LoadedProvider],
        generation: u64,
        kind: SourceKind,
    ) {
        if let Some(existing) = self
            .imported_subscriptions
            .iter_mut()
            .find(|existing| existing.id == subscription.id)
        {
            existing.name.clone_from(&subscription.name);
            existing.source = subscription.source;
            existing.enabled = subscription.enabled;
            existing.state = if subscription.enabled {
                ImportedSubscriptionState::Ready(kind)
            } else {
                ImportedSubscriptionState::None
            };
            existing.providers = providers.to_vec();
            existing.generation = generation;
            existing.refresh_interval = subscription.refresh_interval;
            existing.last_successful_update_unix_secs =
                subscription.last_successful_update_unix_secs;
            return;
        }
        self.imported_subscriptions.push(ImportedSubscription {
            id: subscription.id,
            name: subscription.name,
            source: subscription.source,
            enabled: subscription.enabled,
            state: if subscription.enabled {
                ImportedSubscriptionState::Ready(kind)
            } else {
                ImportedSubscriptionState::None
            },
            providers: providers.to_vec(),
            generation,
            refresh_interval: subscription.refresh_interval,
            last_successful_update_unix_secs: subscription.last_successful_update_unix_secs,
        });
    }
}
