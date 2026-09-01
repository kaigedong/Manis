use super::{
    Context, ControllerState, GroupBenchmarkState, LogLevel, ManisApp, PolicyBenchmarkRun, UiEvent,
    copy, mihomo, record_event, trace_ui,
};

impl ManisApp {
    pub(in crate::app) fn start_policy_group_benchmark(
        &mut self,
        id: &manis_core::PolicyGroupId,
        cx: &mut Context<Self>,
    ) {
        let language = self.language();
        if !matches!(self.controller, ControllerState::Connected { .. }) {
            language
                .localized(copy::app::START_MIHOMO_BEFORE_TESTING_THIS_POLICY_GROUP)
                .clone_into(&mut self.status);
            cx.notify();
            return;
        }
        let Some(group) = self.policy_groups().find(|group| group.id == *id).cloned() else {
            return;
        };
        let key = Self::policy_group_benchmark_key(&group.id);
        if matches!(
            self.managed_policies.benchmarks.get(&key),
            Some(GroupBenchmarkState::Running { .. })
        ) {
            return;
        }
        let targets = group
            .nodes
            .iter()
            .map(mihomo::ProxyDelayTarget::from_policy_node)
            .collect::<Vec<_>>();
        if targets.is_empty() {
            language
                .localized(copy::app::THIS_POLICY_GROUP_HAS_NO_TESTABLE_CANDIDATES)
                .clone_into(&mut self.status);
            cx.notify();
            return;
        }
        let Some(generation) = self.begin_group_benchmark(key.clone()) else {
            language
                .localized(copy::app::ANOTHER_GROUP_IS_BEING_TESTED_WAIT_FOR_IT_TO_FINISH)
                .clone_into(&mut self.status);
            cx.notify();
            return;
        };
        self.status = copy::app::testing_policy_candidates(language, &group.name, targets.len());
        trace_ui(UiEvent::GroupBenchmarkStarted);

        let runtime = self.runtime.clone();
        let group_id = group.id.clone();
        let group_name = group.name.clone();
        let group_kind = group.kind;
        let total = targets.len();
        let run = PolicyBenchmarkRun {
            key,
            generation,
            group_id,
            group_kind,
            total,
        };
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn(async move {
                    if group_kind == manis_core::PolicyGroupKind::Direct {
                        runtime
                            .test_proxy_delay_targets_with_progress(&targets, |_name, _delay| {})
                            .map(|delays| mihomo::PolicyGroupBenchmarkSnapshot {
                                delays,
                                current: None,
                            })
                    } else {
                        runtime.test_policy_group_delay(&group_name, &targets)
                    }
                })
                .await;
            this.update(cx, |this, cx| {
                this.finish_policy_group_benchmark(run, result, cx);
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn finish_policy_group_benchmark(
        &mut self,
        run: PolicyBenchmarkRun,
        result: Result<mihomo::PolicyGroupBenchmarkSnapshot, mihomo::LoadError>,
        cx: &mut Context<Self>,
    ) {
        let PolicyBenchmarkRun {
            key,
            generation,
            group_id,
            group_kind,
            total,
        } = run;
        let language = self.language();
        if self.managed_policies.active_benchmark_generation != Some(generation) {
            return;
        }
        self.managed_policies.active_benchmark_generation = None;
        let (delays, current, failure) = match result {
            Ok(snapshot) => (Some(snapshot.delays), snapshot.current, None),
            Err(error) => {
                record_event(
                    LogLevel::Warn,
                    "group.delay.failed",
                    format!("group={} error={error}", group_id.as_str()),
                );
                (
                    None,
                    None,
                    Some(Self::benchmark_failure_description(language, &error)),
                )
            }
        };
        if let Some(delays) = delays.as_ref()
            && let Some(catalog) = self.catalog.as_mut()
        {
            let _ = catalog.apply_group_benchmark(&group_id, current.as_deref(), delays);
        }
        let Some(state) = self.managed_policies.benchmarks.get_mut(&key) else {
            cx.notify();
            return;
        };
        let accepted = match delays {
            Some(delays) => state.complete(generation, total, delays),
            None => state.fail(generation, failure.clone()),
        };
        if !accepted {
            return;
        }
        match state {
            GroupBenchmarkState::Complete { summary, .. } => {
                trace_ui(UiEvent::GroupBenchmarkSucceeded);
                self.status = Self::policy_benchmark_status(
                    language,
                    group_kind,
                    current.as_deref(),
                    *summary,
                );
            }
            GroupBenchmarkState::Failed { .. } => {
                trace_ui(UiEvent::GroupBenchmarkFailed);
                self.status = format!(
                    "{}：{}",
                    language.localized(copy::app::POLICY_GROUP_BENCHMARK_FAILED),
                    failure.as_deref().unwrap_or_else(|| {
                        language.localized(copy::common::MIHOMO_DID_NOT_RETURN_A_RESULT)
                    })
                );
            }
            GroupBenchmarkState::Idle | GroupBenchmarkState::Running { .. } => {
                record_event(
                    LogLevel::Warn,
                    "group_benchmark.completion_rejected",
                    "reason=unexpected_state_after_completion",
                );
                return;
            }
        }
        self.persist_group_benchmarks();
        cx.notify();
    }
}
