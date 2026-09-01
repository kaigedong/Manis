use super::*;

#[test]
fn domain_keyword_rules_validate_and_render_to_mihomo_yaml() {
    let mut profile = Profile::qx_default(fixture_secret()).expect("default profile is valid");
    profile.rules.insert(
        0,
        Rule::DomainKeyword {
            value: "google".to_owned(),
            policy: global_exit_policy(),
        },
    );

    let yaml = render_mihomo_yaml(&profile).expect("profile should render");

    assert!(yaml.contains("- \"DOMAIN-KEYWORD,google,__MANIS_GLOBAL__\""));
    assert!(!yaml.contains("- \"MATCH,__MANIS_GLOBAL__\""));
}

#[test]
fn destination_port_rules_render_to_mihomo_yaml_without_a_generated_fallback() {
    let mut profile = Profile::qx_default(fixture_secret()).expect("default profile is valid");
    profile.rules.insert(
        0,
        Rule::DstPort {
            port: 22,
            policy: PolicyRef::Direct,
        },
    );

    let yaml = render_mihomo_yaml(&profile).expect("profile should render");

    assert!(yaml.contains("- \"DST-PORT,22,DIRECT\""));
    assert!(!yaml.contains("- \"MATCH,__MANIS_GLOBAL__\""));
}

#[test]
fn destination_port_rules_render_to_sing_box_json() {
    let vless = VlessProxy::parse_share_link(
        "vless://00000000-0000-4000-8000-000000000000@198.51.100.7:443?security=reality&encryption=none&pbk=fixture_reality-public-key&headerType=&fp=chrome&type=tcp&flow=xtls-rprx-vision&sni=cdn.example.invalid#Reality%20TCP",
    )
    .expect("Reality TCP fixture should parse");
    let mut profile = Profile::qx_sources(Vec::new(), vec![vless], 17_890)
        .expect("manual VLESS fixture should build a profile");
    profile.rules.insert(
        0,
        Rule::DstPort {
            port: 22,
            policy: PolicyRef::Direct,
        },
    );

    let json = render_sing_box_json(
        &profile,
        &SingBoxOptions::new("127.0.0.1:19090", "fixture-controller-secret"),
    )
    .expect("supported profile should render");

    let document: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(
        document["route"]["rules"][2],
        serde_json::json!({"port": [22], "action": "route", "outbound": "direct"})
    );
}

#[test]
fn domain_and_port_rules_render_exactly_for_both_kernels() {
    let mut mihomo_profile =
        Profile::qx_default(fixture_secret()).expect("default profile is valid");
    mihomo_profile.rules.insert(
        0,
        Rule::All {
            conditions: vec![
                manis_profile::RuleCondition::DomainSuffix("github.com".to_owned()),
                manis_profile::RuleCondition::DstPort(22),
            ],
            policy: PolicyRef::Direct,
        },
    );
    let yaml = render_mihomo_yaml(&mihomo_profile).expect("compound rule should render");
    assert!(yaml.contains("AND,((DOMAIN-SUFFIX,github.com),(DST-PORT,22)),DIRECT"));

    let vless = VlessProxy::parse_share_link(
        "vless://00000000-0000-4000-8000-000000000000@198.51.100.7:443?security=reality&encryption=none&pbk=fixture_reality-public-key&headerType=&fp=chrome&type=tcp&flow=xtls-rprx-vision&sni=cdn.example.invalid#Reality%20TCP",
    )
    .expect("Reality TCP fixture should parse");
    let mut sing_box_profile = Profile::qx_sources(Vec::new(), vec![vless], 17_890)
        .expect("manual VLESS fixture should build a profile");
    sing_box_profile.rules.insert(
        0,
        Rule::All {
            conditions: vec![
                manis_profile::RuleCondition::IpCidr {
                    value: "192.0.2.10/32".to_owned(),
                    no_resolve: true,
                },
                manis_profile::RuleCondition::DstPort(22),
            ],
            policy: PolicyRef::Direct,
        },
    );
    let json = render_sing_box_json(
        &sing_box_profile,
        &SingBoxOptions::new("127.0.0.1:19090", "fixture-controller-secret"),
    )
    .expect("compound rule should render");
    let document: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(
        document["route"]["rules"][2],
        serde_json::json!({
            "type": "logical", "mode": "and",
            "rules": [{"ip_cidr": ["192.0.2.10/32"]}, {"port": [22]}],
            "action": "route", "outbound": "direct"
        })
    );
}

#[test]
fn quantumult_x_rule_list_parses_rules_and_reports_actionable_diagnostics() {
    let parsed = QxRuleList::parse(
        r"
# comment
DOMAIN-KEYWORD,google,PROXY
DOMAIN-SUFFIX,githubusercontent.com,PROXY
DOMAIN,example.com,proxy
IP-CIDR,192.0.2.0/24,DIRECT
DOMAIN-SUFFIX,,PROXY
DOMAIN,bad.example,DIRECT,unexpected
",
    );

    assert_eq!(parsed.rules.len(), 3);
    assert_eq!(parsed.rules[0].kind, QxRuleKind::DomainKeyword);
    assert_eq!(parsed.rules[0].value, "google");
    assert_eq!(parsed.rules[0].source_policy.as_str(), "PROXY");
    assert_eq!(parsed.rules[1].kind, QxRuleKind::DomainSuffix);
    assert_eq!(parsed.rules[2].kind, QxRuleKind::Domain);
    assert_eq!(parsed.rules[2].source_policy.as_str(), "proxy");
    assert_eq!(parsed.source_policies().len(), 1);
    assert!(
        parsed
            .source_policies()
            .iter()
            .any(|name| name.as_str() == "PROXY")
    );
    assert_eq!(parsed.diagnostics.len(), 3);
    assert_eq!(parsed.diagnostics[0].line_number, 6);
    assert_eq!(
        parsed.diagnostics[0].kind,
        QxRuleDiagnosticKind::UnsupportedRuleType
    );
    assert_eq!(parsed.diagnostics[1].line_number, 7);
    assert_eq!(
        parsed.diagnostics[1].kind,
        QxRuleDiagnosticKind::InvalidValue
    );
    assert_eq!(parsed.diagnostics[2].line_number, 8);
    assert_eq!(
        parsed.diagnostics[2].kind,
        QxRuleDiagnosticKind::InvalidFieldCount
    );
}

#[test]
fn quantumult_x_host_aliases_match_domain_rules_case_insensitively() {
    for (host_kind, domain_kind, value) in [
        ("HOST", "DOMAIN", "service.example.com"),
        ("HOST-SUFFIX", "DOMAIN-SUFFIX", "example.com"),
        ("HOST-KEYWORD", "DOMAIN-KEYWORD", "example"),
    ] {
        let expected = QxRuleList::parse(&format!("{domain_kind},{value},Example"));
        assert_eq!(expected.rules.len(), 1);
        for spelling in [host_kind.to_owned(), host_kind.to_ascii_lowercase()] {
            let parsed = QxRuleList::parse(&format!("  {spelling}, {value}, Example  \r\n"));
            assert_eq!(
                parsed, expected,
                "QX alias {spelling} must retain its semantics"
            );
        }
    }
}

#[test]
fn quantumult_x_rule_list_maps_source_policies_to_local_profile_rules() {
    let parsed = QxRuleList::parse(
        r"
HOST-KEYWORD,google,PROXY
host-suffix,githubusercontent.com,PROXY
HoSt,example.com,proxy
",
    );
    assert!(parsed.diagnostics.is_empty());
    let proxy = Name::parse(MANIS_GLOBAL_GROUP_NAME).expect("valid internal group");
    let mapped_rules = parsed
        .to_profile_rules(|source| {
            (source.as_str().eq_ignore_ascii_case("PROXY")).then(|| PolicyRef::Group(proxy.clone()))
        })
        .expect("all fixture policies should map");

    let mut profile = Profile::qx_default(fixture_secret()).expect("default profile is valid");
    profile.rules.splice(0..0, mapped_rules);
    let yaml = render_mihomo_yaml(&profile).expect("mapped QX rules should render");

    assert!(yaml.contains("- \"DOMAIN-KEYWORD,google,__MANIS_GLOBAL__\""));
    assert!(yaml.contains("- \"DOMAIN-SUFFIX,githubusercontent.com,__MANIS_GLOBAL__\""));
    assert!(yaml.contains("- \"DOMAIN,example.com,__MANIS_GLOBAL__\""));
}

#[test]
fn quantumult_x_rule_list_requires_policy_mapping_before_import() {
    let parsed = QxRuleList::parse("DOMAIN-KEYWORD,google,PROXY\n");

    let error = parsed
        .to_profile_rules(|_source| None)
        .expect_err("unmapped source policy should be actionable");

    assert_eq!(error.source_policy().as_str(), "PROXY");
    assert!(error.to_string().contains("PROXY"));
}

#[test]
fn renderer_escapes_double_quoted_yaml_scalars() {
    let profile = Profile {
        mode: ProfileMode::Rule,
        mixed_port: 7891,
        log_level: LogLevel::Silent,
        store_selected: true,
        proxy_server_nameservers: vec![
            ProxyDnsServer::parse_https("https://223.5.5.5/dns-query").expect("valid DNS"),
        ],
        proxies: Vec::new(),
        providers: vec![ProxyProvider {
            name: Name::parse("subscription").expect("valid name"),
            source: manis_profile::ProxyProviderSource::Http(fixture_secret()),
            interval_secs: 86_400,
            path: "providers/subscription.yaml".to_owned(),
            health_check: HealthCheck {
                enabled: true,
                interval_secs: 600,
                url: "https://www.gstatic.com/generate_204?x=\"slash\\path".to_owned(),
            },
        }],
        groups: vec![PolicyGroup {
            name: Name::parse("Proxy").expect("valid group"),
            icon: None,
            kind: PolicyGroupKind::Select {
                proxies: vec![PolicyRef::Direct],
                use_providers: vec![Name::parse("subscription").expect("valid provider")],
                filter: None,
            },
        }],
        rules: vec![Rule::Match {
            policy: PolicyRef::Group(Name::parse("Proxy").expect("valid group")),
        }],
    };

    let yaml = render_mihomo_yaml(&profile).expect("profile renders");

    assert!(yaml.contains("log-level: \"silent\""));
    assert!(yaml.contains("url: \"https://www.gstatic.com/generate_204?x=\\\"slash\\\\path\""));
}
