use relay_core::{
    CompactNavigation, PolicyGroupId, PolicyWorkspaceState, ProxyId, RouteEvidence, WindowSizeClass,
};

const STREAMING: PolicyGroupId = PolicyGroupId("streaming");
const SEARCH: PolicyGroupId = PolicyGroupId("search");
const HK_01: ProxyId = ProxyId("hk-01");
const SG_02: ProxyId = ProxyId("sg-02");

#[test]
fn classifies_the_three_adaptive_widths() {
    assert_eq!(WindowSizeClass::for_width(720.0), WindowSizeClass::Compact);
    assert_eq!(WindowSizeClass::for_width(1060.0), WindowSizeClass::Medium);
    assert_eq!(WindowSizeClass::for_width(1420.0), WindowSizeClass::Wide);
}

#[test]
fn resizing_preserves_the_active_policy_and_node() {
    let mut state = PolicyWorkspaceState::demo();
    state.select_group(SEARCH);
    state.select_node(SG_02);

    state.resize(720.0);
    state.resize(1420.0);

    assert_eq!(state.selected_group, Some(SEARCH));
    assert_eq!(state.selected_node, Some(SG_02));
}

#[test]
fn compact_group_selection_opens_detail_and_back_returns_to_list() {
    let mut state = PolicyWorkspaceState::demo();
    state.resize(720.0);
    state.select_group(SEARCH);

    assert_eq!(state.compact_navigation, CompactNavigation::GroupDetail);

    state.navigate_back();
    assert_eq!(state.compact_navigation, CompactNavigation::GroupList);
}

#[test]
fn switching_groups_restores_each_groups_node_selection() {
    let mut state = PolicyWorkspaceState::demo();
    state.select_group(SEARCH);
    state.select_node(SG_02);
    state.select_group(STREAMING);
    state.select_node(HK_01);
    state.select_group(SEARCH);

    assert_eq!(state.selected_node, Some(SG_02));
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
            policy: STREAMING,
            proxy: HK_01,
        } if domain == "youtube.com"
    ));
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
