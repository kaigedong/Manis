use super::{Context, ManisApp, Message, SubscriptionStoreError, copy, mihomo};

impl ManisApp {
    /// Everything one routing mutation can change, so a failed save restores all of it.
    pub(in crate::app) fn routing_rollback_snapshot(&self) -> crate::app::RoutingApplyRollback {
        crate::app::RoutingApplyRollback {
            manual_rules: self.manual_rules.clone(),
            local_rule_groups: self.local_rule_groups.clone(),
            group_order: self.rule_sources.group_order.clone(),
        }
    }

    fn restore_routing_rollback(&mut self, rollback: crate::app::RoutingApplyRollback) {
        self.manual_rules = rollback.manual_rules;
        self.local_rule_groups = rollback.local_rule_groups;
        self.rule_sources.group_order = rollback.group_order;
    }

    /// Saves the whole routing rule set: the group order, local groups and manual rules.
    pub(in crate::app) fn persist_routing_rules(
        &mut self,
        completion: String,
        previous: crate::app::RoutingApplyRollback,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.configuration_transfer.active {
            self.restore_routing_rollback(previous);
            return false;
        }
        let language = self.language();
        if self.routing_apply_state.is_busy() {
            language
                .message(Message::RoutingApplyBusy)
                .clone_into(&mut self.status);
            self.restore_routing_rollback(previous);
            cx.notify();
            return false;
        }
        let Some(store_dir) = self.subscription_store_dir.clone() else {
            language
                .message(Message::ManualRulesLocationUnavailable)
                .clone_into(&mut self.status);
            self.restore_routing_rollback(previous);
            cx.notify();
            return false;
        };
        self.sync_routing_rule_group_order();
        self.start_routing_runtime_apply(store_dir, completion, previous, cx);
        true
    }

    pub(in crate::app) fn start_routing_runtime_apply(
        &mut self,
        store_dir: std::path::PathBuf,
        completion: String,
        rollback: crate::app::RoutingApplyRollback,
        cx: &mut Context<Self>,
    ) {
        let started = self.routing_apply_state.begin();
        debug_assert!(started, "routing apply must be idle before spawning");
        if !started {
            return;
        }
        self.status = format!(
            "{} · {}",
            completion,
            self.language().message(Message::ApplyingChanges)
        );
        let runtime = self.runtime.clone();
        let executor = cx.background_executor().clone();
        let rules = self.manual_rules.clone();
        let local_groups = self.local_rule_groups.clone();
        let order = self.rule_sources.group_order.clone();
        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn(async move {
                    crate::app::mutate_saved_sources(&runtime, &store_dir, |staged| {
                        mihomo::save_routing_rule_group_order_in(staged, &order)?;
                        mihomo::save_local_routing_rule_groups_in(staged, &local_groups)?;
                        crate::manual_rule::save_manual_rules_in(staged, &rules)
                            .map_err(|_| SubscriptionStoreError::StoreUnavailable)
                    })
                })
                .await;
            this.update(cx, |this, cx| {
                this.routing_apply_state.finish();
                if !matches!(&result, Ok(crate::app::SourceMutation::Committed { .. })) {
                    this.restore_routing_rollback(rollback);
                }
                this.status = match result {
                    Ok(crate::app::SourceMutation::Committed { apply, .. }) => {
                        apply.reconcile_proxy_mode(&mut this.proxy_mode);
                        format!("{completion}{}", apply.status_suffix(this.language()))
                    }
                    Ok(crate::app::SourceMutation::RollbackAttempted {
                        apply,
                        rollback_error,
                    }) => {
                        apply.reconcile_proxy_mode(&mut this.proxy_mode);
                        let suffix = apply.status_suffix_after_rollback_attempt(
                            this.language(),
                            rollback_error.as_ref(),
                        );
                        format!("{completion}{suffix}")
                    }
                    Err(error) => format!(
                        "{}{}",
                        this.language().message(Message::ManualRulesSaveFailed),
                        copy::configuration::subscription_store_error(this.language(), error),
                    ),
                };
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    pub(in crate::app) fn sync_routing_rule_group_order(&mut self) {
        self.rule_sources.group_order = mihomo::normalized_routing_rule_group_order(
            &self.rule_sources.group_order,
            !self.manual_rules.is_empty(),
            &self.rule_sources.sources,
            &self.local_rule_groups,
        );
    }

    pub(in crate::app) fn reorder_routing_rule_group(
        &mut self,
        group_id: &str,
        anchor_id: &str,
        after: bool,
        cx: &mut Context<Self>,
    ) {
        if self.configuration_transfer.active || self.routing_apply_state.is_busy() {
            return;
        }
        self.sync_routing_rule_group_order();
        let previous = self.routing_rollback_snapshot();
        if !mihomo::place_routing_rule_group(
            &mut self.rule_sources.group_order,
            group_id,
            anchor_id,
            after,
        ) {
            return;
        }
        let Some(store_dir) = self.subscription_store_dir.clone() else {
            self.rule_sources.group_order = previous.group_order;
            self.language()
                .message(Message::ManualRulesLocationUnavailable)
                .clone_into(&mut self.status);
            cx.notify();
            return;
        };
        self.start_routing_runtime_apply(
            store_dir,
            self.language()
                .localized(copy::configuration::RULE_GROUP_ORDER_UPDATED)
                .to_owned(),
            previous,
            cx,
        );
    }

    pub(in crate::app) fn move_routing_rule_group(
        &mut self,
        group_id: &str,
        direction: mihomo::MoveDirection,
        cx: &mut Context<Self>,
    ) {
        if self.configuration_transfer.active {
            return;
        }
        if self.routing_apply_state.is_busy() {
            self.language()
                .message(Message::RoutingApplyBusy)
                .clone_into(&mut self.status);
            cx.notify();
            return;
        }
        self.sync_routing_rule_group_order();
        let previous = self.routing_rollback_snapshot();
        if !mihomo::move_routing_rule_group(&mut self.rule_sources.group_order, group_id, direction)
        {
            return;
        }
        let Some(store_dir) = self.subscription_store_dir.clone() else {
            self.rule_sources.group_order = previous.group_order;
            return;
        };
        let language = self.language();
        let completion = match direction {
            mihomo::MoveDirection::Up => {
                language.localized(copy::configuration::RULE_GROUP_MOVED_UP)
            }
            mihomo::MoveDirection::Down => {
                language.localized(copy::configuration::RULE_GROUP_MOVED_DOWN)
            }
        }
        .to_owned();
        self.start_routing_runtime_apply(store_dir, completion, previous, cx);
    }
}
