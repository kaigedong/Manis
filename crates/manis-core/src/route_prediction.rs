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
        domain: RouteDomain,
        rule: RoutingRule,
        target: RouteTarget,
    },
    NeedsConnection {
        domain: RouteDomain,
        blocking_rule: Option<RoutingRule>,
        reason: RoutePredictionReason,
    },
}

impl PolicyCatalog {
    #[must_use]
    pub fn predict_domain(&self, domain: &RouteDomain) -> DomainRoutePrediction {
        for rule in self.routing_rules().filter(|rule| !rule.disabled) {
            match domain_rule_match(rule, domain.as_str()) {
                RuleMatch::Matches => {
                    return DomainRoutePrediction::Matched {
                        domain: domain.clone(),
                        rule: rule.clone(),
                        target: self.route_target(&rule.target),
                    };
                }
                RuleMatch::DoesNotMatch => {}
                RuleMatch::NeedsConnectionContext => {
                    return DomainRoutePrediction::NeedsConnection {
                        domain: domain.clone(),
                        blocking_rule: Some(rule.clone()),
                        reason: RoutePredictionReason::RuleNeedsConnectionContext,
                    };
                }
            }
        }
        DomainRoutePrediction::NeedsConnection {
            domain: domain.clone(),
            blocking_rule: None,
            reason: RoutePredictionReason::NoMatchingRule,
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

fn domain_rule_match(rule: &RoutingRule, domain: &str) -> RuleMatch {
    let kind = rule.kind.to_ascii_uppercase();
    let payload = rule
        .payload
        .trim()
        .trim_end_matches('.')
        .to_ascii_lowercase();
    let matches = match kind.as_str() {
        "DOMAIN" => domain == payload,
        "DOMAIN-SUFFIX" => {
            let suffix = payload
                .strip_prefix("+.")
                .or_else(|| payload.strip_prefix('.'))
                .unwrap_or(&payload);
            domain == suffix
                || domain
                    .strip_suffix(suffix)
                    .is_some_and(|prefix| prefix.ends_with('.'))
        }
        "DOMAIN-KEYWORD" => !payload.is_empty() && domain.contains(&payload),
        "DOMAIN-WILDCARD" if payload.is_ascii() => wildcard_matches(&payload, domain),
        "MATCH" | "FINAL" => true,
        _ => return RuleMatch::NeedsConnectionContext,
    };
    if matches {
        RuleMatch::Matches
    } else {
        RuleMatch::DoesNotMatch
    }
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
