use super::{
    Context, LogLevel, ManisApp, SourceMutation, SourceRuntimeApply, SubscriptionStoreError, copy,
    mihomo, record_event,
};

impl ManisApp {
    pub(in crate::app) fn update_qx_rule_source_target(
        &mut self,
        id: String,
        target: String,
        cx: &mut Context<Self>,
    ) {
        if self.configuration_transfer.active {
            return;
        }
        if self.source_refresh_busy() || !self.qx_rule_targets().contains(&target) {
            return;
        }
        let Some(store_dir) = self.subscription_store_dir.clone() else {
            self.language()
                .localized(copy::configuration::COULD_NOT_DETERMINE_WHERE_TO_SAVE_THE_RULE_SOURCE)
                .clone_into(&mut self.status);
            cx.notify();
            return;
        };
        let Some(source) = self
            .rule_sources
            .sources
            .iter()
            .find(|source| source.id == id)
        else {
            return;
        };
        if self.effective_rule_target(source.target_policy.as_str(), self.language()) == target {
            self.rule_sources.target_popover = None;
            cx.notify();
            return;
        }

        self.rule_sources.import_generation = self.rule_sources.import_generation.wrapping_add(1);
        let generation = self.rule_sources.import_generation;
        self.rule_sources
            .target_updates
            .insert(id.clone(), generation);
        self.rule_sources.target_popover = None;
        self.status = format!(
            "{} {target}",
            self.language()
                .localized(copy::configuration::SAVING_RULE_SOURCE_POLICY)
        );
        let runtime = self.runtime.clone();
        let executor = cx.background_executor().clone();
        let task_id = id.clone();
        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn(async move {
                    crate::app::mutate_saved_sources(&runtime, &store_dir, |store_dir| {
                        mihomo::update_qx_rule_source_target_in(store_dir, &task_id, &target)
                    })
                })
                .await;
            this.update(cx, |this, cx| {
                this.finish_qx_rule_target_update(&id, generation, result, cx);
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn finish_qx_rule_target_update(
        &mut self,
        id: &str,
        generation: u64,
        result: Result<
            crate::app::SourceMutation<mihomo::StoredQxRuleSource>,
            SubscriptionStoreError,
        >,
        cx: &mut Context<Self>,
    ) {
        if self.rule_sources.target_updates.get(id) != Some(&generation) {
            return;
        }
        self.rule_sources.target_updates.remove(id);
        match result {
            Ok(SourceMutation::Committed {
                value: stored,
                apply,
            }) => {
                self.finish_successful_qx_rule_target_update(id, stored, &apply);
            }
            Ok(SourceMutation::RollbackAttempted {
                apply,
                rollback_error,
            }) => {
                self.status = format!(
                    "{}{}",
                    self.language()
                        .localized(copy::configuration::FAILED_TO_SAVE_RULE_SOURCE_POLICY),
                    apply.status_suffix_after_rollback_attempt(
                        self.language(),
                        rollback_error.as_ref()
                    )
                );
            }
            Err(error) => {
                self.status = format!(
                    "{}: {}",
                    self.language()
                        .localized(copy::configuration::FAILED_TO_SAVE_RULE_SOURCE_POLICY),
                    copy::configuration::subscription_store_error(self.language(), error)
                );
            }
        }
        cx.notify();
    }

    fn finish_successful_qx_rule_target_update(
        &mut self,
        id: &str,
        stored: mihomo::StoredQxRuleSource,
        apply: &SourceRuntimeApply,
    ) {
        let language = self.language();
        let target = stored.target_policy.as_str().to_owned();
        if let Some(source) = self
            .rule_sources
            .sources
            .iter_mut()
            .find(|source| source.id == id)
        {
            *source = stored;
        }
        apply.reconcile_proxy_mode(&mut self.proxy_mode);
        self.status = format!(
            "{} {target}{}",
            language.localized(copy::configuration::RULE_SOURCE_POLICY_SET_TO),
            apply.status_suffix(language)
        );
        record_event(
            LogLevel::Info,
            "routing.rule_source.target.updated",
            format!("source_id={id} target={target}"),
        );
    }
}
