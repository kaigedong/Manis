use super::{
    Context, DueRemoteSource, ImportSubscriptionError, ImportedSubscriptionState, ManisApp,
    SourceKind, SourceLoadOutcome, SourceMutation, SubscriptionRefreshResult, UiEvent, copy,
    mihomo, mutate_saved_sources, source_kind, trace_ui,
};

impl ManisApp {
    pub(in crate::app) fn restore_imported_subscriptions(&mut self, cx: &mut Context<Self>) {
        let pending: Vec<_> = self
            .imported_subscriptions
            .iter()
            .filter(|subscription| {
                matches!(subscription.state, ImportedSubscriptionState::Pending(_))
            })
            .map(|subscription| subscription.id.clone())
            .collect();
        for id in pending {
            self.refresh_imported_subscription(id, cx);
        }
    }

    pub(in crate::app) fn refresh_imported_subscription(
        &mut self,
        id: String,
        cx: &mut Context<Self>,
    ) {
        if self.configuration_transfer.active || self.proxy_source_editor.is_importing() {
            return;
        }
        let language = self.language();
        let Some(store_dir) = self.subscription_store_dir.clone() else {
            return;
        };
        let Some(subscription) = self
            .imported_subscriptions
            .iter_mut()
            .find(|subscription| subscription.id == id)
        else {
            return;
        };
        if matches!(
            subscription.state,
            ImportedSubscriptionState::None
                | ImportedSubscriptionState::Refreshing(_)
                | ImportedSubscriptionState::Removing(_)
        ) || !subscription.enabled
        {
            return;
        }
        let kind = source_kind(&subscription.source);
        let source = subscription.source.clone();
        self.subscription_action_generation = self.subscription_action_generation.wrapping_add(1);
        let generation = self.subscription_action_generation;
        subscription.generation = generation;
        subscription.state = ImportedSubscriptionState::Refreshing(kind);
        language
            .localized(copy::app::UPDATING_SUBSCRIPTION_NODES)
            .clone_into(&mut self.status);
        trace_ui(UiEvent::SourceRestoreStarted);

        let executor = cx.background_executor().clone();
        let runtime = self.runtime.clone();
        let task_id = id.clone();
        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn(async move {
                    let proxy_nameservers =
                        mihomo::discover_subscription_proxy_nameservers(&source);
                    let providers = mihomo::preview_imported_subscription(&source)
                        .map_err(ImportSubscriptionError::Preview)?;
                    let mutation = mutate_saved_sources(&runtime, &store_dir, |store_dir| {
                        let mut stored = mihomo::mark_subscription_source_update_success_in(
                            store_dir,
                            &task_id,
                            mihomo::current_unix_secs(),
                        )?;
                        if !proxy_nameservers.is_empty() {
                            stored = mihomo::update_subscription_source_proxy_nameservers_in(
                                store_dir,
                                &task_id,
                                &proxy_nameservers,
                            )?;
                        }
                        Ok(stored)
                    })
                    .map_err(ImportSubscriptionError::Store)?;
                    Ok::<_, ImportSubscriptionError>(SourceLoadOutcome {
                        providers,
                        mutation,
                    })
                })
                .await;
            this.update(cx, |this, cx| {
                this.finish_subscription_refresh(&id, generation, kind, result, cx);
            })
            .ok();
        })
        .detach();
    }

    pub(in crate::app) fn finish_subscription_refresh(
        &mut self,
        id: &str,
        generation: u64,
        kind: SourceKind,
        result: SubscriptionRefreshResult,
        cx: &mut Context<Self>,
    ) {
        let language = self.language();
        let Some(subscription) = self
            .imported_subscriptions
            .iter_mut()
            .find(|subscription| subscription.id == id)
        else {
            return;
        };
        if subscription.generation != generation {
            return;
        }
        match result {
            Ok(SourceLoadOutcome {
                providers,
                mutation:
                    SourceMutation::Committed {
                        value: stored,
                        apply,
                    },
            }) => {
                let node_count: usize = providers.iter().map(|provider| provider.nodes.len()).sum();
                subscription.providers = providers;
                subscription.state = ImportedSubscriptionState::Ready(kind);
                subscription.refresh_interval = stored.refresh_interval;
                subscription.last_successful_update_unix_secs =
                    stored.last_successful_update_unix_secs;
                self.rule_sources
                    .refresh_retry_not_before
                    .remove(&DueRemoteSource::Subscription(id.to_owned()).scheduler_key());
                apply.reconcile_proxy_mode(&mut self.proxy_mode);
                self.status = copy::app::subscription_updated(
                    language,
                    node_count,
                    &apply.status_suffix(language),
                );
                trace_ui(UiEvent::SourceRestoreSucceeded);
            }
            Ok(SourceLoadOutcome {
                mutation:
                    SourceMutation::RollbackAttempted {
                        apply,
                        rollback_error,
                    },
                ..
            }) => {
                subscription.state = ImportedSubscriptionState::Pending(kind);
                self.status = format!(
                    "{}{}",
                    language.localized(copy::app::SUBSCRIPTION_UPDATE_FAILED_TITLE),
                    apply.status_suffix_after_rollback_attempt(language, rollback_error.as_ref())
                );
                trace_ui(UiEvent::SourceRestoreFailed);
            }
            Err(ImportSubscriptionError::Preview(error)) => {
                subscription.state = ImportedSubscriptionState::Unavailable(kind, error);
                self.status = format!(
                    "{}{}",
                    language.localized(copy::app::SUBSCRIPTION_UPDATE_FAILED_PREFIX),
                    copy::configuration::subscription_preview_error(language, error)
                );
                trace_ui(UiEvent::SourceRestoreFailed);
            }
            Err(ImportSubscriptionError::Store(error)) => {
                subscription.state = ImportedSubscriptionState::StoreError(error);
                self.status = format!(
                    "{}{}",
                    language.localized(
                        copy::app::SUBSCRIPTION_LOADED_BUT_ITS_UPDATE_TIME_COULD_NOT_BE_SAVED
                    ),
                    copy::configuration::subscription_store_error(language, error)
                );
                trace_ui(UiEvent::SourceRestoreFailed);
            }
        }
        cx.notify();
    }
}
