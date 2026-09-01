use super::{
    BTreeMap, Context, GroupBenchmarkState, MAX_GROUP_BENCHMARK_NODES, ManisApp, ProxyDelayTarget,
    UiEvent, copy, mihomo, trace_ui,
};

impl ManisApp {
    pub(in crate::app) fn clear_managed_policy_benchmarks(
        &mut self,
        id: &str,
        previous_name: Option<&str>,
        next_name: Option<&str>,
    ) {
        self.managed_policies
            .benchmarks
            .remove(&Self::managed_policy_benchmark_key(id));
        for name in previous_name.into_iter().chain(next_name) {
            self.managed_policies
                .benchmarks
                .remove(&format!("policy:{name}"));
        }
    }

    pub(in crate::app) fn start_source_group_benchmark(
        &mut self,
        id: &str,
        name: &str,
        targets: Vec<ProxyDelayTarget>,
        cx: &mut Context<Self>,
    ) {
        let key = Self::source_group_benchmark_key(id);
        if matches!(
            self.managed_policies.benchmarks.get(&key),
            Some(GroupBenchmarkState::Running { .. })
        ) {
            return;
        }
        if targets.is_empty() {
            self.language()
                .localized(copy::nodes::THIS_SOURCE_HAS_NO_NODES_TO_TEST)
                .clone_into(&mut self.status);
            cx.notify();
            return;
        }
        if targets.len() > MAX_GROUP_BENCHMARK_NODES {
            let language = self.language();
            format!(
                "{}; {}",
                Self::group_limit_label(targets.len(), language),
                Self::single_test_limit_label(MAX_GROUP_BENCHMARK_NODES, language)
            )
            .clone_into(&mut self.status);
            cx.notify();
            return;
        }
        let Some(generation) = self.begin_group_benchmark(key.clone()) else {
            self.language()
                .localized(copy::nodes::A_GROUP_TEST_IS_ALREADY_RUNNING_WAIT_FOR_IT_TO)
                .clone_into(&mut self.status);
            cx.notify();
            return;
        };
        let language = self.language();
        self.status = format!(
            "{} “{name}” · {}",
            language.localized(copy::nodes::TESTING_SOURCE),
            Self::node_count_label(targets.len(), language)
        );
        trace_ui(UiEvent::GroupBenchmarkStarted);

        let runtime = self.runtime.clone();
        let progress =
            std::sync::Arc::new(std::sync::Mutex::new(std::collections::VecDeque::new()));
        self.poll_group_benchmark_progress(generation, key.clone(), progress.clone(), cx);
        let total = targets.len();
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn(async move {
                    runtime.test_proxy_delay_targets_with_progress(
                        &targets,
                        move |node_name, delay| {
                            if let Ok(mut updates) = progress.lock() {
                                updates.push_back((node_name.to_owned(), delay));
                            }
                        },
                    )
                })
                .await;
            this.update(cx, |this, cx| {
                this.finish_source_group_benchmark(&key, generation, total, result, cx);
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    pub(in crate::app) fn finish_source_group_benchmark(
        &mut self,
        key: &str,
        generation: u64,
        total: usize,
        result: Result<BTreeMap<String, u16>, mihomo::LoadError>,
        cx: &mut Context<Self>,
    ) {
        let language = self.language();
        if self.managed_policies.active_benchmark_generation != Some(generation) {
            return;
        }
        self.managed_policies.active_benchmark_generation = None;
        let Some(state) = self.managed_policies.benchmarks.get_mut(key) else {
            cx.notify();
            return;
        };
        let failure = result
            .as_ref()
            .err()
            .map(|error| Self::benchmark_failure_description(language, error));
        let accepted = match result {
            Ok(delays) => state.complete(generation, total, delays),
            Err(_error) => state.fail(generation, failure.clone()),
        };
        if !accepted {
            return;
        }
        match state {
            GroupBenchmarkState::Complete { summary, .. } => {
                trace_ui(UiEvent::GroupBenchmarkSucceeded);
                self.status = format!(
                    "{}: {}",
                    language.localized(copy::nodes::SOURCE_TEST_COMPLETED),
                    Self::success_fraction_label(summary.succeeded, summary.total, language)
                );
            }
            GroupBenchmarkState::Failed { .. } => {
                trace_ui(UiEvent::GroupBenchmarkFailed);
                self.status = format!(
                    "{}：{}",
                    language.localized(copy::nodes::SOURCE_TEST_FAILED),
                    failure.as_deref().unwrap_or_else(|| {
                        language.localized(copy::common::MIHOMO_DID_NOT_RETURN_A_RESULT)
                    })
                );
            }
            _ => return,
        }
        self.persist_group_benchmarks();
        cx.notify();
    }
}
