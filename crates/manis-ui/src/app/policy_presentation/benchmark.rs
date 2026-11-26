use std::collections::{BTreeMap, BTreeSet};

use super::{
    Context, ControllerState, Duration, GroupBenchmarkProgressQueue, GroupBenchmarkState,
    GroupBenchmarkSummary, LogLevel, ManisApp, NodeLatencyRecord, PolicyCatalog, PolicyGroup,
    benchmark_timestamp, mihomo, record_event, stored_workspace,
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
        let benchmark_dir = stored_workspace::benchmark_store_dir(store_dir);
        if let Err(error) = stored_workspace::save_benchmark_cache_in(
            &benchmark_dir,
            &self.managed_policies.benchmarks,
            &self.managed_policies.node_latencies,
            &self.exit_ips,
        ) {
            record_event(
                LogLevel::Warn,
                "group_benchmark.persistence_failed",
                error.to_string(),
            );
        }
    }

    /// Merges per-node latency history reported by Mihomo into the shared node latency map
    /// whenever the kernel's observation is newer than (or absent from) the cached record.
    pub(in crate::app) fn ingest_snapshot_node_latencies(
        &mut self,
        snapshot: &mihomo::LoadedSnapshot,
    ) {
        for (name, (delay_ms, measured_at)) in &snapshot.node_latency_records {
            let incoming = NodeLatencyRecord::measured(*delay_ms, *measured_at);
            match self.managed_policies.node_latencies.get(name).copied() {
                Some(existing)
                    if existing.measured_at_epoch_secs > incoming.measured_at_epoch_secs
                        || (existing.measured_at_epoch_secs == incoming.measured_at_epoch_secs
                            && existing.delay_ms.is_some()
                            && incoming.delay_ms.is_none()) => {}
                _ => {
                    self.managed_policies
                        .node_latencies
                        .insert(name.clone(), incoming);
                }
            }
        }
    }

    fn merged_shared_node_latencies(&self) -> BTreeMap<String, Option<u16>> {
        let mut by_timestamp = BTreeMap::<String, (Option<u16>, u64)>::new();
        for state in self.managed_policies.benchmarks.values() {
            let GroupBenchmarkState::Complete {
                delays,
                finished_at_epoch_secs,
                ..
            } = state
            else {
                continue;
            };
            for (name, delay) in delays {
                if *delay == 0 {
                    continue;
                }
                match by_timestamp.get(name).copied() {
                    Some((_, existing_ts)) if existing_ts > *finished_at_epoch_secs => {}
                    _ => {
                        by_timestamp.insert(name.clone(), (Some(*delay), *finished_at_epoch_secs));
                    }
                }
            }
        }
        for (name, record) in &self.managed_policies.node_latencies {
            match by_timestamp.get(name).copied() {
                Some((existing_delay, existing_ts))
                    if existing_ts > record.measured_at_epoch_secs
                        || (existing_ts == record.measured_at_epoch_secs
                            && existing_delay.is_some()
                            && record.delay_ms.is_none()) => {}
                _ => {
                    by_timestamp.insert(
                        name.clone(),
                        (record.delay_ms, record.measured_at_epoch_secs),
                    );
                }
            }
        }
        by_timestamp
            .into_iter()
            .map(|(name, (delay, _))| (name, delay))
            .collect()
    }

    pub(in crate::app) fn apply_completed_policy_benchmarks(&self, catalog: &mut PolicyCatalog) {
        let shared = self.merged_shared_node_latencies();
        if !shared.is_empty() {
            let _ = catalog.apply_node_latencies(&shared);
        }
    }

    pub(in crate::app) fn apply_shared_latencies_to_providers(
        &self,
        providers: &mut [mihomo::LoadedProvider],
    ) {
        let shared = self.merged_shared_node_latencies();
        if shared.is_empty() {
            return;
        }
        for provider in providers {
            for node in &mut provider.nodes {
                let Some(measured) = shared.get(&node.name).copied() else {
                    continue;
                };
                if let Some(delay) = measured.filter(|delay| *delay > 0) {
                    node.latency_label = Some(format!("{delay} ms"));
                    node.alive = Some(true);
                } else {
                    node.latency_label = None;
                    node.alive = Some(false);
                }
            }
        }
    }

    /// Records fresh per-node benchmark results in the shared latency table and propagates them to
    /// every policy group, source provider row, and overlapping cached group benchmark summary.
    pub(in crate::app) fn record_shared_node_latencies(
        &mut self,
        tested_nodes: &[String],
        delays: &BTreeMap<String, u16>,
        measured_at: u64,
    ) {
        let mut updates = BTreeMap::<String, Option<u16>>::new();
        for name in tested_nodes {
            updates.insert(name.clone(), None);
        }
        for (name, delay) in delays {
            if *delay > 0 {
                updates.insert(name.clone(), Some(*delay));
            }
        }
        if updates.is_empty() {
            return;
        }
        for (name, delay) in &updates {
            self.managed_policies.node_latencies.insert(
                name.clone(),
                NodeLatencyRecord::measured(*delay, measured_at),
            );
        }
        if let Some(catalog) = self.catalog.as_mut() {
            let _ = catalog.apply_node_latencies(&updates);
        }
        for provider in &mut self.source_providers {
            for node in &mut provider.nodes {
                let Some(measured) = updates.get(&node.name).copied() else {
                    continue;
                };
                if let Some(delay) = measured.filter(|delay| *delay > 0) {
                    node.latency_label = Some(format!("{delay} ms"));
                    node.alive = Some(true);
                } else {
                    node.latency_label = None;
                    node.alive = Some(false);
                }
            }
        }
        self.propagate_updates_to_cached_group_benchmarks(&updates, measured_at);
    }

    fn propagate_updates_to_cached_group_benchmarks(
        &mut self,
        updates: &BTreeMap<String, Option<u16>>,
        measured_at: u64,
    ) {
        let group_members: Vec<(String, Vec<String>)> = self
            .catalog
            .as_ref()
            .map(|catalog| {
                catalog
                    .iter()
                    .map(|group| {
                        (
                            Self::policy_group_benchmark_key(&group.id),
                            group.nodes.iter().map(|node| node.name.clone()).collect(),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();

        for (key, members) in group_members {
            let touches_group = members.iter().any(|name| updates.contains_key(name));
            if !touches_group {
                continue;
            }
            if let Some(GroupBenchmarkState::Complete {
                summary,
                delays,
                finished_at_epoch_secs,
                ..
            }) = self.managed_policies.benchmarks.get_mut(&key)
            {
                for member in &members {
                    let Some(measured) = updates.get(member).copied() else {
                        continue;
                    };
                    match measured.filter(|delay| *delay > 0) {
                        Some(delay) => {
                            delays.insert(member.clone(), delay);
                        }
                        None => {
                            delays.remove(member);
                        }
                    }
                }
                *summary =
                    GroupBenchmarkSummary::from_delays(members.len(), delays.values().copied());
                *finished_at_epoch_secs = (*finished_at_epoch_secs).max(measured_at);
            }
        }
    }

    fn record_live_node_latency(&mut self, name: &str, delay: Option<u16>, measured_at: u64) {
        let normalized = delay.filter(|ms| *ms > 0);
        self.managed_policies.node_latencies.insert(
            name.to_owned(),
            NodeLatencyRecord::measured(normalized, measured_at),
        );
        let single = BTreeMap::from([(name.to_owned(), normalized)]);
        if let Some(catalog) = self.catalog.as_mut() {
            let _ = catalog.apply_node_latencies(&single);
        }
        for provider in &mut self.source_providers {
            for node in &mut provider.nodes {
                if node.name == name {
                    if let Some(ms) = normalized {
                        node.latency_label = Some(format!("{ms} ms"));
                        node.alive = Some(true);
                    } else {
                        node.latency_label = None;
                        node.alive = Some(false);
                    }
                }
            }
        }
    }

    /// Resolves `(latency_ms, measured_at_epoch_secs)` for a physical node from the shared node
    /// latency cache, falling back to completed benchmarks or the caller's snapshot value.
    pub(in crate::app) fn resolved_node_latency(
        &self,
        node_name: &str,
        fallback_ms: Option<u64>,
    ) -> (Option<u64>, Option<u64>) {
        if let Some(record) = self.managed_policies.node_latencies.get(node_name) {
            let ms = record.delay_ms.map(u64::from).or(fallback_ms);
            return (ms, record.measured_at());
        }
        let mut best: Option<(u64, u64)> = None;
        for state in self.managed_policies.benchmarks.values() {
            match state {
                GroupBenchmarkState::Complete {
                    delays,
                    finished_at_epoch_secs,
                    ..
                } => {
                    if let Some(delay) = delays.get(node_name).copied().filter(|ms| *ms > 0) {
                        match best {
                            Some((_, ts)) if ts > *finished_at_epoch_secs => {}
                            _ => best = Some((u64::from(delay), *finished_at_epoch_secs)),
                        }
                    }
                }
                GroupBenchmarkState::Running { results, .. } => {
                    if let Some(Some(delay)) = results.get(node_name).copied()
                        && delay > 0
                    {
                        best = Some((u64::from(delay), benchmark_timestamp()));
                    }
                }
                GroupBenchmarkState::Idle | GroupBenchmarkState::Failed { .. } => {}
            }
        }
        if let Some((ms, ts)) = best {
            return (Some(ms), (ts > 0).then_some(ts));
        }
        (fallback_ms, None)
    }

    /// Resolves `(latency_ms, measured_at_epoch_secs)` for a policy target, following nested
    /// policy groups until it reaches the underlying physical exit node.
    pub(in crate::app) fn resolved_policy_exit_latency(
        &self,
        candidate_name: &str,
        fallback_ms: Option<u64>,
    ) -> (Option<u64>, Option<u64>) {
        let mut current = candidate_name;
        let mut visited = BTreeSet::new();
        while visited.insert(current) {
            let Some(group) = self
                .catalog
                .as_ref()
                .and_then(|catalog| catalog.iter().find(|group| group.name == current))
            else {
                break;
            };
            let Some(next) = self
                .selected_node_for_policy(group)
                .map(|node| node.name.as_str())
            else {
                break;
            };
            current = next;
        }
        self.resolved_node_latency(current, fallback_ms)
    }

    pub(in crate::app) fn url_test_group_needs_refresh(&self, group: &PolicyGroup) -> bool {
        if group.kind != manis_core::PolicyGroupKind::UrlTest || group.nodes.is_empty() {
            return false;
        }
        let tolerance_ms = self
            .managed_policies
            .groups
            .iter()
            .find(|managed| managed.name == group.name)
            .map_or(0, |managed| u64::from(managed.switch_tolerance_ms));
        let fastest_ms = group
            .nodes
            .iter()
            .filter_map(|node| {
                self.resolved_node_latency(&node.name, node.latency_ms.map(u64::from))
                    .0
            })
            .min();
        let Some(fastest_ms) = fastest_ms else {
            return false;
        };
        let current_ms = group.target.as_deref().and_then(|target| {
            let fallback = group
                .nodes
                .iter()
                .find(|node| node.name == target)
                .and_then(|node| node.latency_ms)
                .map(u64::from);
            self.resolved_node_latency(target, fallback).0
        });
        match current_ms {
            None => true,
            Some(current_ms) => fastest_ms.saturating_add(tolerance_ms) < current_ms,
        }
    }

    pub(in crate::app) fn group_nodes_fresh_in_shared_cache(
        &self,
        group: &PolicyGroup,
        now_epoch_secs: u64,
        interval_secs: u32,
    ) -> bool {
        let physical_nodes = group
            .nodes
            .iter()
            .filter(|node| node.kind == manis_core::PolicyCandidateKind::Node)
            .collect::<Vec<_>>();
        !physical_nodes.is_empty()
            && physical_nodes.iter().all(|node| {
                self.managed_policies
                    .node_latencies
                    .get(&node.name)
                    .is_some_and(|record| record.is_fresh(now_epoch_secs, interval_secs))
            })
    }

    /// Re-evaluates affected `url-test` policy groups in Mihomo after underlying nodes are tested
    /// so automatic lowest-latency groups switch to the genuinely fastest exit immediately.
    pub(in crate::app) fn sync_affected_url_test_groups(
        &mut self,
        tested_nodes: &[String],
        exclude_group_id: Option<&manis_core::PolicyGroupId>,
        cx: &mut Context<Self>,
    ) {
        if !matches!(self.controller, ControllerState::Connected { .. })
            || self.configuration_transfer.active
            || tested_nodes.is_empty()
        {
            return;
        }
        let tested_set = tested_nodes
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
        let affected: Vec<(
            manis_core::PolicyGroupId,
            String,
            Vec<mihomo::ProxyDelayTarget>,
        )> = self
            .catalog
            .as_ref()
            .map(|catalog| {
                catalog
                    .iter()
                    .filter(|group| {
                        group.kind == manis_core::PolicyGroupKind::UrlTest
                            && exclude_group_id.is_none_or(|excluded| excluded != &group.id)
                            && group
                                .nodes
                                .iter()
                                .any(|node| tested_set.contains(node.name.as_str()))
                            && self.url_test_group_needs_refresh(group)
                    })
                    .map(|group| {
                        let targets = group
                            .nodes
                            .iter()
                            .map(mihomo::ProxyDelayTarget::from_policy_node)
                            .collect::<Vec<_>>();
                        (group.id.clone(), group.name.clone(), targets)
                    })
                    .collect()
            })
            .unwrap_or_default();
        if affected.is_empty() {
            return;
        }

        let runtime = self.runtime.clone();
        let live_generation = self.live_generation;
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let outcomes = executor
                .spawn(async move {
                    affected
                        .into_iter()
                        .map(|(group_id, group_name, targets)| {
                            let result = runtime.test_policy_group_delay(&group_name, &targets);
                            let tested = targets
                                .into_iter()
                                .map(|target| target.name().to_owned())
                                .collect::<Vec<_>>();
                            (group_id, tested, result)
                        })
                        .collect::<Vec<_>>()
                })
                .await;
            this.update(cx, |this, cx| {
                this.apply_url_test_sync_outcomes(live_generation, outcomes, cx);
            })
            .ok();
        })
        .detach();
    }

    fn apply_url_test_sync_outcomes(
        &mut self,
        live_generation: u64,
        outcomes: Vec<(
            manis_core::PolicyGroupId,
            Vec<String>,
            Result<mihomo::PolicyGroupBenchmarkSnapshot, mihomo::LoadError>,
        )>,
        cx: &mut Context<Self>,
    ) {
        if self.live_generation != live_generation {
            return;
        }
        let mut changed = false;
        let now = benchmark_timestamp();
        for (group_id, tested, result) in outcomes {
            let Ok(snapshot) = result else {
                continue;
            };
            if let Some(catalog) = self.catalog.as_mut() {
                changed |= catalog.apply_group_benchmark(
                    &group_id,
                    snapshot.current.as_deref(),
                    &snapshot.delays,
                );
            }
            if !snapshot.delays.is_empty() {
                self.record_shared_node_latencies(&tested, &snapshot.delays, now);
                let key = Self::policy_group_benchmark_key(&group_id);
                if !self
                    .managed_policies
                    .benchmarks
                    .get(&key)
                    .is_some_and(GroupBenchmarkState::is_running)
                {
                    let summary = GroupBenchmarkSummary::from_delays(
                        tested.len(),
                        snapshot.delays.values().copied(),
                    );
                    self.managed_policies.benchmarks.insert(
                        key,
                        GroupBenchmarkState::Complete {
                            generation: self.managed_policies.benchmark_generation,
                            summary,
                            delays: snapshot.delays,
                            finished_at_epoch_secs: now,
                        },
                    );
                }
                changed = true;
            }
        }
        if changed {
            self.persist_group_benchmarks();
            cx.notify();
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
        let now = benchmark_timestamp();
        for (name, delay) in drained {
            if let Some(state) = self.managed_policies.benchmarks.get_mut(&key) {
                let _ = state.record(generation, &name, delay);
            }
            self.record_live_node_latency(&name, delay, now);
            changed = true;
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
