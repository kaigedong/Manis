use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
    sync::Arc,
};

/// A proxy core supported by Relay's kernel-neutral configuration boundary.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum KernelKind {
    /// `MetaCubeX` Mihomo, kept as the compatibility-first default.
    #[default]
    Mihomo,
    /// `SagerNet` sing-box.
    SingBox,
}

impl KernelKind {
    /// Returns the stable value written to user-owned configuration files.
    #[must_use]
    pub const fn persistence_key(self) -> &'static str {
        match self {
            Self::Mihomo => "mihomo",
            Self::SingBox => "sing-box",
        }
    }

    /// Parses a stable persisted value without guessing aliases.
    #[must_use]
    pub fn parse(input: &str) -> Option<Self> {
        match input {
            "mihomo" => Some(Self::Mihomo),
            "sing-box" => Some(Self::SingBox),
            _ => None,
        }
    }

    /// Returns the product name shown in the UI.
    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Mihomo => "Mihomo",
            Self::SingBox => "sing-box",
        }
    }

    /// Returns only capabilities Relay can preserve without changing semantics.
    #[must_use]
    pub const fn capabilities(self) -> KernelCapabilities {
        match self {
            Self::Mihomo => KernelCapabilities {
                subscription_providers: true,
                manual_vless: true,
                selector: true,
                url_test: true,
                fallback: true,
                load_balance: true,
                clash_api: true,
                tun: true,
            },
            Self::SingBox => KernelCapabilities {
                subscription_providers: false,
                manual_vless: true,
                selector: true,
                url_test: true,
                fallback: false,
                load_balance: false,
                clash_api: true,
                tun: false,
            },
        }
    }
}

/// Features that are both native to a kernel and implemented by Relay's adapter.
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct KernelCapabilities {
    pub subscription_providers: bool,
    pub manual_vless: bool,
    pub selector: bool,
    pub url_test: bool,
    pub fallback: bool,
    pub load_balance: bool,
    pub clash_api: bool,
    pub tun: bool,
}

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
    pub kind: PolicyCandidateKind,
    pub provider: Option<String>,
    pub detail: String,
    pub latency_ms: Option<u16>,
    pub alive: Option<bool>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PolicyCandidateKind {
    Node,
    NodeGroup,
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
    pub target: String,
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
            current.clone_into(&mut group.target);
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
        candidate.name.clone_into(&mut group.target);
        true
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
    Nodes,
    RoutingRules,
    Activity,
    Logs,
    Configuration,
}

impl PrimaryWorkspace {
    #[must_use]
    pub const fn navigation_order() -> &'static [Self; 6] {
        &[
            Self::Nodes,
            Self::Policies,
            Self::RoutingRules,
            Self::Activity,
            Self::Logs,
            Self::Configuration,
        ]
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ProxyMode {
    #[default]
    Off,
    System,
    Tun,
}

impl ProxyMode {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Off => "关闭代理",
            Self::System => "系统代理",
            Self::Tun => "TUN 代理",
        }
    }

    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::Off => Self::System,
            Self::System => Self::Tun,
            Self::Tun => Self::Off,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum RoutingMode {
    Direct,
    Global,
    #[default]
    Rule,
}

impl RoutingMode {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Direct => "直连",
            Self::Global => "全局",
            Self::Rule => "规则",
        }
    }

    #[must_use]
    pub const fn wire_value(self) -> &'static str {
        match self {
            Self::Direct => "direct",
            Self::Global => "global",
            Self::Rule => "rule",
        }
    }

    #[must_use]
    pub fn parse_wire_value(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "direct" => Some(Self::Direct),
            "global" => Some(Self::Global),
            "rule" => Some(Self::Rule),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum NodeAvailabilityFilter {
    #[default]
    All,
    Available,
    Unavailable,
    Untested,
}

impl NodeAvailabilityFilter {
    #[must_use]
    pub fn includes(self, alive: Option<bool>) -> bool {
        match self {
            Self::All => true,
            Self::Available => alive == Some(true),
            Self::Unavailable => alive == Some(false),
            Self::Untested => alive.is_none(),
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct NodeWorkspaceState {
    pub filter: NodeAvailabilityFilter,
    collapsed_groups: BTreeSet<String>,
}

impl NodeWorkspaceState {
    pub fn select_filter(&mut self, filter: NodeAvailabilityFilter) {
        self.filter = filter;
    }

    #[must_use]
    pub fn includes(&self, alive: Option<bool>) -> bool {
        self.filter.includes(alive)
    }

    pub fn toggle_group(&mut self, group_id: &str) {
        if group_id.is_empty() {
            return;
        }
        if !self.collapsed_groups.remove(group_id) {
            self.collapsed_groups.insert(group_id.to_owned());
        }
    }

    #[must_use]
    pub fn is_group_collapsed(&self, group_id: &str) -> bool {
        self.collapsed_groups.contains(group_id)
    }

    pub fn replace_collapsed_groups<'a>(&mut self, group_ids: impl IntoIterator<Item = &'a str>) {
        self.collapsed_groups = group_ids
            .into_iter()
            .filter(|group_id| !group_id.is_empty())
            .map(str::to_owned)
            .collect();
    }

    pub fn collapsed_group_ids(&self) -> impl Iterator<Item = &str> {
        self.collapsed_groups.iter().map(String::as_str)
    }
}

const MAX_NODE_GROUP_NAME_BYTES: usize = 96;
const MAX_NODE_GROUP_MATCH_BYTES: usize = 256;
const MAX_NODE_IDENTITY_NAME_BYTES: usize = 512;
const MAX_NODE_GROUP_MEMBERS: usize = 128;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum NodeGroupIcon {
    #[default]
    Bolt,
    Globe,
    Shield,
    Compass,
}

impl NodeGroupIcon {
    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::Bolt => Self::Globe,
            Self::Globe => Self::Shield,
            Self::Shield => Self::Compass,
            Self::Compass => Self::Bolt,
        }
    }

    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Bolt => "bolt",
            Self::Globe => "globe",
            Self::Shield => "shield",
            Self::Compass => "compass",
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Bolt => "闪电",
            Self::Globe => "地球",
            Self::Shield => "盾牌",
            Self::Compass => "罗盘",
        }
    }

    /// Parses the stable persistence key.
    ///
    /// # Errors
    /// Returns [`NodeGroupError::InvalidIcon`] for unknown keys.
    pub fn parse_key(value: &str) -> Result<Self, NodeGroupError> {
        match value {
            "bolt" => Ok(Self::Bolt),
            "globe" => Ok(Self::Globe),
            "shield" => Ok(Self::Shield),
            "compass" => Ok(Self::Compass),
            _ => Err(NodeGroupError::InvalidIcon),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum NodeGroupStrategy {
    #[default]
    Manual,
    LowestLatency,
}

impl NodeGroupStrategy {
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::LowestLatency => "latency",
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Manual => "手动选择",
            Self::LowestLatency => "延迟优选",
        }
    }

    /// Parses the stable persistence key.
    ///
    /// # Errors
    /// Returns [`NodeGroupError::InvalidStrategy`] for unknown keys.
    pub fn parse_key(value: &str) -> Result<Self, NodeGroupError> {
        match value {
            "manual" => Ok(Self::Manual),
            "latency" => Ok(Self::LowestLatency),
            _ => Err(NodeGroupError::InvalidStrategy),
        }
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct NodeIdentity {
    pub source_id: String,
    pub node_name: String,
}

impl NodeIdentity {
    /// Creates a stable node identity from its source and display name.
    ///
    /// # Errors
    /// Returns [`NodeGroupError::InvalidMember`] for unsafe or oversized values.
    pub fn new(source_id: &str, node_name: &str) -> Result<Self, NodeGroupError> {
        if !valid_node_group_id(source_id)
            || !valid_plain_node_group_value(node_name, MAX_NODE_IDENTITY_NAME_BYTES)
        {
            return Err(NodeGroupError::InvalidMember);
        }
        Ok(Self {
            source_id: source_id.to_owned(),
            node_name: node_name.to_owned(),
        })
    }

    fn is_valid(&self) -> bool {
        valid_node_group_id(&self.source_id)
            && valid_plain_node_group_value(&self.node_name, MAX_NODE_IDENTITY_NAME_BYTES)
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum NodeGroupMatcher {
    #[default]
    All,
    NameContains(String),
    Explicit(BTreeSet<NodeIdentity>),
}

impl NodeGroupMatcher {
    /// Creates a case-insensitive node-name matcher.
    ///
    /// # Errors
    /// Returns [`NodeGroupError::InvalidMatcher`] for empty, unsafe, or oversized values.
    pub fn name_contains(value: &str) -> Result<Self, NodeGroupError> {
        if !valid_plain_node_group_value(value, MAX_NODE_GROUP_MATCH_BYTES) {
            return Err(NodeGroupError::InvalidMatcher);
        }
        Ok(Self::NameContains(value.to_owned()))
    }

    #[must_use]
    pub const fn key(&self) -> &'static str {
        match self {
            Self::All => "all",
            Self::NameContains(_) => "name",
            Self::Explicit(_) => "explicit",
        }
    }

    fn is_valid(&self) -> bool {
        match self {
            Self::All => true,
            Self::NameContains(value) => {
                valid_plain_node_group_value(value, MAX_NODE_GROUP_MATCH_BYTES)
            }
            Self::Explicit(members) => {
                members.len() <= MAX_NODE_GROUP_MEMBERS
                    && members.iter().all(NodeIdentity::is_valid)
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NodePolicyGroup {
    pub id: String,
    pub name: String,
    pub icon: NodeGroupIcon,
    pub strategy: NodeGroupStrategy,
    pub test_interval_secs: u32,
    pub matcher: NodeGroupMatcher,
}

impl NodePolicyGroup {
    /// Creates a policy group with manual selection and an all-node matcher.
    ///
    /// # Errors
    /// Returns an ID or name validation error when either value is unsafe.
    pub fn new(id: &str, name: &str) -> Result<Self, NodeGroupError> {
        if !valid_node_group_id(id) {
            return Err(NodeGroupError::InvalidId);
        }
        if !valid_plain_node_group_value(name, MAX_NODE_GROUP_NAME_BYTES) {
            return Err(NodeGroupError::InvalidName);
        }
        Ok(Self {
            id: id.to_owned(),
            name: name.to_owned(),
            icon: NodeGroupIcon::default(),
            strategy: NodeGroupStrategy::default(),
            test_interval_secs: 600,
            matcher: NodeGroupMatcher::default(),
        })
    }

    /// Updates the user-facing group name.
    ///
    /// # Errors
    /// Returns [`NodeGroupError::InvalidName`] for empty, unsafe, or oversized values.
    pub fn rename(&mut self, name: &str) -> Result<(), NodeGroupError> {
        if !valid_plain_node_group_value(name, MAX_NODE_GROUP_NAME_BYTES) {
            return Err(NodeGroupError::InvalidName);
        }
        name.clone_into(&mut self.name);
        Ok(())
    }

    /// Replaces the candidate matcher after validating its bounds.
    ///
    /// # Errors
    /// Returns [`NodeGroupError::InvalidMatcher`] when the matcher is unsafe or oversized.
    pub fn set_matcher(&mut self, matcher: NodeGroupMatcher) -> Result<(), NodeGroupError> {
        if !matcher.is_valid() {
            return Err(NodeGroupError::InvalidMatcher);
        }
        self.matcher = matcher;
        Ok(())
    }

    /// Sets how often Mihomo reevaluates this automatic strategy group.
    ///
    /// # Errors
    /// Returns [`NodeGroupError::InvalidTestInterval`] outside 30 seconds to 24 hours.
    pub fn set_test_interval_secs(&mut self, seconds: u32) -> Result<(), NodeGroupError> {
        if !(30..=86_400).contains(&seconds) {
            return Err(NodeGroupError::InvalidTestInterval);
        }
        self.test_interval_secs = seconds;
        Ok(())
    }

    #[must_use]
    pub fn matches(&self, source_id: &str, node_name: &str) -> bool {
        match &self.matcher {
            NodeGroupMatcher::All => true,
            NodeGroupMatcher::NameContains(value) => {
                node_name.to_lowercase().contains(&value.to_lowercase())
            }
            NodeGroupMatcher::Explicit(members) => members.contains(&NodeIdentity {
                source_id: source_id.to_owned(),
                node_name: node_name.to_owned(),
            }),
        }
    }

    /// Adds a missing explicit member or removes an existing one.
    ///
    /// Returns `true` when the member is present after the operation.
    pub fn toggle_member(&mut self, member: NodeIdentity) -> bool {
        let NodeGroupMatcher::Explicit(members) = &mut self.matcher else {
            return false;
        };
        if members.remove(&member) {
            return false;
        }
        if members.len() >= MAX_NODE_GROUP_MEMBERS || !member.is_valid() {
            return false;
        }
        members.insert(member)
    }

    #[must_use]
    pub fn member_count(&self) -> usize {
        match &self.matcher {
            NodeGroupMatcher::Explicit(members) => members.len(),
            NodeGroupMatcher::All | NodeGroupMatcher::NameContains(_) => 0,
        }
    }

    /// Validates the complete group before persistence or configuration generation.
    ///
    /// # Errors
    /// Returns the first invalid ID, name, or matcher field.
    pub fn validate(&self) -> Result<(), NodeGroupError> {
        if !valid_node_group_id(&self.id) {
            return Err(NodeGroupError::InvalidId);
        }
        if !valid_plain_node_group_value(&self.name, MAX_NODE_GROUP_NAME_BYTES) {
            return Err(NodeGroupError::InvalidName);
        }
        if !self.matcher.is_valid() {
            return Err(NodeGroupError::InvalidMatcher);
        }
        if !(30..=86_400).contains(&self.test_interval_secs) {
            return Err(NodeGroupError::InvalidTestInterval);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NodeGroupError {
    InvalidId,
    InvalidName,
    InvalidIcon,
    InvalidStrategy,
    InvalidMatcher,
    InvalidMember,
    InvalidTestInterval,
}

impl fmt::Display for NodeGroupError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidId => "node group id is invalid",
            Self::InvalidName => "node group name is invalid",
            Self::InvalidIcon => "node group icon is invalid",
            Self::InvalidStrategy => "node group strategy is invalid",
            Self::InvalidMatcher => "node group matcher is invalid",
            Self::InvalidMember => "node group member is invalid",
            Self::InvalidTestInterval => "node group test interval is invalid",
        })
    }
}

impl Error for NodeGroupError {}

fn valid_node_group_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 160
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b':' | b'-' | b'_'))
}

fn valid_plain_node_group_value(value: &str, max_bytes: usize) -> bool {
    !value.is_empty()
        && value.len() <= max_bytes
        && value.trim() == value
        && !value.chars().any(char::is_control)
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
