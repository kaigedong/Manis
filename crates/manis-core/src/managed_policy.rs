use std::{collections::BTreeSet, error::Error, fmt};

const MAX_MANAGED_POLICY_NAME_BYTES: usize = 96;
const MAX_POLICY_CANDIDATE_MATCH_BYTES: usize = 256;
const MAX_NODE_IDENTITY_NAME_BYTES: usize = 512;
const MAX_POLICY_CANDIDATES: usize = 128;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ManagedPolicyIcon {
    #[default]
    None,
    Bolt,
    Globe,
    Shield,
    Compass,
}

impl ManagedPolicyIcon {
    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::None => Self::Bolt,
            Self::Bolt => Self::Globe,
            Self::Globe => Self::Shield,
            Self::Shield => Self::Compass,
            Self::Compass => Self::None,
        }
    }

    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Bolt => "bolt",
            Self::Globe => "globe",
            Self::Shield => "shield",
            Self::Compass => "compass",
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::None => "首字",
            Self::Bolt => "闪电",
            Self::Globe => "地球",
            Self::Shield => "盾牌",
            Self::Compass => "罗盘",
        }
    }

    /// Parses the stable persistence key.
    ///
    /// # Errors
    /// Returns [`ManagedPolicyError::InvalidIcon`] for unknown keys.
    pub fn parse_key(value: &str) -> Result<Self, ManagedPolicyError> {
        match value {
            "none" => Ok(Self::None),
            "bolt" => Ok(Self::Bolt),
            "globe" => Ok(Self::Globe),
            "shield" => Ok(Self::Shield),
            "compass" => Ok(Self::Compass),
            _ => Err(ManagedPolicyError::InvalidIcon),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ManagedPolicyStrategy {
    #[default]
    Manual,
    LowestLatency,
}

impl ManagedPolicyStrategy {
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
    /// Returns [`ManagedPolicyError::InvalidStrategy`] for unknown keys.
    pub fn parse_key(value: &str) -> Result<Self, ManagedPolicyError> {
        match value {
            "manual" => Ok(Self::Manual),
            "latency" => Ok(Self::LowestLatency),
            _ => Err(ManagedPolicyError::InvalidStrategy),
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
    /// Returns [`ManagedPolicyError::InvalidMember`] for unsafe or oversized values.
    pub fn new(source_id: &str, node_name: &str) -> Result<Self, ManagedPolicyError> {
        if !valid_stable_identity_component(source_id)
            || !valid_plain_policy_value(node_name, MAX_NODE_IDENTITY_NAME_BYTES)
        {
            return Err(ManagedPolicyError::InvalidMember);
        }
        Ok(Self {
            source_id: source_id.to_owned(),
            node_name: node_name.to_owned(),
        })
    }

    fn is_valid(&self) -> bool {
        valid_stable_identity_component(&self.source_id)
            && valid_plain_policy_value(&self.node_name, MAX_NODE_IDENTITY_NAME_BYTES)
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum PolicyCandidateMatcher {
    #[default]
    All,
    NameContains(String),
    Explicit(BTreeSet<NodeIdentity>),
}

impl PolicyCandidateMatcher {
    /// Creates a case-insensitive node-name matcher.
    ///
    /// # Errors
    /// Returns [`ManagedPolicyError::InvalidMatcher`] for empty, unsafe, or oversized values.
    pub fn name_contains(value: &str) -> Result<Self, ManagedPolicyError> {
        if !valid_plain_policy_value(value, MAX_POLICY_CANDIDATE_MATCH_BYTES) {
            return Err(ManagedPolicyError::InvalidMatcher);
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
                valid_plain_policy_value(value, MAX_POLICY_CANDIDATE_MATCH_BYTES)
            }
            Self::Explicit(members) => {
                members.len() <= MAX_POLICY_CANDIDATES && members.iter().all(NodeIdentity::is_valid)
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManagedPolicyGroup {
    pub id: String,
    pub name: String,
    pub icon: ManagedPolicyIcon,
    pub strategy: ManagedPolicyStrategy,
    pub test_interval_secs: u32,
    /// Required latency improvement before switching a healthy automatic exit.
    pub switch_tolerance_ms: u16,
    pub matcher: PolicyCandidateMatcher,
}

impl ManagedPolicyGroup {
    pub const DEFAULT_SWITCH_TOLERANCE_MS: u16 = 150;

    /// Creates a policy group with manual selection and an all-node matcher.
    ///
    /// # Errors
    /// Returns an ID or name validation error when either value is unsafe.
    pub fn new(id: &str, name: &str) -> Result<Self, ManagedPolicyError> {
        if !valid_stable_identity_component(id) {
            return Err(ManagedPolicyError::InvalidId);
        }
        if !valid_plain_policy_value(name, MAX_MANAGED_POLICY_NAME_BYTES) {
            return Err(ManagedPolicyError::InvalidName);
        }
        Ok(Self {
            id: id.to_owned(),
            name: name.to_owned(),
            icon: ManagedPolicyIcon::default(),
            strategy: ManagedPolicyStrategy::default(),
            test_interval_secs: 600,
            switch_tolerance_ms: Self::DEFAULT_SWITCH_TOLERANCE_MS,
            matcher: PolicyCandidateMatcher::default(),
        })
    }

    /// Updates the user-facing group name.
    ///
    /// # Errors
    /// Returns [`ManagedPolicyError::InvalidName`] for empty, unsafe, or oversized values.
    pub fn rename(&mut self, name: &str) -> Result<(), ManagedPolicyError> {
        if !valid_plain_policy_value(name, MAX_MANAGED_POLICY_NAME_BYTES) {
            return Err(ManagedPolicyError::InvalidName);
        }
        name.clone_into(&mut self.name);
        Ok(())
    }

    /// Replaces the candidate matcher after validating its bounds.
    ///
    /// # Errors
    /// Returns [`ManagedPolicyError::InvalidMatcher`] when the matcher is unsafe or oversized.
    pub fn set_matcher(
        &mut self,
        matcher: PolicyCandidateMatcher,
    ) -> Result<(), ManagedPolicyError> {
        if !matcher.is_valid() {
            return Err(ManagedPolicyError::InvalidMatcher);
        }
        self.matcher = matcher;
        Ok(())
    }

    /// Sets how often Mihomo reevaluates this automatic strategy group.
    ///
    /// # Errors
    /// Returns [`ManagedPolicyError::InvalidTestInterval`] outside 30 seconds to 24 hours.
    pub fn set_test_interval_secs(&mut self, seconds: u32) -> Result<(), ManagedPolicyError> {
        if !(30..=86_400).contains(&seconds) {
            return Err(ManagedPolicyError::InvalidTestInterval);
        }
        self.test_interval_secs = seconds;
        Ok(())
    }

    #[must_use]
    pub fn matches(&self, source_id: &str, node_name: &str) -> bool {
        match &self.matcher {
            PolicyCandidateMatcher::All => true,
            PolicyCandidateMatcher::NameContains(value) => {
                node_name.to_lowercase().contains(&value.to_lowercase())
            }
            PolicyCandidateMatcher::Explicit(members) => members.contains(&NodeIdentity {
                source_id: source_id.to_owned(),
                node_name: node_name.to_owned(),
            }),
        }
    }

    /// Adds a missing explicit member or removes an existing one.
    ///
    /// Returns `true` when the member is present after the operation.
    pub fn toggle_member(&mut self, member: NodeIdentity) -> bool {
        let PolicyCandidateMatcher::Explicit(members) = &mut self.matcher else {
            return false;
        };
        if members.remove(&member) {
            return false;
        }
        if members.len() >= MAX_POLICY_CANDIDATES || !member.is_valid() {
            return false;
        }
        members.insert(member)
    }

    #[must_use]
    pub fn member_count(&self) -> usize {
        match &self.matcher {
            PolicyCandidateMatcher::Explicit(members) => members.len(),
            PolicyCandidateMatcher::All | PolicyCandidateMatcher::NameContains(_) => 0,
        }
    }

    /// Validates the complete group before persistence or configuration generation.
    ///
    /// # Errors
    /// Returns the first invalid ID, name, or matcher field.
    pub fn validate(&self) -> Result<(), ManagedPolicyError> {
        if !valid_stable_identity_component(&self.id) {
            return Err(ManagedPolicyError::InvalidId);
        }
        if !valid_plain_policy_value(&self.name, MAX_MANAGED_POLICY_NAME_BYTES) {
            return Err(ManagedPolicyError::InvalidName);
        }
        if !self.matcher.is_valid() {
            return Err(ManagedPolicyError::InvalidMatcher);
        }
        if !(30..=86_400).contains(&self.test_interval_secs) {
            return Err(ManagedPolicyError::InvalidTestInterval);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ManagedPolicyError {
    InvalidId,
    InvalidName,
    InvalidIcon,
    InvalidStrategy,
    InvalidMatcher,
    InvalidMember,
    InvalidTestInterval,
}

impl fmt::Display for ManagedPolicyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidId => "managed policy group id is invalid",
            Self::InvalidName => "managed policy group name is invalid",
            Self::InvalidIcon => "managed policy group icon is invalid",
            Self::InvalidStrategy => "managed policy group strategy is invalid",
            Self::InvalidMatcher => "managed policy group matcher is invalid",
            Self::InvalidMember => "managed policy group member is invalid",
            Self::InvalidTestInterval => "managed policy group test interval is invalid",
        })
    }
}

impl Error for ManagedPolicyError {}

fn valid_stable_identity_component(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 160
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b':' | b'-' | b'_'))
}

fn valid_plain_policy_value(value: &str, max_bytes: usize) -> bool {
    !value.is_empty()
        && value.len() <= max_bytes
        && value.trim() == value
        && !value.chars().any(char::is_control)
}
