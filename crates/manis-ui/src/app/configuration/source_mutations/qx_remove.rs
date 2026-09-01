use super::{
    Context, ManisApp, QxRuleImportFeedback, SourceMutation, SubscriptionStoreError, copy, mihomo,
};

impl ManisApp {
    pub(in crate::app) fn remove_qx_rule_source(&mut self, id: String, cx: &mut Context<Self>) {
        if self.configuration_transfer.active {
            return;
        }
        let Some(store_dir) = self.subscription_store_dir.clone() else {
            return;
        };
        self.rule_sources.import_generation = self.rule_sources.import_generation.wrapping_add(1);
        let generation = self.rule_sources.import_generation;
        self.rule_sources.feedback = QxRuleImportFeedback::Importing;
        self.language()
            .localized(copy::configuration::REMOVING_REMOTE_QX_RULES)
            .clone_into(&mut self.status);
        let runtime = self.runtime.clone();
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn(async move {
                    crate::app::mutate_saved_sources(&runtime, &store_dir, |store_dir| {
                        mihomo::remove_qx_rule_source_in(store_dir, &id).map(|()| id.clone())
                    })
                })
                .await;
            this.update(cx, |this, cx| {
                if this.rule_sources.import_generation != generation {
                    return;
                }
                match result {
                    Ok(SourceMutation::Committed { value: id, apply }) => {
                        this.rule_sources.sources.retain(|source| source.id != id);
                        let order_saved = this.persist_routing_rule_group_order();
                        this.rule_sources.refreshes.remove(&id);
                        this.rule_sources.refresh_retry_not_before.remove(
                            &crate::app::DueRemoteSource::QxRule(id.clone()).scheduler_key(),
                        );
                        this.rule_sources.feedback = QxRuleImportFeedback::Idle;
                        let language = this.language();
                        apply.reconcile_proxy_mode(&mut this.proxy_mode);
                        if order_saved {
                            this.status = copy::configuration::qx_rules_removed(
                                language,
                                &apply.status_suffix(language),
                            );
                        }
                    }
                    Ok(SourceMutation::RollbackAttempted {
                        apply,
                        rollback_error,
                    }) => {
                        this.rule_sources.feedback = QxRuleImportFeedback::StoreFailed(
                            SubscriptionStoreError::StoreUnavailable,
                        );
                        this.status = format!(
                            "{}{}",
                            this.language()
                                .localized(copy::configuration::REMOTE_QX_RULE_REMOVAL_FAILED),
                            apply.status_suffix_after_rollback_attempt(
                                this.language(),
                                rollback_error.as_ref()
                            )
                        );
                    }
                    Err(error) => {
                        this.rule_sources.feedback = QxRuleImportFeedback::StoreFailed(error);
                        this.status = format!(
                            "{}: {}",
                            this.language()
                                .localized(copy::configuration::REMOTE_QX_RULE_REMOVAL_FAILED),
                            copy::configuration::subscription_store_error(this.language(), error)
                        );
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    pub(in crate::app) fn set_qx_rule_source_enabled(
        &mut self,
        id: String,
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
        self.rule_sources.import_generation = self.rule_sources.import_generation.wrapping_add(1);
        let generation = self.rule_sources.import_generation;
        self.rule_sources
            .target_updates
            .insert(id.clone(), generation);
        self.language()
            .localized(copy::configuration::APPLYING_RULE_SOURCE_STATE)
            .clone_into(&mut self.status);
        let runtime = self.runtime.clone();
        let executor = cx.background_executor().clone();
        let task_id = id.clone();
        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn(async move {
                    crate::app::mutate_saved_sources(&runtime, &store_dir, |store_dir| {
                        mihomo::update_qx_rule_source_enabled_in(store_dir, &task_id, enabled)
                    })
                })
                .await;
            this.update(cx, |this, cx| {
                if this.rule_sources.target_updates.get(&id) != Some(&generation) {
                    return;
                }
                this.rule_sources.target_updates.remove(&id);
                match result {
                    Ok(SourceMutation::Committed {
                        value: stored,
                        apply,
                    }) => {
                        let language = this.language();
                        let enabled = stored.enabled;
                        if let Some(source) = this
                            .rule_sources
                            .sources
                            .iter_mut()
                            .find(|source| source.id == id)
                        {
                            *source = stored;
                        }
                        apply.reconcile_proxy_mode(&mut this.proxy_mode);
                        this.status = format!(
                            "{}{}",
                            if enabled {
                                language.localized(copy::configuration::RULE_SOURCE_ENABLED)
                            } else {
                                language.localized(copy::configuration::RULE_SOURCE_DISABLED)
                            },
                            apply.status_suffix(language)
                        );
                    }
                    Ok(SourceMutation::RollbackAttempted {
                        apply,
                        rollback_error,
                    }) => {
                        this.status = format!(
                            "{}{}",
                            this.language()
                                .localized(copy::configuration::FAILED_TO_CHANGE_RULE_SOURCE_STATE),
                            apply.status_suffix_after_rollback_attempt(
                                this.language(),
                                rollback_error.as_ref()
                            )
                        );
                    }
                    Err(error) => {
                        this.status = format!(
                            "{}: {}",
                            this.language()
                                .localized(copy::configuration::FAILED_TO_CHANGE_RULE_SOURCE_STATE),
                            copy::configuration::subscription_store_error(this.language(), error)
                        );
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }
}
