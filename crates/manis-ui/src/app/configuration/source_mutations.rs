use gpui::{Context, Entity};
use manis_profile::SecretUrl;

use crate::app::{
    ImportedSubscriptionState, ManisApp, QxRuleList, SourceMutation, SourceRuntimeApply,
};
use crate::{
    diagnostics::{LogLevel, begin_operation, record_event, record_operation},
    localization::copy,
    mihomo::{self, SubscriptionStoreError},
    rule_source::download_qx_rule_document_secret,
    subscription_input::SubscriptionTextInput,
};

use super::{QxRuleSaveRequest, SubscriptionToggleCompletion, save_qx_rule_source};

mod model;
mod qx_import;
mod qx_refresh;
mod qx_remove;
mod qx_target;
mod subscription;
pub(in crate::app) use model::{
    ImportQxRuleError, ImportQxRuleSuccess, QxRuleEditorPopover, QxRuleImportFeedback,
    QxRuleImportResult, QxRuleSourceRefreshState, RuleSourceState,
};
