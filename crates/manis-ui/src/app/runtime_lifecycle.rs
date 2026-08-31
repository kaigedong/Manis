impl ManisApp {
    fn start_app_update_polling(cx: &mut Context<Self>) {
        let timer = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            loop {
                if this
                    .update(cx, |this, cx| this.check_for_app_update(false, cx))
                    .is_err()
                {
                    break;
                }
                timer.timer(Duration::from_hours(1)).await;
            }
        })
        .detach();
    }

    fn check_for_app_update(&mut self, manual: bool, cx: &mut Context<Self>) {
        if self.app_update_state.is_busy()
            || matches!(self.app_update_state, AppUpdateState::Ready(_))
        {
            return;
        }
        let Ok(app_path) = cx.app_path() else {
            self.app_update_state = AppUpdateState::Unsupported;
            if manual {
                self.language()
                    .localized(copy::app_update::UNSUPPORTED)
                    .clone_into(&mut self.status);
            }
            cx.notify();
            return;
        };
        if !app_update::installation_supported(&app_path) {
            self.app_update_state = AppUpdateState::Unsupported;
            if manual {
                self.language()
                    .localized(copy::app_update::UNSUPPORTED)
                    .clone_into(&mut self.status);
            }
            cx.notify();
            return;
        }

        self.app_update_state = AppUpdateState::Checking;
        if manual {
            self.language()
                .localized(copy::app_update::CHECKING)
                .clone_into(&mut self.status);
        }
        cx.notify();

        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let checked = executor
                .spawn(async { app_update::check_for_update() })
                .await;
            let Some(available) = (match checked {
                Ok(available) => available,
                Err(error) => {
                    this.update(cx, |this, cx| {
                        this.finish_app_update_failure(error, manual, cx);
                    })
                    .ok();
                    return;
                }
            }) else {
                this.update(cx, |this, cx| {
                    this.app_update_state = AppUpdateState::Current;
                    if manual {
                        this.language()
                            .localized(copy::app_update::UP_TO_DATE)
                            .clone_into(&mut this.status);
                    }
                    cx.notify();
                })
                .ok();
                return;
            };

            let version = available.version.clone();
            this.update(cx, |this, cx| {
                this.app_update_state = AppUpdateState::Downloading(version.clone());
                if manual {
                    this.language()
                        .localized(copy::app_update::DOWNLOADING)
                        .clone_into(&mut this.status);
                }
                cx.notify();
            })
            .ok();

            let staged = executor
                .spawn(async move { app_update::stage_update(&available) })
                .await;
            this.update(cx, |this, cx| match staged {
                Ok(staged) => {
                    record_event(
                        LogLevel::Info,
                        "app.update.ready",
                        format!("version={}", staged.version),
                    );
                    this.status = copy::app_update::ready_version(
                        this.language(),
                        &staged.version,
                    );
                    this.app_update_state = AppUpdateState::Ready(staged);
                    cx.notify();
                }
                Err(error) => this.finish_app_update_failure(error, manual, cx),
            })
            .ok();
        })
        .detach();
    }

    fn finish_app_update_failure(
        &mut self,
        error: AppUpdateError,
        manual: bool,
        cx: &mut Context<Self>,
    ) {
        record_event(LogLevel::Warn, "app.update.failed", error.to_string());
        if manual {
            self.app_update_state = AppUpdateState::Failed(error);
            self.status = format!(
                "{}: {}",
                self.language().localized(copy::app_update::UPDATE_FAILED),
                copy::app_update::error(self.language(), error)
            );
        } else {
            self.app_update_state = AppUpdateState::Idle;
        }
        cx.notify();
    }

    fn restart_with_app_update(&mut self, cx: &mut Context<Self>) {
        let AppUpdateState::Ready(staged) = self.app_update_state.clone() else {
            return;
        };
        let Ok(app_path) = cx.app_path() else {
            self.finish_app_update_failure(AppUpdateError::UnsupportedInstallation, true, cx);
            return;
        };
        let version = staged.version.clone();
        self.app_update_state = AppUpdateState::Installing(version.clone());
        self.language()
            .localized(copy::app_update::INSTALLING)
            .clone_into(&mut self.status);
        cx.notify();

        let executor = cx.background_executor().clone();
        let runtime = self.runtime.clone();
        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn(async move {
                    // The approved controller belongs to the current bundle. Stop its core
                    // before replacing that bundle, while its pinned signature still matches.
                    runtime.stop_managed().map_err(|error| {
                        record_event(LogLevel::Error, "app.update.stop.failed", error);
                        AppUpdateError::InstallFailed
                    })?;
                    app_update::install_staged_update(&staged, &app_path)
                })
                .await;
            this.update(cx, |this, cx| match result {
                Ok(restart_path) => {
                    record_event(
                        LogLevel::Info,
                        "app.update.install.succeeded",
                        format!("version={version}"),
                    );
                    if let Some(path) = restart_path {
                        cx.set_restart_path(path);
                    }
                    cx.restart();
                }
                Err(error) => {
                    // A stopped core must not leave system proxy or DNS settings pointing at it.
                    this.shutdown_for_quit(cx).detach();
                    this.finish_app_update_failure(error, true, cx);
                }
            })
            .ok();
        })
        .detach();
    }

    fn switch_kernel(&mut self, requested: KernelKind, cx: &mut Context<Self>) {
        let language = self.language();
        if self.kernel_switch_state.is_busy() || self.runtime.kind() == requested {
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
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn(async move {
                    let prepared = KernelRuntime::prepare_with_language(
                        requested,
                        Some(&store_dir),
                        language,
                    )?;
                    kernel::save_kernel_kind_in(&store_dir, requested)
                        .map_err(|error| error.to_string())?;
                    if let Err(message) = previous.stop_managed() {
                        let _ = kernel::save_kernel_kind_in(&store_dir, previous_kind);
                        return Err(message);
                    }
                    Ok::<_, String>(prepared)
                })
                .await;
            this.update(cx, |this, cx| {
                let language = this.language();
                this.kernel_switch_state = KernelSwitchState::Idle;
                match result {
                    Ok(runtime) => {
                        this.runtime = runtime;
                        this.controller = ControllerState::Disconnected;
                        this.live_generation = this.live_generation.wrapping_add(1);
                        this.live_runtime = None;
                        this.proxy_mode = ProxyMode::Off;
                        this.status =
                            copy::app::switched_kernel(language, requested.display_name());
                    }
                    Err(message) => {
                        this.status = copy::app::kernel_switch_failed(
                            language,
                            requested.display_name(),
                            &message,
                        );
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn update_mihomo_core(&mut self, cx: &mut Context<Self>) {
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
        if matches!(self.controller, ControllerState::Connecting { .. }) {
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
        let candidate_names = group
            .nodes
            .iter()
            .map(|node| node.name.clone())
            .collect::<Vec<_>>();
        if candidate_names.is_empty() {
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
            copy::app::testing_policy_candidates(language, &group.name, candidate_names.len());
        trace_ui(UiEvent::GroupBenchmarkStarted);

        let runtime = self.runtime.clone();
        let group_id = group.id.clone();
        let group_name = group.name.clone();
        let group_kind = group.kind;
        let total = candidate_names.len();
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
                            .test_proxy_candidates_delay(&group_name, &candidate_names)
                            .map(|delays| mihomo::PolicyGroupBenchmarkSnapshot {
                                delays,
                                current: None,
                            })
                    } else {
                        runtime.test_policy_group_delay(&group_name, &candidate_names)
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
            Err(error) => (None, None, Some(error.to_string())),
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
            None => state.fail(generation),
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
        if self.managed_health_tick >= 10 {
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
        self.dropped_kernel_logs = self.dropped_kernel_logs.saturating_add(update.dropped_logs);
        for entry in update.logs {
            if self.kernel_logs.len() == 500 {
                self.kernel_logs.pop_front();
                self.dropped_kernel_logs = self.dropped_kernel_logs.saturating_add(1);
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
