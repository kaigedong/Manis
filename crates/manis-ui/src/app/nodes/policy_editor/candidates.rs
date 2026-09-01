use super::{
    BTreeSet, ManagedPolicyGroup, ManagedPolicyStrategy, ManisApp, NodeIdentity,
    PolicyCandidateKind, PolicyCandidateMatcher, PolicyNode, ProxyId, copy,
};

impl ManisApp {
    pub(in crate::app) fn node_inventory(&self) -> Vec<NodeIdentity> {
        let has_local_sources =
            !self.imported_subscriptions.is_empty() || !self.saved_single_nodes.is_empty();
        let mut inventory = Vec::new();
        let mut seen = BTreeSet::new();
        for group in self.node_source_groups(has_local_sources, self.language()) {
            for provider in group.providers {
                for node in &provider.nodes {
                    if let Ok(identity) = NodeIdentity::new(&group.id, &node.name)
                        && seen.insert(identity.clone())
                    {
                        inventory.push(identity);
                    }
                }
            }
            for node in group.saved_nodes {
                if let Ok(identity) = NodeIdentity::new(&group.id, &node.name)
                    && seen.insert(identity.clone())
                {
                    inventory.push(identity);
                }
            }
        }
        inventory
    }

    pub(in crate::app) fn policy_candidate_inventory(&self) -> Vec<NodeIdentity> {
        let mut inventory = ["PROXY", "DIRECT", "REJECT"]
            .into_iter()
            .filter_map(|name| NodeIdentity::new("builtin", name).ok())
            .collect::<Vec<_>>();
        inventory.extend(self.node_inventory());
        let editing_id = self
            .managed_policies
            .draft
            .as_ref()
            .and_then(|draft| draft.editing_id.as_deref());
        for group in &self.managed_policies.groups {
            if editing_id != Some(group.id.as_str())
                && let Ok(identity) =
                    NodeIdentity::new(&format!("policy:{}", group.id), &group.name)
            {
                inventory.push(identity);
            }
        }
        inventory
    }

    pub(in crate::app) fn managed_policy_candidate_count(
        &self,
        group: &ManagedPolicyGroup,
    ) -> usize {
        self.node_inventory()
            .iter()
            .filter(|node| group.matches(&node.source_id, &node.node_name))
            .count()
    }

    pub(in crate::app) fn managed_policy_candidate_names(
        &self,
        group: &ManagedPolicyGroup,
    ) -> Vec<String> {
        self.managed_policy_candidate_nodes(group)
            .into_iter()
            .map(|node| node.name)
            .collect()
    }

    pub(in crate::app) fn managed_policy_candidate_nodes(
        &self,
        group: &ManagedPolicyGroup,
    ) -> Vec<PolicyNode> {
        let language = self.language();
        let has_local_sources =
            !self.imported_subscriptions.is_empty() || !self.saved_single_nodes.is_empty();
        let mut candidates = Vec::new();
        let mut seen = BTreeSet::new();
        for source in self.node_source_groups(has_local_sources, language) {
            for provider in source.providers {
                for node in &provider.nodes {
                    if group.matches(&source.id, &node.name) && seen.insert(node.name.clone()) {
                        candidates.push(PolicyNode {
                            id: ProxyId::new(format!("{}:{}", source.id, node.name)),
                            name: node.name.clone(),
                            kind: PolicyCandidateKind::Node,
                            provider: Some(source.name.clone()),
                            detail: node.protocol.clone(),
                            latency_ms: node
                                .latency_label
                                .as_deref()
                                .and_then(|label| label.strip_suffix(" ms"))
                                .and_then(|delay| delay.parse().ok()),
                            alive: node.alive,
                        });
                    }
                }
            }
            for node in source.saved_nodes {
                if group.matches(&source.id, &node.name) && seen.insert(node.name.clone()) {
                    candidates.push(PolicyNode {
                        id: ProxyId::new(format!("{}:{}", source.id, node.name)),
                        name: node.name.clone(),
                        kind: PolicyCandidateKind::Node,
                        provider: Some(source.name.clone()),
                        detail: node.protocol.to_owned(),
                        latency_ms: None,
                        alive: None,
                    });
                }
            }
        }
        if matches!(group.matcher, PolicyCandidateMatcher::Explicit(_)) {
            for name in ["PROXY", "DIRECT", "REJECT"] {
                let is_proxy = name == "PROXY";
                let runtime_name = if is_proxy {
                    manis_profile::MANIS_GLOBAL_GROUP_NAME
                } else {
                    name
                };
                if group.matches("builtin", name) && seen.insert(runtime_name.to_owned()) {
                    candidates.push(PolicyNode {
                        id: ProxyId::new(format!("builtin:{name}")),
                        name: runtime_name.to_owned(),
                        kind: if is_proxy {
                            PolicyCandidateKind::PolicyGroup
                        } else {
                            PolicyCandidateKind::Node
                        },
                        provider: Some(language.localized(copy::nodes::BUILT_IN).to_owned()),
                        detail: if is_proxy {
                            language
                                .localized(copy::nodes::FOLLOW_HOME_SELECTION)
                                .to_owned()
                        } else {
                            name.to_owned()
                        },
                        latency_ms: None,
                        alive: None,
                    });
                }
            }
            for policy in &self.managed_policies.groups {
                let source_id = format!("policy:{}", policy.id);
                if policy.id != group.id
                    && group.matches(&source_id, &policy.name)
                    && seen.insert(policy.name.clone())
                {
                    candidates.push(PolicyNode {
                        id: ProxyId::new(source_id),
                        name: policy.name.clone(),
                        kind: PolicyCandidateKind::PolicyGroup,
                        provider: None,
                        detail: language
                            .localized(match policy.strategy {
                                ManagedPolicyStrategy::Manual => copy::app::MANUAL_SELECTION,
                                ManagedPolicyStrategy::LowestLatency => {
                                    copy::app::AUTOMATIC_SELECTION
                                }
                            })
                            .to_owned(),
                        latency_ms: None,
                        alive: None,
                    });
                }
            }
        }
        candidates
    }
}
