use std::{collections::BTreeMap, error::Error, fmt, sync::Arc};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WindowSizeClass {
    Compact,
    Medium,
    Wide,
}

impl WindowSizeClass {
    #[must_use]
    pub fn for_width(width: f32) -> Self {
        if width >= 1_280.0 {
            Self::Wide
        } else if width >= 900.0 {
            Self::Medium
        } else {
            Self::Compact
        }
    }
}

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
    pub provider: Option<String>,
    pub detail: String,
    pub latency_ms: Option<u16>,
    pub alive: Option<bool>,
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
    pub kind: String,
    pub target: String,
    pub nodes: Vec<PolicyNode>,
    pub rules: Vec<PolicyRule>,
    pub rules_total: usize,
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
pub struct PolicyCatalog {
    primary: PolicyGroup,
    remaining: Vec<PolicyGroup>,
}

impl PolicyCatalog {
    #[must_use]
    pub fn from_primary(primary: PolicyGroup, remaining: Vec<PolicyGroup>) -> Self {
        Self { primary, remaining }
    }

    /// Builds a catalog while preserving the source order.
    ///
    /// # Errors
    ///
    /// Returns [`EmptyPolicyCatalog`] when `groups` is empty.
    pub fn try_new(groups: Vec<PolicyGroup>) -> Result<Self, EmptyPolicyCatalog> {
        let mut groups = groups.into_iter();
        let primary = groups.next().ok_or(EmptyPolicyCatalog)?;
        Ok(Self::from_primary(primary, groups.collect()))
    }

    pub fn iter(&self) -> impl Iterator<Item = &PolicyGroup> {
        std::iter::once(&self.primary).chain(&self.remaining)
    }

    #[must_use]
    pub fn select(&self, id: Option<&PolicyGroupId>) -> &PolicyGroup {
        id.and_then(|id| self.iter().find(|group| group.id == *id))
            .unwrap_or(&self.primary)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompactNavigation {
    GroupList,
    GroupDetail,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PrimaryWorkspace {
    #[default]
    Policies,
    Configuration,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ConfigurationSection {
    #[default]
    Sources,
    Groups,
    Rules,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ConfigurationWorkspaceState {
    pub section: ConfigurationSection,
    pub selected_rule: usize,
}

impl ConfigurationWorkspaceState {
    pub fn select_section(&mut self, section: ConfigurationSection) {
        self.section = section;
    }

    pub fn select_rule(&mut self, index: usize, rule_count: usize) {
        self.selected_rule = index.min(rule_count.saturating_sub(1));
        self.section = ConfigurationSection::Rules;
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RouteEvidence {
    Predicted {
        domain: String,
        rule: &'static str,
        policy: PolicyGroupId,
        proxy: ProxyId,
    },
    Observed {
        domain: String,
        rule: String,
        policy: PolicyGroupId,
        chain: Vec<ProxyId>,
    },
    NeedsConnection {
        domain: String,
        reason: &'static str,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PolicyWorkspaceState {
    pub size_class: WindowSizeClass,
    pub selected_group: Option<PolicyGroupId>,
    pub selected_node: Option<ProxyId>,
    pub compact_navigation: CompactNavigation,
    selections: BTreeMap<PolicyGroupId, ProxyId>,
}

impl Default for PolicyWorkspaceState {
    fn default() -> Self {
        Self {
            size_class: WindowSizeClass::Wide,
            selected_group: None,
            selected_node: None,
            compact_navigation: CompactNavigation::GroupList,
            selections: BTreeMap::new(),
        }
    }
}

impl PolicyWorkspaceState {
    #[must_use]
    pub fn demo() -> Self {
        let streaming = PolicyGroupId::new("streaming");
        let hk_01 = ProxyId::new("hk-01");
        let search = PolicyGroupId::new("search");
        let sg_02 = ProxyId::new("sg-02");
        let selections = BTreeMap::from([
            (streaming.clone(), hk_01.clone()),
            (search.clone(), sg_02.clone()),
        ]);

        Self {
            selected_group: Some(streaming),
            selected_node: Some(hk_01),
            selections,
            ..Self::default()
        }
    }

    pub fn resize(&mut self, width: f32) {
        self.size_class = WindowSizeClass::for_width(width);
    }

    pub fn select_group(&mut self, group: PolicyGroupId) {
        self.selected_node = self.selections.get(&group).cloned();
        self.selected_group = Some(group);
        if self.size_class == WindowSizeClass::Compact {
            self.compact_navigation = CompactNavigation::GroupDetail;
        }
    }

    pub fn select_node(&mut self, proxy: ProxyId) {
        if let Some(group) = &self.selected_group {
            self.selections.insert(group.clone(), proxy.clone());
            self.selected_node = Some(proxy);
        }
    }

    pub fn navigate_back(&mut self) {
        self.compact_navigation = CompactNavigation::GroupList;
    }

    pub fn replace_source_selection(&mut self, group: PolicyGroupId, proxy: Option<ProxyId>) {
        self.selections.clear();
        if let Some(proxy) = &proxy {
            self.selections.insert(group.clone(), proxy.clone());
        }
        self.selected_group = Some(group);
        self.selected_node = proxy;
        self.compact_navigation = CompactNavigation::GroupList;
    }

    #[must_use]
    pub fn predict(&self, domain: &str) -> RouteEvidence {
        if domain == "process-dependent.example" {
            return RouteEvidence::NeedsConnection {
                domain: domain.to_owned(),
                reason: "该规则依赖进程信息，需要实际连接才能确认",
            };
        }

        let (policy, fallback_proxy) =
            if domain.ends_with("youtube.com") || domain.ends_with("netflix.com") {
                (PolicyGroupId::new("streaming"), ProxyId::new("hk-01"))
            } else if domain.ends_with("openai.com") || domain.ends_with("google.com") {
                (PolicyGroupId::new("search"), ProxyId::new("sg-02"))
            } else {
                return RouteEvidence::NeedsConnection {
                    domain: domain.to_owned(),
                    reason: "缺少可确定的域名规则，需要实际连接才能确认",
                };
            };

        RouteEvidence::Predicted {
            domain: domain.to_owned(),
            rule: "DOMAIN-SUFFIX",
            policy: policy.clone(),
            proxy: self
                .selections
                .get(&policy)
                .cloned()
                .unwrap_or(fallback_proxy),
        }
    }
}
