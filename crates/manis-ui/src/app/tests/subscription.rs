use super::{
    BTreeMap, DueRemoteSource, ImportedSubscription, ImportedSubscriptionState, Language, ManisApp,
    PolicyCandidateKind, PolicyNode, ProxyId, SourceKind, fs, mihomo,
};

#[test]
fn app_startup_detects_a_privately_imported_subscription() {
    let root = std::env::temp_dir().join(format!("manis-app-import-{}", std::process::id()));
    if root.exists() {
        fs::remove_dir_all(&root).expect("remove stale fixture");
    }
    let store = root.join("subscriptions");
    mihomo::save_imported_subscription_in(
        &store,
        "https://subscription.example.invalid/client?token=fixture",
    )
    .expect("save fixture subscription");

    let app =
        ManisApp::with_fixture_controller_and_subscription_store("http://127.0.0.1:9090", store);

    assert_eq!(app.imported_subscriptions.len(), 1);
    assert_eq!(
        app.imported_subscriptions[0].state,
        ImportedSubscriptionState::Pending(SourceKind::HttpsSubscription)
    );
    assert_eq!(
        app.imported_subscriptions[0].refresh_interval,
        mihomo::RemoteSourceRefreshInterval::Manual
    );
    assert_eq!(
        app.imported_subscriptions[0].last_successful_update_unix_secs,
        0
    );
    fs::remove_dir_all(root).expect("remove fixture");
}

#[test]
fn policy_node_source_uses_the_imported_subscription_name() {
    let mut app = ManisApp::with_fixture_controller("http://127.0.0.1:9090");
    app.imported_subscriptions.push(ImportedSubscription {
        id: "subscription:fixture".to_owned(),
        name: "NaiU_Net".to_owned(),
        source: manis_profile::SecretUrl::parse_subscription(
            "https://subscription.example.invalid/client?name=NaiU_Net",
        )
        .expect("fixture subscription"),
        enabled: true,
        state: ImportedSubscriptionState::Ready(SourceKind::HttpsSubscription),
        providers: Vec::new(),
        generation: 0,
        refresh_interval: mihomo::RemoteSourceRefreshInterval::Manual,
        last_successful_update_unix_secs: 0,
    });
    let node = PolicyNode {
        id: ProxyId::new("HK 03"),
        name: "HK 03".to_owned(),
        kind: PolicyCandidateKind::Node,
        provider: Some("Subscription 1".to_owned()),
        detail: "Trojan".to_owned(),
        latency_ms: None,
        alive: None,
    };

    assert_eq!(
        app.policy_node_source_label(&node, Language::SimplifiedChinese),
        "NaiU_Net"
    );
}

#[test]
fn scheduled_refresh_selects_one_due_source_with_subscriptions_first() {
    let subscription = ImportedSubscription {
        id: "subscription:fixture".to_owned(),
        name: "Fixture".to_owned(),
        source: manis_profile::SecretUrl::parse_subscription(
            "https://subscription.example.invalid/client",
        )
        .expect("fixture subscription"),
        enabled: true,
        state: ImportedSubscriptionState::Ready(SourceKind::HttpsSubscription),
        providers: Vec::new(),
        generation: 0,
        refresh_interval: mihomo::RemoteSourceRefreshInterval::Hourly,
        last_successful_update_unix_secs: 100,
    };
    let mut rule_source = mihomo::StoredQxRuleSource {
        id: "qx-rule-source:fixture".to_owned(),
        name: None,
        source: manis_profile::SecretUrl::parse_https("https://rules.example.invalid/list")
            .expect("fixture rule URL"),
        enabled: true,
        target_policy: manis_profile::Name::parse("Proxy").expect("fixture policy"),
        content: "DOMAIN-SUFFIX,example.com,Proxy".to_owned(),
        rule_count: 1,
        diagnostic_count: 0,
        refresh_interval: mihomo::RemoteSourceRefreshInterval::Hourly,
        last_successful_update_unix_secs: 100,
    };

    assert_eq!(
        super::next_due_remote_source(
            std::slice::from_ref(&subscription),
            std::slice::from_ref(&rule_source),
            &BTreeMap::new(),
            3_700,
        ),
        Some(DueRemoteSource::Subscription(subscription.id.clone()))
    );

    let mut disabled_subscription = subscription.clone();
    disabled_subscription.enabled = false;
    assert_eq!(
        super::next_due_remote_source(
            &[disabled_subscription],
            std::slice::from_ref(&rule_source),
            &BTreeMap::new(),
            3_700,
        ),
        Some(DueRemoteSource::QxRule(rule_source.id.clone()))
    );

    let mut second_subscription = subscription.clone();
    second_subscription.id = "subscription:second".to_owned();
    let retry_not_before = BTreeMap::from([(
        DueRemoteSource::Subscription(subscription.id.clone()).scheduler_key(),
        4_000,
    )]);
    assert_eq!(
        super::next_due_remote_source(
            &[subscription, second_subscription.clone()],
            std::slice::from_ref(&rule_source),
            &retry_not_before,
            3_700,
        ),
        Some(DueRemoteSource::Subscription(second_subscription.id))
    );

    rule_source.enabled = false;
    assert_eq!(
        super::next_due_remote_source(&[], &[rule_source.clone()], &BTreeMap::new(), 3_700),
        None
    );
    rule_source.enabled = true;
    rule_source.refresh_interval = mihomo::RemoteSourceRefreshInterval::Manual;
    assert_eq!(
        super::next_due_remote_source(&[], &[rule_source], &BTreeMap::new(), 3_700),
        None
    );
}

#[test]
fn app_startup_restores_saved_qx_rule_sources() {
    let root = std::env::temp_dir().join(format!("manis-app-qx-rule-{}", std::process::id()));
    if root.exists() {
        fs::remove_dir_all(&root).expect("remove stale fixture");
    }
    let store = root.join("subscriptions");
    mihomo::save_qx_rule_source_in(
        &store,
        "https://rules.example.invalid/airports.list",
        "Proxy",
        "DOMAIN-SUFFIX,example.com,Proxy",
    )
    .expect("save QX rule fixture");

    let app =
        ManisApp::with_fixture_controller_and_subscription_store("http://127.0.0.1:9090", store);

    assert_eq!(app.rule_sources.sources.len(), 1);
    assert!(app.rule_sources.sources[0].enabled);
    assert_eq!(app.rule_sources.sources[0].rule_count, 1);
    assert_eq!(app.rule_sources.sources[0].target_policy.as_str(), "Proxy");
    fs::remove_dir_all(root).expect("remove fixture");
}
