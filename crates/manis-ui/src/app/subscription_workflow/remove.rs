use super::{
    Context, DueRemoteSource, ImportedSubscriptionState, ManisApp, SourceMutation,
    SubscriptionFeedback, UiEvent, copy, mihomo, mutate_saved_sources, source_kind, trace_ui,
};

impl ManisApp {
    pub(in crate::app) fn remove_imported_subscription(
        &mut self,
        id: String,
        cx: &mut Context<Self>,
    ) {
        if self.configuration_transfer.active || self.source_refresh_busy() {
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
        let kind = source_kind(&subscription.source);
        self.subscription_action_generation = self.subscription_action_generation.wrapping_add(1);
        let generation = self.subscription_action_generation;
        subscription.generation = generation;
        self.proxy_source_editor.feedback = SubscriptionFeedback::Idle;
        subscription.state = ImportedSubscriptionState::Removing(kind);
        language
            .localized(copy::app::REMOVING_IMPORTED_SUBSCRIPTION)
            .clone_into(&mut self.status);
        trace_ui(UiEvent::SourceRemoveStarted);

        let executor = cx.background_executor().clone();
        let remove_id = id.clone();
        let runtime = self.runtime.clone();
        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn(async move {
                    mutate_saved_sources(&runtime, &store_dir, |store_dir| {
                        mihomo::remove_subscription_source_in(store_dir, &remove_id)
                    })
                })
                .await;
            this.update(cx, |this, cx| {
                let language = this.language();
                let Some(index) = this
                    .imported_subscriptions
                    .iter()
                    .position(|subscription| subscription.id == id)
                else {
                    return;
                };
                if this.imported_subscriptions[index].generation != generation {
                    return;
                }
                match result {
                    Ok(SourceMutation::Committed { apply, .. }) => {
                        this.imported_subscriptions.remove(index);
                        this.rule_sources
                            .refresh_retry_not_before
                            .remove(&DueRemoteSource::Subscription(id.clone()).scheduler_key());
                        apply.reconcile_proxy_mode(&mut this.proxy_mode);
                        this.status = format!(
                            "{}{}",
                            language.localized(copy::app::IMPORTED_SUBSCRIPTION_REMOVED),
                            apply.status_suffix(language)
                        );
                        trace_ui(UiEvent::SourceRemoveSucceeded);
                    }
                    Ok(SourceMutation::RollbackAttempted {
                        apply,
                        rollback_error,
                    }) => {
                        this.imported_subscriptions[index].state =
                            ImportedSubscriptionState::Ready(kind);
                        this.status = format!(
                            "{}{}",
                            language.localized(copy::app::IMPORTED_SUBSCRIPTION_REMOVAL_FAILED),
                            apply.status_suffix_after_rollback_attempt(
                                language,
                                rollback_error.as_ref()
                            )
                        );
                        trace_ui(UiEvent::SourceRemoveFailed);
                    }
                    Err(error) => {
                        this.imported_subscriptions[index].state =
                            ImportedSubscriptionState::StoreError(error);
                        this.status = format!(
                            "{}{}",
                            language.localized(copy::app::COULD_NOT_REMOVE_SUBSCRIPTION),
                            copy::configuration::subscription_store_error(language, error)
                        );
                        trace_ui(UiEvent::SourceRemoveFailed);
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}
