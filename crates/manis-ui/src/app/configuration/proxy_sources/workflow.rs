use super::{
    Context, Entity, ManisApp, ProxySourceEditorKind, ProxySourceEditorTarget, SourceKind,
    SourceMutation, SourceRuntimeApply, SubscriptionFeedback, SubscriptionStoreError,
    SubscriptionTextInput, UiEvent, copy, mihomo, trace_ui, validate_single_node_preview,
    validate_subscription_preview,
};

impl ManisApp {
    pub(in crate::app) fn submit_source_import(
        &mut self,
        input: &Entity<SubscriptionTextInput>,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.configuration_transfer.active
            || self.source_refresh_busy()
            || self.routing_apply_state.is_busy()
        {
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
                match self.proxy_source_editor.target.kind() {
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
                if matches!(
                    self.proxy_source_editor.target,
                    ProxySourceEditorTarget::Subscription { .. }
                ) {
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
                if matches!(
                    self.proxy_source_editor.target,
                    ProxySourceEditorTarget::SingleNode { .. }
                ) {
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
                    crate::app::SubscriptionImportRequest {
                        input: input_value,
                        name,
                        refresh_interval: self.proxy_source_editor.refresh_interval,
                        enabled: self.proxy_source_editor.enabled,
                        editing_id: self
                            .proxy_source_editor
                            .target
                            .editing_id()
                            .map(str::to_owned),
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

    pub(in crate::app) fn import_single_node(
        &mut self,
        input_value: String,
        name: String,
        preview: crate::subscription::SubscriptionPreview,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.configuration_transfer.active
            || self.source_refresh_busy()
            || self.routing_apply_state.is_busy()
        {
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
        self.proxy_source_editor.import_generation =
            self.proxy_source_editor.import_generation.wrapping_add(1);
        let generation = self.proxy_source_editor.import_generation;
        self.proxy_source_editor.feedback = SubscriptionFeedback::Importing(SourceKind::SingleNode);
        self.language()
            .localized(copy::configuration::VALIDATING_AND_SAVING_SINGLE_NODE_SOURCE)
            .clone_into(&mut self.status);
        if let Some(input) = self.proxy_source_editor.input.as_ref() {
            input.update(cx, |input, cx| input.set_enabled(false, cx));
        }
        let runtime = self.runtime.clone();
        let editing_id = self
            .proxy_source_editor
            .target
            .editing_id()
            .map(str::to_owned);
        let enabled = self.proxy_source_editor.enabled;
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn(async move {
                    let providers = mihomo::preview_single_node(&input_value)?;
                    let transaction =
                        crate::app::mutate_saved_sources(&runtime, &store_dir, |store_dir| {
                            if let Some(id) = editing_id {
                                mihomo::update_single_node_source_in(
                                    store_dir,
                                    &id,
                                    &input_value,
                                    &name,
                                    enabled,
                                )
                            } else {
                                mihomo::save_single_node_source_with_options_in(
                                    store_dir,
                                    &input_value,
                                    &name,
                                    enabled,
                                )
                            }
                        })?;
                    Ok::<_, SubscriptionStoreError>(crate::app::SourceLoadOutcome {
                        providers,
                        mutation: transaction,
                    })
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

    pub(in crate::app) fn finish_single_node_import(
        &mut self,
        generation: u64,
        preview: crate::subscription::SubscriptionPreview,
        result: crate::app::SingleNodeImportResult,
        cx: &mut Context<Self>,
    ) {
        if self.proxy_source_editor.import_generation != generation {
            return;
        }
        if let Some(input) = self.proxy_source_editor.input.as_ref() {
            input.update(cx, |input, cx| input.set_enabled(true, cx));
        }
        match result {
            Ok(crate::app::SourceLoadOutcome {
                providers,
                mutation:
                    SourceMutation::Committed {
                        value: stored,
                        apply,
                    },
            }) => {
                self.finish_saved_single_node(stored, &apply, providers, preview, cx);
            }
            Ok(crate::app::SourceLoadOutcome {
                mutation:
                    SourceMutation::RollbackAttempted {
                        apply,
                        rollback_error,
                    },
                ..
            }) => {
                let language = self.language();
                apply.reconcile_proxy_mode(&mut self.proxy_mode);
                self.proxy_source_editor.feedback =
                    SubscriptionFeedback::StoreFailed(SubscriptionStoreError::StoreUnavailable);
                self.status = format!(
                    "{}{}",
                    language.localized(copy::configuration::SINGLE_NODE_SOURCE_SAVE_FAILED),
                    apply.status_suffix_after_rollback_attempt(language, rollback_error.as_ref())
                );
                trace_ui(UiEvent::SourceImportFailed);
                cx.notify();
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

    pub(in crate::app) fn finish_saved_single_node(
        &mut self,
        stored: mihomo::StoredSingleNode,
        apply: &SourceRuntimeApply,
        providers: Vec<mihomo::LoadedProvider>,
        preview: crate::subscription::SubscriptionPreview,
        cx: &mut Context<Self>,
    ) {
        let language = self.language();
        apply.reconcile_proxy_mode(&mut self.proxy_mode);
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
        self.proxy_source_editor.target.reset();
        self.proxy_source_editor.error = None;
        self.status =
            copy::configuration::single_node_saved(language, &apply.status_suffix(language));
        trace_ui(UiEvent::SourceImportSucceeded);
        cx.notify();
    }
}
