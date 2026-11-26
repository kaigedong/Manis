use super::{
    BTreeMap, GroupBenchmarkState, Language, ManisApp, complete_benchmark, fs, mihomo,
    stored_workspace,
};

#[test]
fn benchmark_failure_keeps_the_reason_and_ignores_stale_results() {
    let mut state = GroupBenchmarkState::running(2);
    assert!(!state.fail(1, Some("stale failure".to_owned())));
    assert!(state.is_running());
    assert!(state.fail(2, Some("HTTP 404".to_owned())));
    assert!(matches!(
        &state,
        GroupBenchmarkState::Failed {
            generation: 2,
            message: Some(message),
            finished_at_epoch_secs,
        }
        if message == "HTTP 404" && *finished_at_epoch_secs > 0
    ));
    assert!(!state.fail(1, Some("stale failure".to_owned())));
    let old: GroupBenchmarkState = serde_json::from_str(r#"{"Failed":{"generation":1}}"#)
        .expect("old benchmark state still loads");
    assert!(matches!(
        old,
        GroupBenchmarkState::Failed { message: None, .. }
    ));
}

#[test]
fn benchmark_failures_distinguish_probes_from_controller_connections() {
    use manis_mihomo::MihomoError;
    let describe = |error| {
        ManisApp::benchmark_failure_description(
            Language::SimplifiedChinese,
            &mihomo::LoadError::Mihomo(error),
        )
    };
    for (code, detail) in [(404, "HTTP 404"), (503, "测速网址"), (504, "超时上限 5 秒")] {
        let message = describe(MihomoError::HttpStatus {
            status_code: code,
            reason: "fixture".to_owned(),
            body_preview: "private response must not reach the UI".to_owned(),
        });
        assert!(message.contains(detail));
        assert!(!message.contains("请检查 Mihomo 连接"));
        assert!(!message.contains("private response"));
    }
    let timeout = describe(MihomoError::Io(std::io::ErrorKind::TimedOut.into()));
    assert!(timeout.contains("读取上限 9 秒"));
    let unavailable = describe(MihomoError::Io(
        std::io::ErrorKind::ConnectionRefused.into(),
    ));
    assert!(unavailable.contains("无法访问本地内核"));
    assert_ne!(timeout, unavailable);
}

#[test]
fn benchmark_age_copy_is_localized_in_minutes() {
    assert_eq!(
        crate::localization::copy::app::benchmark_age(Language::SimplifiedChinese, 59),
        "不到 1 分钟前测速"
    );
    assert_eq!(
        crate::localization::copy::app::benchmark_age(Language::SimplifiedChinese, 125),
        "2 分钟前测速"
    );
    assert_eq!(
        crate::localization::copy::app::benchmark_age(Language::English, 125),
        "Tested 2 min ago"
    );
}

#[test]
fn starting_manual_benchmark_replaces_persisted_completed_result() {
    let root = std::env::temp_dir().join(format!(
        "manis-app-benchmark-replace-{}",
        std::process::id()
    ));
    if root.exists() {
        fs::remove_dir_all(&root).expect("remove stale fixture");
    }
    let store = root.join("subscriptions");
    let key = "source:fixture".to_owned();
    stored_workspace::save_group_benchmarks_in(
        &store,
        &BTreeMap::from([(key.clone(), complete_benchmark("Tokyo", 64))]),
    )
    .expect("save benchmark state");
    let mut app = ManisApp::with_fixture_controller_and_subscription_store(
        "http://127.0.0.1:9090",
        store.clone(),
    );

    assert_eq!(app.begin_group_benchmark(key.clone()), Some(1));

    let restored =
        stored_workspace::load_group_benchmarks_in(&store).expect("load benchmark state");
    assert!(!restored.contains_key(&key));
    assert!(matches!(
        app.managed_policies.benchmarks.get(&key),
        Some(GroupBenchmarkState::Running { .. })
    ));
    fs::remove_dir_all(root).expect("remove benchmark fixture");
}

#[test]
fn latency_tooltip_formats_seconds_under_one_minute_and_minutes_above() {
    use crate::localization::copy::app::{latency_age, latency_tooltip};

    assert_eq!(latency_age(Language::SimplifiedChinese, 0), "0 秒前");
    assert_eq!(latency_age(Language::SimplifiedChinese, 42), "42 秒前");
    assert_eq!(latency_age(Language::SimplifiedChinese, 59), "59 秒前");
    assert_eq!(latency_age(Language::SimplifiedChinese, 60), "1 分钟前");
    assert_eq!(latency_age(Language::SimplifiedChinese, 430), "7 分钟前");

    assert_eq!(latency_age(Language::English, 42), "42s ago");
    assert_eq!(latency_age(Language::English, 430), "7 min ago");

    assert_eq!(
        latency_tooltip(Language::SimplifiedChinese, Some(101), Some(1_000), 1_018),
        "延迟 101 ms · 18 秒前测速"
    );
    assert_eq!(
        latency_tooltip(Language::SimplifiedChinese, Some(101), Some(1_000), 1_305),
        "延迟 101 ms · 5 分钟前测速"
    );
    assert_eq!(
        latency_tooltip(Language::English, Some(101), Some(1_000), 1_018),
        "Latency 101 ms · tested 18s ago"
    );
    assert_eq!(
        latency_tooltip(Language::SimplifiedChinese, None, Some(1_000), 1_015),
        "测速失败 · 15 秒前测速"
    );
    assert_eq!(
        latency_tooltip(Language::SimplifiedChinese, None, None, 1_015),
        "尚未测速"
    );
}

fn fixture_policy_node(name: &str, latency_ms: Option<u16>) -> manis_core::PolicyNode {
    manis_core::PolicyNode {
        id: manis_core::ProxyId::new(name),
        name: name.to_owned(),
        kind: manis_core::PolicyCandidateKind::Node,
        provider: Some("Subscription 1".to_owned()),
        detail: "Trojan".to_owned(),
        latency_ms,
        alive: latency_ms.map(|_| true),
    }
}

fn fixture_provider_node(name: &str) -> mihomo::LoadedProviderNode {
    mihomo::LoadedProviderNode {
        name: name.to_owned(),
        protocol: "Trojan".to_owned(),
        latency_label: None,
        alive: None,
    }
}

#[test]
fn shared_node_latencies_propagate_across_all_policy_groups_and_sources() {
    use manis_core::{PolicyCatalog, PolicyGroup, PolicyGroupId, PolicyGroupKind};

    let mut app = ManisApp::with_fixture_controller("http://127.0.0.1:9090");
    app.catalog = Some(
        PolicyCatalog::try_new(vec![
            PolicyGroup {
                id: PolicyGroupId::new("最低延迟"),
                name: "最低延迟".to_owned(),
                kind: PolicyGroupKind::UrlTest,
                target: Some("233boy".to_owned()),
                nodes: vec![
                    fixture_policy_node("233boy", Some(231)),
                    fixture_policy_node("HK 01", None),
                    fixture_policy_node("HK 06", None),
                    fixture_policy_node("US 01", None),
                ],
                rules_total: 0,
                rules: Vec::new(),
            },
            PolicyGroup {
                id: PolicyGroupId::new("美国"),
                name: "美国".to_owned(),
                kind: PolicyGroupKind::UrlTest,
                target: Some("US 01".to_owned()),
                nodes: vec![fixture_policy_node("US 01", None)],
                rules_total: 0,
                rules: Vec::new(),
            },
        ])
        .expect("policy catalog"),
    );
    app.source_providers = vec![mihomo::LoadedProvider {
        name: "Subscription 1".to_owned(),
        vehicle_type: Some("HTTP".to_owned()),
        nodes: vec![
            fixture_provider_node("HK 01"),
            fixture_provider_node("US 01"),
        ],
    }];

    app.record_shared_node_latencies(
        &["HK 01".to_owned(), "HK 06".to_owned(), "US 01".to_owned()],
        &BTreeMap::from([
            ("HK 01".to_owned(), 101),
            ("HK 06".to_owned(), 100),
            ("US 01".to_owned(), 182),
        ]),
        2_000,
    );

    let catalog = app.catalog.as_ref().expect("catalog");
    let auto_group = catalog.select(Some(&PolicyGroupId::new("最低延迟")));
    let delay_of = |name: &str| {
        auto_group
            .nodes
            .iter()
            .find(|n| n.name == name)
            .and_then(|n| n.latency_ms)
    };
    assert_eq!(delay_of("HK 01"), Some(101));
    assert_eq!(delay_of("HK 06"), Some(100));
    assert_eq!(delay_of("US 01"), Some(182));
    assert!(
        app.url_test_group_needs_refresh(auto_group),
        "lowest-latency group on 231ms must detect faster 100ms candidate"
    );

    let us_group = catalog.select(Some(&PolicyGroupId::new("美国")));
    assert_eq!(us_group.nodes[0].latency_ms, Some(182));
    assert_eq!(
        app.source_providers[0].nodes[0].latency_label.as_deref(),
        Some("101 ms")
    );
    assert_eq!(
        app.source_providers[0].nodes[1].latency_label.as_deref(),
        Some("182 ms")
    );
    assert_eq!(
        app.resolved_node_latency("HK 06", None),
        (Some(100), Some(2_000))
    );
}

#[test]
fn benchmark_cache_persists_shared_node_latencies_and_fresh_exit_ips() {
    use crate::app::{NodeLatencyRecord, benchmark_timestamp};

    let root = std::env::temp_dir().join(format!(
        "manis-app-benchmark-cache-shared-{}",
        std::process::id()
    ));
    if root.exists() {
        fs::remove_dir_all(&root).expect("remove stale fixture");
    }
    let store = root.join("cache");
    let now = benchmark_timestamp();
    let node_latencies = BTreeMap::from([(
        "HK 01".to_owned(),
        NodeLatencyRecord::measured(Some(98), now),
    )]);
    let exit_ips = BTreeMap::from([
        ("HK 01".to_owned(), (Some("1.2.3.4".to_owned()), now)),
        (
            "Expired Node".to_owned(),
            (Some("5.6.7.8".to_owned()), now.saturating_sub(3_600)),
        ),
    ]);

    stored_workspace::save_benchmark_cache_in(&store, &BTreeMap::new(), &node_latencies, &exit_ips)
        .expect("save cache");
    let restored = stored_workspace::load_benchmark_cache_in(&store).expect("load cache");

    assert_eq!(restored.node_latencies, node_latencies);
    assert_eq!(
        restored.exit_ips.get("HK 01"),
        Some(&(Some("1.2.3.4".to_owned()), now))
    );
    assert!(!restored.exit_ips.contains_key("Expired Node"));

    fs::remove_dir_all(root).expect("remove benchmark cache fixture");
}
