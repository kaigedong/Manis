use std::{error::Error, fmt, net::IpAddr};

use crate::{PolicyCatalog, PolicyGroupId};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RouteDomain(String);

impl RouteDomain {
    /// Parses a destination hostname without accepting a URL, port, wildcard, or IP address.
    ///
    /// # Errors
    ///
    /// Returns [`RouteDomainError`] when the input is not a plain DNS hostname.
    pub fn parse(input: &str) -> Result<Self, RouteDomainError> {
        let value = input.trim();
        if value.is_empty() {
            return Err(RouteDomainError::Empty);
        }
        let value = value.strip_suffix('.').unwrap_or(value);
        if value.len() > 253 {
            return Err(RouteDomainError::TooLong);
        }
        if value.parse::<IpAddr>().is_ok() {
            return Err(RouteDomainError::IpAddress);
        }
        if !value.is_ascii()
            || value.split('.').any(|label| {
                label.is_empty()
                    || label.len() > 63
                    || label.starts_with('-')
                    || label.ends_with('-')
                    || !label
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
            })
        {
            return Err(RouteDomainError::InvalidFormat);
        }
        Ok(Self(value.to_ascii_lowercase()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RouteDomainError {
    Empty,
    TooLong,
    IpAddress,
    InvalidFormat,
}

impl fmt::Display for RouteDomainError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Empty => "route domain is empty",
            Self::TooLong => "route domain is too long",
            Self::IpAddress => "route domain must not be an IP address",
            Self::InvalidFormat => "route domain is not a plain DNS hostname",
        })
    }
}

impl Error for RouteDomainError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RouteQuery {
    domain: RouteDomain,
    port: u16,
    explicit_port: bool,
}

impl RouteQuery {
    pub const DEFAULT_PORT: u16 = 443;

    /// Parses a domain with an optional destination port. A missing port means HTTPS (`443`).
    ///
    /// # Errors
    ///
    /// Returns [`RouteQueryError`] when either the domain or port is invalid.
    pub fn parse(input: &str) -> Result<Self, RouteQueryError> {
        let value = input.trim();
        if value.contains("://") {
            return Err(RouteDomainError::InvalidFormat.into());
        }
        let (domain, port, explicit_port) = match value.split_once(':') {
            Some((domain, port)) if !domain.contains(':') => {
                let port = port
                    .parse::<u16>()
                    .ok()
                    .filter(|port| *port != 0)
                    .ok_or(RouteQueryError::InvalidPort)?;
                (RouteDomain::parse(domain)?, port, true)
            }
            Some(_) => return Err(RouteDomainError::InvalidFormat.into()),
            None => (RouteDomain::parse(value)?, Self::DEFAULT_PORT, false),
        };
        Ok(Self {
            domain,
            port,
            explicit_port,
        })
    }

    #[must_use]
    pub fn domain(&self) -> &RouteDomain {
        &self.domain
    }

    #[must_use]
    pub const fn port(&self) -> u16 {
        self.port
    }

    #[must_use]
    pub const fn has_explicit_port(&self) -> bool {
        self.explicit_port
    }

    fn from_domain(domain: RouteDomain) -> Self {
        Self {
            domain,
            port: Self::DEFAULT_PORT,
            explicit_port: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RouteQueryError {
    Domain(RouteDomainError),
    InvalidPort,
}

impl fmt::Display for RouteQueryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Domain(error) => error.fmt(formatter),
            Self::InvalidPort => formatter.write_str("route port must be between 1 and 65535"),
        }
    }
}

impl Error for RouteQueryError {}

impl From<RouteDomainError> for RouteQueryError {
    fn from(error: RouteDomainError) -> Self {
        Self::Domain(error)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoutingRule {
    pub index: u32,
    pub kind: String,
    pub payload: String,
    pub target: String,
    pub disabled: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RouteTarget {
    Policy(PolicyGroupId),
    Direct,
    Reject,
    Named(String),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RoutePredictionReason {
    RuleNeedsConnectionContext,
    NoMatchingRule,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DomainRoutePrediction {
    Matched {
        query: RouteQuery,
        rule: RoutingRule,
        target: RouteTarget,
        uncertain_rules: Vec<RoutingRule>,
    },
    NeedsConnection {
        query: RouteQuery,
        blocking_rule: Option<RoutingRule>,
        reason: RoutePredictionReason,
    },
}

impl PolicyCatalog {
    #[must_use]
    pub fn predict_domain(&self, domain: &RouteDomain) -> DomainRoutePrediction {
        self.predict_route(&RouteQuery::from_domain(domain.clone()))
    }

    #[must_use]
    pub fn predict_route(&self, query: &RouteQuery) -> DomainRoutePrediction {
        let mut uncertain_rules = Vec::new();
        for rule in self.routing_rules().filter(|rule| !rule.disabled) {
            match route_rule_match(rule, query) {
                RuleMatch::Matches => {
                    return DomainRoutePrediction::Matched {
                        query: query.clone(),
                        rule: rule.clone(),
                        target: self.route_target(&rule.target),
                        uncertain_rules,
                    };
                }
                RuleMatch::DoesNotMatch => {}
                RuleMatch::NeedsConnectionContext => {
                    uncertain_rules.push(rule.clone());
                }
            }
        }
        DomainRoutePrediction::NeedsConnection {
            query: query.clone(),
            blocking_rule: uncertain_rules.first().cloned(),
            reason: if uncertain_rules.is_empty() {
                RoutePredictionReason::NoMatchingRule
            } else {
                RoutePredictionReason::RuleNeedsConnectionContext
            },
        }
    }

    fn route_target(&self, target: &str) -> RouteTarget {
        match target.to_ascii_uppercase().as_str() {
            "DIRECT" => RouteTarget::Direct,
            "REJECT" | "REJECT-DROP" => RouteTarget::Reject,
            _ => self
                .iter()
                .find(|group| group.name == target || group.id.as_str() == target)
                .map_or_else(
                    || RouteTarget::Named(target.to_owned()),
                    |group| RouteTarget::Policy(group.id.clone()),
                ),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RuleMatch {
    Matches,
    DoesNotMatch,
    NeedsConnectionContext,
}

fn route_rule_match(rule: &RoutingRule, query: &RouteQuery) -> RuleMatch {
    let kind = canonical_rule_kind(&rule.kind);
    let payload = rule
        .payload
        .trim()
        .trim_end_matches('.')
        .to_ascii_lowercase();
    let matches = match kind.as_str() {
        "DOMAIN" => query.domain.as_str() == payload,
        "DOMAINSUFFIX" => {
            let suffix = payload
                .strip_prefix("+.")
                .or_else(|| payload.strip_prefix('.'))
                .unwrap_or(&payload);
            query.domain.as_str() == suffix
                || query
                    .domain
                    .as_str()
                    .strip_suffix(suffix)
                    .is_some_and(|prefix| prefix.ends_with('.'))
        }
        "DOMAINKEYWORD" => !payload.is_empty() && query.domain.as_str().contains(&payload),
        "DOMAINWILDCARD" if payload.is_ascii() => wildcard_matches(&payload, query.domain.as_str()),
        "DSTPORT" => match destination_port_matches(&payload, query.port) {
            Some(matches) => matches,
            None => return RuleMatch::NeedsConnectionContext,
        },
        "MATCH" | "FINAL" => true,
        _ => return RuleMatch::NeedsConnectionContext,
    };
    if matches {
        RuleMatch::Matches
    } else {
        RuleMatch::DoesNotMatch
    }
}

fn canonical_rule_kind(kind: &str) -> String {
    kind.chars()
        .filter(char::is_ascii_alphanumeric)
        .flat_map(char::to_uppercase)
        .collect()
}

fn destination_port_matches(payload: &str, port: u16) -> Option<bool> {
    let mut saw_segment = false;
    for segment in payload.split('/') {
        saw_segment = true;
        if destination_port_segment_matches(segment.trim(), port)? {
            return Some(true);
        }
    }
    saw_segment.then_some(false)
}

fn destination_port_segment_matches(segment: &str, port: u16) -> Option<bool> {
    if let Ok(expected) = segment.parse::<u16>() {
        return Some(port == expected);
    }
    let (start, end) = segment.split_once('-')?;
    let start = start.trim().parse::<u16>().ok()?;
    let end = end.trim().parse::<u16>().ok()?;
    Some(start <= port && port <= end)
}

fn wildcard_matches(pattern: &str, value: &str) -> bool {
    let pattern = pattern.as_bytes();
    let value = value.as_bytes();
    let mut previous = vec![false; value.len() + 1];
    previous[0] = true;

    for token in pattern {
        let mut current = vec![false; value.len() + 1];
        match token {
            b'*' => {
                current[0] = previous[0];
                for index in 1..=value.len() {
                    current[index] = previous[index] || current[index - 1];
                }
            }
            b'?' => {
                current[1..].copy_from_slice(&previous[..value.len()]);
            }
            literal => {
                for index in 1..=value.len() {
                    current[index] = previous[index - 1] && *literal == value[index - 1];
                }
            }
        }
        previous = current;
    }

    previous[value.len()]
}
