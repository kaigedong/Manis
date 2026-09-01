use crate::{PolicyRef, Rule, RuleCondition};

pub(crate) fn policy_name(policy: &PolicyRef) -> &str {
    match policy {
        PolicyRef::Direct => "DIRECT",
        PolicyRef::Reject => "REJECT",
        PolicyRef::Group(name) | PolicyRef::Proxy(name) => name.as_str(),
    }
}

pub(crate) fn render_rule(rule: &Rule) -> String {
    match rule {
        Rule::Domain { value, policy } => format!("DOMAIN,{value},{}", policy_name(policy)),
        Rule::DomainKeyword { value, policy } => {
            format!("DOMAIN-KEYWORD,{value},{}", policy_name(policy))
        }
        Rule::DomainSuffix { value, policy } => {
            format!("DOMAIN-SUFFIX,{value},{}", policy_name(policy))
        }
        Rule::DomainWildcard { value, policy } => {
            format!("DOMAIN-WILDCARD,{value},{}", policy_name(policy))
        }
        Rule::IpCidr {
            value,
            policy,
            no_resolve,
        } => {
            let kind = if value.contains(':') {
                "IP-CIDR6"
            } else {
                "IP-CIDR"
            };
            let suffix = if *no_resolve { ",no-resolve" } else { "" };
            format!("{kind},{value},{}{suffix}", policy_name(policy))
        }
        Rule::IpAsn {
            asn,
            policy,
            no_resolve,
        } => {
            let suffix = if *no_resolve { ",no-resolve" } else { "" };
            format!("IP-ASN,{asn},{}{suffix}", policy_name(policy))
        }
        Rule::GeoIp {
            country,
            policy,
            no_resolve,
        } => {
            let suffix = if *no_resolve { ",no-resolve" } else { "" };
            format!("GEOIP,{country},{}{suffix}", policy_name(policy))
        }
        Rule::DstPort { port, policy } => format!("DST-PORT,{port},{}", policy_name(policy)),
        Rule::All { conditions, policy } => {
            let conditions = conditions
                .iter()
                .map(render_rule_condition)
                .map(|condition| format!("({condition})"))
                .collect::<Vec<_>>()
                .join(",");
            format!("AND,({conditions}),{}", policy_name(policy))
        }
        Rule::Match { policy } => format!("MATCH,{}", policy_name(policy)),
    }
}

fn render_rule_condition(condition: &RuleCondition) -> String {
    match condition {
        RuleCondition::Domain(value) => format!("DOMAIN,{value}"),
        RuleCondition::DomainKeyword(value) => format!("DOMAIN-KEYWORD,{value}"),
        RuleCondition::DomainSuffix(value) => format!("DOMAIN-SUFFIX,{value}"),
        RuleCondition::DomainWildcard(value) => format!("DOMAIN-WILDCARD,{value}"),
        RuleCondition::IpCidr { value, no_resolve } => {
            let kind = if value.contains(':') {
                "IP-CIDR6"
            } else {
                "IP-CIDR"
            };
            let suffix = if *no_resolve { ",no-resolve" } else { "" };
            format!("{kind},{value}{suffix}")
        }
        RuleCondition::IpAsn { asn, no_resolve } => {
            let suffix = if *no_resolve { ",no-resolve" } else { "" };
            format!("IP-ASN,{asn}{suffix}")
        }
        RuleCondition::GeoIp {
            country,
            no_resolve,
        } => {
            let suffix = if *no_resolve { ",no-resolve" } else { "" };
            format!("GEOIP,{country}{suffix}")
        }
        RuleCondition::DstPort(port) => format!("DST-PORT,{port}"),
    }
}
