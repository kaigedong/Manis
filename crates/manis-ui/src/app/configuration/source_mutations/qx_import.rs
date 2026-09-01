use super::{
    Context, Entity, ImportQxRuleError, ImportQxRuleSuccess, LogLevel, ManisApp,
    QxRuleImportFeedback, QxRuleSaveRequest, SecretUrl, SourceRuntimeApply, SubscriptionStoreError,
    SubscriptionTextInput, begin_operation, copy, mihomo, record_event, record_operation,
    save_qx_rule_source,
};

impl ManisApp {
    pub(in crate::app) fn submit_qx_rule_import(
        &mut self,
        input: &Entity<SubscriptionTextInput>,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.configuration_transfer.active {
            return false;
        }
        let url = input.read(cx).value().trim().to_owned();
        let name = self
            .inputs
            .qx_rule_name
            .as_ref()
            .map(|input| input.read(cx).value().trim().to_owned())
            .unwrap_or_default();
        let target = self.rule_sources.target_policy.clone();
        let editing_id = self.rule_sources.editor_source_id.clone();
        let refresh_interval = self.rule_sources.editor_refresh_interval;
        let operation_id = begin_operation(
            "configuration.rule_source.save.requested",
            format!(
                "editing={} target={target} known_sources={}",
                editing_id.is_some(),
                self.rule_sources.sources.len()
            ),
        );
        let Ok(parsed_source) = SecretUrl::parse_https(&url) else {
            self.rule_sources.feedback =
                QxRuleImportFeedback::StoreFailed(SubscriptionStoreError::InvalidSource);
            self.language()
                .localized(copy::configuration::ENTER_A_VALID_HTTPS_RULE_URL)
                .clone_into(&mut self.status);
            cx.notify();
            return false;
        };
        let Some(store_dir) = self.subscription_store_dir.clone() else {
            self.rule_sources.feedback =
                QxRuleImportFeedback::StoreFailed(SubscriptionStoreError::DataDirectoryUnavailable);
            self.language()
                .localized(copy::configuration::COULD_NOT_DETERMINE_WHERE_TO_SAVE_RULES)
                .clone_into(&mut self.status);
            record_operation(
                operation_id,
                LogLevel::Error,
                "configuration.rule_source.add.failed",
                "phase=store reason=data_directory_unavailable",
            );
            cx.notify();
            return false;
        };
        if self.reject_duplicate_qx_rule_source(
            &parsed_source,
            editing_id.as_deref(),
            operation_id,
            cx,
        ) {
            return false;
        }
        self.rule_sources.import_generation = self.rule_sources.import_generation.wrapping_add(1);
        let generation = self.rule_sources.import_generation;
        self.rule_sources.feedback = QxRuleImportFeedback::Importing;
        self.language()
            .localized(copy::configuration::DOWNLOADING_AND_PARSING_QX_RULES)
            .clone_into(&mut self.status);
        input.update(cx, |input, cx| input.set_enabled(false, cx));
        if let Some(input) = self.inputs.qx_rule_name.as_ref() {
            input.update(cx, |input, cx| input.set_enabled(false, cx));
        }
        let runtime = self.runtime.clone();
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn(async move {
                    save_qx_rule_source(
                        &runtime,
                        &store_dir,
                        &QxRuleSaveRequest {
                            url,
                            name,
                            target,
                            editing_id,
                            refresh_interval,
                        },
                    )
                })
                .await;
            this.update(cx, |this, cx| {
                this.finish_qx_rule_import(generation, operation_id, result, cx);
            })
            .ok();
        })
        .detach();
        cx.notify();
        true
    }

    fn reject_duplicate_qx_rule_source(
        &mut self,
        parsed_source: &SecretUrl,
        editing_id: Option<&str>,
        operation_id: u64,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some((source_id, rule_count, stored_target)) = self
            .rule_sources
            .sources
            .iter()
            .find(|source| {
                source.source == *parsed_source && editing_id != Some(source.id.as_str())
            })
            .map(|source| {
                (
                    source.id.clone(),
                    source.rule_count,
                    source.target_policy.clone(),
                )
            })
        else {
            return false;
        };
        let target_policy = self.effective_rule_target(stored_target.as_str(), self.language());
        self.rule_sources.feedback = QxRuleImportFeedback::AlreadyExists {
            source_id: source_id.clone(),
            rule_count,
            target_policy: target_policy.clone(),
        };
        self.language()
            .localized(copy::configuration::RULE_SOURCE_ALREADY_EXISTS_NO_DUPLICATE_WAS_ADDED)
            .clone_into(&mut self.status);
        record_operation(
            operation_id,
            LogLevel::Warn,
            "configuration.rule_source.add.duplicate",
            format!("existing_id={source_id} rules={rule_count} target={target_policy}"),
        );
        cx.notify();
        true
    }

    fn finish_qx_rule_import(
        &mut self,
        generation: u64,
        operation_id: u64,
        result: crate::app::QxRuleImportResult,
        cx: &mut Context<Self>,
    ) {
        if self.rule_sources.import_generation != generation {
            return;
        }
        if let Some(input) = self.inputs.qx_rule.as_ref() {
            input.update(cx, |input, cx| input.set_enabled(true, cx));
        }
        if let Some(input) = self.inputs.qx_rule_name.as_ref() {
            input.update(cx, |input, cx| input.set_enabled(true, cx));
        }
        match result {
            Ok(ImportQxRuleSuccess::Imported { stored, apply }) => {
                self.finish_imported_qx_rule(operation_id, stored, &apply, cx);
            }
            Ok(ImportQxRuleSuccess::AlreadyExists { stored }) => {
                self.finish_existing_qx_rule(operation_id, stored);
            }
            Ok(ImportQxRuleSuccess::RolledBack {
                apply,
                rollback_error,
            }) => {
                self.rule_sources.feedback = QxRuleImportFeedback::Idle;
                self.status = apply
                    .status_suffix_after_rollback_attempt(self.language(), rollback_error.as_ref());
            }
            Err(error) => self.finish_failed_qx_rule_import(operation_id, &error),
        }
        cx.notify();
    }

    fn finish_imported_qx_rule(
        &mut self,
        operation_id: u64,
        stored: mihomo::StoredQxRuleSource,
        apply: &SourceRuntimeApply,
        cx: &mut Context<Self>,
    ) {
        let language = self.language();
        let rule_count = stored.rule_count;
        let diagnostic_count = stored.diagnostic_count;
        let stored_id = stored.id.clone();
        let target_policy = self.effective_rule_target(stored.target_policy.as_str(), language);
        if let Some(existing) = self
            .rule_sources
            .sources
            .iter_mut()
            .find(|source| source.id == stored_id)
        {
            *existing = stored;
        } else {
            self.rule_sources.sources.push(stored);
        }
        let order_saved = self.persist_routing_rule_group_order();
        self.rule_sources.refreshes.remove(&stored_id);
        self.rule_sources.feedback = QxRuleImportFeedback::Imported {
            rule_count,
            diagnostic_count,
        };
        if let Some(input) = self.inputs.qx_rule.as_ref() {
            input.update(cx, SubscriptionTextInput::clear_without_event);
        }
        if let Some(input) = self.inputs.qx_rule_name.as_ref() {
            input.update(cx, SubscriptionTextInput::clear_without_event);
        }
        apply.reconcile_proxy_mode(&mut self.proxy_mode);
        if order_saved {
            self.status = copy::configuration::qx_rules_applied(
                language,
                copy::configuration::QxRuleAction::Imported,
                rule_count,
                &apply.status_suffix(language),
            );
        }
        record_operation(
            operation_id,
            LogLevel::Info,
            "configuration.rule_source.add.succeeded",
            format!(
                "id={stored_id} rules={rule_count} skipped={diagnostic_count} target={target_policy}"
            ),
        );
    }

    fn finish_existing_qx_rule(&mut self, operation_id: u64, stored: mihomo::StoredQxRuleSource) {
        let target_policy =
            self.effective_rule_target(stored.target_policy.as_str(), self.language());
        let source_id = stored.id.clone();
        let rule_count = stored.rule_count;
        if !self
            .rule_sources
            .sources
            .iter()
            .any(|source| source.id == source_id)
        {
            self.rule_sources.sources.push(stored);
        }
        let order_saved = self.persist_routing_rule_group_order();
        self.rule_sources.feedback = QxRuleImportFeedback::AlreadyExists {
            source_id: source_id.clone(),
            rule_count,
            target_policy: target_policy.clone(),
        };
        if order_saved {
            self.language()
                .localized(copy::configuration::RULE_SOURCE_ALREADY_EXISTS_NO_DUPLICATE_WAS_ADDED)
                .clone_into(&mut self.status);
        }
        record_operation(
            operation_id,
            LogLevel::Warn,
            "configuration.rule_source.add.duplicate",
            format!("existing_id={source_id} rules={rule_count} target={target_policy}"),
        );
    }

    pub(in crate::app) fn persist_routing_rule_group_order(&mut self) -> bool {
        if self.configuration_transfer.active {
            return true;
        }
        self.sync_routing_rule_group_order();
        if let Some(store_dir) = self.subscription_store_dir.as_ref()
            && let Err(error) =
                mihomo::save_routing_rule_group_order_in(store_dir, &self.rule_sources.group_order)
        {
            self.source_store_error = Some(error);
            self.status = format!(
                "{}: {}",
                self.language()
                    .localized(copy::configuration::QX_RULE_SAVE_FAILED),
                copy::configuration::subscription_store_error(self.language(), error),
            );
            record_event(
                LogLevel::Error,
                "configuration.rule_group_order.persistence_failed",
                error.to_string(),
            );
            return false;
        }
        true
    }

    fn finish_failed_qx_rule_import(&mut self, operation_id: u64, error: &ImportQxRuleError) {
        match error {
            ImportQxRuleError::Download(error) => {
                self.status = format!(
                    "{}: {}",
                    self.language()
                        .localized(copy::configuration::QX_RULE_DOWNLOAD_FAILED),
                    copy::configuration::rule_download_error(self.language(), *error)
                );
                self.rule_sources.feedback = QxRuleImportFeedback::DownloadFailed(*error);
                record_operation(
                    operation_id,
                    LogLevel::Error,
                    "configuration.rule_source.add.failed",
                    "phase=download",
                );
            }
            ImportQxRuleError::InvalidDocument => {
                self.rule_sources.feedback = QxRuleImportFeedback::InvalidDocument;
                self.language()
                    .localized(
                        copy::configuration::QX_RULES_NOT_IMPORTED_NO_RECOGNIZABLE_DOMAIN_RULES,
                    )
                    .clone_into(&mut self.status);
                record_operation(
                    operation_id,
                    LogLevel::Error,
                    "configuration.rule_source.add.failed",
                    "phase=parse reason=no_recognizable_domain_rules",
                );
            }
            ImportQxRuleError::Store(error) => {
                self.status = format!(
                    "{}: {}",
                    self.language()
                        .localized(copy::configuration::QX_RULE_SAVE_FAILED),
                    copy::configuration::subscription_store_error(self.language(), *error)
                );
                self.rule_sources.feedback = QxRuleImportFeedback::StoreFailed(*error);
                record_operation(
                    operation_id,
                    LogLevel::Error,
                    "configuration.rule_source.add.failed",
                    "phase=store",
                );
            }
        }
    }
}
