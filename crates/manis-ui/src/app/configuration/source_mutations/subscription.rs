use super::{
    Context, ImportedSubscriptionState, ManisApp, SourceMutation, SubscriptionStoreError,
    SubscriptionToggleCompletion, copy, mihomo,
};

impl ManisApp {
    pub(in crate::app) fn set_subscription_enabled(
        &mut self,
        id: &str,
        enabled: bool,
        cx: &mut Context<Self>,
    ) {
        if self.configuration_transfer.active {
            return;
        }
        if self.source_refresh_busy() {
            return;
        }
        let Some(store_dir) = self.subscription_store_dir.clone() else {
            return;
        };
        let Some(source) = self
            .imported_subscriptions
            .iter_mut()
            .find(|source| source.id == id)
        else {
            return;
        };
        let previous_state = source.state;
        let previous_enabled = source.enabled;
        let kind = crate::app::source_kind(&source.source);
        self.subscription_action_generation = self.subscription_action_generation.wrapping_add(1);
        let generation = self.subscription_action_generation;
        source.generation = generation;
        source.state = ImportedSubscriptionState::Refreshing(kind);
        let completion = SubscriptionToggleCompletion {
            id: id.to_owned(),
            generation,
            kind,
            previous_state,
            previous_enabled,
        };
        self.language()
            .localized(copy::configuration::APPLYING_SUBSCRIPTION_STATE)
            .clone_into(&mut self.status);
        let runtime = self.runtime.clone();
        let executor = cx.background_executor().clone();
        let task_id = id.to_owned();
        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn(async move {
                    crate::app::mutate_saved_sources(&runtime, &store_dir, |store_dir| {
                        mihomo::update_subscription_source_enabled_in(store_dir, &task_id, enabled)
                    })
                })
                .await;
            this.update(cx, |this, cx| {
                this.finish_subscription_toggle(completion, result, cx);
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn finish_subscription_toggle(
        &mut self,
        completion: SubscriptionToggleCompletion,
        result: Result<
            crate::app::SourceMutation<mihomo::StoredSubscription>,
            SubscriptionStoreError,
        >,
        cx: &mut Context<Self>,
    ) {
        let language = self.language();
        let Some(source) = self
            .imported_subscriptions
            .iter_mut()
            .find(|source| source.id == completion.id)
        else {
            return;
        };
        if source.generation != completion.generation {
            return;
        }
        let refresh_after_enable = match result {
            Ok(SourceMutation::Committed {
                value: stored,
                apply,
            }) => {
                source.enabled = stored.enabled;
                source.state = if stored.enabled {
                    ImportedSubscriptionState::Pending(completion.kind)
                } else {
                    ImportedSubscriptionState::None
                };
                apply.reconcile_proxy_mode(&mut self.proxy_mode);
                self.status = format!(
                    "{}{}",
                    if stored.enabled {
                        language.localized(copy::configuration::SUBSCRIPTION_ENABLED)
                    } else {
                        language.localized(copy::configuration::SUBSCRIPTION_DISABLED)
                    },
                    apply.status_suffix(language)
                );
                stored.enabled
            }
            Ok(SourceMutation::RollbackAttempted {
                apply,
                rollback_error,
            }) => {
                source.enabled = completion.previous_enabled;
                source.state = completion.previous_state;
                self.status = format!(
                    "{}{}",
                    language.localized(copy::configuration::FAILED_TO_CHANGE_SUBSCRIPTION_STATE),
                    apply.status_suffix_after_rollback_attempt(language, rollback_error.as_ref())
                );
                false
            }
            Err(error) => {
                source.enabled = completion.previous_enabled;
                source.state = completion.previous_state;
                self.status = format!(
                    "{}: {}",
                    language.localized(copy::configuration::FAILED_TO_CHANGE_SUBSCRIPTION_STATE),
                    copy::configuration::subscription_store_error(language, error)
                );
                false
            }
        };
        if refresh_after_enable {
            self.refresh_imported_subscription(completion.id, cx);
        }
        cx.notify();
    }
}
