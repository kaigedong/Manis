use super::{
    ControllerState, ManisApp, PolicyCandidateKind, PolicyCatalog, PolicyGroup, PolicyGroupId,
    PolicyGroupKind, PolicyNode, ProxyId, complete_benchmark, mihomo,
};

#[gpui::test]
fn routing_mode_completion_preserves_apply_and_persistence_outcomes(cx: &mut gpui::TestAppContext) {
    use gpui::AppContext as _;
    use manis_core::RoutingMode;

    for (result, applied, persistence_failed) in [
        (Ok(super::PreferencePersistence::Saved), true, false),
        (Ok(super::PreferencePersistence::Skipped), true, false),
        (
            Ok(super::PreferencePersistence::Failed(
                mihomo::SubscriptionStoreError::StoreUnavailable,
            )),
            true,
            true,
        ),
        (
            Err(mihomo::LoadError::Runtime("fixture failure".to_owned())),
            false,
            false,
        ),
    ] {
        let app = cx.new(|_| ManisApp::with_fixture_controller("http://127.0.0.1:1"));
        app.update(cx, |app, cx| {
            app.routing_mode = RoutingMode::Rule;
            app.proxy_runtime.mode = RoutingMode::Rule;
            app.routing_mode_busy = Some(RoutingMode::Direct);
            app.finish_routing_mode_change(RoutingMode::Direct, 0, result, cx);

            let expected_mode = if applied {
                RoutingMode::Direct
            } else {
                RoutingMode::Rule
            };
            assert_eq!(app.routing_mode, expected_mode);
            assert_eq!(app.proxy_runtime.mode, expected_mode);
            assert_eq!(app.routing_mode_busy, None);
            let persistence_warning = app
                .language()
                .localized(crate::localization::copy::app::RESTART_PREFERENCE_COULD_NOT_BE_SAVED);
            assert_eq!(app.status.contains(persistence_warning), persistence_failed);
            assert_eq!(app.status.contains("fixture failure"), !applied);
        });
    }
}

#[test]
fn disconnected_app_starts_without_mock_policy_groups() {
    let app = ManisApp::with_fixture_controller("http://127.0.0.1:9090");

    assert!(app.catalog.is_none());
    assert_eq!(app.workspace.selected_group, None);
    assert_eq!(app.workspace.selected_node, None);
}

#[test]
fn live_stream_failures_are_global_and_clear_after_recovery() {
    use crate::mihomo::{LiveStreamPhase, LiveStreamStatus};
    let mut app = ManisApp::with_fixture_controller("http://127.0.0.1:1");
    app.controller = ControllerState::Connected {
        endpoint: "http://127.0.0.1:1".to_owned(),
        version: "fixture".to_owned(),
        active_connections: 0,
        download_total: 0,
        upload_total: 0,
    };
    app.live_status = LiveStreamStatus {
        activity: LiveStreamPhase::ControllerUnavailable,
        logs: LiveStreamPhase::InterruptedHttp(401),
    };
    let issue = app
        .live_status_issue()
        .expect("stream failure must remain visible");
    assert!(
        issue.contains(
            app.language()
                .message(crate::localization::Message::NetworkActivity)
        )
    );
    assert!(
        issue.contains("401"),
        "a logs error must not be hidden by an activity error"
    );
    app.primary_workspace = manis_core::PrimaryWorkspace::Nodes;
    assert_eq!(app.live_status_issue().as_deref(), Some(issue.as_str()));

    app.live_status.activity = LiveStreamPhase::Live;
    assert!(
        app.live_status_issue()
            .expect("logs still failing")
            .contains("401")
    );
    app.live_status.logs = LiveStreamPhase::Live;
    assert!(app.live_status_issue().is_none());

    app.live_status.logs = LiveStreamPhase::ControllerUnavailable;
    app.controller = ControllerState::Disconnected;
    assert!(
        app.live_status_issue().is_none(),
        "do not override the disconnected state with stale stream errors"
    );
}

#[test]
fn runtime_snapshot_populates_real_policy_groups() {
    let mut app = ManisApp::with_fixture_controller("http://127.0.0.1:9090");
    let policy_id = PolicyGroupId::new("runtime-policy");
    let node_id = ProxyId::new("runtime-node");
    let catalog = PolicyCatalog::try_new(vec![PolicyGroup {
        id: policy_id.clone(),
        name: "Runtime policy".to_owned(),
        kind: PolicyGroupKind::Selector,
        target: Some("Runtime node".to_owned()),
        nodes: vec![PolicyNode {
            id: node_id.clone(),
            name: "Runtime node".to_owned(),
            kind: PolicyCandidateKind::Node,
            provider: Some("Runtime provider".to_owned()),
            detail: "VLESS".to_owned(),
            latency_ms: Some(42),
            alive: Some(true),
        }],
        rules_total: 1,
        rules: Vec::new(),
    }])
    .expect("runtime policy catalog");

    app.apply_mihomo_snapshot(
        "http://127.0.0.1:9090".to_owned(),
        mihomo::LoadedSnapshot {
            catalog: Some(catalog),
            providers: Vec::new(),
            version: "fixture".to_owned(),
            active_connections: 0,
            download_total: 0,
            upload_total: 0,
            observed_routes: Vec::new(),
            connections: Vec::new(),
            runtime: manis_mihomo::RuntimeConfig::default(),
        },
    );

    assert_eq!(app.policy_groups().count(), 1);
    assert_eq!(app.workspace.selected_group, Some(policy_id));
    assert_eq!(app.workspace.selected_node, Some(node_id));
}

#[test]
fn runtime_snapshot_keeps_completed_manual_policy_benchmark_latency() {
    let mut app = ManisApp::with_fixture_controller("http://127.0.0.1:9090");
    let policy_id = PolicyGroupId::new("runtime-policy");
    app.managed_policies.benchmarks.insert(
        ManisApp::policy_group_benchmark_key(&policy_id),
        complete_benchmark("Runtime node", 77),
    );
    let catalog = PolicyCatalog::try_new(vec![PolicyGroup {
        id: policy_id.clone(),
        name: "Runtime policy".to_owned(),
        kind: PolicyGroupKind::Selector,
        target: Some("Runtime node".to_owned()),
        nodes: vec![PolicyNode {
            id: ProxyId::new("runtime-node"),
            name: "Runtime node".to_owned(),
            kind: PolicyCandidateKind::Node,
            provider: Some("Runtime provider".to_owned()),
            detail: "VLESS".to_owned(),
            latency_ms: Some(12),
            alive: Some(true),
        }],
        rules_total: 1,
        rules: Vec::new(),
    }])
    .expect("runtime policy catalog");

    app.apply_mihomo_snapshot(
        "http://127.0.0.1:9090".to_owned(),
        mihomo::LoadedSnapshot {
            catalog: Some(catalog),
            providers: Vec::new(),
            version: "fixture".to_owned(),
            active_connections: 0,
            download_total: 0,
            upload_total: 0,
            observed_routes: Vec::new(),
            connections: Vec::new(),
            runtime: manis_mihomo::RuntimeConfig::default(),
        },
    );

    let selected = app
        .catalog
        .as_ref()
        .expect("catalog")
        .select(Some(&policy_id));
    assert_eq!(selected.nodes[0].latency_ms, Some(77));
}

#[test]
fn runtime_snapshot_without_user_policy_groups_still_connects_cleanly() {
    let mut app = ManisApp::with_fixture_controller("http://127.0.0.1:9090");
    app.workspace.replace_source_selection(
        PolicyGroupId::new("stale-policy"),
        Some(ProxyId::new("stale-node")),
    );

    app.apply_mihomo_snapshot(
        "http://127.0.0.1:9090".to_owned(),
        mihomo::LoadedSnapshot {
            catalog: None,
            providers: Vec::new(),
            version: "fixture".to_owned(),
            active_connections: 0,
            download_total: 0,
            upload_total: 0,
            observed_routes: Vec::new(),
            connections: Vec::new(),
            runtime: manis_mihomo::RuntimeConfig::default(),
        },
    );

    assert!(app.catalog.is_none());
    assert_eq!(app.workspace.selected_group, None);
    assert_eq!(app.workspace.selected_node, None);
    assert!(matches!(app.controller, ControllerState::Connected { .. }));
}
