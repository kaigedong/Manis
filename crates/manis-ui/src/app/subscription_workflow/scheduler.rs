use super::{
    BTreeMap, Context, Duration, ImportedSubscription, ImportedSubscriptionState, ManisApp,
    QxRuleImportFeedback, QxRuleSourceRefreshState, SecretUrl, SourceKind, StoredQxRuleSource,
    mihomo,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::app) enum DueRemoteSource {
    Subscription(String),
    QxRule(String),
}

impl DueRemoteSource {
    pub(in crate::app) fn scheduler_key(&self) -> String {
        match self {
            Self::Subscription(id) => format!("subscription:{id}"),
            Self::QxRule(id) => format!("qx-rule:{id}"),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(in crate::app) enum SourceRefreshSchedulerState {
    #[default]
    Stopped,
    Started,
}

pub(in crate::app) fn next_due_remote_source(
    subscriptions: &[ImportedSubscription],
    qx_rule_sources: &[StoredQxRuleSource],
    retry_not_before: &BTreeMap<String, u64>,
    now_unix_secs: u64,
) -> Option<DueRemoteSource> {
    subscriptions
        .iter()
        .find(|source| {
            source.enabled
                && !matches!(
                    source.state,
                    ImportedSubscriptionState::None
                        | ImportedSubscriptionState::Pending(_)
                        | ImportedSubscriptionState::Refreshing(_)
                        | ImportedSubscriptionState::Removing(_)
                )
                && source
                    .refresh_interval
                    .is_due(source.last_successful_update_unix_secs, now_unix_secs)
                && retry_not_before
                    .get(&DueRemoteSource::Subscription(source.id.clone()).scheduler_key())
                    .is_none_or(|retry_at| now_unix_secs >= *retry_at)
        })
        .map(|source| DueRemoteSource::Subscription(source.id.clone()))
        .or_else(|| {
            qx_rule_sources
                .iter()
                .find(|source| {
                    source.enabled
                        && source
                            .refresh_interval
                            .is_due(source.last_successful_update_unix_secs, now_unix_secs)
                        && retry_not_before
                            .get(&DueRemoteSource::QxRule(source.id.clone()).scheduler_key())
                            .is_none_or(|retry_at| now_unix_secs >= *retry_at)
                })
                .map(|source| DueRemoteSource::QxRule(source.id.clone()))
        })
}

pub(in crate::app) fn source_kind(subscription: &SecretUrl) -> SourceKind {
    if subscription.is_https() {
        SourceKind::HttpsSubscription
    } else {
        SourceKind::HttpSubscription
    }
}

impl ManisApp {
    pub(in crate::app) fn source_refresh_busy(&self) -> bool {
        self.proxy_source_editor.is_importing()
            || self.imported_subscriptions.iter().any(|source| {
                matches!(
                    source.state,
                    ImportedSubscriptionState::Refreshing(_)
                        | ImportedSubscriptionState::Removing(_)
                )
            })
            || self.rule_sources.feedback == QxRuleImportFeedback::Importing
            || self
                .rule_sources
                .refreshes
                .values()
                .any(QxRuleSourceRefreshState::is_refreshing)
            || !self.rule_sources.target_updates.is_empty()
    }

    pub(in crate::app) fn refresh_next_due_source(&mut self, cx: &mut Context<Self>) {
        if self.configuration_transfer.active || self.source_refresh_busy() {
            return;
        }
        let now = mihomo::current_unix_secs();
        let due = next_due_remote_source(
            &self.imported_subscriptions,
            &self.rule_sources.sources,
            &self.rule_sources.refresh_retry_not_before,
            now,
        );
        if let Some(source) = due.as_ref() {
            self.rule_sources
                .refresh_retry_not_before
                .insert(source.scheduler_key(), now.saturating_add(300));
        }
        match due {
            Some(DueRemoteSource::Subscription(id)) => {
                self.refresh_imported_subscription(id, cx);
            }
            Some(DueRemoteSource::QxRule(id)) => self.refresh_qx_rule_source(id, cx),
            None => {}
        }
    }

    pub(in crate::app) fn ensure_source_refresh_scheduler(&mut self, cx: &mut Context<Self>) {
        if self.rule_sources.refresh_scheduler == SourceRefreshSchedulerState::Started
            || self.subscription_store_dir.is_none()
        {
            return;
        }
        self.rule_sources.refresh_scheduler = SourceRefreshSchedulerState::Started;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_secs(60))
                    .await;
                let Some(this) = this.upgrade() else {
                    break;
                };
                this.update(cx, ManisApp::refresh_next_due_source);
            }
        })
        .detach();
    }
}
