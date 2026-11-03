use std::collections::BTreeMap;

use crate::app::{SourceRefreshSchedulerState, SourceRuntimeApply};
use crate::mihomo::{RemoteSourceRefreshInterval, StoredQxRuleSource, SubscriptionStoreError};
use crate::rule_source::RuleDownloadError;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(in crate::app) enum QxRuleImportFeedback {
    #[default]
    Idle,
    Importing,
    Imported {
        rule_count: usize,
        diagnostic_count: usize,
    },
    AlreadyExists {
        source_id: String,
        rule_count: usize,
        target_policy: String,
    },
    InvalidDocument,
    DownloadFailed(RuleDownloadError),
    StoreFailed(SubscriptionStoreError),
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(in crate::app) enum QxRuleEditorPopover {
    #[default]
    None,
    Target,
    Interval,
}

pub(in crate::app) enum ImportQxRuleError {
    Download(RuleDownloadError),
    InvalidDocument,
    Store(SubscriptionStoreError),
}

pub(in crate::app) enum ImportQxRuleSuccess {
    Imported {
        stored: StoredQxRuleSource,
        apply: SourceRuntimeApply,
    },
    AlreadyExists {
        stored: StoredQxRuleSource,
    },
    RolledBack {
        apply: SourceRuntimeApply,
        rollback_error: Option<SubscriptionStoreError>,
    },
}

pub(in crate::app) type QxRuleImportResult = Result<ImportQxRuleSuccess, ImportQxRuleError>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::app) enum QxRuleSourceRefreshState {
    Refreshing { generation: u64 },
    Failed { generation: u64, message: String },
}

impl QxRuleSourceRefreshState {
    pub(in crate::app) fn is_refreshing(&self) -> bool {
        matches!(self, Self::Refreshing { .. })
    }
}

pub(in crate::app) struct RuleSourceState {
    pub(in crate::app) sources: Vec<StoredQxRuleSource>,
    pub(in crate::app) group_order: Vec<String>,
    pub(in crate::app) feedback: QxRuleImportFeedback,
    pub(in crate::app) target_policy: String,
    pub(in crate::app) editor_source_id: Option<String>,
    pub(in crate::app) editor_refresh_interval: RemoteSourceRefreshInterval,
    pub(in crate::app) editor_popover: QxRuleEditorPopover,
    pub(in crate::app) import_generation: u64,
    pub(in crate::app) refreshes: BTreeMap<String, QxRuleSourceRefreshState>,
    pub(in crate::app) target_updates: BTreeMap<String, u64>,
    pub(in crate::app) target_popover: Option<String>,
    pub(in crate::app) refresh_retry_not_before: BTreeMap<String, u64>,
    pub(in crate::app) refresh_scheduler: SourceRefreshSchedulerState,
}

impl RuleSourceState {
    pub(in crate::app) fn restored(
        sources: Vec<StoredQxRuleSource>,
        group_order: Vec<String>,
        target_policy: String,
    ) -> Self {
        Self {
            sources,
            group_order,
            feedback: QxRuleImportFeedback::Idle,
            target_policy,
            editor_source_id: None,
            editor_refresh_interval: RemoteSourceRefreshInterval::Manual,
            editor_popover: QxRuleEditorPopover::None,
            import_generation: 0,
            refreshes: BTreeMap::new(),
            target_updates: BTreeMap::new(),
            target_popover: None,
            refresh_retry_not_before: BTreeMap::new(),
            refresh_scheduler: SourceRefreshSchedulerState::Stopped,
        }
    }
}
