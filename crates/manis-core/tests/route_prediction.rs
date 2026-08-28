use manis_core::{
    DomainRoutePrediction, PolicyCandidateKind, PolicyCatalog, PolicyGroup, PolicyGroupId,
    PolicyGroupKind, PolicyNode, ProxyId, RouteDomain, RouteTarget, RoutingRule,
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
fn prediction_stops_at_an_earlier_rule_that_needs_connection_context() {
    let catalog = catalog(vec![
        rule(1, "PROCESS-NAME", "curl", "DIRECT"),
        rule(2, "DOMAIN-SUFFIX", "example.com", "Streaming"),
    ]);
    let domain = RouteDomain::parse("video.example.com").expect("valid domain");

    let prediction = catalog.predict_domain(&domain);

    assert!(matches!(
        prediction,
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
