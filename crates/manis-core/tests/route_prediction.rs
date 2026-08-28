use manis_core::{
    DomainRoutePrediction, PolicyCandidateKind, PolicyCatalog, PolicyGroup, PolicyGroupId,
    PolicyGroupKind, PolicyNode, ProxyId, RouteDomain, RouteQuery, RouteTarget, RoutingRule,
};

fn policy(name: &str, target: &str) -> PolicyGroup {
    PolicyGroup {
        id: PolicyGroupId::new(name),
        name: name.to_owned(),
        kind: PolicyGroupKind::Selector,
        target: target.to_owned(),
        nodes: vec![PolicyNode {
            id: ProxyId::new(target),
            name: target.to_owned(),
            kind: PolicyCandidateKind::Node,
            provider: None,
            detail: "fixture".to_owned(),
            latency_ms: None,
            alive: None,
        }],
        rules: Vec::new(),
        rules_total: 0,
    }
}

fn rule(index: u32, kind: &str, payload: &str, target: &str) -> RoutingRule {
    RoutingRule {
        index,
        kind: kind.to_owned(),
        payload: payload.to_owned(),
        target: target.to_owned(),
        disabled: false,
    }
}

fn catalog(rules: Vec<RoutingRule>) -> PolicyCatalog {
    PolicyCatalog::try_new_with_rules(
        vec![
            policy("Streaming", "Hong Kong"),
            policy("Search", "Singapore"),
        ],
        rules,
    )
    .expect("fixture catalog")
}

#[test]
fn route_domain_normalizes_case_whitespace_and_a_trailing_root_dot() {
    let domain = RouteDomain::parse("  WWW.Example.COM.  ").expect("valid domain");

    assert_eq!(domain.as_str(), "www.example.com");
}

#[test]
fn route_domain_rejects_urls_ip_addresses_and_malformed_labels() {
    for input in [
        "https://example.com",
        "192.0.2.1",
        "example.com/path",
        "-edge.example.com",
        "edge..example.com",
    ] {
        assert!(RouteDomain::parse(input).is_err(), "accepted {input}");
    }
}

#[test]
fn route_query_defaults_to_https_and_accepts_an_explicit_port() {
    let default_https = RouteQuery::parse("google.com").expect("valid route query");
    let explicit_ssh = RouteQuery::parse("github.com:22").expect("valid route query");

    assert_eq!(default_https.domain().as_str(), "google.com");
    assert_eq!(default_https.port(), 443);
    assert!(!default_https.has_explicit_port());
    assert_eq!(explicit_ssh.domain().as_str(), "github.com");
    assert_eq!(explicit_ssh.port(), 22);
    assert!(explicit_ssh.has_explicit_port());

    for input in ["google.com:", "google.com:0", "google.com:65536"] {
        assert!(RouteQuery::parse(input).is_err(), "accepted {input}");
    }
}

#[test]
fn default_https_port_skips_an_earlier_ssh_rule_and_matches_the_domain() {
    let catalog = catalog(vec![
        rule(1, "DstPort", "22", "DIRECT"),
        rule(2, "DomainSuffix", "google.com", "Search"),
        rule(3, "MATCH", "", "DIRECT"),
    ]);
    let query = RouteQuery::parse("google.com").expect("valid route query");

    assert!(matches!(
        catalog.predict_route(&query),
        DomainRoutePrediction::Matched {
            rule,
            target: RouteTarget::Policy(policy),
            uncertain_rules,
            ..
        } if rule.index == 2
            && policy == PolicyGroupId::new("Search")
            && uncertain_rules.is_empty()
    ));
}

#[test]
fn slash_separated_destination_ports_follow_mihomo_format() {
    let catalog = catalog(vec![
        rule(1, "DstPort", "80/8080/443/8443", "Search"),
        rule(2, "MATCH", "", "DIRECT"),
    ]);
    let query = RouteQuery::parse("google.com").expect("valid route query");

    assert!(matches!(
        catalog.predict_route(&query),
        DomainRoutePrediction::Matched {
            rule,
            target: RouteTarget::Policy(policy),
            ..
        } if rule.index == 1 && policy == PolicyGroupId::new("Search")
    ));
}

#[test]
fn explicit_ssh_port_matches_the_earlier_port_rule() {
    let catalog = catalog(vec![
        rule(1, "DST-PORT", "22", "DIRECT"),
        rule(2, "DOMAIN-SUFFIX", "github.com", "Search"),
    ]);
    let query = RouteQuery::parse("github.com:22").expect("valid route query");

    assert!(matches!(
        catalog.predict_route(&query),
        DomainRoutePrediction::Matched {
            rule,
            target: RouteTarget::Direct,
            uncertain_rules,
            ..
        } if rule.index == 1 && uncertain_rules.is_empty()
    ));
}

#[test]
fn prediction_uses_the_first_matching_rule_across_all_policy_groups() {
    let catalog = catalog(vec![
        rule(8, "DOMAIN-SUFFIX", "example.com", "Search"),
        rule(3, "DOMAIN", "video.example.com", "Streaming"),
        rule(99, "MATCH", "", "DIRECT"),
    ]);
    let domain = RouteDomain::parse("video.example.com").expect("valid domain");

    let prediction = catalog.predict_domain(&domain);

    assert!(matches!(
        prediction,
        DomainRoutePrediction::Matched { rule, target: RouteTarget::Policy(policy), .. }
            if rule.index == 3 && policy == PolicyGroupId::new("Streaming")
    ));
}

#[test]
fn domain_suffix_matching_respects_label_boundaries() {
    let catalog = catalog(vec![
        rule(1, "DOMAIN-SUFFIX", "example.com", "Search"),
        rule(2, "MATCH", "", "DIRECT"),
    ]);

    let subdomain = RouteDomain::parse("api.example.com").expect("valid domain");
    let lookalike = RouteDomain::parse("notexample.com").expect("valid domain");

    assert!(matches!(
        catalog.predict_domain(&subdomain),
        DomainRoutePrediction::Matched { rule, .. } if rule.index == 1
    ));
    assert!(matches!(
        catalog.predict_domain(&lookalike),
        DomainRoutePrediction::Matched { rule, target: RouteTarget::Direct, .. }
            if rule.index == 2
    ));
}

#[test]
fn domain_wildcards_follow_mihomo_star_and_question_mark_semantics() {
    let catalog = catalog(vec![
        rule(1, "DOMAIN-WILDCARD", "video?.*.example.com", "Streaming"),
        rule(2, "MATCH", "", "REJECT"),
    ]);

    let matching = RouteDomain::parse("video1.cdn.example.com").expect("valid domain");
    let missing_question_mark_character =
        RouteDomain::parse("video.cdn.example.com").expect("valid domain");

    assert!(matches!(
        catalog.predict_domain(&matching),
        DomainRoutePrediction::Matched { rule, .. } if rule.index == 1
    ));
    assert!(matches!(
        catalog.predict_domain(&missing_question_mark_character),
        DomainRoutePrediction::Matched {
            target: RouteTarget::Reject,
            ..
        }
    ));
}

#[test]
fn prediction_reports_an_earlier_context_rule_but_keeps_the_domain_result() {
    let catalog = catalog(vec![
        rule(1, "PROCESS-NAME", "curl", "DIRECT"),
        rule(2, "DOMAIN-SUFFIX", "example.com", "Streaming"),
    ]);
    let query = RouteQuery::parse("video.example.com").expect("valid route query");

    let prediction = catalog.predict_route(&query);

    assert!(matches!(
        prediction,
        DomainRoutePrediction::Matched { rule, uncertain_rules, .. }
            if rule.index == 2
                && uncertain_rules.len() == 1
                && uncertain_rules[0].index == 1
                && uncertain_rules[0].kind == "PROCESS-NAME"
    ));
}

#[test]
fn prediction_still_requires_connection_when_only_context_rules_can_decide() {
    let catalog = catalog(vec![rule(1, "PROCESS-NAME", "curl", "DIRECT")]);
    let query = RouteQuery::parse("video.example.com").expect("valid route query");

    assert!(matches!(
        catalog.predict_route(&query),
        DomainRoutePrediction::NeedsConnection { blocking_rule: Some(rule), .. }
            if rule.index == 1 && rule.kind == "PROCESS-NAME"
    ));
}

#[test]
fn disabled_context_rules_do_not_block_a_domain_prediction() {
    let mut disabled = rule(1, "PROCESS-NAME", "curl", "DIRECT");
    disabled.disabled = true;
    let catalog = catalog(vec![
        disabled,
        rule(2, "DOMAIN-KEYWORD", "example", "Search"),
    ]);
    let domain = RouteDomain::parse("video.example.com").expect("valid domain");

    assert!(matches!(
        catalog.predict_domain(&domain),
        DomainRoutePrediction::Matched { rule, .. } if rule.index == 2
    ));
}
