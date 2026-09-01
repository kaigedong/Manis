use std::collections::BTreeSet;

use super::optional;
use crate::{
    LogLevel, OutboundProxy, PolicyGroup, PolicyGroupKind, PolicyRef, Profile, ProfileError,
    ProfileMode, Rule, RuleCondition, SingBoxOptions, VlessProxy, VlessSecurity, VlessTransport,
};
use serde_json::{Value, json};

pub(crate) fn render(profile: &Profile, options: &SingBoxOptions) -> Result<String, ProfileError> {
    if !profile.providers.is_empty() {
        return Err(ProfileError::UnsupportedKernelFeature(
            "subscription providers",
        ));
    }
    if profile
        .proxies
        .iter()
        .any(|proxy| matches!(proxy.name().as_str(), "GLOBAL" | "direct" | "block"))
        || profile
            .groups
            .iter()
            .any(|group| matches!(group.name.as_str(), "GLOBAL" | "direct" | "block"))
    {
        return Err(ProfileError::UnsupportedKernelFeature(
            "a reserved sing-box outbound tag",
        ));
    }
    for group in &profile.groups {
        let (providers, filter) = match &group.kind {
            PolicyGroupKind::Select {
                use_providers,
                filter,
                ..
            }
            | PolicyGroupKind::UrlTest {
                use_providers,
                filter,
                ..
            } => (use_providers, filter),
        };
        if !providers.is_empty() || filter.is_some() {
            return Err(ProfileError::UnsupportedKernelFeature(
                "provider-backed policy groups",
            ));
        }
    }

    let mut outbounds = vec![
        json!({"type": "direct", "tag": "direct"}),
        json!({"type": "block", "tag": "block"}),
    ];
    for proxy in &profile.proxies {
        let OutboundProxy::Vless(proxy) = proxy;
        outbounds.push(vless(proxy)?);
    }
    let default = profile
        .proxies
        .first()
        .ok_or(ProfileError::UnsupportedKernelFeature(
            "GLOBAL selection without manual proxy nodes",
        ))?;
    outbounds.push(json!({
        "type": "selector", "tag": "GLOBAL",
        "outbounds": profile.proxies.iter().map(|proxy| proxy.name().as_str()).collect::<Vec<_>>(),
        "default": default.name().as_str(), "interrupt_exist_connections": false,
    }));
    outbounds.extend(profile.groups.iter().map(group));
    let mut rules = vec![
        json!({"clash_mode": "Direct", "action": "route", "outbound": "direct"}),
        json!({"clash_mode": "Global", "action": "route", "outbound": "GLOBAL"}),
    ];
    if profile.mode == ProfileMode::Rule {
        for entry in &profile.rules {
            if !matches!(entry, Rule::Match { .. }) {
                rules.push(rule(entry)?);
            }
        }
    }
    let document = json!({
        "log": {
            "level": match profile.log_level { LogLevel::Silent => "error", LogLevel::Warning => "warn", LogLevel::Info => "info" },
            "timestamp": true,
        },
        "inbounds": [{"type": "mixed", "tag": "mixed-in", "listen": "127.0.0.1", "listen_port": profile.mixed_port}],
        "outbounds": outbounds,
        "route": {
            "rules": rules, "rule_set": rule_sets(profile),
            "final": sing_box_terminal_policy(profile), "auto_detect_interface": true,
        },
        "experimental": {
            "cache_file": {"enabled": true},
            "clash_api": {
                "external_controller": options.controller, "secret": options.secret,
                "default_mode": match profile.mode { ProfileMode::Direct => "Direct", ProfileMode::Global => "Global", ProfileMode::Rule => "Rule" },
            },
        },
    });
    serde_json::to_string_pretty(&document)
        .map(|mut json| {
            json.push('\n');
            json
        })
        .map_err(|_| ProfileError::Serialization("sing-box JSON"))
}

fn vless(proxy: &VlessProxy) -> Result<Value, ProfileError> {
    if !matches!(proxy.transport, VlessTransport::Tcp) {
        return Err(ProfileError::UnsupportedKernelFeature(
            "non-TCP VLESS transport",
        ));
    }
    let mut value = json!({
        "type": "vless", "tag": proxy.name.as_str(), "server": proxy.server,
        "server_port": proxy.port, "uuid": proxy.uuid,
    });
    optional(&mut value, "flow", proxy.flow.as_deref());
    optional(
        &mut value,
        "packet_encoding",
        proxy.packet_encoding.as_deref(),
    );
    if proxy.security != VlessSecurity::None {
        let mut tls = json!({"enabled": true});
        optional(&mut tls, "server_name", proxy.servername.as_deref());
        if proxy.skip_cert_verify {
            tls["insecure"] = json!(true);
        }
        if !proxy.alpn.is_empty() {
            tls["alpn"] = json!(proxy.alpn);
        }
        if let Some(fingerprint) = &proxy.client_fingerprint {
            tls["utls"] = json!({"enabled": true, "fingerprint": fingerprint});
        }
        if proxy.security == VlessSecurity::Reality {
            let public_key = proxy
                .reality_public_key
                .as_deref()
                .ok_or(ProfileError::InvalidVless)?;
            tls["reality"] = json!({
                "enabled": true, "public_key": public_key,
                "short_id": proxy.reality_short_id.as_deref().unwrap_or_default(),
            });
        }
        value["tls"] = tls;
    }
    Ok(value)
}

fn group(group: &PolicyGroup) -> Value {
    let mut value = json!({
        "type": match group.kind { PolicyGroupKind::Select { .. } => "selector", PolicyGroupKind::UrlTest { .. } => "urltest" },
        "tag": group.name.as_str(),
    });
    let proxies = match &group.kind {
        PolicyGroupKind::Select { proxies, .. } | PolicyGroupKind::UrlTest { proxies, .. } => {
            proxies
        }
    };
    value["outbounds"] = json!(proxies.iter().map(sing_box_policy_name).collect::<Vec<_>>());
    match &group.kind {
        PolicyGroupKind::Select { .. } => {
            if let Some(first) = proxies.first() {
                value["default"] = json!(sing_box_policy_name(first));
            }
        }
        PolicyGroupKind::UrlTest {
            url,
            interval_secs,
            tolerance,
            ..
        } => {
            value["url"] = json!(url);
            value["interval"] = json!(format!("{interval_secs}s"));
            value["tolerance"] = json!(tolerance.unwrap_or(50));
        }
    }
    value["interrupt_exist_connections"] = json!(false);
    value
}

fn rule(rule: &Rule) -> Result<Value, ProfileError> {
    let (mut value, policy) = match rule {
        Rule::Domain { value, policy } => (json!({"domain": [value]}), policy),
        Rule::DomainKeyword { value, policy } => (json!({"domain_keyword": [value]}), policy),
        Rule::DomainSuffix { value, policy } => (json!({"domain_suffix": [value]}), policy),
        Rule::DomainWildcard { value, policy } => (
            json!({"domain_regex": [domain_wildcard_regex(value)]}),
            policy,
        ),
        Rule::IpCidr { value, policy, .. } => (json!({"ip_cidr": [value]}), policy),
        Rule::IpAsn { .. } => {
            return Err(ProfileError::UnsupportedKernelFeature(
                "IP-ASN routing rules",
            ));
        }
        Rule::GeoIp {
            country, policy, ..
        } => (
            json!({"rule_set": format!("geoip-{}", country.to_ascii_lowercase())}),
            policy,
        ),
        Rule::DstPort { port, policy } => (json!({"port": [port]}), policy),
        Rule::All { conditions, policy } => (
            json!({
                "type": "logical", "mode": "and",
                "rules": conditions.iter().map(condition).collect::<Result<Vec<_>, _>>()?,
            }),
            policy,
        ),
        Rule::Match { policy } => (json!({}), policy),
    };
    value["action"] = json!("route");
    value["outbound"] = json!(sing_box_policy_name(policy));
    Ok(value)
}

fn condition(condition: &RuleCondition) -> Result<Value, ProfileError> {
    Ok(match condition {
        RuleCondition::Domain(value) => json!({"domain": [value]}),
        RuleCondition::DomainKeyword(value) => json!({"domain_keyword": [value]}),
        RuleCondition::DomainSuffix(value) => json!({"domain_suffix": [value]}),
        RuleCondition::DomainWildcard(value) => {
            json!({"domain_regex": [domain_wildcard_regex(value)]})
        }
        RuleCondition::IpCidr { value, .. } => json!({"ip_cidr": [value]}),
        RuleCondition::IpAsn { .. } => {
            return Err(ProfileError::UnsupportedKernelFeature(
                "IP-ASN routing rules",
            ));
        }
        RuleCondition::GeoIp { country, .. } => {
            json!({"rule_set": format!("geoip-{}", country.to_ascii_lowercase())})
        }
        RuleCondition::DstPort(port) => json!({"port": [port]}),
    })
}

fn rule_sets(profile: &Profile) -> Vec<Value> {
    let mut countries = BTreeSet::new();
    for rule in &profile.rules {
        match rule {
            Rule::GeoIp { country, .. } => {
                countries.insert(country.to_ascii_lowercase());
            }
            Rule::All { conditions, .. } => {
                for condition in conditions {
                    match condition {
                        RuleCondition::GeoIp { country, .. } => {
                            countries.insert(country.to_ascii_lowercase());
                        }
                        RuleCondition::Domain(_)
                        | RuleCondition::DomainKeyword(_)
                        | RuleCondition::DomainSuffix(_)
                        | RuleCondition::DomainWildcard(_)
                        | RuleCondition::IpCidr { .. }
                        | RuleCondition::IpAsn { .. }
                        | RuleCondition::DstPort(_) => {}
                    }
                }
            }
            Rule::Domain { .. }
            | Rule::DomainKeyword { .. }
            | Rule::DomainSuffix { .. }
            | Rule::DomainWildcard { .. }
            | Rule::IpCidr { .. }
            | Rule::IpAsn { .. }
            | Rule::DstPort { .. }
            | Rule::Match { .. } => {}
        }
    }
    countries.iter().map(|country| json!({
        "type": "remote", "tag": format!("geoip-{country}"), "format": "binary",
        "url": format!("https://raw.githubusercontent.com/SagerNet/sing-geoip/rule-set/geoip-{country}.srs"),
        "update_interval": "1d",
    })).collect()
}

fn domain_wildcard_regex(value: &str) -> String {
    let mut regex = String::from("^");
    for character in value.chars() {
        match character {
            '*' => regex.push_str(".*"),
            '?' => regex.push('.'),
            '.' | '+' | '(' | ')' | '[' | ']' | '{' | '}' | '^' | '$' | '|' | '\\' => {
                regex.push('\\');
                regex.push(character);
            }
            _ => regex.push(character),
        }
    }
    regex.push('$');
    regex
}

fn sing_box_terminal_policy(profile: &Profile) -> &str {
    profile
        .rules
        .iter()
        .rev()
        .find_map(|rule| match rule {
            Rule::Match { policy } => Some(sing_box_policy_name(policy)),
            _ => None,
        })
        .unwrap_or("direct")
}

fn sing_box_policy_name(policy: &PolicyRef) -> &str {
    match policy {
        PolicyRef::Direct => "direct",
        PolicyRef::Reject => "block",
        PolicyRef::Group(name) | PolicyRef::Proxy(name) => name.as_str(),
    }
}
