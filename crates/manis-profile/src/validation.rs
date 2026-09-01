use std::{
    collections::HashSet,
    path::{Component, Path},
};

use crate::{
    MANIS_GLOBAL_GROUP_NAME, Name, PolicyGroup, PolicyGroupKind, PolicyRef, ProfileError,
    ProxyDnsServer, Rule, RuleCondition, UserPolicyGroup, UserPolicyGroupKind, is_https_url,
    is_plain_value,
};

pub(crate) fn compile_user_groups(
    user_groups: Vec<UserPolicyGroup>,
    provider_names: &[Name],
    proxy_names: &HashSet<Name>,
    test_url: &str,
) -> Result<Vec<PolicyGroup>, ProfileError> {
    let group_names = user_groups
        .iter()
        .map(|group| group.name.clone())
        .collect::<HashSet<_>>();
    if group_names.len() != user_groups.len() {
        return Err(ProfileError::DuplicateName);
    }
    user_groups
        .into_iter()
        .map(|group| {
            if group.name.as_str().eq_ignore_ascii_case("GLOBAL")
                || group.name.as_str() == MANIS_GLOBAL_GROUP_NAME
            {
                return Err(ProfileError::InvalidValue("reserved proxy group name"));
            }
            if group.provider_indexes.is_empty()
                && group.direct_proxies.is_empty()
                && group.direct_policies.is_empty()
            {
                return Err(ProfileError::InvalidValue("user proxy group"));
            }
            if group
                .icon
                .as_deref()
                .is_some_and(|value| !is_group_metadata(value))
                || group
                    .filter
                    .as_deref()
                    .is_some_and(|value| !is_group_metadata(value))
            {
                return Err(ProfileError::InvalidValue("user proxy group"));
            }

            let mut seen_providers = HashSet::new();
            let mut use_providers = Vec::with_capacity(group.provider_indexes.len());
            for index in group.provider_indexes {
                let Some(provider) = provider_names.get(index) else {
                    return Err(ProfileError::DanglingReference);
                };
                if !seen_providers.insert(index) {
                    return Err(ProfileError::DuplicateName);
                }
                use_providers.push(provider.clone());
            }

            let mut seen_proxies = HashSet::new();
            let mut proxies = Vec::with_capacity(group.direct_proxies.len());
            for name in group.direct_proxies {
                if !proxy_names.contains(&name) {
                    return Err(ProfileError::DanglingReference);
                }
                if !seen_proxies.insert(name.clone()) {
                    return Err(ProfileError::DuplicateName);
                }
                proxies.push(PolicyRef::Proxy(name));
            }
            let global_exit_name = Name::parse(MANIS_GLOBAL_GROUP_NAME)?;
            let mut seen_policies = HashSet::new();
            for policy in group.direct_policies {
                if let PolicyRef::Group(name) = &policy
                    && (name == &group.name
                        || (!group_names.contains(name) && name != &global_exit_name))
                {
                    return Err(ProfileError::DanglingReference);
                }
                if matches!(policy, PolicyRef::Proxy(_)) || !seen_policies.insert(policy.clone()) {
                    return Err(ProfileError::DuplicateName);
                }
                proxies.push(policy);
            }

            let kind = match group.kind {
                UserPolicyGroupKind::Select => PolicyGroupKind::Select {
                    proxies,
                    use_providers,
                    filter: group.filter,
                },
                UserPolicyGroupKind::UrlTest {
                    tolerance,
                    interval_secs,
                } => PolicyGroupKind::UrlTest {
                    proxies,
                    use_providers,
                    filter: group.filter,
                    url: test_url.to_owned(),
                    interval_secs,
                    tolerance: Some(tolerance),
                },
            };
            Ok(PolicyGroup {
                name: group.name,
                icon: group.icon,
                kind,
            })
        })
        .collect()
}

fn validate_policy_refs(
    policies: &[PolicyRef],
    groups: &HashSet<&Name>,
    proxies: &HashSet<&Name>,
) -> Result<(), ProfileError> {
    for policy in policies {
        validate_policy_ref(policy, groups, proxies)?;
    }
    Ok(())
}

pub(crate) fn validate_groups(
    groups: &[PolicyGroup],
    group_names: &HashSet<&Name>,
    proxy_names: &HashSet<&Name>,
    provider_names: &HashSet<&Name>,
) -> Result<(), ProfileError> {
    for group in groups {
        if group
            .icon
            .as_deref()
            .is_some_and(|value| !is_group_metadata(value))
        {
            return Err(ProfileError::InvalidValue("proxy group icon"));
        }
        match &group.kind {
            PolicyGroupKind::Select {
                proxies,
                use_providers,
                filter,
            } => {
                if proxies.is_empty() && use_providers.is_empty() {
                    return Err(ProfileError::InvalidValue("select group"));
                }
                validate_group_filter(filter.as_deref())?;
                validate_policy_refs(proxies, group_names, proxy_names)?;
                validate_provider_refs(use_providers, provider_names)?;
            }
            PolicyGroupKind::UrlTest {
                proxies,
                use_providers,
                filter,
                url,
                interval_secs,
                tolerance: _,
            } => {
                if (proxies.is_empty() && use_providers.is_empty())
                    || *interval_secs == 0
                    || !is_https_url(url)
                {
                    return Err(ProfileError::InvalidValue("url-test group"));
                }
                validate_group_filter(filter.as_deref())?;
                validate_policy_refs(proxies, group_names, proxy_names)?;
                validate_provider_refs(use_providers, provider_names)?;
            }
        }
    }
    Ok(())
}

fn validate_group_filter(filter: Option<&str>) -> Result<(), ProfileError> {
    if filter.is_some_and(|value| !is_group_metadata(value)) {
        Err(ProfileError::InvalidValue("proxy group filter"))
    } else {
        Ok(())
    }
}

fn validate_policy_ref(
    policy: &PolicyRef,
    groups: &HashSet<&Name>,
    proxies: &HashSet<&Name>,
) -> Result<(), ProfileError> {
    let known = match policy {
        PolicyRef::Direct | PolicyRef::Reject => true,
        PolicyRef::Group(name) => groups.contains(name),
        PolicyRef::Proxy(name) => proxies.contains(name),
    };
    known.then_some(()).ok_or(ProfileError::DanglingReference)
}

pub(crate) fn validate_rule(
    rule: &Rule,
    groups: &HashSet<&Name>,
    proxies: &HashSet<&Name>,
) -> Result<(), ProfileError> {
    let policy = match rule {
        Rule::Domain { value, policy }
        | Rule::DomainKeyword { value, policy }
        | Rule::DomainSuffix { value, policy }
        | Rule::DomainWildcard { value, policy } => {
            if !is_rule_value(value) {
                return Err(ProfileError::InvalidValue("domain rule"));
            }
            policy
        }
        Rule::IpCidr { value, policy, .. } => {
            if !is_ip_cidr(value) {
                return Err(ProfileError::InvalidValue("IP-CIDR rule"));
            }
            policy
        }
        Rule::IpAsn { asn, policy, .. } => {
            if *asn == 0 {
                return Err(ProfileError::InvalidValue("IP-ASN rule"));
            }
            policy
        }
        Rule::GeoIp {
            country, policy, ..
        } => {
            if !is_rule_value(country) {
                return Err(ProfileError::InvalidValue("GEOIP rule"));
            }
            policy
        }
        Rule::DstPort { port, policy } => {
            if *port == 0 {
                return Err(ProfileError::InvalidValue("destination port rule"));
            }
            policy
        }
        Rule::All { conditions, policy } => {
            if conditions.len() < 2 {
                return Err(ProfileError::InvalidValue("compound rule"));
            }
            for condition in conditions {
                validate_rule_condition(condition)?;
            }
            policy
        }
        Rule::Match { policy } => policy,
    };
    validate_policy_ref(policy, groups, proxies)
}

fn validate_provider_refs(providers: &[Name], known: &HashSet<&Name>) -> Result<(), ProfileError> {
    if providers.iter().all(|name| known.contains(name)) {
        Ok(())
    } else {
        Err(ProfileError::DanglingReference)
    }
}

pub(crate) fn default_proxy_dns_servers() -> Vec<ProxyDnsServer> {
    [
        "https://223.5.5.5/dns-query",
        "https://1.12.12.12/dns-query",
    ]
    .into_iter()
    .map(|value| ProxyDnsServer::parse_https(value).expect("built-in proxy DNS must be valid"))
    .collect()
}

fn is_group_metadata(value: &str) -> bool {
    is_plain_value(value, 1024)
}

pub(crate) fn is_rule_value(value: &str) -> bool {
    is_plain_value(value, 1024) && !value.contains(',')
}

fn validate_rule_condition(condition: &RuleCondition) -> Result<(), ProfileError> {
    match condition {
        RuleCondition::Domain(value)
        | RuleCondition::DomainKeyword(value)
        | RuleCondition::DomainSuffix(value)
        | RuleCondition::DomainWildcard(value) => {
            if !is_rule_value(value) {
                return Err(ProfileError::InvalidValue("compound domain rule"));
            }
        }
        RuleCondition::IpCidr { value, .. } => {
            if !is_ip_cidr(value) {
                return Err(ProfileError::InvalidValue("compound IP-CIDR rule"));
            }
        }
        RuleCondition::IpAsn { asn, .. } => {
            if *asn == 0 {
                return Err(ProfileError::InvalidValue("compound IP-ASN rule"));
            }
        }
        RuleCondition::GeoIp { country, .. } => {
            if !is_rule_value(country) {
                return Err(ProfileError::InvalidValue("compound GEOIP rule"));
            }
        }
        RuleCondition::DstPort(port) => {
            if *port == 0 {
                return Err(ProfileError::InvalidValue("compound destination port rule"));
            }
        }
    }
    Ok(())
}

fn is_ip_cidr(value: &str) -> bool {
    let Some((address, prefix)) = value.split_once('/') else {
        return false;
    };
    if value.matches('/').count() != 1 {
        return false;
    }
    let Ok(address) = address.parse::<std::net::IpAddr>() else {
        return false;
    };
    let Ok(prefix) = prefix.parse::<u8>() else {
        return false;
    };
    match address {
        std::net::IpAddr::V4(_) => prefix <= 32,
        std::net::IpAddr::V6(_) => prefix <= 128,
    }
}

pub(crate) fn is_safe_relative_path(value: &str) -> bool {
    !value.is_empty()
        && !value.chars().any(char::is_control)
        && !Path::new(value).is_absolute()
        && Path::new(value)
            .components()
            .all(|component| matches!(component, Component::CurDir | Component::Normal(_)))
}
