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
    assert_eq!(
        state,
        GroupBenchmarkState::Failed {
            generation: 2,
            message: Some("HTTP 404".to_owned()),
        }
    );
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
