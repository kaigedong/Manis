use std::{collections::BTreeMap, error::Error, fmt, sync::Arc};

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct PolicyGroupId(pub Arc<str>);

impl PolicyGroupId {
    #[must_use]
    pub fn new(value: impl Into<Arc<str>>) -> Self {
        Self(value.into())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for PolicyGroupId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for PolicyGroupId {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ProxyId(pub Arc<str>);

impl ProxyId {
    #[must_use]
    pub fn new(value: impl Into<Arc<str>>) -> Self {
        Self(value.into())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for ProxyId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for ProxyId {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PolicyNode {
    pub id: ProxyId,
    pub name: String,
    pub kind: PolicyCandidateKind,
    pub provider: Option<String>,
    pub detail: String,
    pub latency_ms: Option<u16>,
    pub alive: Option<bool>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PolicyCandidateKind {
    Node,
    PolicyGroup,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PolicyRule {
    pub index: u32,
    pub kind: String,
    pub payload: String,
    pub hit_count: Option<u64>,
    pub disabled: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PolicyGroup {
    pub id: PolicyGroupId,
    pub name: String,
    pub kind: PolicyGroupKind,
    /// Runtime-selected candidate, or `None` when the group has no candidates.
    pub target: Option<String>,
    pub nodes: Vec<PolicyNode>,
    pub rules: Vec<PolicyRule>,
    pub rules_total: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PolicyGroupKind {
    Selector,
    UrlTest,
    Fallback,
    LoadBalance,
    Direct,
}

impl PolicyGroupKind {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Selector => "手动选择",
            Self::UrlTest => "自动选择",
            Self::Fallback => "故障转移",
            Self::LoadBalance => "负载均衡",
            Self::Direct => "直连",
        }
    }

    #[must_use]
    pub const fn allows_manual_selection(self) -> bool {
        matches!(self, Self::Selector)
    }

    #[must_use]
    pub const fn is_automatic(self) -> bool {
        matches!(self, Self::UrlTest | Self::Fallback | Self::LoadBalance)
    }
}

impl PolicyGroup {
    #[must_use]
    pub fn rules_count(&self) -> usize {
        self.rules_total
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EmptyPolicyCatalog;

impl fmt::Display for EmptyPolicyCatalog {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("policy catalog must contain at least one group")
    }
}

impl Error for EmptyPolicyCatalog {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoutingRule {
    pub index: u32,
    pub kind: String,
    pub payload: String,
    pub target: String,
    pub disabled: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PolicyCatalog {
    primary: PolicyGroup,
    remaining: Vec<PolicyGroup>,
    routing_rules: Vec<RoutingRule>,
}

impl PolicyCatalog {
    #[must_use]
    pub fn from_primary(primary: PolicyGroup, remaining: Vec<PolicyGroup>) -> Self {
        Self {
            primary,
            remaining,
            routing_rules: Vec::new(),
        }
    }

    /// Builds a catalog while preserving the source order.
    ///
    /// # Errors
    ///
    /// Returns [`EmptyPolicyCatalog`] when `groups` is empty.
    pub fn try_new(groups: Vec<PolicyGroup>) -> Result<Self, EmptyPolicyCatalog> {
        Self::try_new_with_rules(groups, Vec::new())
    }

    /// Builds a catalog with the complete ordered routing rule list.
    ///
    /// # Errors
    ///
    /// Returns [`EmptyPolicyCatalog`] when `groups` is empty.
    pub fn try_new_with_rules(
        groups: Vec<PolicyGroup>,
        mut routing_rules: Vec<RoutingRule>,
    ) -> Result<Self, EmptyPolicyCatalog> {
        let mut groups = groups.into_iter();
        let primary = groups.next().ok_or(EmptyPolicyCatalog)?;
        routing_rules.sort_by_key(|rule| rule.index);
        Ok(Self {
            primary,
            remaining: groups.collect(),
            routing_rules,
        })
    }

    pub fn iter(&self) -> impl Iterator<Item = &PolicyGroup> {
        std::iter::once(&self.primary).chain(&self.remaining)
    }

    pub fn routing_rules(&self) -> impl Iterator<Item = &RoutingRule> {
        self.routing_rules.iter()
    }

    #[must_use]
    pub fn group(&self, id: &PolicyGroupId) -> Option<&PolicyGroup> {
        self.iter().find(|group| group.id == *id)
    }

    #[must_use]
    pub fn select(&self, id: Option<&PolicyGroupId>) -> &PolicyGroup {
        id.and_then(|id| self.iter().find(|group| group.id == *id))
            .unwrap_or(&self.primary)
    }

    /// Applies fresh delay measurements and the runtime-selected winner to one group.
    ///
    /// Returns `false` when the group no longer exists. An unknown winner is ignored so a stale
    /// or malformed controller response cannot point the UI outside the group's candidates.
    pub fn apply_group_benchmark(
        &mut self,
        id: &PolicyGroupId,
        current: Option<&str>,
        delays: &BTreeMap<String, u16>,
    ) -> bool {
        let Some(group) = std::iter::once(&mut self.primary)
            .chain(&mut self.remaining)
            .find(|group| group.id == *id)
        else {
            return false;
        };
        if let Some(current) =
            current.filter(|current| group.nodes.iter().any(|node| node.name == *current))
        {
            group.target = Some(current.to_owned());
        }
        for node in &mut group.nodes {
            let Some(delay) = delays.get(&node.name).copied() else {
                continue;
            };
            if delay == 0 {
                node.latency_ms = None;
                node.alive = Some(false);
            } else {
                node.latency_ms = Some(delay);
                node.alive = Some(true);
            }
        }
        true
    }

    /// Records a selector group's validated target without rebuilding the catalog.
    ///
    /// `group_id_or_name` may match either the stable group ID or the display name. `target` may
    /// match a candidate ID or name, but the catalog stores the candidate's display name.
    /// Returns `false` for missing groups, non-selector groups, or targets outside the candidates.
    pub fn apply_selector_target(&mut self, group_id_or_name: &str, target: &str) -> bool {
        let Some(group) = std::iter::once(&mut self.primary)
            .chain(&mut self.remaining)
            .find(|group| group.id.as_str() == group_id_or_name || group.name == group_id_or_name)
        else {
            return false;
        };
        if !group.kind.allows_manual_selection() {
            return false;
        }
        let Some(candidate) = group
            .nodes
            .iter()
            .find(|node| node.id.as_str() == target || node.name == target)
        else {
            return false;
        };
        group.target = Some(candidate.name.clone());
        true
    }
}
