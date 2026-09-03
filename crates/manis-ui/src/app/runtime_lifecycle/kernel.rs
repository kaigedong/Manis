use super::{
    Context, ControllerRuntime, ControllerState, LiveStreamPhase, LiveStreamStatus, LoadedSnapshot,
    LogLevel, ManagedPolicyRuntimeState, ManisApp, MihomoCoreUpdateOutcome, MihomoCoreUpdateState,
    ProxyMode, UiEvent, begin_operation, copy, core_update, mihomo, perform_mihomo_core_update,
    record_event, record_operation, trace_ui,
};

impl ManisApp {
    pub(in crate::app) fn update_mihomo_core(&mut self, cx: &mut Context<Self>) {
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

    pub(in crate::app) fn connect_mihomo(&mut self, cx: &mut Context<Self>) {
        if self.configuration_transfer.active
            || matches!(self.controller, ControllerState::Connecting { .. })
        {
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

    pub(in crate::app) fn apply_mihomo_snapshot(
        &mut self,
        endpoint: String,
        snapshot: LoadedSnapshot,
    ) {
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
                .target
                .as_deref()
                .and_then(|target| primary.nodes.iter().find(|node| node.name == target))
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
        let system_proxy_applied = match self.system_proxy.lock() {
            Ok(system) => Some(system.is_applied()),
            Err(_poisoned) => {
                record_event(
                    LogLevel::Error,
                    "system_proxy.state_unavailable",
                    "reason=lock_poisoned",
                );
                None
            }
        };
        self.proxy_mode = if snapshot.runtime.tun.enable {
            ProxyMode::Tun
        } else {
            match system_proxy_applied {
                Some(true) => ProxyMode::System,
                Some(false) => ProxyMode::Off,
                None => self.proxy_mode,
            }
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
}
