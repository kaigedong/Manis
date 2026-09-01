use super::{
    Context, Duration, GroupBenchmarkProgressQueue, GroupBenchmarkState, LogLevel, ManisApp,
    PolicyCatalog, PolicyGroup, record_event, stored_workspace,
};

impl ManisApp {
    pub(in crate::app) fn policy_group_benchmarkable(group: &PolicyGroup) -> bool {
        !group.nodes.is_empty()
    }

    pub(in crate::app) fn source_group_benchmark_key(id: &str) -> String {
        format!("source:{id}")
    }

    pub(in crate::app) fn managed_policy_benchmark_key(id: &str) -> String {
        format!("user:{id}")
    }

    pub(in crate::app) fn policy_group_benchmark_key(id: &manis_core::PolicyGroupId) -> String {
        format!("policy:{}", id.as_str())
    }

    pub(in crate::app) fn persist_group_benchmarks(&self) {
        if self.configuration_transfer.active {
            return;
        }
        let Some(store_dir) = self.subscription_store_dir.as_ref() else {
            return;
        };
        if let Err(error) =
            stored_workspace::save_group_benchmarks_in(store_dir, &self.managed_policies.benchmarks)
        {
            record_event(
                LogLevel::Warn,
                "group_benchmark.persistence_failed",
                error.to_string(),
            );
        }
    }

    pub(in crate::app) fn apply_completed_policy_benchmarks(&self, catalog: &mut PolicyCatalog) {
        for (key, state) in &self.managed_policies.benchmarks {
            let Some(group_id) = key.strip_prefix("policy:") else {
                continue;
            };
            let Some(delays) = state.complete_delays() else {
                continue;
            };
            let _ = catalog.apply_group_benchmark(
                &manis_core::PolicyGroupId::new(group_id),
                None,
                delays,
            );
        }
    }

    pub(in crate::app) fn begin_group_benchmark(&mut self, key: String) -> Option<u64> {
        if self.configuration_transfer.active {
            return None;
        }
        if self.managed_policies.active_benchmark_generation.is_some() {
            return None;
        }
        self.managed_policies.benchmark_generation =
            self.managed_policies.benchmark_generation.wrapping_add(1);
        let generation = self.managed_policies.benchmark_generation;
        self.managed_policies
            .benchmarks
            .insert(key, GroupBenchmarkState::running(generation));
        self.managed_policies.active_benchmark_generation = Some(generation);
        self.persist_group_benchmarks();
        Some(generation)
    }

    pub(in crate::app) fn poll_group_benchmark_progress(
        &mut self,
        generation: u64,
        key: String,
        updates: GroupBenchmarkProgressQueue,
        cx: &mut Context<Self>,
    ) {
        let drained = updates
            .lock()
            .map(|mut updates| updates.drain(..).collect::<Vec<_>>())
            .unwrap_or_default();
        let mut changed = false;
        if let Some(state) = self.managed_policies.benchmarks.get_mut(&key) {
            for (name, delay) in drained {
                changed |= state.record(generation, &name, delay);
            }
        }
        if changed {
            cx.notify();
        }
        if self.managed_policies.active_benchmark_generation != Some(generation) {
            return;
        }
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(40))
                .await;
            if let Some(this) = this.upgrade() {
                this.update(cx, |this, cx| {
                    this.poll_group_benchmark_progress(generation, key, updates, cx);
                });
            }
        })
        .detach();
    }
}
