impl ManisApp {
    fn submit_qx_rule_import(
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
        result: super::QxRuleImportResult,
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
        self.persist_routing_rule_group_order();
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
        self.status = copy::configuration::qx_rules_applied(
            language,
            copy::configuration::QxRuleAction::Imported,
            rule_count,
            &apply.status_suffix(language),
        );
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
        self.persist_routing_rule_group_order();
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
    }

    fn persist_routing_rule_group_order(&mut self) {
        if self.configuration_transfer.active {
            return;
        }
        self.sync_routing_rule_group_order();
        if let Some(store_dir) = self.subscription_store_dir.as_ref() {
            let _ =
                mihomo::save_routing_rule_group_order_in(store_dir, &self.rule_sources.group_order);
        }
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

    fn remove_qx_rule_source(&mut self, id: String, cx: &mut Context<Self>) {
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
                    super::mutate_saved_sources(&runtime, &store_dir, |store_dir| {
                        mihomo::remove_qx_rule_source_in(store_dir, &id).map(|()| id.clone())
                    })
                })
                .await;
            this.update(cx, |this, cx| {
                if this.rule_sources.import_generation != generation {
                    return;
                }
                match result {
                    Ok(transaction) if transaction.value.is_some() => {
                        let id = transaction.value.expect("checked committed mutation");
                        this.rule_sources.sources.retain(|source| source.id != id);
                        this.sync_routing_rule_group_order();
                        if let Some(store_dir) = this.subscription_store_dir.as_ref() {
                            let _ = mihomo::save_routing_rule_group_order_in(
                                store_dir,
                                &this.rule_sources.group_order,
                            );
                        }
                        this.rule_sources.refreshes.remove(&id);
                        this.rule_sources
                            .refresh_retry_not_before
                            .remove(&super::DueRemoteSource::QxRule(id.clone()).scheduler_key());
                        this.rule_sources.feedback = QxRuleImportFeedback::Idle;
                        let language = this.language();
                        transaction.apply.reconcile_proxy_mode(&mut this.proxy_mode);
                        this.status = copy::configuration::qx_rules_removed(
                            language,
                            &transaction.apply.status_suffix(language),
                        );
                    }
                    Ok(transaction) => {
                        this.rule_sources.feedback = QxRuleImportFeedback::StoreFailed(
                            SubscriptionStoreError::StoreUnavailable,
                        );
                        this.status = format!(
                            "{}{}",
                            this.language()
                                .localized(copy::configuration::REMOTE_QX_RULE_REMOVAL_FAILED),
                            transaction
                                .apply
                                .status_suffix_after_rollback_attempt(this.language(), transaction.rollback_error.as_ref())
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

    fn set_subscription_enabled(&mut self, id: &str, enabled: bool, cx: &mut Context<Self>) {
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
        let kind = super::source_kind(&source.source);
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
                    super::mutate_saved_sources(&runtime, &store_dir, |store_dir| {
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
        result: Result<super::SourceMutation<mihomo::StoredSubscription>, SubscriptionStoreError>,
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
            Ok(transaction) if transaction.value.is_some() => {
                let stored = transaction.value.expect("checked committed mutation");
                source.enabled = stored.enabled;
                source.state = if stored.enabled {
                    ImportedSubscriptionState::Pending(completion.kind)
                } else {
                    ImportedSubscriptionState::None
                };
                transaction.apply.reconcile_proxy_mode(&mut self.proxy_mode);
                self.status = format!(
                    "{}{}",
                    if stored.enabled {
                        language.localized(copy::configuration::SUBSCRIPTION_ENABLED)
                    } else {
                        language.localized(copy::configuration::SUBSCRIPTION_DISABLED)
                    },
                    transaction.apply.status_suffix(language)
                );
                stored.enabled
            }
            Ok(transaction) => {
                source.enabled = completion.previous_enabled;
                source.state = completion.previous_state;
                self.status = format!(
                    "{}{}",
                    language.localized(copy::configuration::FAILED_TO_CHANGE_SUBSCRIPTION_STATE),
                    transaction
                        .apply
                        .status_suffix_after_rollback_attempt(language, transaction.rollback_error.as_ref())
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

    fn set_qx_rule_source_enabled(&mut self, id: String, enabled: bool, cx: &mut Context<Self>) {
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
                    super::mutate_saved_sources(&runtime, &store_dir, |store_dir| {
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
                    Ok(transaction) if transaction.value.is_some() => {
                        let stored = transaction.value.expect("checked committed mutation");
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
                        transaction.apply.reconcile_proxy_mode(&mut this.proxy_mode);
                        this.status = format!(
                            "{}{}",
                            if enabled {
                                language.localized(copy::configuration::RULE_SOURCE_ENABLED)
                            } else {
                                language.localized(copy::configuration::RULE_SOURCE_DISABLED)
                            },
                            transaction.apply.status_suffix(language)
                        );
                    }
                    Ok(transaction) => {
                        this.status = format!(
                            "{}{}",
                            this.language()
                                .localized(copy::configuration::FAILED_TO_CHANGE_RULE_SOURCE_STATE),
                            transaction
                                .apply
                                .status_suffix_after_rollback_attempt(this.language(), transaction.rollback_error.as_ref())
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

    fn update_qx_rule_source_target(&mut self, id: String, target: String, cx: &mut Context<Self>) {
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
                    super::mutate_saved_sources(&runtime, &store_dir, |store_dir| {
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
        result: Result<super::SourceMutation<mihomo::StoredQxRuleSource>, SubscriptionStoreError>,
        cx: &mut Context<Self>,
    ) {
        if self.rule_sources.target_updates.get(id) != Some(&generation) {
            return;
        }
        self.rule_sources.target_updates.remove(id);
        match result {
            Ok(transaction) if transaction.value.is_some() => {
                self.finish_successful_qx_rule_target_update(id, transaction);
            }
            Ok(transaction) => {
                self.status = format!(
                    "{}{}",
                    self.language()
                        .localized(copy::configuration::FAILED_TO_SAVE_RULE_SOURCE_POLICY),
                    transaction
                        .apply
                        .status_suffix_after_rollback_attempt(self.language(), transaction.rollback_error.as_ref())
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
        mut transaction: super::SourceMutation<mihomo::StoredQxRuleSource>,
    ) {
        let stored = transaction
            .value
            .take()
            .expect("checked committed mutation");
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
        transaction.apply.reconcile_proxy_mode(&mut self.proxy_mode);
        self.status = format!(
            "{} {target}{}",
            language.localized(copy::configuration::RULE_SOURCE_POLICY_SET_TO),
            transaction.apply.status_suffix(language)
        );
        record_event(
            LogLevel::Info,
            "routing.rule_source.target.updated",
            format!("source_id={id} target={target}"),
        );
    }

    pub(super) fn refresh_qx_rule_source(&mut self, id: String, cx: &mut Context<Self>) {
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
            .rule_sources
            .sources
            .iter()
            .find(|source| source.id == id)
        else {
            return;
        };
        let url = source.source.clone();
        self.rule_sources.import_generation = self.rule_sources.import_generation.wrapping_add(1);
        let generation = self.rule_sources.import_generation;
        self.rule_sources.refreshes.insert(
            id.clone(),
            QxRuleSourceRefreshState::Refreshing { generation },
        );
        self.language()
            .localized(copy::configuration::UPDATING_REMOTE_QX_RULES)
            .clone_into(&mut self.status);
        let runtime = self.runtime.clone();
        let executor = cx.background_executor().clone();
        let task_id = id.clone();
        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn(async move {
                    let content = download_qx_rule_document_secret(&url)
                        .map_err(ImportQxRuleError::Download)?;
                    let parsed = QxRuleList::parse(&content);
                    if parsed.rules.is_empty() {
                        return Err(ImportQxRuleError::InvalidDocument);
                    }
                    let transaction = super::mutate_saved_sources(&runtime, &store_dir, |store_dir| {
                        mihomo::replace_qx_rule_source_content_in(
                            store_dir,
                            &task_id,
                            &content,
                            mihomo::current_unix_secs(),
                        )
                    })
                    .map_err(ImportQxRuleError::Store)?;
                    Ok::<_, ImportQxRuleError>(transaction)
                })
                .await;
            this.update(cx, |this, cx| {
                this.finish_qx_rule_source_refresh(&id, generation, result, cx);
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn finish_qx_rule_source_refresh(
        &mut self,
        id: &str,
        generation: u64,
        result: super::QxRuleRefreshResult,
        cx: &mut Context<Self>,
    ) {
        if !matches!(
            self.rule_sources.refreshes.get(id),
            Some(QxRuleSourceRefreshState::Refreshing { generation: active })
                if *active == generation
        ) {
            return;
        }
        match result {
            Ok(transaction) if transaction.value.is_some() => {
                self.finish_successful_qx_rule_refresh(id, transaction);
            }
            Err(error) => self.finish_failed_qx_rule_refresh(id, generation, &error),
            Ok(transaction) => {
                let message = "runtime apply failed".to_owned();
                self.rule_sources.refreshes.insert(
                    id.to_owned(),
                    QxRuleSourceRefreshState::Failed {
                        generation,
                        message,
                    },
                );
                self.status = format!(
                    "{}{}",
                    self.language()
                        .localized(copy::configuration::REMOTE_QX_RULE_UPDATE_FAILED),
                    transaction
                        .apply
                        .status_suffix_after_rollback_attempt(self.language(), transaction.rollback_error.as_ref())
                );
            }
        }
        cx.notify();
    }

    fn finish_successful_qx_rule_refresh(
        &mut self,
        id: &str,
        mut transaction: super::SourceMutation<mihomo::StoredQxRuleSource>,
    ) {
        let stored = transaction
            .value
            .take()
            .expect("checked committed mutation");
        let language = self.language();
        let rule_count = stored.rule_count;
        if let Some(source) = self
            .rule_sources
            .sources
            .iter_mut()
            .find(|source| source.id == id)
        {
            *source = stored;
        }
        self.rule_sources.refreshes.remove(id);
        self.rule_sources
            .refresh_retry_not_before
            .remove(&super::DueRemoteSource::QxRule(id.to_owned()).scheduler_key());
        transaction.apply.reconcile_proxy_mode(&mut self.proxy_mode);
        self.status = copy::configuration::qx_rules_applied(
            language,
            copy::configuration::QxRuleAction::Updated,
            rule_count,
            &transaction.apply.status_suffix(language),
        );
    }

    fn finish_failed_qx_rule_refresh(
        &mut self,
        id: &str,
        generation: u64,
        error: &ImportQxRuleError,
    ) {
        let language = self.language();
        let message = match error {
            ImportQxRuleError::Download(error) => {
                copy::configuration::rule_download_error(language, *error).to_owned()
            }
            ImportQxRuleError::InvalidDocument => self
                .language()
                .localized(copy::configuration::NO_RECOGNIZABLE_DOMAIN_RULES)
                .to_owned(),
            ImportQxRuleError::Store(error) => {
                copy::configuration::subscription_store_error(language, *error).to_owned()
            }
        };
        self.rule_sources.refreshes.insert(
            id.to_owned(),
            QxRuleSourceRefreshState::Failed {
                generation,
                message: message.clone(),
            },
        );
        self.status = format!(
            "{}: {message}",
            language.localized(copy::configuration::QX_RULE_UPDATE_FAILED)
        );
    }
}
