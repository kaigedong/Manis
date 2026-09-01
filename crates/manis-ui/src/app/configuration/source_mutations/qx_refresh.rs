use super::{
    Context, ImportQxRuleError, ManisApp, QxRuleList, QxRuleSourceRefreshState, SourceMutation,
    SourceRuntimeApply, copy, download_qx_rule_document_secret, mihomo,
};

impl ManisApp {
    pub(in crate::app) fn refresh_qx_rule_source(&mut self, id: String, cx: &mut Context<Self>) {
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
                    let transaction =
                        crate::app::mutate_saved_sources(&runtime, &store_dir, |store_dir| {
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
        result: crate::app::QxRuleRefreshResult,
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
            Ok(SourceMutation::Committed {
                value: stored,
                apply,
            }) => {
                self.finish_successful_qx_rule_refresh(id, stored, &apply);
            }
            Err(error) => self.finish_failed_qx_rule_refresh(id, generation, &error),
            Ok(SourceMutation::RollbackAttempted {
                apply,
                rollback_error,
            }) => {
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
                    apply.status_suffix_after_rollback_attempt(
                        self.language(),
                        rollback_error.as_ref()
                    )
                );
            }
        }
        cx.notify();
    }

    fn finish_successful_qx_rule_refresh(
        &mut self,
        id: &str,
        stored: mihomo::StoredQxRuleSource,
        apply: &SourceRuntimeApply,
    ) {
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
            .remove(&crate::app::DueRemoteSource::QxRule(id.to_owned()).scheduler_key());
        apply.reconcile_proxy_mode(&mut self.proxy_mode);
        self.status = copy::configuration::qx_rules_applied(
            language,
            copy::configuration::QxRuleAction::Updated,
            rule_count,
            &apply.status_suffix(language),
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
