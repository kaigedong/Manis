use super::{IpAddr, KernelKind, fmt};
use manis_profile::Name;

const MAX_PARAMETER_BYTES: usize = 1_024;
pub(crate) const MAX_CONDITIONS: usize = 4;
pub(crate) const LEGACY_GENERATED_PROXY_GROUP_NAME: &str = "Proxy";

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
    Final,
}

impl ManualRuleKind {
    pub(crate) const ALL: [Self; 11] = [
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
        Self::Final,
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
            Self::Final => "FINAL",
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
            Self::Final => "FINAL",
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
            Self::Final => "final",
        }
    }

    pub(super) fn from_storage_key(value: &str) -> Option<Self> {
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
            "final" => Self::Final,
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
    pub(super) kind: ManualRuleKind,
    pub(super) parameter: String,
}

impl ManualRuleCondition {
    pub(crate) const fn kind(&self) -> ManualRuleKind {
        self.kind
    }

    pub(crate) fn parameter(&self) -> &str {
        &self.parameter
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ManualRuleMatcher {
    Conditions(Vec<ManualRuleCondition>),
    Final,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ManualRule {
    matcher: ManualRuleMatcher,
    pub(super) target: Name,
    pub(super) enabled: bool,
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
        if conditions
            .iter()
            .any(|(kind, _parameter)| *kind == ManualRuleKind::Final)
        {
            if conditions.len() != 1 {
                return Err(ManualRuleError::FinalMustStandAlone);
            }
            if !conditions[0].1.trim().is_empty() {
                return Err(ManualRuleError::FinalHasNoParameter);
            }
            return Self::final_rule(target);
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
        Ok(Self {
            matcher: ManualRuleMatcher::Conditions(conditions),
            target,
            enabled: true,
        })
    }

    pub(crate) fn final_rule(target: &str) -> Result<Self, ManualRuleError> {
        let target = Name::parse(target).map_err(|_error| ManualRuleError::InvalidPolicy)?;
        Ok(Self {
            matcher: ManualRuleMatcher::Final,
            target,
            enabled: true,
        })
    }

    pub(crate) fn conditions(&self) -> &[ManualRuleCondition] {
        match &self.matcher {
            ManualRuleMatcher::Conditions(conditions) => conditions,
            ManualRuleMatcher::Final => &[],
        }
    }

    pub(crate) const fn is_final(&self) -> bool {
        matches!(&self.matcher, ManualRuleMatcher::Final)
    }

    pub(crate) fn target(&self) -> &str {
        self.target.as_str()
    }

    pub(crate) const fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub(crate) fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    pub(crate) fn same_definition(&self, other: &Self) -> bool {
        self.matcher == other.matcher && self.target == other.target
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
    FinalMustStandAlone,
    FinalHasNoParameter,
    FinalAlreadyExists,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ManualRuleEditError {
    Missing,
    Duplicate,
    FinalAlreadyExists,
}

pub(crate) fn replace_manual_rule(
    rules: &mut [ManualRule],
    index: usize,
    mut replacement: ManualRule,
) -> Result<ManualRule, ManualRuleEditError> {
    if index >= rules.len() {
        return Err(ManualRuleEditError::Missing);
    }
    if rules
        .iter()
        .enumerate()
        .any(|(candidate_index, candidate)| {
            candidate_index != index && candidate.same_definition(&replacement)
        })
    {
        return Err(ManualRuleEditError::Duplicate);
    }
    if replacement.is_final()
        && rules
            .iter()
            .enumerate()
            .any(|(candidate_index, candidate)| candidate_index != index && candidate.is_final())
    {
        return Err(ManualRuleEditError::FinalAlreadyExists);
    }
    replacement.enabled = rules[index].enabled;
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
            Self::Unavailable => formatter.write_str("manual routing rule store is unavailable"),
            Self::Corrupt => formatter.write_str("manual routing rule file is corrupt"),
        }
    }
}

impl std::error::Error for ManualRuleStoreError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ManualRuleCompileError {
    UnsupportedType(ManualRuleKind),
    CorruptValue,
    MultipleFinalRules,
}

impl fmt::Display for ManualRuleCompileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedType(kind) => write!(
                formatter,
                "the active kernel cannot represent a {} manual routing rule exactly",
                kind.qx_label()
            ),
            Self::CorruptValue => formatter.write_str("manual routing rule value is invalid"),
            Self::MultipleFinalRules => {
                formatter.write_str("only one FINAL routing rule may be configured")
            }
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
        ManualRuleKind::Final => Err(ManualRuleError::FinalHasNoParameter),
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
