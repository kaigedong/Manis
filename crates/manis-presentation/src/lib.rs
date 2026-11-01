//! Platform-neutral presentation for the node page.
//!
//! The desktop and the Android UI used to derive this model separately: the desktop rebuilt
//! source groups from its providers, while Android read the raw Mihomo `GLOBAL` selector. That
//! difference leaked internal names such as `__MANIS_GLOBAL__` and merged every subscription into
//! one list. This crate owns the shared rules instead: display names, source grouping, current
//! selection, delays and which runtime groups are user-facing.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

/// The hidden Mihomo selector that carries the global choice.
pub const GLOBAL_GROUP: &str = "__MANIS_GLOBAL__";
/// The hidden exit-IP probe group.
pub const EXIT_IP_PROBE_GROUP: &str = "__MANIS_EXIT_IP_PROBE__";
/// What the UI calls the global selector.
pub const GLOBAL_LABEL: &str = "全局";
/// The source card that holds runtime nodes no saved source claimed.
pub const OTHER_SOURCE_ID: &str = "other";

const BUILTIN_NODES: &[&str] = &[
    "DIRECT",
    "REJECT",
    "COMPATIBLE",
    "PASS",
    "PASS-RULE",
    "REJECT-DROP",
];

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NodeSourceKind {
    Subscription,
    Manual,
    Other,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeView {
    pub name: String,
    pub display_name: String,
    pub delay_ms: Option<u16>,
    pub current: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeSourceView {
    pub id: String,
    pub name: String,
    pub detail: String,
    pub kind: NodeSourceKind,
    pub enabled: bool,
    pub nodes: Vec<NodeView>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GlobalView {
    pub label: String,
    pub current: Option<String>,
    pub total: usize,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PolicyGroupView {
    pub id: String,
    pub label: String,
    pub kind: String,
    pub current: Option<String>,
    pub members: Vec<NodeView>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodePageView {
    pub global: GlobalView,
    pub sources: Vec<NodeSourceView>,
    pub policy_groups: Vec<PolicyGroupView>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeSourceInput {
    pub id: String,
    pub name: String,
    pub detail: String,
    pub kind: NodeSourceKind,
    pub enabled: bool,
    pub node_names: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeNodeInput {
    pub delay_ms: Option<u16>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeGroupInput {
    pub id: String,
    pub label: String,
    pub kind: String,
    pub current: Option<String>,
    pub members: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodePageInput {
    /// Whether the core is running. When false, cached source names are shown as a preview.
    pub connected: bool,
    pub sources: Vec<NodeSourceInput>,
    pub global_members: Vec<String>,
    pub global_current: Option<String>,
    pub runtime_nodes: BTreeMap<String, RuntimeNodeInput>,
    pub policy_groups: Vec<RuntimeGroupInput>,
}

impl NodePageView {
    /// Builds the page from the JSON shape the Android host and the desktop adapters share.
    ///
    /// # Errors
    /// Returns a serde error when the request is not a valid [`NodePageInput`].
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        Ok(Self::build(serde_json::from_str(json)?))
    }

    #[must_use]
    pub fn build(input: NodePageInput) -> Self {
        let global_members = unique(&input.global_members);
        let global_set = global_members.iter().cloned().collect::<BTreeSet<_>>();
        let mut assigned = BTreeSet::new();
        let mut sources = Vec::new();

        for source in input.sources {
            let names = if input.connected {
                source
                    .node_names
                    .into_iter()
                    .filter(|name| global_set.contains(name))
                    .collect::<Vec<_>>()
            } else {
                source.node_names
            };
            let nodes = names
                .into_iter()
                .map(|name| {
                    assigned.insert(name.clone());
                    node_view(&name, input.global_current.as_deref(), &input.runtime_nodes)
                })
                .collect::<Vec<_>>();
            if source.enabled && nodes.is_empty() && input.connected {
                continue;
            }
            sources.push(NodeSourceView {
                id: source.id,
                name: source.name,
                detail: source.detail,
                kind: source.kind,
                enabled: source.enabled,
                nodes,
            });
        }

        let group_names = input
            .policy_groups
            .iter()
            .map(|group| group.id.clone())
            .collect::<BTreeSet<_>>();
        let mut policy_groups = Vec::new();
        for group in input.policy_groups {
            if is_internal_policy_name(&group.id) {
                continue;
            }
            let members = group
                .members
                .into_iter()
                .filter(|name| !is_internal_policy_name(name))
                .map(|name| node_view(&name, group.current.as_deref(), &input.runtime_nodes))
                .collect();
            policy_groups.push(PolicyGroupView {
                id: group.id,
                label: display_name(&group.label),
                kind: group.kind,
                current: group.current.as_deref().map(display_name),
                members,
            });
        }

        if input.connected {
            let leftovers = global_members
                .iter()
                .filter(|name| {
                    !assigned.contains(*name)
                        && !group_names.contains(*name)
                        && !BUILTIN_NODES.contains(&name.as_str())
                })
                .map(|name| node_view(name, input.global_current.as_deref(), &input.runtime_nodes))
                .collect::<Vec<_>>();
            if !leftovers.is_empty() {
                sources.push(NodeSourceView {
                    id: OTHER_SOURCE_ID.to_owned(),
                    name: "其他节点".to_owned(),
                    detail: "未归入已保存来源".to_owned(),
                    kind: NodeSourceKind::Other,
                    enabled: true,
                    nodes: leftovers,
                });
            }
        }

        let total = if input.connected {
            global_members.len()
        } else {
            sources.iter().map(|source| source.nodes.len()).sum()
        };
        Self {
            global: GlobalView {
                label: GLOBAL_LABEL.to_owned(),
                current: input.global_current.as_deref().map(display_name),
                total,
            },
            sources,
            policy_groups,
        }
    }
}

/// Maps controller-only names to what the user should see.
#[must_use]
pub fn display_name(name: &str) -> String {
    match name {
        GLOBAL_GROUP | "GLOBAL" => GLOBAL_LABEL.to_owned(),
        EXIT_IP_PROBE_GROUP => "出口探测".to_owned(),
        other => other.to_owned(),
    }
}

/// Whether a runtime group is an implementation detail and must not be listed as a policy group.
#[must_use]
pub fn is_internal_policy_name(name: &str) -> bool {
    name == "GLOBAL" || name.starts_with("__MANIS_")
}

fn node_view(
    name: &str,
    current: Option<&str>,
    runtime_nodes: &BTreeMap<String, RuntimeNodeInput>,
) -> NodeView {
    NodeView {
        name: name.to_owned(),
        display_name: display_name(name),
        delay_ms: runtime_nodes.get(name).and_then(|node| node.delay_ms),
        current: current == Some(name),
    }
}

fn unique(names: &[String]) -> Vec<String> {
    let mut seen = BTreeSet::new();
    names
        .iter()
        .filter(|name| seen.insert((*name).clone()))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(name: &str, nodes: &[&str]) -> NodeSourceInput {
        NodeSourceInput {
            id: name.to_lowercase(),
            name: name.to_owned(),
            detail: "detail".to_owned(),
            kind: NodeSourceKind::Subscription,
            enabled: true,
            node_names: nodes.iter().map(|node| (*node).to_owned()).collect(),
        }
    }

    fn runtime(names: &[(&str, Option<u16>)]) -> BTreeMap<String, RuntimeNodeInput> {
        names
            .iter()
            .map(|(name, delay)| ((*name).to_owned(), RuntimeNodeInput { delay_ms: *delay }))
            .collect()
    }

    #[test]
    fn global_is_never_rendered_as_its_internal_name() {
        assert_eq!(display_name(GLOBAL_GROUP), GLOBAL_LABEL);
        assert_eq!(display_name("GLOBAL"), GLOBAL_LABEL);
        assert_eq!(display_name(EXIT_IP_PROBE_GROUP), "出口探测");
    }

    #[test]
    fn connected_sources_are_split_and_unknown_nodes_are_other() {
        let view = NodePageView::build(NodePageInput {
            connected: true,
            sources: vec![
                source("订阅", &["HK 01", "JP 02"]),
                source("手动", &["ManualA"]),
            ],
            global_members: vec![
                "HK 01".into(),
                "JP 02".into(),
                "ManualA".into(),
                "Extra".into(),
            ],
            global_current: Some("HK 01".into()),
            runtime_nodes: runtime(&[("HK 01", Some(42)), ("Extra", Some(88))]),
            policy_groups: Vec::new(),
        });
        assert_eq!(view.global.total, 4);
        assert_eq!(view.global.current.as_deref(), Some("HK 01"));
        assert_eq!(view.sources.len(), 3);
        assert_eq!(view.sources[0].nodes.len(), 2);
        assert!(view.sources[0].nodes[0].current);
        assert_eq!(view.sources[0].nodes[0].delay_ms, Some(42));
        assert_eq!(view.sources[2].id, OTHER_SOURCE_ID);
        assert_eq!(view.sources[2].nodes[0].name, "Extra");
    }

    #[test]
    fn offline_preview_keeps_cached_names_even_without_runtime_members() {
        let view = NodePageView::build(NodePageInput {
            connected: false,
            sources: vec![source("订阅", &["HK 01", "JP 02"])],
            global_members: Vec::new(),
            global_current: None,
            runtime_nodes: BTreeMap::new(),
            policy_groups: Vec::new(),
        });
        assert_eq!(view.global.total, 2);
        assert_eq!(view.sources[0].nodes.len(), 2);
    }

    #[test]
    fn internal_policy_groups_are_hidden() {
        let view = NodePageView::build(NodePageInput {
            connected: true,
            sources: Vec::new(),
            global_members: Vec::new(),
            global_current: None,
            runtime_nodes: BTreeMap::new(),
            policy_groups: vec![
                RuntimeGroupInput {
                    id: GLOBAL_GROUP.into(),
                    label: GLOBAL_GROUP.into(),
                    kind: "Selector".into(),
                    current: None,
                    members: Vec::new(),
                },
                RuntimeGroupInput {
                    id: "自动选择".into(),
                    label: "自动选择".into(),
                    kind: "URLTest".into(),
                    current: Some("HK 01".into()),
                    members: vec!["HK 01".into()],
                },
            ],
        });
        assert_eq!(view.policy_groups.len(), 1);
        assert_eq!(view.policy_groups[0].label, "自动选择");
    }

    #[test]
    fn a_cached_node_missing_from_the_runtime_is_dropped_while_connected() {
        let view = NodePageView::build(NodePageInput {
            connected: true,
            sources: vec![source("订阅", &["HK 01", "Gone"])],
            global_members: vec!["HK 01".into()],
            global_current: None,
            runtime_nodes: BTreeMap::new(),
            policy_groups: Vec::new(),
        });
        assert_eq!(view.sources[0].nodes.len(), 1);
        assert_eq!(view.sources[0].nodes[0].name, "HK 01");
    }
}
