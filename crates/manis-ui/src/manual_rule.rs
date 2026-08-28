//! User-authored routing rules with QX-compatible match types.

use std::fmt;
#[cfg(not(windows))]
use std::fs;
use std::net::IpAddr;
use std::path::Path;

use manis_core::KernelKind;
#[cfg(not(windows))]
use manis_profile::write_private_atomic;
use manis_profile::{Name, PolicyRef, Profile, Rule};

#[cfg(not(windows))]
const MANUAL_RULES_FILE: &str = "manual-routing-rules.state";
#[cfg(not(windows))]
const MANUAL_RULES_VERSION_V1: &str = "manis.manual-routing-rules.v1";
#[cfg(not(windows))]
const MANUAL_RULES_VERSION_V2: &str = "manis.manual-routing-rules.v2";
#[cfg(not(windows))]
const MAX_MANUAL_RULES_FILE_BYTES: u64 = 256 * 1024;
const MAX_PARAMETER_BYTES: usize = 1_024;
pub(crate) const MAX_CONDITIONS: usize = 4;
const LEGACY_GENERATED_PROXY_GROUP_NAME: &str = "Proxy";

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum ManualRuleKind {
    #[default]
    Host,
    HostSuffix,
    HostWildcard,
    HostKeyword,
    UserAgent,
    IpCidr,
    Ip6Cidr,
    GeoIp,
    IpAsn,
    DstPort,
}

impl ManualRuleKind {
    pub(crate) const ALL: [Self; 10] = [
        Self::Host,
        Self::HostSuffix,
        Self::HostWildcard,
        Self::HostKeyword,
        Self::UserAgent,
        Self::IpCidr,
        Self::Ip6Cidr,
        Self::GeoIp,
        Self::IpAsn,
        Self::DstPort,
    ];

    pub(crate) const fn qx_label(self) -> &'static str {
        match self {
            Self::Host => "HOST",
            Self::HostSuffix => "HOST-SUFFIX",
            Self::HostWildcard => "HOST-WILDCARD",
            Self::HostKeyword => "HOST-KEYWORD",
            Self::UserAgent => "USER-AGENT",
            Self::IpCidr => "IP-CIDR",
            Self::Ip6Cidr => "IP6-CIDR",
            Self::GeoIp => "GEOIP",
            Self::IpAsn => "IP-ASN",
            Self::DstPort => "DST-PORT",
        }
    }

    pub(crate) const fn display_label(self) -> &'static str {
        match self {
            Self::Host => "DOMAIN",
            Self::HostSuffix => "DOMAIN-SUFFIX",
            Self::HostWildcard => "DOMAIN-WILDCARD",
            Self::HostKeyword => "DOMAIN-KEYWORD",
            Self::UserAgent => "USER-AGENT",
            Self::IpCidr => "IP-CIDR",
            Self::Ip6Cidr => "IP6-CIDR",
            Self::GeoIp => "GEOIP",
            Self::IpAsn => "IP-ASN",
            Self::DstPort => "DST-PORT",
        }
    }

    pub(crate) const fn storage_key(self) -> &'static str {
        match self {
            Self::Host => "host",
            Self::HostSuffix => "host-suffix",
            Self::HostWildcard => "host-wildcard",
            Self::HostKeyword => "host-keyword",
            Self::UserAgent => "user-agent",
            Self::IpCidr => "ip-cidr",
            Self::Ip6Cidr => "ip6-cidr",
            Self::GeoIp => "geoip",
            Self::IpAsn => "ip-asn",
            Self::DstPort => "dst-port",
        }
    }

    #[cfg(not(windows))]
    fn from_storage_key(value: &str) -> Option<Self> {
        Some(match value {
            "host" => Self::Host,
            "host-suffix" => Self::HostSuffix,
            "host-wildcard" => Self::HostWildcard,
            "host-keyword" => Self::HostKeyword,
            "user-agent" => Self::UserAgent,
            "ip-cidr" => Self::IpCidr,
            "ip6-cidr" => Self::Ip6Cidr,
            "geoip" => Self::GeoIp,
            "ip-asn" => Self::IpAsn,
            "dst-port" => Self::DstPort,
            _ => return None,
        })
    }

    pub(crate) const fn supported_by(self, kernel: KernelKind) -> bool {
        match self {
            Self::UserAgent => false,
            Self::IpAsn => matches!(kernel, KernelKind::Mihomo),
            _ => true,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ManualRuleCondition {
    kind: ManualRuleKind,
    parameter: String,
}

impl ManualRuleCondition {
    pub(crate) const fn kind(&self) -> ManualRuleKind {
        self.kind
    }

    pub(crate) fn parameter(&self) -> &str {
        &self.parameter
    }

    fn to_profile_condition(&self) -> Result<manis_profile::RuleCondition, ManualRuleCompileError> {
        Ok(match self.kind {
            ManualRuleKind::Host => manis_profile::RuleCondition::Domain(self.parameter.clone()),
            ManualRuleKind::HostSuffix => {
                manis_profile::RuleCondition::DomainSuffix(self.parameter.clone())
            }
            ManualRuleKind::HostWildcard => {
                manis_profile::RuleCondition::DomainWildcard(self.parameter.clone())
            }
            ManualRuleKind::HostKeyword => {
                manis_profile::RuleCondition::DomainKeyword(self.parameter.clone())
            }
            ManualRuleKind::UserAgent => {
                return Err(ManualRuleCompileError::UnsupportedType(self.kind));
            }
            ManualRuleKind::IpCidr | ManualRuleKind::Ip6Cidr => {
                manis_profile::RuleCondition::IpCidr {
                    value: self.parameter.clone(),
                    no_resolve: true,
                }
            }
            ManualRuleKind::GeoIp => manis_profile::RuleCondition::GeoIp {
                country: self.parameter.clone(),
                no_resolve: true,
            },
            ManualRuleKind::IpAsn => manis_profile::RuleCondition::IpAsn {
                asn: self
                    .parameter
                    .parse()
                    .map_err(|_error| ManualRuleCompileError::CorruptValue)?,
                no_resolve: true,
            },
            ManualRuleKind::DstPort => manis_profile::RuleCondition::DstPort(
                self.parameter
                    .parse()
                    .map_err(|_error| ManualRuleCompileError::CorruptValue)?,
            ),
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ManualRule {
    conditions: Vec<ManualRuleCondition>,
    target: Name,
}

impl ManualRule {
    pub(crate) fn parse(
        kind: ManualRuleKind,
        parameter: &str,
        target: &str,
    ) -> Result<Self, ManualRuleError> {
        Self::parse_conditions(vec![(kind, parameter.to_owned())], target)
    }

    pub(crate) fn parse_conditions(
        conditions: Vec<(ManualRuleKind, String)>,
        target: &str,
    ) -> Result<Self, ManualRuleError> {
        if conditions.is_empty() {
            return Err(ManualRuleError::Empty);
        }
        if conditions.len() > MAX_CONDITIONS {
            return Err(ManualRuleError::TooManyConditions);
        }
        let conditions = conditions
            .into_iter()
            .map(|(kind, parameter)| {
                normalize_parameter(kind, &parameter)
                    .map(|parameter| ManualRuleCondition { kind, parameter })
            })
            .collect::<Result<Vec<_>, _>>()?;
        for (index, condition) in conditions.iter().enumerate() {
            if conditions[..index].contains(condition) {
                return Err(ManualRuleError::DuplicateCondition);
            }
        }
        let target = Name::parse(target).map_err(|_error| ManualRuleError::InvalidPolicy)?;
        Ok(Self { conditions, target })
    }

    pub(crate) fn conditions(&self) -> &[ManualRuleCondition] {
        &self.conditions
    }

    pub(crate) fn target(&self) -> &str {
        self.target.as_str()
    }

    fn to_profile_rule(
        &self,
        legacy_proxy_target: Option<&PolicyRef>,
    ) -> Result<Rule, ManualRuleCompileError> {
        let policy = match self.target.as_str() {
            "DIRECT" => PolicyRef::Direct,
            "REJECT" => PolicyRef::Reject,
            LEGACY_GENERATED_PROXY_GROUP_NAME => legacy_proxy_target
                .cloned()
                .unwrap_or_else(|| PolicyRef::Group(self.target.clone())),
            _ => PolicyRef::Group(self.target.clone()),
        };
        if self.conditions.len() > 1 {
            return Ok(Rule::All {
                conditions: self
                    .conditions
                    .iter()
                    .map(ManualRuleCondition::to_profile_condition)
                    .collect::<Result<Vec<_>, _>>()?,
                policy,
            });
        }
        let condition = self.conditions[0].to_profile_condition()?;
        Ok(match condition {
            manis_profile::RuleCondition::Domain(value) => Rule::Domain { value, policy },
            manis_profile::RuleCondition::DomainKeyword(value) => {
                Rule::DomainKeyword { value, policy }
            }
            manis_profile::RuleCondition::DomainSuffix(value) => {
                Rule::DomainSuffix { value, policy }
            }
            manis_profile::RuleCondition::DomainWildcard(value) => {
                Rule::DomainWildcard { value, policy }
            }
            manis_profile::RuleCondition::IpCidr { value, no_resolve } => Rule::IpCidr {
                value,
                policy,
                no_resolve,
            },
            manis_profile::RuleCondition::IpAsn { asn, no_resolve } => Rule::IpAsn {
                asn,
                policy,
                no_resolve,
            },
            manis_profile::RuleCondition::GeoIp {
                country,
                no_resolve,
            } => Rule::GeoIp {
                country,
                policy,
                no_resolve,
            },
            manis_profile::RuleCondition::DstPort(port) => Rule::DstPort { port, policy },
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ManualRuleError {
    Empty,
    InvalidDomain,
    InvalidWildcard,
    InvalidKeyword,
    InvalidIpv4Cidr,
    InvalidIpv6Cidr,
    InvalidGeoIp,
    InvalidAsn,
    InvalidDestinationPort,
    InvalidPolicy,
    UnsupportedByKernel,
    Duplicate,
    DuplicateCondition,
    TooManyConditions,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ManualRuleEditError {
    Missing,
    Duplicate,
}

pub(crate) fn replace_manual_rule(
    rules: &mut [ManualRule],
    index: usize,
    replacement: ManualRule,
) -> Result<ManualRule, ManualRuleEditError> {
    if index >= rules.len() {
        return Err(ManualRuleEditError::Missing);
    }
    if rules
        .iter()
        .enumerate()
        .any(|(candidate_index, candidate)| candidate_index != index && candidate == &replacement)
    {
        return Err(ManualRuleEditError::Duplicate);
    }
    Ok(std::mem::replace(&mut rules[index], replacement))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ManualRuleStoreError {
    Unavailable,
    Corrupt,
}

impl fmt::Display for ManualRuleStoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable => formatter.write_str("手动分流规则存储不可用"),
            Self::Corrupt => formatter.write_str("手动分流规则文件已损坏"),
        }
    }
}

impl std::error::Error for ManualRuleStoreError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ManualRuleCompileError {
    UnsupportedType(ManualRuleKind),
    CorruptValue,
}

impl fmt::Display for ManualRuleCompileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedType(kind) => write!(
                formatter,
                "当前内核无法精确执行 {} 手动分流规则",
                kind.qx_label()
            ),
            Self::CorruptValue => formatter.write_str("手动分流规则参数无效"),
        }
    }
}

impl std::error::Error for ManualRuleCompileError {}

fn normalize_parameter(kind: ManualRuleKind, parameter: &str) -> Result<String, ManualRuleError> {
    let value = parameter.trim();
    if value.is_empty() {
        return Err(ManualRuleError::Empty);
    }
    if value.len() > MAX_PARAMETER_BYTES
        || value.contains([',', '\t', '\n', '\r'])
        || value.chars().any(char::is_control)
    {
        return Err(match kind {
            ManualRuleKind::Host | ManualRuleKind::HostSuffix => ManualRuleError::InvalidDomain,
            ManualRuleKind::HostWildcard => ManualRuleError::InvalidWildcard,
            ManualRuleKind::IpCidr => ManualRuleError::InvalidIpv4Cidr,
            ManualRuleKind::Ip6Cidr => ManualRuleError::InvalidIpv6Cidr,
            ManualRuleKind::GeoIp => ManualRuleError::InvalidGeoIp,
            ManualRuleKind::IpAsn => ManualRuleError::InvalidAsn,
            ManualRuleKind::DstPort => ManualRuleError::InvalidDestinationPort,
            _ => ManualRuleError::InvalidKeyword,
        });
    }
    match kind {
        ManualRuleKind::Host | ManualRuleKind::HostSuffix => {
            let value = value.to_ascii_lowercase();
            is_domain(&value)
                .then_some(value)
                .ok_or(ManualRuleError::InvalidDomain)
        }
        ManualRuleKind::HostWildcard => {
            let value = value.to_ascii_lowercase();
            is_domain_wildcard(&value)
                .then_some(value)
                .ok_or(ManualRuleError::InvalidWildcard)
        }
        ManualRuleKind::HostKeyword | ManualRuleKind::UserAgent => Ok(value.to_owned()),
        ManualRuleKind::IpCidr => normalize_cidr(value, false),
        ManualRuleKind::Ip6Cidr => normalize_cidr(value, true),
        ManualRuleKind::GeoIp => {
            let value = value.to_ascii_uppercase();
            (value.len() == 2 && value.bytes().all(|byte| byte.is_ascii_alphabetic()))
                .then_some(value)
                .ok_or(ManualRuleError::InvalidGeoIp)
        }
        ManualRuleKind::IpAsn => value
            .parse::<u32>()
            .ok()
            .filter(|asn| *asn > 0)
            .map(|asn| asn.to_string())
            .ok_or(ManualRuleError::InvalidAsn),
        ManualRuleKind::DstPort => value
            .parse::<u16>()
            .ok()
            .filter(|port| *port > 0)
            .map(|port| port.to_string())
            .ok_or(ManualRuleError::InvalidDestinationPort),
    }
}

fn is_domain(value: &str) -> bool {
    value.len() <= 253
        && value.contains('.')
        && value.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
}

fn is_domain_wildcard(value: &str) -> bool {
    value.len() <= 253
        && value.contains('.')
        && !value.starts_with('.')
        && !value.ends_with('.')
        && !value.contains("..")
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'*' | b'?')
        })
}

fn normalize_cidr(value: &str, ipv6: bool) -> Result<String, ManualRuleError> {
    let error = if ipv6 {
        ManualRuleError::InvalidIpv6Cidr
    } else {
        ManualRuleError::InvalidIpv4Cidr
    };
    let Some((address, prefix)) = value.split_once('/') else {
        return Err(error);
    };
    if value.matches('/').count() != 1 {
        return Err(error);
    }
    let address = address.parse::<IpAddr>().map_err(|_error| error)?;
    let prefix = prefix.parse::<u8>().map_err(|_error| error)?;
    match (address, ipv6) {
        (IpAddr::V4(address), false) if prefix <= 32 => Ok(format!("{address}/{prefix}")),
        (IpAddr::V6(address), true) if prefix <= 128 => Ok(format!("{address}/{prefix}")),
        _ => Err(error),
    }
}

pub(crate) fn append_manual_rules(
    profile: &mut Profile,
    rules: &[ManualRule],
    kernel: KernelKind,
) -> Result<(), ManualRuleCompileError> {
    for rule in rules {
        if let Some(condition) = rule
            .conditions
            .iter()
            .find(|condition| !condition.kind.supported_by(kernel))
        {
            return Err(ManualRuleCompileError::UnsupportedType(condition.kind));
        }
    }
    let has_user_named_proxy = profile
        .groups
        .iter()
        .any(|group| group.name.as_str() == LEGACY_GENERATED_PROXY_GROUP_NAME);
    let legacy_proxy_target = (!has_user_named_proxy)
        .then(|| {
            profile
                .groups
                .iter()
                .find(|group| group.name.as_str() != manis_profile::MANIS_GLOBAL_GROUP_NAME)
                .or_else(|| profile.groups.first())
                .map(|group| PolicyRef::Group(group.name.clone()))
        })
        .flatten();
    let compiled = rules
        .iter()
        .map(|rule| rule.to_profile_rule(legacy_proxy_target.as_ref()))
        .collect::<Result<Vec<_>, _>>()?;
    profile.rules.extend(compiled);
    Ok(())
}

#[cfg(not(windows))]
fn encode_manual_rules(rules: &[ManualRule]) -> String {
    let mut contents = String::from(MANUAL_RULES_VERSION_V2);
    contents.push_str("\nlegacy-direct-rules-migrated\t1");
    for rule in rules {
        contents.push_str("\nrule\t");
        contents.push_str(rule.target.as_str());
        for condition in &rule.conditions {
            contents.push('\t');
            contents.push_str(condition.kind.storage_key());
            contents.push('\t');
            contents.push_str(&condition.parameter);
        }
    }
    contents
}

#[cfg(not(windows))]
fn decode_v1_manual_rules<'a>(
    lines: impl Iterator<Item = &'a str>,
) -> Result<Vec<ManualRule>, ManualRuleStoreError> {
    lines
        .filter(|line| !line.is_empty())
        .map(|line| {
            let mut fields = line.split('\t');
            let kind = fields
                .next()
                .and_then(ManualRuleKind::from_storage_key)
                .ok_or(ManualRuleStoreError::Corrupt)?;
            let parameter = fields.next().ok_or(ManualRuleStoreError::Corrupt)?;
            let target = fields.next().ok_or(ManualRuleStoreError::Corrupt)?;
            if fields.next().is_some() {
                return Err(ManualRuleStoreError::Corrupt);
            }
            ManualRule::parse(kind, parameter, target)
                .map_err(|_error| ManualRuleStoreError::Corrupt)
        })
        .collect()
}

#[cfg(not(windows))]
fn decode_v2_manual_rules<'a>(
    mut lines: impl Iterator<Item = &'a str>,
) -> Result<Vec<ManualRule>, ManualRuleStoreError> {
    if lines.next() != Some("legacy-direct-rules-migrated\t1") {
        return Err(ManualRuleStoreError::Corrupt);
    }
    lines
        .filter(|line| !line.is_empty())
        .map(|line| {
            let fields = line.split('\t').collect::<Vec<_>>();
            if fields.first() != Some(&"rule")
                || fields.len() < 4
                || !(fields.len() - 2).is_multiple_of(2)
            {
                return Err(ManualRuleStoreError::Corrupt);
            }
            let target = fields[1];
            let conditions = fields[2..]
                .as_chunks::<2>()
                .0
                .iter()
                .map(|pair| {
                    ManualRuleKind::from_storage_key(pair[0])
                        .map(|kind| (kind, pair[1].to_owned()))
                        .ok_or(ManualRuleStoreError::Corrupt)
                })
                .collect::<Result<Vec<_>, _>>()?;
            ManualRule::parse_conditions(conditions, target)
                .map_err(|_error| ManualRuleStoreError::Corrupt)
        })
        .collect()
}

#[cfg(not(windows))]
fn decode_manual_rules(contents: &str) -> Result<(Vec<ManualRule>, bool), ManualRuleStoreError> {
    let mut lines = contents.lines();
    match lines.next() {
        Some(MANUAL_RULES_VERSION_V1) => decode_v1_manual_rules(lines).map(|rules| (rules, false)),
        Some(MANUAL_RULES_VERSION_V2) => decode_v2_manual_rules(lines).map(|rules| (rules, true)),
        _ => Err(ManualRuleStoreError::Corrupt),
    }
}

fn convert_legacy_direct_rule(
    rule: crate::direct_rule::DirectRule,
) -> Result<ManualRule, ManualRuleStoreError> {
    let (kind, parameter) = match rule {
        crate::direct_rule::DirectRule::Port(port) => (ManualRuleKind::DstPort, port.to_string()),
        crate::direct_rule::DirectRule::DomainSuffix(domain) => {
            (ManualRuleKind::HostSuffix, domain)
        }
    };
    ManualRule::parse(kind, &parameter, "DIRECT").map_err(|_error| ManualRuleStoreError::Corrupt)
}

fn merge_legacy_direct_rules(
    rules: Vec<ManualRule>,
    legacy: Vec<crate::direct_rule::DirectRule>,
) -> Result<Vec<ManualRule>, ManualRuleStoreError> {
    let mut merged = legacy
        .into_iter()
        .map(convert_legacy_direct_rule)
        .collect::<Result<Vec<_>, _>>()?;
    for rule in rules {
        if !merged.contains(&rule) {
            merged.push(rule);
        }
    }
    Ok(merged)
}

#[cfg(not(windows))]
fn read_manual_rules_document(
    directory: &Path,
) -> Result<(Vec<ManualRule>, bool), ManualRuleStoreError> {
    let path = directory.join(MANUAL_RULES_FILE);
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok((Vec::new(), false));
        }
        Err(_error) => return Err(ManualRuleStoreError::Unavailable),
    };
    if !metadata.is_file() || metadata.len() > MAX_MANUAL_RULES_FILE_BYTES {
        return Err(ManualRuleStoreError::Corrupt);
    }
    let contents = fs::read_to_string(path).map_err(|_error| ManualRuleStoreError::Corrupt)?;
    decode_manual_rules(&contents)
}

#[cfg(not(windows))]
fn map_legacy_store_error(error: crate::direct_rule::DirectRuleStoreError) -> ManualRuleStoreError {
    match error {
        crate::direct_rule::DirectRuleStoreError::Unavailable => ManualRuleStoreError::Unavailable,
        crate::direct_rule::DirectRuleStoreError::Corrupt => ManualRuleStoreError::Corrupt,
    }
}

#[cfg(not(windows))]
fn migrate_legacy_direct_rules_in(
    directory: &Path,
    rules: Vec<ManualRule>,
) -> Result<Vec<ManualRule>, ManualRuleStoreError> {
    let legacy =
        crate::direct_rule::load_direct_rules_in(directory).map_err(map_legacy_store_error)?;
    let merged = merge_legacy_direct_rules(rules, legacy)?;
    save_manual_rules_in(directory, &merged)?;
    Ok(merged)
}

#[cfg(not(windows))]
pub(crate) fn save_manual_rules_in(
    directory: &Path,
    rules: &[ManualRule],
) -> Result<(), ManualRuleStoreError> {
    write_private_atomic(
        directory,
        MANUAL_RULES_FILE,
        encode_manual_rules(rules).as_bytes(),
    )
    .map(|_path| ())
    .map_err(|_error| ManualRuleStoreError::Unavailable)
}

#[cfg(windows)]
pub(crate) fn save_manual_rules_in(
    _directory: &Path,
    _rules: &[ManualRule],
) -> Result<(), ManualRuleStoreError> {
    Err(ManualRuleStoreError::Unavailable)
}

#[cfg(not(windows))]
pub(crate) fn load_manual_rules_in(
    directory: &Path,
) -> Result<Vec<ManualRule>, ManualRuleStoreError> {
    let (rules, legacy_migrated) = read_manual_rules_document(directory)?;
    if legacy_migrated {
        return Ok(rules);
    }
    migrate_legacy_direct_rules_in(directory, rules)
}

#[cfg(windows)]
pub(crate) fn load_manual_rules_in(
    directory: &Path,
) -> Result<Vec<ManualRule>, ManualRuleStoreError> {
    let legacy =
        crate::direct_rule::load_direct_rules_in(directory).map_err(|error| match error {
            crate::direct_rule::DirectRuleStoreError::Unavailable => {
                ManualRuleStoreError::Unavailable
            }
            crate::direct_rule::DirectRuleStoreError::Corrupt => ManualRuleStoreError::Corrupt,
        })?;
    merge_legacy_direct_rules(Vec::new(), legacy)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{
        ManualRule, ManualRuleEditError, ManualRuleError, ManualRuleKind, append_manual_rules,
        load_manual_rules_in, replace_manual_rule, save_manual_rules_in,
    };

    #[test]
    fn qx_parameter_shapes_are_validated_and_normalized() {
        let cases = [
            (ManualRuleKind::Host, "EXAMPLE.com", "example.com"),
            (ManualRuleKind::HostSuffix, "example.com", "example.com"),
            (
                ManualRuleKind::HostWildcard,
                "*.Example.?om",
                "*.example.?om",
            ),
            (ManualRuleKind::HostKeyword, "google", "google"),
            (ManualRuleKind::UserAgent, "*abc?", "*abc?"),
            (ManualRuleKind::IpCidr, "192.168.0.1/24", "192.168.0.1/24"),
            (
                ManualRuleKind::Ip6Cidr,
                "2001:4860:4860::8888/32",
                "2001:4860:4860::8888/32",
            ),
            (ManualRuleKind::GeoIp, "us", "US"),
            (ManualRuleKind::IpAsn, "06185", "6185"),
            (ManualRuleKind::DstPort, "022", "22"),
        ];
        for (kind, input, expected) in cases {
            let rule = ManualRule::parse(kind, input, "Proxy").expect("valid QX parameter");
            assert_eq!(rule.conditions()[0].parameter(), expected);
        }
    }

    #[test]
    fn domain_rule_labels_match_imported_rule_terminology() {
        assert_eq!(ManualRuleKind::Host.display_label(), "DOMAIN");
        assert_eq!(ManualRuleKind::HostSuffix.display_label(), "DOMAIN-SUFFIX");
        assert_eq!(
            ManualRuleKind::HostWildcard.display_label(),
            "DOMAIN-WILDCARD"
        );
        assert_eq!(
            ManualRuleKind::HostKeyword.display_label(),
            "DOMAIN-KEYWORD"
        );
    }

    #[test]
    fn address_families_and_unsafe_values_are_rejected() {
        assert_eq!(
            ManualRule::parse(ManualRuleKind::IpCidr, "2001:db8::/32", "Proxy"),
            Err(ManualRuleError::InvalidIpv4Cidr)
        );
        assert_eq!(
            ManualRule::parse(ManualRuleKind::Ip6Cidr, "192.0.2.0/24", "Proxy"),
            Err(ManualRuleError::InvalidIpv6Cidr)
        );
        assert_eq!(
            ManualRule::parse(ManualRuleKind::Host, "https://example.com", "Proxy"),
            Err(ManualRuleError::InvalidDomain)
        );
        assert_eq!(
            ManualRule::parse(ManualRuleKind::HostKeyword, "a,b", "Proxy"),
            Err(ManualRuleError::InvalidKeyword)
        );
        assert_eq!(
            ManualRule::parse(ManualRuleKind::DstPort, "0", "DIRECT"),
            Err(ManualRuleError::InvalidDestinationPort)
        );
    }

    #[test]
    fn manual_rules_append_in_source_order_without_generated_fallbacks() {
        let mut profile = manis_profile::Profile::qx_default(
            manis_profile::SecretUrl::parse_https("https://example.invalid/subscription")
                .expect("fixture URL"),
        )
        .expect("fixture profile");
        let rules = vec![
            ManualRule::parse(ManualRuleKind::Host, "example.com", "DIRECT").expect("rule"),
            ManualRule::parse(ManualRuleKind::IpAsn, "13335", "Proxy").expect("rule"),
        ];
        append_manual_rules(&mut profile, &rules, manis_core::KernelKind::Mihomo)
            .expect("supported rules");
        let yaml = manis_profile::render_mihomo_yaml(&profile).expect("rendered profile");
        assert!(
            yaml.find("DOMAIN,example.com,DIRECT") < yaml.find("IP-ASN,13335,__MANIS_GLOBAL__")
        );
        assert!(!yaml.contains("GEOIP,CN,DIRECT"));
        assert!(!yaml.contains("MATCH,__MANIS_GLOBAL__"));
    }

    #[test]
    fn compound_domain_and_port_rule_compiles_as_an_exact_and_match() {
        let mut profile = manis_profile::Profile::qx_default(
            manis_profile::SecretUrl::parse_https("https://example.invalid/subscription")
                .expect("fixture URL"),
        )
        .expect("fixture profile");
        let rule = ManualRule::parse_conditions(
            vec![
                (ManualRuleKind::HostSuffix, "github.com".to_owned()),
                (ManualRuleKind::DstPort, "22".to_owned()),
            ],
            "DIRECT",
        )
        .expect("compound rule");
        append_manual_rules(&mut profile, &[rule], manis_core::KernelKind::Mihomo)
            .expect("supported rule");

        let yaml = manis_profile::render_mihomo_yaml(&profile).expect("rendered profile");
        assert!(yaml.contains("AND,((DOMAIN-SUFFIX,github.com),(DST-PORT,22)),DIRECT"));
    }

    #[cfg(not(windows))]
    #[test]
    fn rules_round_trip_through_private_storage() -> Result<(), Box<dyn std::error::Error>> {
        let root = std::env::temp_dir().join(format!("manis-manual-rules-{}", std::process::id()));
        if root.exists() {
            fs::remove_dir_all(&root)?;
        }
        fs::create_dir_all(&root)?;
        let rules = vec![
            ManualRule::parse_conditions(
                vec![
                    (ManualRuleKind::HostSuffix, "example.com".to_owned()),
                    (ManualRuleKind::DstPort, "22".to_owned()),
                ],
                "Proxy",
            )?,
            ManualRule::parse(ManualRuleKind::GeoIp, "US", "DIRECT")?,
        ];
        save_manual_rules_in(&root, &rules)?;
        assert_eq!(load_manual_rules_in(&root)?, rules);
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn replacing_a_manual_rule_preserves_order_and_ignores_itself_for_duplicates() {
        let first = ManualRule::parse(ManualRuleKind::HostSuffix, "example.com", "DIRECT")
            .expect("first rule");
        let second =
            ManualRule::parse(ManualRuleKind::DstPort, "22", "DIRECT").expect("second rule");
        let replacement = ManualRule::parse(ManualRuleKind::HostSuffix, "github.com", "DIRECT")
            .expect("replacement rule");
        let mut rules = vec![first.clone(), second.clone()];

        let previous = replace_manual_rule(&mut rules, 0, replacement.clone())
            .expect("distinct replacement should succeed");

        assert_eq!(previous, first);
        assert_eq!(rules, vec![replacement.clone(), second.clone()]);
        assert_eq!(
            replace_manual_rule(&mut rules, 0, replacement),
            Ok(rules[0].clone()),
            "saving an unchanged rule must not be treated as a duplicate"
        );
        assert_eq!(
            replace_manual_rule(&mut rules, 0, second),
            Err(ManualRuleEditError::Duplicate)
        );
        assert_eq!(
            replace_manual_rule(&mut rules, 9, first),
            Err(ManualRuleEditError::Missing)
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn legacy_direct_rules_migrate_once_into_manual_rules() -> Result<(), Box<dyn std::error::Error>>
    {
        use std::os::unix::fs::PermissionsExt;

        let root =
            std::env::temp_dir().join(format!("manis-manual-migration-{}", std::process::id()));
        if root.exists() {
            fs::remove_dir_all(&root)?;
        }
        fs::create_dir_all(&root)?;
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700))?;
        let legacy_path = root.join("direct-rules.state");
        fs::write(
            &legacy_path,
            "manis.direct-rules.v1\nport\t22\ndomain-suffix\tgithub.com",
        )?;
        fs::set_permissions(&legacy_path, fs::Permissions::from_mode(0o600))?;

        let migrated = load_manual_rules_in(&root)?;
        assert_eq!(migrated.len(), 2);
        assert_eq!(migrated[0].conditions()[0].kind(), ManualRuleKind::DstPort);
        assert_eq!(migrated[0].target(), "DIRECT");
        assert_eq!(
            migrated[1].conditions()[0].kind(),
            ManualRuleKind::HostSuffix
        );

        save_manual_rules_in(&root, &migrated[1..])?;
        let reloaded = load_manual_rules_in(&root)?;
        assert_eq!(reloaded, migrated[1..]);

        fs::remove_dir_all(root)?;
        Ok(())
    }

    impl std::fmt::Display for ManualRuleError {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(formatter, "{self:?}")
        }
    }

    impl std::error::Error for ManualRuleError {}
}
