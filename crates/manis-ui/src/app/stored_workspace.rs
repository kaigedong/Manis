use std::path::PathBuf;

use manis_core::{ManagedPolicyGroup, RoutingMode};

use super::{
    ImportedSubscription, StoredQxRuleSource, StoredSingleNode, SubscriptionStoreError, mihomo,
};

pub(super) struct StoredWorkspace {
    pub(super) imported_subscriptions: Vec<ImportedSubscription>,
    pub(super) saved_single_nodes: Vec<StoredSingleNode>,
    pub(super) qx_rule_sources: Vec<StoredQxRuleSource>,
    pub(super) routing_rule_group_order: Vec<String>,
    pub(super) collapsed_groups: Vec<String>,
    pub(super) managed_policy_groups: Vec<ManagedPolicyGroup>,
    pub(super) node_selection_preferences: mihomo::NodeSelectionPreferences,
    pub(super) routing_mode: RoutingMode,
    pub(super) error: Option<SubscriptionStoreError>,
}

impl StoredWorkspace {
    pub(super) fn load(directory: Option<&PathBuf>) -> Self {
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
            routing_mode: RoutingMode::Rule,
            error: None,
        }
    }
}
