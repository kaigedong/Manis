impl ManisApp {
    fn start_app_update_polling(cx: &mut Context<Self>) {
        let timer = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            loop {
                if this
                    .update(cx, Self::check_for_app_update)
                    .is_err()
                {
                    break;
                }
                timer.timer(Duration::from_hours(1)).await;
            }
        })
        .detach();
    }

    fn check_for_app_update(&mut self, cx: &mut Context<Self>) {
        if self.runtime.is_fixture() || matches!(self.app_update_state, AppUpdateState::Checking) {
            return;
        }
        let previous = std::mem::replace(&mut self.app_update_state, AppUpdateState::Checking);
        cx.notify();
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let checked = executor
                .spawn(async { app_update::check_for_update() })
                .await;
            this.update(cx, |this, cx| {
                this.finish_app_update_check(checked, previous);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn finish_app_update_check(
        &mut self,
        result: Result<Option<AvailableUpdate>, AppUpdateError>,
        previous: AppUpdateState,
    ) {
        self.app_update_state = match result {
            Ok(Some(update)) => {
                if !matches!(&previous, AppUpdateState::Available(known) if known == &update) {
                    record_event(
                        LogLevel::Info,
                        "app.update.available",
                        format!("version={}", update.version),
                    );
                    self.status =
                        copy::app_update::available_version(self.language(), &update.version);
                }
                AppUpdateState::Available(update)
            }
            Ok(None) => AppUpdateState::Current,
            Err(error) => {
                record_event(LogLevel::Warn, "app.update.check.failed", error.to_string());
                // A temporary network failure must not hide an already discovered release.
                match previous {
                    AppUpdateState::Available(_) => previous,
                    _ => AppUpdateState::Failed(error),
                }
            }
        };
    }

    fn switch_kernel(&mut self, requested: KernelKind, cx: &mut Context<Self>) {
        if self.configuration_transfer.active {
            return;
        }
        let language = self.language();
        if self.kernel_switch_state.is_busy() || self.runtime.kind() == requested {
            return;
        }
        if self.proxy_mode_busy.is_some() {
            language
                .localized(copy::app::WAIT_FOR_THE_PROXY_MODE_CHANGE_TO_FINISH_BEFORE_CHANGING_KERNELS)
                .clone_into(&mut self.status);
            cx.notify();
            return;
        }
        let Some(store_dir) = self.subscription_store_dir.clone() else {
            language
                .localized(copy::app::THE_LOCAL_CONFIGURATION_DIRECTORY_IS_UNAVAILABLE_THE_KERNEL_CANNOT_BE)
                .clone_into(&mut self.status);
            cx.notify();
            return;
        };
        self.kernel_switch_state = KernelSwitchState::Preparing;
        self.status = format!(
            "{} {} {}",
            language.localized(copy::app::VALIDATING),
            requested.display_name(),
            language.localized(copy::app::CONFIGURATION)
        );
        let previous = self.runtime.clone();
        let previous_kind = previous.kind();
        let previous_mode = self.proxy_mode;
        self.proxy_mode_busy = Some(ProxyMode::Off);
        let system_proxy = self.system_proxy.clone();
        let tun_dns = self.tun_dns.clone();
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn(async move {
                    let previous_for_stop = previous.clone();
                    let previous_for_restore = previous.clone();
                    perform_kernel_switch(
                        requested,
                        previous_kind,
                        previous_mode,
                        || {
                            KernelRuntime::prepare_with_language(
                                requested,
                                Some(&store_dir),
                                language,
                            )
                        },
                        |kind| kernel::save_kernel_kind_in(&store_dir, kind)
                            .map(|_path| ())
                            .map_err(|error| error.to_string()),
                        |active| {
                            apply_proxy_mode_transition(
                                &previous_for_restore,
                                &system_proxy,
                                &tun_dns,
                                active,
                                ProxyMode::Off,
                                ProxyPorts {
                                    http: None,
                                    socks: None,
                                },
                                language,
                            )
                        },
                        || previous_for_stop.stop_managed(),
                    )
                })
                .await;
            this.update(cx, |this, cx| {
                this.finish_kernel_switch(requested, result, cx);
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn finish_kernel_switch(
        &mut self,
        requested: KernelKind,
        result: Result<KernelRuntime, KernelSwitchFailure>,
        cx: &mut Context<Self>,
    ) {
        let language = self.language();
        self.kernel_switch_state = KernelSwitchState::Idle;
        if self.proxy_mode_busy == Some(ProxyMode::Off) {
            self.proxy_mode_busy = None;
        }
        match result {
            Ok(runtime) => {
                self.runtime = runtime;
                self.controller = ControllerState::Disconnected;
                self.live_generation = self.live_generation.wrapping_add(1);
                self.live_runtime = None;
                self.proxy_mode = ProxyMode::Off;
                self.status = copy::app::switched_kernel(language, requested.display_name());
            }
            Err(failure) => {
                if failure.proxy_mode_restored {
                    self.proxy_mode = ProxyMode::Off;
                }
                self.status = copy::app::kernel_switch_failed(
                    language,
                    requested.display_name(),
                    &failure.message,
                );
            }
        }
        cx.notify();
    }

    fn update_mihomo_core(&mut self, cx: &mut Context<Self>) {
        if self.configuration_transfer.active {
            return;
        }
        if self.mihomo_core_update_state.is_busy() {
            return;
        }
        if self.proxy_mode != ProxyMode::Off {
            self.language()
                .localized(copy::app::TURN_OFF_THE_ACTIVE_PROXY_MODE_BEFORE_UPDATING_MIHOMO)
                .clone_into(&mut self.status);
            cx.notify();
            return;
        }
        let Some(store_dir) = self.subscription_store_dir.clone() else {
            self.language()
                .localized(
                    copy::app::THE_MANIS_DATA_DIRECTORY_IS_UNAVAILABLE_MIHOMO_CANNOT_BE_UPDATED,
                )
                .clone_into(&mut self.status);
            cx.notify();
            return;
        };

        let language = self.language();
        let reconnect = matches!(self.controller, ControllerState::Connected { .. });
        let previous = self.runtime.clone();
        self.mihomo_core_update_state = MihomoCoreUpdateState::Updating;
        self.live_generation = self.live_generation.wrapping_add(1);
        self.live_runtime = None;
        self.controller = ControllerState::Disconnected;
        language
            .localized(copy::app::DOWNLOADING_AND_VERIFYING_THE_STABLE_MIHOMO_RELEASE)
            .clone_into(&mut self.status);

        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let outcome = executor
                .spawn(async move {
                    perform_mihomo_core_update(&previous, &store_dir, language, reconnect)
                })
                .await;
            this.update(cx, |this, cx| {
                this.apply_mihomo_core_update_outcome(outcome, cx);
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn apply_mihomo_core_update_outcome(
        &mut self,
        outcome: MihomoCoreUpdateOutcome,
        cx: &mut Context<Self>,
    ) {
        let language = self.language();
        match outcome {
            MihomoCoreUpdateOutcome::Installed {
                version,
                runtime,
                snapshot,
            } => {
                self.runtime = runtime;
                self.mihomo_core_update_state = MihomoCoreUpdateState::Ready(version.clone());
                self.apply_core_update_snapshot(snapshot, cx);
                self.status = copy::app::mihomo_installed(language, &version);
            }
            MihomoCoreUpdateOutcome::Failed { message, recovered } => {
                self.mihomo_core_update_state = core_update::managed_core_binary_path()
                    .map_or(MihomoCoreUpdateState::Missing, |_path| {
                        MihomoCoreUpdateState::Ready(String::new())
                    });
                self.apply_core_update_snapshot(recovered, cx);
                self.status = format!(
                    "{}{message}",
                    language
                        .localized(copy::app::MIHOMO_UPDATE_FAILED_THE_PREVIOUS_CORE_WAS_RESTORED)
                );
            }
        }
        cx.notify();
    }

    fn apply_core_update_snapshot(
        &mut self,
        result: Option<mihomo::RuntimeSnapshot>,
        cx: &mut Context<Self>,
    ) {
        let Some(result) = result else {
            return;
        };
        let controller_endpoint = result.controller_endpoint.clone();
        let controller_secret = result.controller_secret.clone();
        self.apply_mihomo_snapshot(result.endpoint, result.snapshot);
        self.start_live_runtime(&controller_endpoint, controller_secret.as_deref(), cx);
    }

    fn connect_mihomo(&mut self, cx: &mut Context<Self>) {
        if self.configuration_transfer.active
            || matches!(self.controller, ControllerState::Connecting { .. }) {
            return;
        }

        let language = self.language();
        let operation = begin_operation(
            "kernel.connect.requested",
            format!(
                "kernel={} profile={} endpoint={}",
                self.runtime.kind().display_name(),
                self.runtime.profile_source().diagnostic_key(),
                self.runtime.endpoint_label()
            ),
        );
        self.live_generation = self.live_generation.wrapping_add(1);
        self.live_runtime = None;
        self.live_status = LiveStreamStatus {
            activity: LiveStreamPhase::Connecting,
            logs: LiveStreamPhase::Connecting,
        };

        let endpoint = self.runtime.endpoint_label();
        let kernel_name = self.runtime.kind().display_name();
        let runtime = self.runtime.clone();
        self.controller = ControllerState::Connecting {
            endpoint: endpoint.clone(),
        };
        self.status = copy::app::loading_kernel_data(language, kernel_name, &endpoint);
        trace_ui(UiEvent::MihomoConnectStarted);

        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = executor.spawn(async move { runtime.connect() }).await;
            this.update(cx, |this, cx| {
                let language = this.language();
                match result {
                    Ok(result) => {
                        record_operation(
                            operation,
                            LogLevel::Info,
                            "kernel.connect.succeeded",
                            format!("endpoint={}", result.controller_endpoint),
                        );
                        let controller_endpoint = result.controller_endpoint;
                        let controller_secret = result.controller_secret;
                        this.apply_mihomo_snapshot(result.endpoint, result.snapshot);
                        this.start_live_runtime(
                            &controller_endpoint,
                            controller_secret.as_deref(),
                            cx,
                        );
                        this.sync_saved_node_selections(cx);
                        this.start_pending_policy_benchmark(cx);
                    }
                    Err(error) => {
                        this.managed_policies.pending_benchmark_name = None;
                        record_operation(
                            operation,
                            LogLevel::Error,
                            "kernel.connect.failed",
                            error.to_string(),
                        );
                        trace_ui(UiEvent::MihomoConnectFailed);
                        let endpoint = this
                            .controller
                            .endpoint()
                            .unwrap_or(language.localized(copy::app::LOCAL_CONTROLLER))
                            .to_owned();
                        let message = error.to_string();
                        this.controller = ControllerState::Failed {
                            endpoint,
                            message: message.clone(),
                        };
                        this.status =
                            copy::app::kernel_connection_failed(language, kernel_name, &message);
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn start_pending_policy_benchmark(&mut self, cx: &mut Context<Self>) {
        let Some(policy_name) = self.managed_policies.pending_benchmark_name.take() else {
            return;
        };
        let Some(policy_id) = self
            .policy_groups()
            .find(|group| group.name == policy_name)
            .map(|group| group.id.clone())
        else {
            self.status = copy::app::policy_missing_from_kernel(self.language(), &policy_name);
            cx.notify();
            return;
        };
        self.expanded_policy_group = Some(policy_id.clone());
        self.start_policy_group_benchmark(&policy_id, cx);
    }

    fn apply_mihomo_snapshot(&mut self, endpoint: String, snapshot: LoadedSnapshot) {
        trace_ui(UiEvent::MihomoConnectSucceeded);
        let mut catalog = snapshot.catalog;
        for (group, target) in self.managed_policies.node_selections.iter_policy_targets() {
            if let Some(catalog) = catalog.as_mut() {
                let _ = catalog.apply_selector_target(group, target);
            }
        }
        if let Some(catalog) = catalog.as_mut() {
            self.apply_completed_policy_benchmarks(catalog);
        }
        let selection = catalog.as_ref().map(|catalog| {
            let primary = catalog.select(None);
            let selected_node = primary
                .nodes
                .iter()
                .find(|node| node.name == primary.target)
                .or_else(|| primary.nodes.first())
                .map(|node| node.id.clone());
            (primary.id.clone(), selected_node)
        });
        let policy_group_count = catalog.as_ref().map_or(0, |catalog| catalog.iter().count());
        self.catalog = catalog;
        if let Some((group, selected_node)) = selection {
            self.workspace
                .replace_source_selection(group, selected_node);
        } else {
            self.workspace.clear_source_selection();
        }
        self.source_providers = snapshot.providers;
        self.observed_routes = snapshot.observed_routes;
        self.active_connections = snapshot.connections;
        let system_proxy_applied = self
            .system_proxy
            .lock()
            .is_ok_and(|system| system.is_applied());
        self.proxy_mode = if snapshot.runtime.tun.enable {
            ProxyMode::Tun
        } else if system_proxy_applied {
            ProxyMode::System
        } else {
            ProxyMode::Off
        };
        self.routing_mode = snapshot.runtime.mode;
        self.proxy_runtime = snapshot.runtime;
        self.status = copy::app::snapshot_loaded(
            self.language(),
            policy_group_count,
            snapshot.active_connections,
        );
        self.controller = ControllerState::Connected {
            endpoint,
            version: snapshot.version,
            active_connections: snapshot.active_connections,
            download_total: snapshot.download_total,
            upload_total: snapshot.upload_total,
        };
    }

    fn sync_saved_node_selections(&mut self, cx: &mut Context<Self>) {
        if self.configuration_transfer.active {
            return;
        }
        if !matches!(&*self.runtime, ControllerRuntime::Managed { .. })
            || !matches!(self.controller, ControllerState::Connected { .. })
        {
            return;
        }
        let mut targets = Vec::new();
        if let Some(global) = self.managed_policies.node_selections.global() {
            targets.push(("GLOBAL".to_owned(), global.node_name.clone()));
        }
        targets.extend(self.policy_groups().filter_map(|group| {
            if !group.kind.allows_manual_selection() || group.name.eq_ignore_ascii_case("GLOBAL") {
                return None;
            }
            self.managed_policies
                .node_selections
                .policy_target(&group.name)
                .map(|target| (group.name.clone(), target.to_owned()))
        }));
        if targets.is_empty() {
            return;
        }

        let runtime = self.runtime.clone();
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let results = executor
                .spawn(async move {
                    targets
                        .into_iter()
                        .map(|(group, target)| {
                            let result = if group.eq_ignore_ascii_case("GLOBAL") {
                                runtime.select_global_node(&target)
                            } else {
                                runtime.select_policy_candidate(&group, &target)
                            };
                            (group, target, result)
                        })
                        .collect::<Vec<_>>()
                })
                .await;
            this.update(cx, |this, cx| {
                let mut applied = 0usize;
                let mut failed = 0usize;
                for (group, requested, result) in results {
                    match result {
                        Ok(snapshot) => {
                            let current = snapshot.current.as_deref().unwrap_or(&requested);
                            if let Some(catalog) = this.catalog.as_mut() {
                                let _ = catalog.apply_selector_target(&group, current);
                            }
                            if let Some(stored_group) = this
                                .managed_policies
                                .groups
                                .iter()
                                .find(|candidate| candidate.name == group)
                            {
                                this.managed_policies.runtime_generation =
                                    this.managed_policies.runtime_generation.wrapping_add(1);
                                this.managed_policies.runtime_states.insert(
                                    stored_group.id.clone(),
                                    ManagedPolicyRuntimeState::Ready {
                                        generation: this.managed_policies.runtime_generation,
                                        current: snapshot.current,
                                        candidates: snapshot.candidates,
                                    },
                                );
                            }
                            record_event(
                                LogLevel::Info,
                                "node.selection.restored",
                                format!("group={group}"),
                            );
                            applied += 1;
                        }
                        Err(error) => {
                            record_event(
                                LogLevel::Warn,
                                "node.selection.restore_failed",
                                format!("group={group} error={error}"),
                            );
                            failed += 1;
                        }
                    }
                }
                if applied > 0 || failed > 0 {
                    this.status = copy::app::selections_restored(this.language(), applied, failed);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn start_policy_group_benchmark(
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
        self.status =
            copy::app::testing_policy_candidates(language, &group.name, targets.len());
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
            _ => return,
        }
        self.persist_group_benchmarks();
        cx.notify();
    }

    fn start_live_runtime(
        &mut self,
        endpoint: &str,
        controller_secret: Option<&str>,
        cx: &mut Context<Self>,
    ) {
        self.live_generation = self.live_generation.wrapping_add(1);
        let generation = self.live_generation;
        self.managed_health_tick = 0;
        self.live_runtime = match LiveRuntimeSession::start(endpoint, controller_secret) {
            Ok(session) => Some(session),
            Err(error) => {
                self.live_status = LiveStreamStatus {
                    activity: LiveStreamPhase::StartFailed(error.to_string()),
                    logs: LiveStreamPhase::StartFailed(error.to_string()),
                };
                None
            }
        };
        if self.live_runtime.is_some() {
            self.poll_live_runtime(generation, cx);
        }
    }

    fn poll_live_runtime(&mut self, generation: u64, cx: &mut Context<Self>) {
        if generation != self.live_generation {
            return;
        }
        self.managed_health_tick = self.managed_health_tick.wrapping_add(1);
        if self.managed_health_tick >= 10 && !self.configuration_transfer.is_replacing() {
            self.managed_health_tick = 0;
            if self.fail_safe_stopped_managed_kernel(cx) {
                return;
            }
        }
        let Some(session) = self.live_runtime.as_ref() else {
            return;
        };
        let update = session.drain();
        self.live_status = update.status;
        for entry in update.logs {
            if self.kernel_logs.len() == 500 {
                self.kernel_logs.pop_front();
            }
            self.kernel_logs.push_back(entry);
        }
        if let Some(connections) = update.connections {
            self.active_connections = connections.connections;
            if let ControllerState::Connected {
                active_connections,
                download_total,
                upload_total,
                ..
            } = &mut self.controller
            {
                *active_connections = self.active_connections.len();
                *download_total = connections.download_total;
                *upload_total = connections.upload_total;
            }
        }
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(400))
                .await;
            if let Some(this) = this.upgrade() {
                this.update(cx, |this, cx| this.poll_live_runtime(generation, cx));
            }
        })
        .detach();
    }

    fn fail_safe_stopped_managed_kernel(&mut self, cx: &mut Context<Self>) -> bool {
        let failure = match self.runtime.managed_health() {
            #[cfg(any(test, feature = "snapshot-fixtures"))]
            Ok(ManagedRuntimeHealth::NotManaged) => return false,
            Ok(ManagedRuntimeHealth::Running) => return false,
            Ok(ManagedRuntimeHealth::Stopped) => self
                .language()
                .localized(copy::app::THE_MANIS_MANAGED_KERNEL_STOPPED_UNEXPECTEDLY)
                .to_owned(),
            Err(error) => error.to_string(),
        };

        let language = self.language();
        let mut recovery_error = None;
        let was_system_proxy = self.proxy_mode == ProxyMode::System;
        if was_system_proxy {
            match self.system_proxy.lock() {
                Ok(mut system) => {
                    if let Err(error) = system.disable_with_language(language) {
                        recovery_error = Some(error.to_string());
                    } else {
                        self.proxy_mode = ProxyMode::Off;
                    }
                }
                Err(_poisoned) => {
                    recovery_error = Some(
                        language
                            .localized(copy::app::SYSTEM_PROXY_STATE_LOCK_WAS_DAMAGED)
                            .to_owned(),
                    );
                }
            }
        } else if self.proxy_mode == ProxyMode::Tun {
            match self.tun_dns.lock() {
                Ok(mut dns) => match dns.disable_with_language(language) {
                    Ok(()) => self.proxy_mode = ProxyMode::Off,
                    Err(error) => recovery_error = Some(error.to_string()),
                },
                Err(_poisoned) => {
                    recovery_error = Some(
                        language
                            .localized(copy::app::TUN_DNS_STATE_LOCK_WAS_DAMAGED)
                            .to_owned(),
                    );
                }
            }
        }

        self.live_generation = self.live_generation.wrapping_add(1);
        self.live_runtime = None;
        let endpoint = self.runtime.endpoint_label();
        self.controller = ControllerState::Failed {
            endpoint,
            message: failure.clone(),
        };
        self.status = match recovery_error {
            None if was_system_proxy => {
                format!(
                    "{}{}",
                    failure,
                    language.localized(
                        copy::app::SYSTEM_PROXY_WAS_RESTORED_RECONNECT_TO_RESTART_THE_KERNEL
                    )
                )
            }
            None => format!(
                "{}{}",
                failure,
                language.localized(copy::app::RECONNECT_TO_RESTART_THE_KERNEL)
            ),
            Some(recovery_error) => {
                format!(
                    "{}{}{}",
                    failure,
                    language.localized(copy::app::AUTOMATIC_SYSTEM_PROXY_RECOVERY_FAILED),
                    recovery_error
                )
            }
        };
        cx.notify();
        true
    }
}

struct KernelSwitchFailure {
    message: String,
    proxy_mode_restored: bool,
}

fn perform_kernel_switch(
    requested: KernelKind,
    previous_kind: KernelKind,
    previous_mode: ProxyMode,
    prepare: impl FnOnce() -> Result<KernelRuntime, String>,
    mut save_kernel_kind: impl FnMut(KernelKind) -> Result<(), String>,
    restore_proxy_mode: impl FnOnce(ProxyMode) -> Result<(), String>,
    stop_previous: impl FnOnce() -> Result<(), String>,
) -> Result<KernelRuntime, KernelSwitchFailure> {
    let prepared = prepare().map_err(|message| KernelSwitchFailure {
        message,
        proxy_mode_restored: false,
    })?;
    save_kernel_kind(requested).map_err(|message| KernelSwitchFailure {
        message,
        proxy_mode_restored: false,
    })?;
    let mut proxy_mode_restored = false;
    if previous_mode != ProxyMode::Off {
        if let Err(message) = restore_proxy_mode(previous_mode) {
            let message =
                message_with_selection_rollback(message, save_kernel_kind(previous_kind));
            return Err(KernelSwitchFailure {
                message,
                proxy_mode_restored: false,
            });
        }
        proxy_mode_restored = true;
    }
    if let Err(message) = stop_previous() {
        let message = message_with_selection_rollback(message, save_kernel_kind(previous_kind));
        return Err(KernelSwitchFailure {
            message,
            proxy_mode_restored,
        });
    }
    Ok(prepared)
}

fn message_with_selection_rollback(
    message: String,
    rollback: Result<(), String>,
) -> String {
    match rollback {
        Ok(()) => message,
        Err(rollback) => {
            format!("{message}; also could not restore the previous kernel selection: {rollback}")
        }
    }
}

#[cfg(test)]
mod runtime_lifecycle_tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use manis_core::{KernelKind, ProxyMode};

    use gpui::AppContext as _;

    use super::{
        ControllerRuntime, KernelRuntime, ManisApp, perform_kernel_switch,
    };

    fn fixture_runtime() -> KernelRuntime {
        KernelRuntime::mihomo(ControllerRuntime::Fixture {
            endpoint: "http://127.0.0.1:9090".to_owned(),
        })
    }

    #[test]
    fn active_proxy_is_restored_before_previous_kernel_stops() {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let prepared = fixture_runtime();

        let result = perform_kernel_switch(
            KernelKind::SingBox,
            KernelKind::Mihomo,
            ProxyMode::System,
            {
                let calls = calls.clone();
                let prepared = prepared.clone();
                move || {
                    calls.borrow_mut().push("prepare".to_owned());
                    Ok(prepared)
                }
            },
            {
                let calls = calls.clone();
                move |kind| {
                    calls
                        .borrow_mut()
                        .push(format!("save:{}", kind.persistence_key()));
                    Ok(())
                }
            },
            {
                let calls = calls.clone();
                move |mode| {
                    calls
                        .borrow_mut()
                        .push(format!("restore:{mode:?}->Off"));
                    Ok(())
                }
            },
            {
                let calls = calls.clone();
                move || {
                    calls.borrow_mut().push("stop".to_owned());
                    Ok(())
                }
            },
        );

        assert!(result.is_ok());
        assert_eq!(
            calls.borrow().as_slice(),
            [
                "prepare",
                "save:sing-box",
                "restore:System->Off",
                "stop"
            ]
        );
    }

    #[gpui::test]
    fn switching_kernel_reserves_proxy_mode_even_when_proxy_is_off(
        cx: &mut gpui::TestAppContext,
    ) {
        let store = std::env::temp_dir().join(format!(
            "manis-kernel-switch-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time")
                .as_nanos()
        ));
        let app = cx.new(|_| {
            ManisApp::with_fixture_controller_and_subscription_store(
                "http://127.0.0.1:9090",
                store.join("subscriptions"),
            )
        });

        app.update(cx, |app, cx| {
            assert_eq!(app.proxy_mode, ProxyMode::Off);
            assert!(app.proxy_mode_busy.is_none());

            app.switch_kernel(KernelKind::SingBox, cx);

            assert_eq!(app.proxy_mode_busy, Some(ProxyMode::Off));
            app.apply_proxy_mode(ProxyMode::System, cx);
            assert_eq!(app.proxy_mode, ProxyMode::Off);
            assert_eq!(app.proxy_mode_busy, Some(ProxyMode::Off));
        });
        let _ = std::fs::remove_dir_all(store);
    }

    #[test]
    fn proxy_cleanup_failure_keeps_previous_kernel_running_and_rolls_back_selection() {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let prepared = fixture_runtime();

        let result = perform_kernel_switch(
            KernelKind::SingBox,
            KernelKind::Mihomo,
            ProxyMode::Tun,
            {
                let calls = calls.clone();
                let prepared = prepared.clone();
                move || {
                    calls.borrow_mut().push("prepare".to_owned());
                    Ok(prepared)
                }
            },
            {
                let calls = calls.clone();
                move |kind| {
                    calls
                        .borrow_mut()
                        .push(format!("save:{}", kind.persistence_key()));
                    Ok(())
                }
            },
            {
                let calls = calls.clone();
                move |mode| {
                    calls
                        .borrow_mut()
                        .push(format!("restore:{mode:?}->Off"));
                    Err("restore failed".to_owned())
                }
            },
            {
                let calls = calls.clone();
                move || {
                    calls.borrow_mut().push("stop".to_owned());
                    Ok(())
                }
            },
        );

        let Err(failure) = result else {
            panic!("cleanup failure must abort the kernel switch");
        };
        assert_eq!(failure.message, "restore failed");
        assert!(!failure.proxy_mode_restored);
        assert_eq!(
            calls.borrow().as_slice(),
            [
                "prepare",
                "save:sing-box",
                "restore:Tun->Off",
                "save:mihomo"
            ]
        );
    }

    #[test]
    fn stop_failure_reports_restored_proxy_mode() {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let prepared = fixture_runtime();

        let result = perform_kernel_switch(
            KernelKind::SingBox,
            KernelKind::Mihomo,
            ProxyMode::System,
            {
                let calls = calls.clone();
                let prepared = prepared.clone();
                move || {
                    calls.borrow_mut().push("prepare".to_owned());
                    Ok(prepared)
                }
            },
            {
                let calls = calls.clone();
                move |kind| {
                    calls
                        .borrow_mut()
                        .push(format!("save:{}", kind.persistence_key()));
                    Ok(())
                }
            },
            {
                let calls = calls.clone();
                move |mode| {
                    calls
                        .borrow_mut()
                        .push(format!("restore:{mode:?}->Off"));
                    Ok(())
                }
            },
            {
                let calls = calls.clone();
                move || {
                    calls.borrow_mut().push("stop".to_owned());
                    Err("stop failed".to_owned())
                }
            },
        );

        let Err(failure) = result else {
            panic!("stop failure must fail the kernel switch");
        };
        assert_eq!(failure.message, "stop failed");
        assert!(failure.proxy_mode_restored);
        assert_eq!(
            calls.borrow().as_slice(),
            [
                "prepare",
                "save:sing-box",
                "restore:System->Off",
                "stop",
                "save:mihomo"
            ]
        );
    }

    #[test]
    fn rollback_failure_is_reported_when_cleanup_fails() {
        let prepared = fixture_runtime();

        let result = perform_kernel_switch(
            KernelKind::SingBox,
            KernelKind::Mihomo,
            ProxyMode::System,
            move || Ok(prepared),
            |kind| match kind {
                KernelKind::SingBox => Ok(()),
                KernelKind::Mihomo => Err("selection write failed".to_owned()),
            },
            |_mode| Err("restore failed".to_owned()),
            || Ok(()),
        );

        let Err(failure) = result else {
            panic!("cleanup failure must abort the kernel switch");
        };
        assert_eq!(
            failure.message,
            "restore failed; also could not restore the previous kernel selection: selection write failed"
        );
    }
}
