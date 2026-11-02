//! Saved desktop workspace read by the NSH bridge and the headless engine.

use std::path::Path;

use manis_core::{ManagedPolicyGroup, RoutingMode};

use crate::mihomo::{self, StoredSingleNode, StoredSubscription};

/// The parts of the saved desktop configuration the NSH shell renders.
pub(crate) struct StoredWorkspace {
    pub(crate) imported_subscriptions: Vec<StoredSubscription>,
    pub(crate) saved_single_nodes: Vec<StoredSingleNode>,
    pub(crate) managed_policy_groups: Vec<ManagedPolicyGroup>,
    pub(crate) routing_mode: RoutingMode,
}

impl StoredWorkspace {
    pub(crate) fn load(directory: Option<&Path>) -> Self {
        let Some(directory) = directory else {
            return Self::empty();
        };
        Self {
            imported_subscriptions: mihomo::load_subscription_sources_in(directory)
                .unwrap_or_default(),
            saved_single_nodes: mihomo::load_single_node_sources_in(directory).unwrap_or_default(),
            managed_policy_groups: mihomo::load_managed_policy_groups_in(directory)
                .unwrap_or_default(),
            routing_mode: mihomo::load_routing_mode_in(directory).unwrap_or_default(),
        }
    }

    fn empty() -> Self {
        Self {
            imported_subscriptions: Vec::new(),
            saved_single_nodes: Vec::new(),
            managed_policy_groups: Vec::new(),
            routing_mode: RoutingMode::Rule,
        }
    }
}
