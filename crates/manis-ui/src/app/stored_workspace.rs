use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use manis_core::{ManagedPolicyGroup, RoutingMode};
use manis_profile::write_private_atomic;
use serde::{Deserialize, Serialize};

use crate::diagnostics::{LogLevel, record_event};

use super::{
    GeneratedProfileApply, GroupBenchmarkState, ImportedSubscription, KernelRuntime, Language,
    ManisApp, StoredQxRuleSource, StoredSingleNode, SubscriptionStoreError, copy, mihomo,
};

const BENCHMARK_STATE_FILE: &str = "benchmarks.state";
const BENCHMARK_STATE_VERSION: u8 = 1;
const MAX_BENCHMARK_STATE_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Deserialize, Serialize)]
struct StoredBenchmarks {
    version: u8,
    benchmarks: BTreeMap<String, GroupBenchmarkState>,
}

pub(super) struct StoredWorkspace {
    pub(super) imported_subscriptions: Vec<ImportedSubscription>,
    pub(super) saved_single_nodes: Vec<StoredSingleNode>,
    pub(super) qx_rule_sources: Vec<StoredQxRuleSource>,
    pub(super) routing_rule_group_order: Vec<String>,
    pub(super) collapsed_groups: Vec<String>,
    pub(super) managed_policy_groups: Vec<ManagedPolicyGroup>,
    pub(super) node_selection_preferences: mihomo::NodeSelectionPreferences,
    pub(super) benchmarks: BTreeMap<String, GroupBenchmarkState>,
    pub(super) routing_mode: RoutingMode,
    pub(super) error: Option<SubscriptionStoreError>,
}

impl StoredWorkspace {
    pub(super) fn load(directory: Option<&Path>) -> Self {
        let Some(directory) = directory else {
            return Self::empty();
        };
        let subscriptions = mihomo::load_subscription_sources_in(directory);
        let nodes = mihomo::load_single_node_sources_in(directory);
        let qx_rule_sources = mihomo::load_qx_rule_sources_in(directory);
        let routing_rule_group_order = mihomo::load_routing_rule_group_order_in(directory);
        let collapsed = mihomo::load_collapsed_groups_in(directory);
        let policy_groups = mihomo::load_managed_policy_groups_in(directory);
        let node_selection_preferences = mihomo::load_node_selection_preferences_in(directory);
        let benchmarks = load_group_benchmarks_in(directory);
        if let Err(error) = &benchmarks {
            record_event(
                LogLevel::Warn,
                "group_benchmark.restore_failed",
                error.to_string(),
            );
        }
        let routing_mode = mihomo::load_routing_mode_in(directory);
        let error = [
            subscriptions.is_err(),
            nodes.is_err(),
            qx_rule_sources.is_err(),
            routing_rule_group_order.is_err(),
            collapsed.is_err(),
            policy_groups.is_err(),
            node_selection_preferences.is_err(),
            routing_mode.is_err(),
        ]
        .into_iter()
        .any(std::convert::identity)
        .then_some(SubscriptionStoreError::StoredSourceUnavailable);
        Self {
            imported_subscriptions: subscriptions
                .unwrap_or_default()
                .into_iter()
                .map(ImportedSubscription::from_stored)
                .collect(),
            saved_single_nodes: nodes.unwrap_or_default(),
            qx_rule_sources: qx_rule_sources.unwrap_or_default(),
            routing_rule_group_order: routing_rule_group_order.unwrap_or_default(),
            collapsed_groups: collapsed.unwrap_or_default(),
            managed_policy_groups: policy_groups.unwrap_or_default(),
            node_selection_preferences: node_selection_preferences.unwrap_or_default(),
            benchmarks: benchmarks.unwrap_or_default(),
            routing_mode: routing_mode.unwrap_or_default(),
            error,
        }
    }

    fn empty() -> Self {
        Self {
            imported_subscriptions: Vec::new(),
            saved_single_nodes: Vec::new(),
            qx_rule_sources: Vec::new(),
            routing_rule_group_order: Vec::new(),
            collapsed_groups: Vec::new(),
            managed_policy_groups: Vec::new(),
            node_selection_preferences: mihomo::NodeSelectionPreferences::default(),
            benchmarks: BTreeMap::new(),
            routing_mode: RoutingMode::Rule,
            error: None,
        }
    }
}

pub(super) fn load_group_benchmarks_in(
    directory: &Path,
) -> Result<BTreeMap<String, GroupBenchmarkState>, SubscriptionStoreError> {
    let path = directory.join(BENCHMARK_STATE_FILE);
    if !path.exists() {
        return Ok(BTreeMap::new());
    }
    let metadata =
        fs::metadata(&path).map_err(|_error| SubscriptionStoreError::StoreUnavailable)?;
    if metadata.len() > MAX_BENCHMARK_STATE_BYTES {
        return Err(SubscriptionStoreError::StoredSourceUnavailable);
    }
    let contents = fs::read_to_string(&path)
        .map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)?;
    let mut stored: StoredBenchmarks = serde_json::from_str(&contents)
        .map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)?;
    if stored.version != BENCHMARK_STATE_VERSION {
        return Err(SubscriptionStoreError::StoredSourceUnavailable);
    }
    stored
        .benchmarks
        .retain(|_, state| state.complete_delays().is_some());
    Ok(stored.benchmarks)
}

pub(super) fn save_group_benchmarks_in(
    directory: &Path,
    benchmarks: &BTreeMap<String, GroupBenchmarkState>,
) -> Result<(), SubscriptionStoreError> {
    let stored = StoredBenchmarks {
        version: BENCHMARK_STATE_VERSION,
        benchmarks: benchmarks
            .iter()
            .filter(|(_, state)| state.complete_delays().is_some())
            .map(|(key, state)| (key.clone(), state.clone()))
            .collect(),
    };
    let contents = serde_json::to_vec(&stored)
        .map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)?;
    write_private_atomic(directory, BENCHMARK_STATE_FILE, &contents)
        .map(|_| ())
        .map_err(|_error| SubscriptionStoreError::StoreUnavailable)
}

impl ManisApp {
    pub(in crate::app) fn restored_workspace_status(
        runtime: &KernelRuntime,
        directory: Option<&Path>,
        workspace: &StoredWorkspace,
        language: Language,
    ) -> String {
        let Some(directory) = directory else {
            return runtime.initial_status_in(language);
        };
        let has_saved_configuration = !workspace.imported_subscriptions.is_empty()
            || !workspace.saved_single_nodes.is_empty()
            || !workspace.qx_rule_sources.is_empty()
            || !workspace.managed_policy_groups.is_empty()
            || workspace.routing_mode != RoutingMode::Rule;
        if !has_saved_configuration {
            return runtime.initial_status_in(language);
        }
        match runtime.apply_saved_sources(directory) {
            Ok(GeneratedProfileApply::Updated) => language
                .localized(copy::app::SAVED_SOURCES_ARE_READY)
                .to_owned(),
            Ok(GeneratedProfileApply::Restarted) => language
                .localized(copy::app::SAVED_SOURCES_ARE_READY_AND_MIHOMO_WAS_RESTARTED)
                .to_owned(),
            Err(error) => format!(
                "{}{error}",
                language
                    .localized(copy::app::SAVED_SOURCES_WERE_LOADED_BUT_THE_CHANGES_COULD_NOT_BE)
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::fs;

    use crate::app::{GroupBenchmarkState, GroupBenchmarkSummary};

    fn temp_store(name: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("{name}-{}", std::process::id()));
        if root.exists() {
            fs::remove_dir_all(&root).expect("remove stale fixture");
        }
        root.join("subscriptions")
    }

    #[test]
    fn group_benchmark_state_round_trips_only_completed_results() {
        let store = temp_store("manis-group-benchmarks");
        let complete = GroupBenchmarkState::Complete {
            generation: 7,
            summary: GroupBenchmarkSummary {
                total: 2,
                succeeded: 1,
                failed: 1,
                minimum_ms: Some(42),
                maximum_ms: Some(42),
                average_ms: Some(42),
            },
            delays: BTreeMap::from([("HK".to_owned(), 42)]),
        };
        let benchmarks = BTreeMap::from([
            ("source:alpha".to_owned(), complete.clone()),
            ("policy:beta".to_owned(), GroupBenchmarkState::running(8)),
            (
                "policy:gamma".to_owned(),
                GroupBenchmarkState::Failed {
                    generation: 9,
                    message: None,
                },
            ),
        ]);

        super::save_group_benchmarks_in(&store, &benchmarks).expect("save benchmark state");
        let restored = super::load_group_benchmarks_in(&store).expect("load benchmark state");

        assert_eq!(
            restored,
            BTreeMap::from([("source:alpha".to_owned(), complete)])
        );

        fs::remove_dir_all(store.parent().expect("fixture root")).expect("remove fixture");
    }

    #[test]
    fn corrupt_group_benchmark_cache_does_not_mark_workspace_sources_broken() {
        let store = temp_store("manis-corrupt-group-benchmarks");
        fs::create_dir_all(&store).expect("create fixture store");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            fs::set_permissions(&store, fs::Permissions::from_mode(0o700))
                .expect("make fixture store private");
        }
        let path = store.join(super::BENCHMARK_STATE_FILE);
        fs::write(&path, "not json").expect("write corrupt cache");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            fs::set_permissions(&path, fs::Permissions::from_mode(0o600))
                .expect("make fixture cache private");
        }

        let workspace = super::StoredWorkspace::load(Some(&store));

        assert!(workspace.benchmarks.is_empty());
        assert_eq!(workspace.error, None);

        fs::remove_dir_all(store.parent().expect("fixture root")).expect("remove fixture");
    }
}
