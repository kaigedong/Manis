use std::collections::BTreeMap;

use manis_core::{
    CompactNavigation, EmptyPolicyCatalog, ManagedPolicyGroup, ManagedPolicyIcon,
    ManagedPolicyStrategy, NodeAvailabilityFilter, NodeIdentity, NodeWorkspaceState,
    PolicyCandidateKind, PolicyCandidateMatcher, PolicyCatalog, PolicyGroup, PolicyGroupId,
    PolicyGroupKind, PolicyNode, PolicyRule, PolicyWorkspaceState, PrimaryWorkspace, ProxyId,
    ProxyMode, RouteEvidence, RoutingMode, WindowSizeClass,
};

fn streaming() -> PolicyGroupId {
    PolicyGroupId::new("streaming")
}

fn search() -> PolicyGroupId {
    PolicyGroupId::new("search")
}

fn hk_01() -> ProxyId {
    ProxyId::new("hk-01")
}

fn sg_02() -> ProxyId {
    ProxyId::new("sg-02")
}

#[test]
fn classifies_the_three_adaptive_widths() {
    assert_eq!(WindowSizeClass::for_width(720.0), WindowSizeClass::Compact);
    assert_eq!(WindowSizeClass::for_width(1060.0), WindowSizeClass::Medium);
    assert_eq!(WindowSizeClass::for_width(1420.0), WindowSizeClass::Wide);
}

#[test]
fn resizing_preserves_the_active_policy_and_node() {
    let mut state = PolicyWorkspaceState::demo();
    state.select_group(search());
    state.select_node(sg_02());

    state.resize(720.0);
    state.resize(1420.0);

    assert_eq!(state.selected_group, Some(search()));
    assert_eq!(state.selected_node, Some(sg_02()));
}

#[test]
fn compact_group_selection_opens_detail_and_back_returns_to_list() {
    let mut state = PolicyWorkspaceState::demo();
    state.resize(720.0);
    state.select_group(search());

    assert_eq!(state.compact_navigation, CompactNavigation::GroupDetail);

    state.navigate_back();
    assert_eq!(state.compact_navigation, CompactNavigation::GroupList);
}

#[test]
fn switching_groups_restores_each_groups_node_selection() {
    let mut state = PolicyWorkspaceState::demo();
    state.select_group(search());
    state.select_node(sg_02());
    state.select_group(streaming());
    state.select_node(hk_01());
    state.select_group(search());

    assert_eq!(state.selected_node, Some(sg_02()));
}

#[test]
fn local_route_result_is_explicitly_predicted() {
    let state = PolicyWorkspaceState::demo();

    let evidence = state.predict("youtube.com");

    assert!(matches!(
        evidence,
        RouteEvidence::Predicted {
            domain,
            rule: "DOMAIN-SUFFIX",
            policy,
            proxy,
        } if domain == "youtube.com" && policy == streaming() && proxy == hk_01()
    ));
}

#[test]
fn accepts_runtime_owned_policy_and_proxy_ids() {
    let policy = PolicyGroupId::new(format!("{}-{}", "real", "policy"));
    let proxy = ProxyId::new(format!("{}-{}", "real", "proxy"));
    let mut state = PolicyWorkspaceState::default();

    state.select_group(policy.clone());
    state.select_node(proxy.clone());
    state.select_group(PolicyGroupId::new("another-policy"));
    state.select_group(policy.clone());

    assert_eq!(state.selected_group, Some(policy));
    assert_eq!(state.selected_node, Some(proxy));
}

#[test]
fn policy_catalog_requires_a_group_and_preserves_runtime_data() -> Result<(), EmptyPolicyCatalog> {
    let group = PolicyGroup {
        id: PolicyGroupId::new(String::from("AI 自动选择")),
        name: String::from("AI 自动选择"),
        kind: PolicyGroupKind::UrlTest,
        target: String::from("新加坡 SG-02"),
        nodes: vec![PolicyNode {
            id: ProxyId::new(String::from("新加坡 SG-02")),
            name: String::from("新加坡 SG-02"),
            kind: PolicyCandidateKind::PolicyGroup,
            provider: Some(String::from("Provider A")),
            detail: String::from("VLESS"),
            latency_ms: Some(54),
            alive: Some(true),
        }],
        rules: vec![PolicyRule {
            index: 27,
            kind: String::from("DOMAIN-SUFFIX"),
            payload: String::from("openai.com"),
            hit_count: Some(9),
            disabled: false,
        }],
        rules_total: 1,
    };

    let catalog = PolicyCatalog::try_new(vec![group])?;
    let selected = catalog.select(Some(&PolicyGroupId::new("AI 自动选择")));

    assert_eq!(selected.target, "新加坡 SG-02");
    assert_eq!(selected.nodes[0].kind, PolicyCandidateKind::PolicyGroup);
    assert_eq!(selected.nodes[0].latency_ms, Some(54));
    assert_eq!(selected.rules[0].hit_count, Some(9));
    assert_eq!(catalog.iter().count(), 1);
    assert_eq!(PolicyCatalog::try_new(Vec::new()), Err(EmptyPolicyCatalog));
    Ok(())
}

#[test]
fn policy_catalog_applies_fresh_group_delays_and_automatic_winner() -> Result<(), EmptyPolicyCatalog>
{
    let group = PolicyGroup {
        id: PolicyGroupId::new("auto-hk"),
        name: "我的香港优选".to_owned(),
        kind: PolicyGroupKind::UrlTest,
        target: "HK-01".to_owned(),
        nodes: vec![
            PolicyNode {
                id: ProxyId::new("HK-01"),
                name: "HK-01".to_owned(),
                kind: PolicyCandidateKind::Node,
                provider: None,
                detail: "VLESS".to_owned(),
                latency_ms: Some(80),
                alive: Some(true),
            },
            PolicyNode {
                id: ProxyId::new("HK-02"),
                name: "HK-02".to_owned(),
                kind: PolicyCandidateKind::Node,
                provider: None,
                detail: "VLESS".to_owned(),
                latency_ms: None,
                alive: None,
            },
        ],
        rules: Vec::new(),
        rules_total: 0,
    };
    let mut catalog = PolicyCatalog::try_new(vec![group])?;
    let delays = BTreeMap::from([("HK-01".to_owned(), 0), ("HK-02".to_owned(), 31)]);

    assert!(catalog.apply_group_benchmark(&PolicyGroupId::new("auto-hk"), Some("HK-02"), &delays,));

    let selected = catalog.select(Some(&PolicyGroupId::new("auto-hk")));
    assert_eq!(selected.target, "HK-02");
    assert_eq!(selected.nodes[0].latency_ms, None);
    assert_eq!(selected.nodes[0].alive, Some(false));
    assert_eq!(selected.nodes[1].latency_ms, Some(31));
    assert_eq!(selected.nodes[1].alive, Some(true));
    assert!(!catalog.apply_group_benchmark(&PolicyGroupId::new("missing"), None, &delays,));
    Ok(())
}

#[test]
fn policy_catalog_records_validated_selector_targets_by_id_or_name()
-> Result<(), EmptyPolicyCatalog> {
    let mut catalog = PolicyCatalog::try_new(vec![
        PolicyGroup {
            id: PolicyGroupId::new("global-id"),
            name: "GLOBAL".to_owned(),
            kind: PolicyGroupKind::Selector,
            target: "DIRECT".to_owned(),
            nodes: vec![
                PolicyNode {
                    id: ProxyId::new("DIRECT"),
                    name: "DIRECT".to_owned(),
                    kind: PolicyCandidateKind::Node,
                    provider: None,
                    detail: "Direct".to_owned(),
                    latency_ms: None,
                    alive: None,
                },
                PolicyNode {
                    id: ProxyId::new("Proxy"),
                    name: "Proxy".to_owned(),
                    kind: PolicyCandidateKind::PolicyGroup,
                    provider: None,
                    detail: "Selector".to_owned(),
                    latency_ms: Some(38),
                    alive: Some(true),
                },
            ],
            rules: Vec::new(),
            rules_total: 0,
        },
        PolicyGroup {
            id: PolicyGroupId::new("auto"),
            name: "Auto".to_owned(),
            kind: PolicyGroupKind::UrlTest,
            target: "Proxy".to_owned(),
            nodes: vec![PolicyNode {
                id: ProxyId::new("Proxy"),
                name: "Proxy".to_owned(),
                kind: PolicyCandidateKind::Node,
                provider: None,
                detail: "VLESS".to_owned(),
                latency_ms: None,
                alive: None,
            }],
            rules: Vec::new(),
            rules_total: 0,
        },
    ])?;

    assert!(catalog.apply_selector_target("global-id", "Proxy"));
    assert_eq!(
        catalog
            .select(Some(&PolicyGroupId::new("global-id")))
            .target,
        "Proxy"
    );
    assert!(catalog.apply_selector_target("GLOBAL", "DIRECT"));
    assert_eq!(
        catalog
            .select(Some(&PolicyGroupId::new("global-id")))
            .target,
        "DIRECT"
    );
    assert!(!catalog.apply_selector_target("GLOBAL", "Missing"));
    assert!(!catalog.apply_selector_target("missing-group", "Proxy"));
    assert!(!catalog.apply_selector_target("Auto", "Proxy"));
    assert_eq!(
        catalog.select(Some(&PolicyGroupId::new("auto"))).target,
        "Proxy"
    );
    Ok(())
}

#[test]
fn only_selector_groups_allow_manual_node_selection() {
    assert!(PolicyGroupKind::Selector.allows_manual_selection());
    assert!(!PolicyGroupKind::UrlTest.allows_manual_selection());
    assert!(!PolicyGroupKind::Fallback.allows_manual_selection());
    assert!(!PolicyGroupKind::LoadBalance.allows_manual_selection());
    assert!(!PolicyGroupKind::Direct.allows_manual_selection());
    assert_eq!(PolicyGroupKind::UrlTest.label(), "自动选择");
    assert_eq!(PolicyGroupKind::Fallback.label(), "故障转移");
}

#[test]
fn replacing_a_data_source_keeps_size_but_resets_navigation_and_selection() {
    let mut state = PolicyWorkspaceState::demo();
    state.resize(720.0);
    state.select_group(search());

    state.replace_source_selection(
        PolicyGroupId::new("真实策略"),
        Some(ProxyId::new("真实节点")),
    );

    assert_eq!(state.size_class, WindowSizeClass::Compact);
    assert_eq!(state.compact_navigation, CompactNavigation::GroupList);
    assert_eq!(state.selected_group, Some(PolicyGroupId::new("真实策略")));
    assert_eq!(state.selected_node, Some(ProxyId::new("真实节点")));
}

#[test]
fn process_dependent_rule_requires_an_actual_connection() {
    let state = PolicyWorkspaceState::demo();

    let evidence = state.predict("process-dependent.example");

    assert!(matches!(
        evidence,
        RouteEvidence::NeedsConnection { reason, .. }
            if reason.contains("进程")
    ));
}

#[test]
fn primary_workspace_switches_between_policy_operation_and_configuration() {
    let mut active = PrimaryWorkspace::default();

    assert_eq!(active, PrimaryWorkspace::Policies);
    active = PrimaryWorkspace::Nodes;
    assert_eq!(active, PrimaryWorkspace::Nodes);
    active = PrimaryWorkspace::RoutingRules;
    assert_eq!(active, PrimaryWorkspace::RoutingRules);
    active = PrimaryWorkspace::Configuration;
    assert_eq!(active, PrimaryWorkspace::Configuration);
}

#[test]
fn primary_navigation_places_routing_rules_between_policies_and_runtime_diagnostics() {
    assert_eq!(
        PrimaryWorkspace::navigation_order(),
        &[
            PrimaryWorkspace::Nodes,
            PrimaryWorkspace::Policies,
            PrimaryWorkspace::RoutingRules,
            PrimaryWorkspace::Activity,
            PrimaryWorkspace::Logs,
            PrimaryWorkspace::Configuration,
        ]
    );
}

#[test]
fn proxy_mode_cycles_through_off_system_and_tun() {
    assert_eq!(ProxyMode::Off.next(), ProxyMode::System);
    assert_eq!(ProxyMode::System.next(), ProxyMode::Tun);
    assert_eq!(ProxyMode::Tun.next(), ProxyMode::Off);
    assert_eq!(ProxyMode::System.label(), "系统代理");
}

#[test]
fn selecting_the_active_proxy_mode_turns_it_off() {
    assert_eq!(ProxyMode::System.toggled(ProxyMode::System), ProxyMode::Off);
    assert_eq!(ProxyMode::Tun.toggled(ProxyMode::Tun), ProxyMode::Off);
}

#[test]
fn selecting_an_inactive_proxy_mode_activates_it() {
    assert_eq!(ProxyMode::Off.toggled(ProxyMode::System), ProxyMode::System);
    assert_eq!(ProxyMode::Off.toggled(ProxyMode::Tun), ProxyMode::Tun);
    assert_eq!(ProxyMode::System.toggled(ProxyMode::Tun), ProxyMode::Tun);
    assert_eq!(ProxyMode::Tun.toggled(ProxyMode::System), ProxyMode::System);
}

#[test]
fn toggling_off_from_off_stays_off() {
    assert_eq!(ProxyMode::Off.toggled(ProxyMode::Off), ProxyMode::Off);
    assert_eq!(ProxyMode::System.toggled(ProxyMode::Off), ProxyMode::Off);
}

#[test]
fn routing_mode_has_stable_labels_and_wire_values() {
    assert_eq!(RoutingMode::Direct.label(), "直连");
    assert_eq!(RoutingMode::Global.label(), "全局");
    assert_eq!(RoutingMode::Rule.label(), "规则");
    assert_eq!(RoutingMode::Direct.wire_value(), "direct");
    assert_eq!(RoutingMode::Global.wire_value(), "global");
    assert_eq!(RoutingMode::Rule.wire_value(), "rule");
    assert_eq!(
        RoutingMode::parse_wire_value("DIRECT"),
        Some(RoutingMode::Direct)
    );
    assert_eq!(
        RoutingMode::parse_wire_value(" global "),
        Some(RoutingMode::Global)
    );
    assert_eq!(
        RoutingMode::parse_wire_value("Rule"),
        Some(RoutingMode::Rule)
    );
    assert_eq!(RoutingMode::parse_wire_value("script"), None);
    assert_eq!(RoutingMode::default(), RoutingMode::Rule);
}

#[test]
fn node_workspace_filters_known_and_untested_availability() {
    let mut state = NodeWorkspaceState::default();

    assert!(state.includes(Some(true)));
    assert!(state.includes(Some(false)));
    assert!(state.includes(None));

    state.select_filter(NodeAvailabilityFilter::Available);
    assert!(state.includes(Some(true)));
    assert!(!state.includes(Some(false)));
    assert!(!state.includes(None));

    state.select_filter(NodeAvailabilityFilter::Unavailable);
    assert!(!state.includes(Some(true)));
    assert!(state.includes(Some(false)));
    assert!(!state.includes(None));

    state.select_filter(NodeAvailabilityFilter::Untested);
    assert!(!state.includes(Some(true)));
    assert!(!state.includes(Some(false)));
    assert!(state.includes(None));
}

#[test]
fn node_workspace_tracks_collapsed_source_groups_independently() {
    let mut state = NodeWorkspaceState::default();

    assert!(!state.is_group_collapsed("subscription:primary"));
    assert!(!state.is_group_collapsed("saved"));

    state.toggle_group("subscription:primary");
    assert!(state.is_group_collapsed("subscription:primary"));
    assert!(!state.is_group_collapsed("saved"));

    state.toggle_group("subscription:primary");
    assert!(!state.is_group_collapsed("subscription:primary"));

    state.replace_collapsed_groups(["subscription:one", "saved"]);
    assert_eq!(
        state.collapsed_group_ids().collect::<Vec<_>>(),
        vec!["saved", "subscription:one"]
    );
}

#[test]
fn managed_policy_validates_identity_and_name_matching() {
    let mut group = ManagedPolicyGroup::new("group-1", "香港自动").expect("valid group");
    assert_eq!(group.strategy, ManagedPolicyStrategy::Manual);
    assert_eq!(group.test_interval_secs, 600);
    assert_eq!(group.icon, ManagedPolicyIcon::None);
    assert!(group.matches("subscription:one", "Hong Kong 01"));

    group.strategy = ManagedPolicyStrategy::LowestLatency;
    group
        .set_matcher(PolicyCandidateMatcher::name_contains("hong kong").expect("valid matcher"))
        .expect("valid matcher update");
    assert!(group.matches("subscription:one", "Hong Kong 01"));
    assert!(group.matches("saved", "HONG KONG Backup"));
    assert!(!group.matches("subscription:one", "Tokyo Edge"));

    group.rename("HK · 最低延迟").expect("valid rename");
    assert_eq!(group.name, "HK · 最低延迟");
    assert!(ManagedPolicyGroup::new("../unsafe", "Unsafe").is_err());
    assert!(group.rename("\n").is_err());
}

#[test]
fn user_named_automatic_group_owns_a_validated_test_interval() {
    let mut group = ManagedPolicyGroup::new("group-interval", "我的香港优选").expect("valid group");
    group.strategy = ManagedPolicyStrategy::LowestLatency;

    group
        .set_test_interval_secs(300)
        .expect("supported interval");
    assert_eq!(group.name, "我的香港优选");
    assert_eq!(group.test_interval_secs, 300);
    assert!(group.set_test_interval_secs(0).is_err());
    assert!(group.set_test_interval_secs(86_401).is_err());
}

#[test]
fn managed_policy_tracks_explicit_candidates_and_cycles_icons() {
    let mut group = ManagedPolicyGroup::new("group-2", "手动节点").expect("valid group");
    group
        .set_matcher(PolicyCandidateMatcher::Explicit(BTreeSet::default()))
        .expect("valid matcher update");
    let tokyo = NodeIdentity::new("subscription:one", "Tokyo Edge").expect("valid node");
    let saved = NodeIdentity::new("saved", "Private Edge").expect("valid node");

    assert!(group.toggle_member(tokyo.clone()));
    assert!(group.toggle_member(saved.clone()));
    assert!(group.matches(&tokyo.source_id, &tokyo.node_name));
    assert!(group.matches(&saved.source_id, &saved.node_name));
    assert!(!group.matches("subscription:two", "Tokyo Edge"));
    assert!(!group.toggle_member(tokyo));
    assert_eq!(group.member_count(), 1);

    assert_eq!(ManagedPolicyIcon::None.next(), ManagedPolicyIcon::Bolt);
    assert_eq!(ManagedPolicyIcon::Bolt.next(), ManagedPolicyIcon::Globe);
    assert_eq!(ManagedPolicyIcon::Globe.next(), ManagedPolicyIcon::Shield);
    assert_eq!(ManagedPolicyIcon::Shield.next(), ManagedPolicyIcon::Compass);
    assert_eq!(ManagedPolicyIcon::Compass.next(), ManagedPolicyIcon::None);
    assert_eq!(
        ManagedPolicyIcon::parse_key("none"),
        Ok(ManagedPolicyIcon::None)
    );
}

use std::collections::BTreeSet;
