use std::{path::Path, sync::Mutex};

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
}

pub(super) enum SourceMutation<T> {
    Committed {
        value: T,
        apply: SourceRuntimeApply,
    },
    RollbackAttempted {
        apply: SourceRuntimeApply,
        rollback_error: Option<SubscriptionStoreError>,
    },
}

pub(super) fn mutate_saved_sources<T>(
    runtime: &KernelRuntime,
    store_dir: &Path,
    mutation: impl FnOnce(&Path) -> Result<T, SubscriptionStoreError>,
) -> Result<SourceMutation<T>, SubscriptionStoreError> {
    mutate_saved_sources_with_apply(store_dir, mutation, || {
        SourceRuntimeApply::from_result(runtime.apply_saved_sources(store_dir))
    })
}

// This lock spans staging, installation, runtime activation and rollback, not
// just the runtime call. UI preference writes remain independent and are never
// restored by a source transaction.
static SOURCE_TRANSACTION_LOCK: Mutex<()> = Mutex::new(());

pub(super) fn mutate_saved_sources_with_apply<T>(
    store_dir: &Path,
    mutation: impl FnOnce(&Path) -> Result<T, SubscriptionStoreError>,
    apply: impl FnOnce() -> SourceRuntimeApply,
) -> Result<SourceMutation<T>, SubscriptionStoreError> {
    let _guard = SOURCE_TRANSACTION_LOCK
        .lock()
        .map_err(|_| SubscriptionStoreError::StoreUnavailable)?;
    let transaction = mihomo::SourceStoreTransaction::begin(store_dir)?;
    let value = mutation(transaction.directory())?;
    let mut changes = transaction.changes()?;
    if let Err(error) = changes.install(store_dir) {
        return Ok(SourceMutation::RollbackAttempted {
            apply: SourceRuntimeApply::Failed(error.to_string()),
            rollback_error: changes.rollback(store_dir).err(),
        });
    }
    let apply = apply();
    if apply.requires_source_rollback() {
        let rollback_error = changes.rollback(store_dir).err();
        return Ok(SourceMutation::RollbackAttempted {
            apply,
            rollback_error,
        });
    }
    Ok(SourceMutation::Committed { value, apply })
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
            Self::MetadataOnly
            | Self::Applied(GeneratedProfileApply::Updated | GeneratedProfileApply::Restarted)
            | Self::ProxyModeLost(_) => self.status_suffix(language),
        }
    }

    pub(super) fn status_suffix_after_rollback_attempt(
        &self,
        language: Language,
        rollback_error: Option<&SubscriptionStoreError>,
    ) -> String {
        let mut status = if rollback_error.is_some() {
            self.status_suffix(language)
        } else {
            self.status_suffix_after_source_rollback(language)
        };
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

#[cfg(all(test, not(windows)))]
mod source_transaction_tests {
    use super::*;
    use crate::localization::{LanguagePreference, save_language_preference_in};
    use std::{fs, sync::mpsc, time::Duration};

    fn store(name: &str) -> std::path::PathBuf {
        std::env::temp_dir()
            .join(format!(
                "manis-{name}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ))
            .join("subscriptions")
    }

    #[test]
    fn failed_multistep_mutation_leaves_no_partial_subscription() {
        let store = store("partial-mutation");
        let runtime = crate::app::ManisApp::with_fixture_controller("http://127.0.0.1:1").runtime;
        let result: Result<SourceMutation<()>, SubscriptionStoreError> =
            mutate_saved_sources(&runtime, &store, |store| {
                mihomo::save_imported_subscription_in(
                    store,
                    "https://example.invalid/subscription",
                )?;
                Err(SubscriptionStoreError::StoreUnavailable)
            });
        assert!(result.is_err());
        let remaining = mihomo::load_subscription_sources_in(&store).unwrap();
        if let Some(parent) = store.parent() {
            let _ = fs::remove_dir_all(parent);
        }
        assert!(remaining.is_empty(), "failed mutation left a saved source");
    }

    #[test]
    fn failed_apply_preserves_an_independent_language_save() {
        let store = store("concurrent-language");
        save_language_preference_in(&store, LanguagePreference::SimplifiedChinese).unwrap();
        let runtime = crate::app::ManisApp::with_fixture_controller("http://127.0.0.1:1").runtime;
        let (ready_tx, ready_rx) = mpsc::channel();
        let (resume_tx, resume_rx) = mpsc::channel();
        let transaction_store = store.clone();
        let writer = std::thread::spawn(move || {
            mutate_saved_sources(&runtime, &transaction_store, |staged| {
                let source = mihomo::save_imported_subscription_in(
                    staged,
                    "https://example.invalid/subscription",
                )?;
                ready_tx.send(()).unwrap();
                resume_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                Ok(source)
            })
        });
        ready_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        save_language_preference_in(&store, LanguagePreference::English).unwrap();
        resume_tx.send(()).unwrap();
        let result = writer.join().unwrap().unwrap();
        assert!(matches!(result, SourceMutation::RollbackAttempted { .. }));
        let actual = fs::read_to_string(store.join("language.preference")).unwrap();
        fs::remove_dir_all(store.parent().unwrap()).unwrap();
        assert_eq!(
            actual, "en\n",
            "rollback overwrote a successfully saved preference"
        );
    }
    #[test]
    fn losing_tun_after_activation_keeps_the_committed_source() {
        let store = store("committed-after-tun-loss");
        manis_profile::write_private_atomic(&store, "source.state", b"before").unwrap();
        let result = mutate_saved_sources_with_apply(
            &store,
            |staged| {
                manis_profile::write_private_atomic(staged, "source.state", b"candidate")
                    .map(|_| "saved source")
                    .map_err(|_| SubscriptionStoreError::StoreUnavailable)
            },
            || SourceRuntimeApply::ProxyModeLost("injected TUN restore failure".to_owned()),
        )
        .unwrap();

        let SourceMutation::Committed { value, apply } = result else {
            panic!("TUN loss after activation must not roll back the source");
        };
        assert_eq!(value, "saved source");
        let mut mode = ProxyMode::Tun;
        assert!(apply.reconcile_proxy_mode(&mut mode));
        assert_eq!(mode, ProxyMode::Off);
        assert_eq!(fs::read(store.join("source.state")).unwrap(), b"candidate");
        fs::remove_dir_all(store.parent().unwrap()).unwrap();
    }

    #[test]
    fn failed_apply_reports_a_rollback_conflict_without_overwriting_newer_data() {
        let store = store("apply-rollback-conflict");
        manis_profile::write_private_atomic(&store, "source.state", b"before").unwrap();
        let result = mutate_saved_sources_with_apply(
            &store,
            |staged| {
                manis_profile::write_private_atomic(staged, "source.state", b"candidate")
                    .map(|_| ())
                    .map_err(|_| SubscriptionStoreError::StoreUnavailable)
            },
            || {
                manis_profile::write_private_atomic(&store, "source.state", b"later save").unwrap();
                SourceRuntimeApply::Failed("injected apply failure".to_owned())
            },
        )
        .unwrap();
        let SourceMutation::RollbackAttempted {
            apply,
            rollback_error,
        } = result
        else {
            panic!("a failed apply must attempt rollback");
        };
        assert!(rollback_error.is_some());
        let status =
            apply.status_suffix_after_rollback_attempt(Language::English, rollback_error.as_ref());
        assert!(status.contains(Language::English.message(Message::StoreRollbackFailed)));
        assert!(!status.contains(Language::English.message(Message::ChangesFailedAndRestored)));
        assert_eq!(fs::read(store.join("source.state")).unwrap(), b"later save");
        fs::remove_dir_all(store.parent().unwrap()).unwrap();
    }
}
