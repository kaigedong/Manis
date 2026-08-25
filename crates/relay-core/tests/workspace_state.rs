use relay_core::{
    CompactNavigation, ConfigurationSection, ConfigurationWorkspaceState, EmptyPolicyCatalog,
    NodeAvailabilityFilter, NodeWorkspaceState, PolicyCatalog, PolicyGroup, PolicyGroupId,
    PolicyNode, PolicyRule, PolicyWorkspaceState, PrimaryWorkspace, ProxyId, ProxyMode,
    RouteEvidence, WindowSizeClass,
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
        kind: String::from("URLTest"),
        target: String::from("新加坡 SG-02"),
        nodes: vec![PolicyNode {
            id: ProxyId::new(String::from("新加坡 SG-02")),
            name: String::from("新加坡 SG-02"),
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
    assert_eq!(selected.nodes[0].latency_ms, Some(54));
    assert_eq!(selected.rules[0].hit_count, Some(9));
    assert_eq!(catalog.iter().count(), 1);
    assert_eq!(PolicyCatalog::try_new(Vec::new()), Err(EmptyPolicyCatalog));
    Ok(())
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
    active = PrimaryWorkspace::Configuration;
    assert_eq!(active, PrimaryWorkspace::Configuration);
}

#[test]
fn primary_navigation_places_nodes_first_and_exposes_activity_and_logs() {
    assert_eq!(
        PrimaryWorkspace::navigation_order(),
        &[
            PrimaryWorkspace::Nodes,
            PrimaryWorkspace::Policies,
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
fn configuration_selection_tracks_only_safe_local_identifiers() {
    let mut state = ConfigurationWorkspaceState::default();

    assert_eq!(state.section, ConfigurationSection::Sources);
    assert_eq!(state.selected_rule, 0);

    state.select_section(ConfigurationSection::Rules);
    state.select_rule(3, 3);

    assert_eq!(state.section, ConfigurationSection::Rules);
    assert_eq!(state.selected_rule, 2);
}

#[test]
fn configuration_rule_selection_handles_an_empty_preview() {
    let mut state = ConfigurationWorkspaceState::default();

    state.select_rule(9, 0);

    assert_eq!(state.selected_rule, 0);
}
