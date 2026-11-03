use super::{
    BTreeMap, ManagedPolicyGroup, ManisApp, NodeIdentity, PolicyCandidateKind, PolicyCatalog,
    PolicyGroup, PolicyGroupId, PolicyGroupKind, PolicyNode, ProxyId, complete_benchmark, fs,
    mihomo, stored_workspace,
};

#[test]
fn manual_selector_with_candidates_is_benchmarkable() {
    let selector = PolicyGroup {
        id: PolicyGroupId::new("Manual Route"),
        name: "Manual Route".to_owned(),
        kind: PolicyGroupKind::Selector,
        target: Some("Hong Kong".to_owned()),
        nodes: vec![PolicyNode {
            id: ProxyId::new("Hong Kong"),
            name: "Hong Kong".to_owned(),
            kind: PolicyCandidateKind::Node,
            provider: Some("Fixture".to_owned()),
            detail: "VLESS".to_owned(),
            latency_ms: None,
            alive: None,
        }],
        rules_total: 0,
        rules: Vec::new(),
    };

    assert!(ManisApp::policy_group_benchmarkable(&selector));

    let empty_selector = PolicyGroup {
        nodes: Vec::new(),
        ..selector
    };
    assert!(!ManisApp::policy_group_benchmarkable(&empty_selector));
}

#[test]
fn policy_settings_only_match_a_saved_manis_group_by_exact_name() {
    let mut app = ManisApp::with_fixture_controller("http://127.0.0.1:9090");
    app.managed_policies.groups.push(
        ManagedPolicyGroup::new("group-deadbeef", "Hong Kong").expect("valid managed policy group"),
    );

    assert_eq!(
        app.editable_policy_group_id("Hong Kong"),
        Some("group-deadbeef")
    );
    assert_eq!(app.editable_policy_group_id("Hong Kong Auto"), None);
    assert_eq!(app.editable_policy_group_id("GLOBAL"), None);
}

#[test]
fn saved_global_node_overrides_runtime_target_without_losing_runtime_state() {
    let mut app = ManisApp::with_fixture_controller("http://127.0.0.1:9090");
    app.catalog = Some(
        PolicyCatalog::try_new(vec![PolicyGroup {
            id: PolicyGroupId::new("GLOBAL"),
            name: "GLOBAL".to_owned(),
            kind: PolicyGroupKind::Selector,
            target: Some("Tokyo".to_owned()),
            nodes: ["Tokyo", "Singapore"]
                .into_iter()
                .map(|name| PolicyNode {
                    id: ProxyId::new(name),
                    name: name.to_owned(),
                    kind: PolicyCandidateKind::Node,
                    provider: None,
                    detail: "VLESS".to_owned(),
                    latency_ms: None,
                    alive: None,
                })
                .collect(),
            rules_total: 0,
            rules: Vec::new(),
        }])
        .expect("fixture global group"),
    );

    assert_eq!(app.global_target(), Some("Tokyo"));
    assert_eq!(app.runtime_global_target(), Some("Tokyo"));

    app.managed_policies
        .node_selections
        .set_global(NodeIdentity::new("saved", "Singapore").expect("valid saved node identity"));
    assert_eq!(app.global_target(), Some("Singapore"));
    assert_eq!(app.runtime_global_target(), Some("Tokyo"));
}

#[test]
fn app_startup_restores_global_and_manual_policy_node_selections() {
    let root =
        std::env::temp_dir().join(format!("manis-app-node-selections-{}", std::process::id()));
    if root.exists() {
        fs::remove_dir_all(&root).expect("remove stale fixture");
    }
    let store = root.join("subscriptions");
    let mut preferences = mihomo::NodeSelectionPreferences::default();
    preferences
        .set_global(NodeIdentity::new("saved", "Singapore").expect("valid saved node identity"));
    preferences
        .set_policy_target("Manual Video", "Tokyo")
        .expect("valid manual policy target");
    mihomo::save_node_selection_preferences_in(&store, &preferences).expect("save node selections");

    let app =
        ManisApp::with_fixture_controller_and_subscription_store("http://127.0.0.1:9090", store);

    assert_eq!(
        app.global_target_identity()
            .map(|identity| (identity.source_id.as_str(), identity.node_name.as_str())),
        Some(("saved", "Singapore"))
    );
    assert_eq!(
        app.managed_policies
            .node_selections
            .policy_target("Manual Video"),
        Some("Tokyo")
    );
    fs::remove_dir_all(root).expect("remove selection fixture");
}

#[test]
fn app_startup_restores_completed_manual_benchmark_results() {
    let root = std::env::temp_dir().join(format!("manis-app-benchmarks-{}", std::process::id()));
    if root.exists() {
        fs::remove_dir_all(&root).expect("remove stale fixture");
    }
    let store = root.join("subscriptions");
    let key = "source:fixture".to_owned();
    let benchmarks = BTreeMap::from([(key.clone(), complete_benchmark("Tokyo", 64))]);
    stored_workspace::save_group_benchmarks_in(&store, &benchmarks).expect("save benchmark state");

    let app =
        ManisApp::with_fixture_controller_and_subscription_store("http://127.0.0.1:9090", store);

    assert_eq!(
        app.managed_policies.benchmarks.get(&key),
        benchmarks.get(&key)
    );
    fs::remove_dir_all(root).expect("remove benchmark fixture");
}

#[test]
fn manual_policy_table_falls_back_to_the_catalog_target() {
    let mut app = ManisApp::with_fixture_controller("http://127.0.0.1:9090");
    let policy_id = PolicyGroupId::new("manual-video");
    app.catalog = Some(
        PolicyCatalog::try_new(vec![PolicyGroup {
            id: policy_id.clone(),
            name: "Manual Video".to_owned(),
            kind: PolicyGroupKind::Selector,
            target: Some("Singapore".to_owned()),
            nodes: ["Tokyo", "Singapore"]
                .into_iter()
                .map(|name| PolicyNode {
                    id: ProxyId::new(name),
                    name: name.to_owned(),
                    kind: PolicyCandidateKind::Node,
                    provider: None,
                    detail: "fixture".to_owned(),
                    latency_ms: None,
                    alive: None,
                })
                .collect(),
            rules_total: 0,
            rules: Vec::new(),
        }])
        .expect("manual policy catalog"),
    );
    app.workspace.select_group(policy_id.clone());

    assert_eq!(
        app.catalog
            .as_ref()
            .map(|catalog| app.node_for_policy(catalog.select(Some(&policy_id))).name),
        Some("Singapore".to_owned())
    );
}

#[test]
fn policy_expansion_survives_switching_between_saved_and_runtime_groups() {
    let mut app = ManisApp::with_fixture_controller("http://127.0.0.1:9090");
    let saved = ManagedPolicyGroup::new("group-deadbeef", "Hong Kong").expect("valid saved policy");
    let runtime = PolicyGroup {
        id: PolicyGroupId::new("Hong Kong"),
        name: "Hong Kong".to_owned(),
        kind: PolicyGroupKind::Selector,
        target: None,
        nodes: Vec::new(),
        rules_total: 0,
        rules: Vec::new(),
    };
    app.managed_policies.groups.push(saved.clone());
    app.expanded_policy_group = Some(PolicyGroupId::new(saved.id.clone()));
    assert!(app.policy_list_card_view(&runtime).expanded);

    app.expanded_policy_group = Some(runtime.id.clone());
    assert!(app.offline_policy_card_view(&saved).expanded);

    app.expanded_policy_group = None;
    assert!(!app.policy_list_card_view(&runtime).expanded);
    assert!(!app.offline_policy_card_view(&saved).expanded);
}

#[test]
fn offline_policy_table_keeps_candidate_metadata_and_source_filtering() {
    let mut app = ManisApp::with_fixture_controller("http://127.0.0.1:9090");
    let node = mihomo::LoadedProviderNode {
        name: "Hong Kong".to_owned(),
        protocol: "Trojan".to_owned(),
        latency_label: Some("42 ms".to_owned()),
        alive: Some(true),
    };
    app.source_providers = vec![
        mihomo::LoadedProvider {
            name: "First".to_owned(),
            vehicle_type: None,
            nodes: vec![node.clone()],
        },
        mihomo::LoadedProvider {
            name: "Second".to_owned(),
            vehicle_type: None,
            nodes: vec![node],
        },
    ];
    let mut policy = ManagedPolicyGroup::new("policy-test", "Manual").expect("policy");
    policy
        .set_matcher(manis_core::PolicyCandidateMatcher::Explicit(
            [NodeIdentity::new("mihomo:1", "Hong Kong").expect("identity")].into(),
        ))
        .expect("matcher");
    let candidates = app.managed_policy_candidate_nodes(&policy);
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].provider.as_deref(), Some("Second"));
    assert_eq!(candidates[0].detail, "Trojan");
    assert_eq!(candidates[0].latency_ms, Some(42));
    assert_eq!(candidates[0].alive, Some(true));
}

#[test]
fn offline_policy_table_includes_explicit_builtins_and_nested_groups() {
    let mut app = ManisApp::with_fixture_controller("http://127.0.0.1:9090");
    let nested = ManagedPolicyGroup::new("policy-nested", "Nested").expect("nested policy");
    app.managed_policies.groups.push(nested);
    let mut policy = ManagedPolicyGroup::new("policy-parent", "Parent").expect("parent policy");
    assert!(app.managed_policy_candidate_nodes(&policy).is_empty());
    policy
        .set_matcher(manis_core::PolicyCandidateMatcher::Explicit(
            [
                NodeIdentity::new("builtin", "DIRECT").expect("builtin"),
                NodeIdentity::new("policy:policy-nested", "Nested").expect("nested"),
            ]
            .into(),
        ))
        .expect("matcher");
    let candidates = app.managed_policy_candidate_nodes(&policy);
    assert_eq!(candidates.len(), 2);
    assert_eq!(candidates[0].name, "DIRECT");
    assert_eq!(candidates[1].kind, PolicyCandidateKind::PolicyGroup);
    assert_eq!(
        app.managed_policy_candidate_names(&policy),
        vec!["DIRECT", "Nested"]
    );
}

#[test]
fn offline_saved_node_keeps_protocol_without_fabricated_latency() {
    let mut app = ManisApp::with_fixture_controller("http://127.0.0.1:9090");
    app.saved_single_nodes.push(mihomo::StoredSingleNode {
        id: "single-test".to_owned(),
        name: "Saved".to_owned(),
        source: crate::subscription::SingleNodeSource::parse(
            "vless://11111111-1111-1111-1111-111111111111@example.com:443#Saved",
        )
        .expect("saved node"),
        enabled: true,
    });
    let policy = ManagedPolicyGroup::new("policy-test", "Manual").expect("policy");
    let candidates = app.managed_policy_candidate_nodes(&policy);
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].name, "Saved");
    assert_eq!(candidates[0].detail, "VLESS");
    assert_eq!(candidates[0].latency_ms, None);
    assert_eq!(candidates[0].alive, None);
}

#[test]
fn offline_manual_policy_selection_uses_only_saved_group_candidates() {
    assert!(super::policy_target_is_selectable(
        false,
        Some(false),
        Some(true)
    ));
    assert!(!super::policy_target_is_selectable(
        false,
        Some(true),
        Some(false)
    ));
    assert!(!super::policy_target_is_selectable(false, Some(true), None));
}

#[test]
fn connected_manual_policy_selection_uses_runtime_catalog() {
    assert!(super::policy_target_is_selectable(
        true,
        Some(true),
        Some(false)
    ));
    assert!(!super::policy_target_is_selectable(
        true,
        Some(false),
        Some(true)
    ));
    assert!(!super::policy_target_is_selectable(true, None, Some(true)));
}
