use std::path::Path;

use super::{
    GeneratedProfileApply, KernelRuntime, Language, LogLevel, Message, ProxyMode,
    SubscriptionStoreError, copy, mihomo, record_event,
};

pub(super) enum SourceRuntimeApply {
    MetadataOnly,
    Applied(GeneratedProfileApply),
    Failed(String),
    ProxyModeLost(String),
}

#[derive(Clone)]
pub(super) struct RoutingApplyRollback {
    pub(super) manual_rules: Vec<crate::manual_rule::ManualRule>,
    pub(super) group_order: Vec<String>,
    pub(super) store_snapshot: mihomo::SubscriptionStoreSnapshot,
}

pub(super) struct SourceMutation<T> {
    pub(super) value: Option<T>,
    pub(super) apply: SourceRuntimeApply,
    pub(super) rollback_error: Option<SubscriptionStoreError>,
}

pub(super) fn mutate_saved_sources<T>(
    runtime: &KernelRuntime,
    store_dir: &Path,
    mutation: impl FnOnce() -> Result<T, SubscriptionStoreError>,
) -> Result<SourceMutation<T>, SubscriptionStoreError> {
    let snapshot = mihomo::SubscriptionStoreSnapshot::capture(store_dir)?;
    let value = mutation()?;
    let apply = SourceRuntimeApply::from_result(runtime.apply_saved_sources(store_dir));
    if apply.requires_source_rollback() {
        let rollback_error = snapshot.restore(store_dir).err();
        return Ok(SourceMutation {
            value: None,
            apply,
            rollback_error,
        });
    }
    Ok(SourceMutation {
        value: Some(value),
        apply,
        rollback_error: None,
    })
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) enum RoutingApplyState {
    #[default]
    Idle,
    Applying,
}

impl RoutingApplyState {
    pub(super) const fn is_busy(self) -> bool {
        matches!(self, Self::Applying)
    }

    pub(super) fn begin(&mut self) -> bool {
        if self.is_busy() {
            return false;
        }
        *self = Self::Applying;
        true
    }

    pub(super) fn finish(&mut self) {
        *self = Self::Idle;
    }
}

impl SourceRuntimeApply {
    pub(super) const fn requires_source_rollback(&self) -> bool {
        matches!(self, Self::Failed(_))
    }

    pub(super) fn status_suffix_after_source_rollback(&self, language: Language) -> String {
        match self {
            Self::Failed(message) => format!(
                "{}{message}",
                language.message(Message::ChangesFailedAndRestored)
            ),
            _ => self.status_suffix(language),
        }
    }

    pub(super) fn status_suffix_after_rollback_attempt(
        &self,
        language: Language,
        rollback_error: Option<&SubscriptionStoreError>,
    ) -> String {
        let mut status = self.status_suffix_after_source_rollback(language);
        if let Some(error) = rollback_error {
            status.push_str(language.message(Message::StoreRollbackFailed));
            status.push_str(copy::configuration::subscription_store_error(
                language, *error,
            ));
        }
        status
    }

    pub(super) fn from_result(result: Result<GeneratedProfileApply, mihomo::LoadError>) -> Self {
        match result {
            Ok(outcome) => Self::Applied(outcome),
            Err(mihomo::LoadError::ProxyModeLost(message)) => Self::ProxyModeLost(message),
            Err(error) => Self::Failed(error.to_string()),
        }
    }

    pub(super) fn reconcile_proxy_mode(&self, mode: &mut ProxyMode) -> bool {
        if matches!(self, Self::ProxyModeLost(_)) && *mode == ProxyMode::Tun {
            *mode = ProxyMode::Off;
            record_event(
                LogLevel::Error,
                "proxy.mode.restore.ui_fallback",
                "active=off reason=tun_restore_failed",
            );
            return true;
        }
        false
    }

    pub(super) fn status_suffix(&self, language: Language) -> String {
        match self {
            Self::MetadataOnly | Self::Applied(GeneratedProfileApply::Updated) => {
                language.message(Message::ChangesApplied).to_owned()
            }
            Self::Applied(GeneratedProfileApply::Restarted) => language
                .message(Message::ChangesAppliedAndRestarted)
                .to_owned(),
            Self::Failed(message) => {
                format!("{}{message}", language.message(Message::SavedChangesFailed))
            }
            Self::ProxyModeLost(message) => {
                format!("{}{message}", language.message(Message::TunRestoreFailed))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use manis_core::ProxyMode;

    use super::{GeneratedProfileApply, RoutingApplyState, SourceRuntimeApply, mihomo};

    #[test]
    fn apply_state_rejects_overlapping_operations() {
        let mut state = RoutingApplyState::default();

        assert!(state.begin());
        assert!(state.is_busy());
        assert!(!state.begin());

        state.finish();
        assert!(!state.is_busy());
        assert!(state.begin());
    }

    #[test]
    fn only_pre_activation_failures_roll_back_saved_sources() {
        let failed = SourceRuntimeApply::from_result(Err(mihomo::LoadError::Runtime(
            "invalid candidate".to_owned(),
        )));
        let applied = SourceRuntimeApply::from_result(Ok(GeneratedProfileApply::Updated));
        let tun_lost = SourceRuntimeApply::from_result(Err(mihomo::LoadError::ProxyModeLost(
            "restore failed".to_owned(),
        )));

        assert!(failed.requires_source_rollback());
        assert!(!applied.requires_source_rollback());
        assert!(!tun_lost.requires_source_rollback());
    }

    #[test]
    fn losing_tun_reconciles_the_visible_proxy_mode() {
        let apply = SourceRuntimeApply::from_result(Err(mihomo::LoadError::ProxyModeLost(
            "restore failed".to_owned(),
        )));
        let mut mode = ProxyMode::Tun;

        assert!(apply.reconcile_proxy_mode(&mut mode));
        assert_eq!(mode, ProxyMode::Off);
    }
}
