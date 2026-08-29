impl ManisApp {
    /// Reports why the requested proxy mode cannot be applied right now.
    ///
    /// The tray uses this to disable a menu item and explain itself instead of letting the user
    /// click an entry that would silently fail.
    pub(crate) fn proxy_mode_block(&self, requested: ProxyMode) -> Option<ProxyModeBlock> {
        proxy_mode_block(
            requested,
            self.proxy_mode_busy,
            if matches!(self.controller, ControllerState::Connected { .. }) {
                ControllerReadiness::Connected
            } else {
                ControllerReadiness::Disconnected
            },
            if self.runtime.is_fixture() {
                TunSupport::FixtureReadOnly
            } else if self
                .runtime
                .capabilities()
                .supports(manis_core::KernelCapability::Tun)
            {
                TunSupport::Supported
            } else {
                TunSupport::KernelUnsupported
            },
        )
    }

    /// Returns the proxy mode the tray shows as checked.
    pub(crate) const fn active_proxy_mode(&self) -> ProxyMode {
        self.proxy_mode
    }

    /// Applies the mode a checkable control stands for, clearing it when it is already active.
    pub(crate) fn toggle_proxy_mode(&mut self, selected: ProxyMode, cx: &mut Context<Self>) {
        self.apply_proxy_mode(self.proxy_mode.toggled(selected), cx);
    }

    fn apply_proxy_mode(&mut self, requested: ProxyMode, cx: &mut Context<Self>) {
        let language = self.language();
        let operation = begin_operation(
            "proxy.mode.requested",
            format!(
                "from={:?} to={requested:?} controller_state={} profile={}",
                self.proxy_mode,
                controller_state_label(&self.controller),
                self.runtime.profile_source().diagnostic_key()
            ),
        );
        if self.reject_proxy_mode_request(requested, operation, cx) {
            return;
        }

        let runtime = self.runtime.clone();
        let system_proxy = self.system_proxy.clone();
        let tun_dns = self.tun_dns.clone();
        let previous = self.proxy_mode;
        let mixed_port = self.proxy_runtime.mixed_port.filter(|port| *port > 0);
        let ports = ProxyPorts {
            http: self
                .proxy_runtime
                .port
                .filter(|port| *port > 0)
                .or(mixed_port),
            socks: self
                .proxy_runtime
                .socks_port
                .filter(|port| *port > 0)
                .or(mixed_port),
        };
        self.proxy_mode_busy = Some(requested);
        self.status = match requested {
            ProxyMode::Tun => language
                .localized(copy::app::PREPARING_THE_MACOS_TUN_HELPER_AND_TRAFFIC_ROUTE)
                .to_owned(),
            _ => format!(
                "{}{}…",
                language.localized(copy::app::SWITCHING_TO),
                proxy_mode_label(language, requested)
            ),
        };

        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn(async move {
                    apply_proxy_mode_transition(
                        &runtime,
                        &system_proxy,
                        &tun_dns,
                        previous,
                        requested,
                        ports,
                        language,
                    )
                })
                .await;
            this.update(cx, |this, cx| {
                this.finish_proxy_mode_change(requested, operation, result, cx);
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn reject_proxy_mode_request(
        &mut self,
        requested: ProxyMode,
        operation: u64,
        cx: &mut Context<Self>,
    ) -> bool {
        let language = self.language();
        if self.proxy_mode_busy.is_some() || requested == self.proxy_mode {
            record_operation(
                operation,
                LogLevel::Warn,
                "proxy.mode.ignored",
                format!(
                    "busy={} already_selected={}",
                    self.proxy_mode_busy.is_some(),
                    requested == self.proxy_mode
                ),
            );
            return true;
        }
        let (reason, status) = if !matches!(self.controller, ControllerState::Connected { .. }) {
            (
                "controller_not_connected",
                format!(
                    "{} {}",
                    language.localized(copy::app::CONNECT_BEFORE_CHANGING_PROXY_MODE),
                    self.runtime.kind().display_name(),
                ),
            )
        } else if requested == ProxyMode::Tun
            && !self
                .runtime
                .capabilities()
                .supports(manis_core::KernelCapability::Tun)
        {
            (
                "kernel_has_no_tun_capability",
                language
                    .localized(copy::app::TUN_IS_NOT_YET_AVAILABLE_FOR_THE_SING_BOX_ADAPTER)
                    .to_owned(),
            )
        } else if requested == ProxyMode::Tun && self.runtime.is_fixture() {
            (
                "fixture_read_only",
                language
                    .localized(copy::app::TEST_FIXTURES_CANNOT_ENABLE_TUN)
                    .to_owned(),
            )
        } else {
            return false;
        };
        record_operation(
            operation,
            LogLevel::Error,
            "proxy.mode.rejected",
            format!("reason={reason}"),
        );
        trace_ui(UiEvent::ProxyModeFailed);
        self.status = status;
        cx.notify();
        true
    }

    fn finish_proxy_mode_change(
        &mut self,
        requested: ProxyMode,
        operation: u64,
        result: Result<(), String>,
        cx: &mut Context<Self>,
    ) {
        let language = self.language();
        self.proxy_mode_busy = None;
        match result {
            Ok(()) => {
                record_operation(
                    operation,
                    LogLevel::Info,
                    "proxy.mode.succeeded",
                    format!("active={requested:?}"),
                );
                self.proxy_mode = requested;
                match requested {
                    ProxyMode::Off => trace_ui(UiEvent::SystemProxyDisabled),
                    ProxyMode::System => trace_ui(UiEvent::SystemProxyEnabled),
                    ProxyMode::Tun => trace_ui(UiEvent::TunProxyEnabled),
                }
                self.status = format!(
                    "{}{}",
                    proxy_mode_label(language, requested),
                    language.localized(copy::app::ENABLED)
                );
            }
            Err(message) => {
                record_operation(
                    operation,
                    LogLevel::Error,
                    "proxy.mode.failed",
                    message.clone(),
                );
                trace_ui(UiEvent::ProxyModeFailed);
                self.status = format!(
                    "{}{message}",
                    language.localized(copy::app::FAILED_TO_CHANGE_PROXY_MODE)
                );
            }
        }
        cx.notify();
    }

    fn apply_routing_mode(&mut self, requested: RoutingMode, cx: &mut Context<Self>) {
        let language = self.language();
        let operation = begin_operation(
            "routing.mode.requested",
            format!(
                "from={:?} to={requested:?} controller_state={} profile={}",
                self.routing_mode,
                controller_state_label(&self.controller),
                self.runtime.profile_source().diagnostic_key()
            ),
        );
        if self.routing_mode_busy.is_some() || requested == self.routing_mode {
            record_operation(
                operation,
                LogLevel::Warn,
                "routing.mode.ignored",
                format!(
                    "busy={} already_selected={}",
                    self.routing_mode_busy.is_some(),
                    requested == self.routing_mode
                ),
            );
            return;
        }
        if !matches!(self.controller, ControllerState::Connected { .. }) {
            record_operation(
                operation,
                LogLevel::Error,
                "routing.mode.rejected",
                "reason=controller_not_connected",
            );
            trace_ui(UiEvent::RoutingModeFailed);
            language
                .localized(copy::app::CONNECT_TO_THE_KERNEL_BEFORE_CHANGING_ROUTING_MODE)
                .clone_into(&mut self.status);
            cx.notify();
            return;
        }
        if self.runtime.is_fixture() {
            record_operation(
                operation,
                LogLevel::Error,
                "routing.mode.rejected",
                "reason=fixture_read_only",
            );
            trace_ui(UiEvent::RoutingModeFailed);
            language
                .localized(copy::app::TEST_FIXTURES_CANNOT_CHANGE_ROUTING_MODE)
                .clone_into(&mut self.status);
            cx.notify();
            return;
        }

        self.routing_mode_busy = Some(requested);
        self.status = format!(
            "{}{}…",
            language.localized(copy::app::SWITCHING_TO),
            routing_mode_label(language, requested)
        );
        let runtime = self.runtime.clone();
        let store_dir = self.subscription_store_dir.clone();
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn(async move {
                    runtime.set_routing_mode(requested)?;
                    let persistence = store_dir
                        .as_deref()
                        .map(|directory| mihomo::save_routing_mode_in(directory, requested))
                        .transpose();
                    Ok::<_, mihomo::LoadError>(persistence)
                })
                .await;
            this.update(cx, |this, cx| {
                this.finish_routing_mode_change(requested, operation, result, cx);
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn finish_routing_mode_change(
        &mut self,
        requested: RoutingMode,
        operation: u64,
        result: RoutingModeApplyResult,
        cx: &mut Context<Self>,
    ) {
        let language = self.language();
        self.routing_mode_busy = None;
        match result {
            Ok(persistence) => {
                record_operation(
                    operation,
                    LogLevel::Info,
                    "routing.mode.succeeded",
                    format!("active={requested:?} persisted={}", persistence.is_ok()),
                );
                self.routing_mode = requested;
                self.proxy_runtime.mode = requested;
                trace_ui(UiEvent::RoutingModeChanged);
                self.status = match requested {
                    RoutingMode::Global => self.global_target().map_or_else(
                        || {
                            language
                                .localized(copy::app::GLOBAL_MODE_ENABLED_CHOOSE_THE_GLOBAL_EXIT_ON_THE_NODES)
                                .to_owned()
                        },
                        |target| copy::app::global_mode_current_exit(language, target),
                    ),
                    _ => format!(
                        "{}{}",
                        routing_mode_label(language, requested),
                        language.localized(copy::app::ENABLED)
                    ),
                };
                if persistence.is_err() {
                    self.status.push_str(
                        language.localized(copy::app::RESTART_PREFERENCE_COULD_NOT_BE_SAVED),
                    );
                }
            }
            Err(error) => {
                record_operation(
                    operation,
                    LogLevel::Error,
                    "routing.mode.failed",
                    error.to_string(),
                );
                trace_ui(UiEvent::RoutingModeFailed);
                self.status = format!(
                    "{}{error}",
                    language.localized(copy::app::FAILED_TO_CHANGE_ROUTING_MODE)
                );
            }
        }
        cx.notify();
    }

    fn select_global_node(&mut self, selected: NodeIdentity, cx: &mut Context<Self>) {
        let language = self.language();
        let selected_name = selected.node_name.clone();
        let operation = begin_operation(
            "global.node.requested",
            format!(
                "controller_state={} profile={} candidate_selected=true",
                controller_state_label(&self.controller),
                self.runtime.profile_source().diagnostic_key()
            ),
        );
        if self.global_selection_busy.is_some() {
            record_operation(
                operation,
                LogLevel::Warn,
                "global.node.ignored",
                "reason=selection_busy",
            );
            return;
        }
        let previous = self.managed_policies.node_selections.clone();
        self.managed_policies.node_selections.set_global(selected);
        if let Some(directory) = self.subscription_store_dir.as_deref()
            && let Err(error) = mihomo::save_node_selection_preferences_in(
                directory,
                &self.managed_policies.node_selections,
            )
        {
            self.managed_policies.node_selections = previous;
            record_operation(
                operation,
                LogLevel::Error,
                "global.node.persistence_failed",
                error.to_string(),
            );
            trace_ui(UiEvent::GlobalNodeSelectionFailed);
            self.status = format!(
                "{}{error}",
                language.localized(copy::app::COULD_NOT_SAVE_THE_GLOBAL_NODE)
            );
            cx.notify();
            return;
        }
        record_operation(
            operation,
            LogLevel::Info,
            "global.node.saved",
            "saved_to_workspace=true",
        );
        trace_ui(UiEvent::GlobalNodeSelected);

        let can_apply_now = matches!(self.controller, ControllerState::Connected { .. })
            && matches!(&*self.runtime, ControllerRuntime::Managed { .. });
        if !can_apply_now {
            record_operation(
                operation,
                LogLevel::Info,
                "global.node.deferred",
                "reason=managed_controller_not_connected",
            );
            self.status = copy::app::saved_global_exit(language, &selected_name, false);
            cx.notify();
            return;
        }

        self.global_selection_busy = Some(selected_name.clone());
        self.status = copy::app::selecting_global_node(language, &selected_name);
        let runtime = self.runtime.clone();
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn({
                    let selected_name = selected_name.clone();
                    async move { runtime.select_global_node(&selected_name) }
                })
                .await;
            this.update(cx, |this, cx| {
                this.finish_global_node_selection(&selected_name, operation, result, cx);
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn finish_global_node_selection(
        &mut self,
        selected_name: &str,
        operation: u64,
        result: Result<mihomo::ManagedPolicyRuntimeSnapshot, mihomo::LoadError>,
        cx: &mut Context<Self>,
    ) {
        let language = self.language();
        self.global_selection_busy = None;
        match result {
            Ok(snapshot) => {
                record_operation(
                    operation,
                    LogLevel::Info,
                    "global.node.succeeded",
                    "global selector confirmed target",
                );
                let current = snapshot.current.as_deref().unwrap_or(selected_name);
                trace_ui(UiEvent::GlobalNodeSelected);
                self.status = copy::app::saved_global_exit(
                    language,
                    current,
                    self.routing_mode == RoutingMode::Global,
                );
            }
            Err(error) => {
                record_operation(
                    operation,
                    LogLevel::Error,
                    "global.node.failed",
                    error.to_string(),
                );
                self.status = copy::app::global_exit_apply_failed(
                    language,
                    selected_name,
                    &error.to_string(),
                );
            }
        }
        cx.notify();
    }

    fn select_policy_node(&mut self, request: PolicySelectionRequest, cx: &mut Context<Self>) {
        let PolicySelectionRequest {
            group_id,
            group_name,
            node_id,
            node_name,
        } = request;
        let language = self.language();
        let operation = begin_operation(
            "policy.node.requested",
            format!("group={group_name} candidate_selected=true"),
        );
        if self.reject_policy_selection_request(&group_id, &group_name, &node_name, operation, cx) {
            return;
        }

        let Some(previous) = self.persist_policy_selection(
            &group_id,
            &group_name,
            &node_id,
            &node_name,
            operation,
            cx,
        ) else {
            return;
        };

        let can_apply_now = matches!(self.controller, ControllerState::Connected { .. })
            && matches!(&*self.runtime, ControllerRuntime::Managed { .. });
        if !can_apply_now {
            self.status = copy::app::deferred_policy_selection(language, &group_name, &node_name);
            cx.notify();
            return;
        }

        if let Some((stored_group_id, candidates)) = self
            .managed_policies
            .groups
            .iter()
            .find(|group| group.name == group_name)
            .map(|group| {
                (
                    group.id.clone(),
                    self.managed_policy_candidate_names(group)
                        .into_iter()
                        .collect::<BTreeSet<_>>(),
                )
            })
        {
            self.managed_policies.runtime_generation =
                self.managed_policies.runtime_generation.wrapping_add(1);
            let generation = self.managed_policies.runtime_generation;
            let state = self
                .managed_policies
                .runtime_states
                .entry(stored_group_id)
                .or_default();
            if !state.begin_selection(generation, &node_name) {
                *state = ManagedPolicyRuntimeState::Selecting {
                    generation,
                    current: previous.policy_target(&group_name).map(str::to_owned),
                    candidates,
                    pending: node_name.clone(),
                };
            }
        }
        self.policy_selection_busy = Some(node_name.clone());
        self.status = copy::app::setting_policy_node(language, &group_name, &node_name);
        let completion = PolicySelectionRequest {
            group_id,
            group_name: group_name.clone(),
            node_id,
            node_name: node_name.clone(),
        };
        let runtime = self.runtime.clone();
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn({
                    let group_name = group_name.clone();
                    let node_name = node_name.clone();
                    async move { runtime.select_policy_candidate(&group_name, &node_name) }
                })
                .await;
            this.update(cx, |this, cx| {
                this.finish_policy_node_selection(completion, operation, result, cx);
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn reject_policy_selection_request(
        &mut self,
        group_id: &PolicyGroupId,
        group_name: &str,
        node_name: &str,
        operation: u64,
        cx: &mut Context<Self>,
    ) -> bool {
        let language = self.language();
        if self.policy_selection_busy.is_some() {
            record_operation(
                operation,
                LogLevel::Warn,
                "policy.node.ignored",
                "reason=selection_busy",
            );
            return true;
        }
        let stored_group = self
            .managed_policies
            .groups
            .iter()
            .find(|group| group.id == group_id.as_str() || group.name == group_name);
        if !matches!(self.controller, ControllerState::Connected { .. }) && stored_group.is_none() {
            record_operation(
                operation,
                LogLevel::Warn,
                "policy.node.rejected",
                "reason=runtime_policy_unavailable",
            );
            language
                .localized(copy::app::START_MIHOMO_BEFORE_SELECTING_A_NODE_FOR_THIS_POLICY_GROUP)
                .clone_into(&mut self.status);
            cx.notify();
            return true;
        }
        let catalog_allows = self
            .policy_groups()
            .find(|group| group.id == *group_id || group.name == group_name)
            .map(|group| {
                group.kind.allows_manual_selection()
                    && group.nodes.iter().any(|node| node.name == node_name)
            });
        let stored_group_allows = stored_group.map(|group| {
            group.strategy == ManagedPolicyStrategy::Manual
                && self
                    .managed_policy_candidate_names(group)
                    .iter()
                    .any(|candidate| candidate == node_name)
        });
        if policy_target_is_selectable(
            matches!(self.controller, ControllerState::Connected { .. }),
            catalog_allows,
            stored_group_allows,
        ) {
            return false;
        }
        record_operation(
            operation,
            LogLevel::Error,
            "policy.node.rejected",
            "reason=not_manual_candidate",
        );
        language
            .localized(copy::app::ONLY_A_CANDIDATE_INSIDE_A_MANUAL_POLICY_CAN_BE_SELECTED)
            .clone_into(&mut self.status);
        cx.notify();
        true
    }

    fn persist_policy_selection(
        &mut self,
        group_id: &PolicyGroupId,
        group_name: &str,
        node_id: &ProxyId,
        node_name: &str,
        operation: u64,
        cx: &mut Context<Self>,
    ) -> Option<mihomo::NodeSelectionPreferences> {
        let language = self.language();
        let previous = self.managed_policies.node_selections.clone();
        if let Err(error) = self
            .managed_policies
            .node_selections
            .set_policy_target(group_name, node_name)
        {
            record_operation(
                operation,
                LogLevel::Error,
                "policy.node.rejected",
                error.to_string(),
            );
            language
                .localized(copy::app::THIS_POLICY_SELECTION_CANNOT_BE_SAVED)
                .clone_into(&mut self.status);
            cx.notify();
            return None;
        }
        if let Some(directory) = self.subscription_store_dir.as_deref()
            && let Err(error) = mihomo::save_node_selection_preferences_in(
                directory,
                &self.managed_policies.node_selections,
            )
        {
            self.managed_policies.node_selections = previous;
            record_operation(
                operation,
                LogLevel::Error,
                "policy.node.persistence_failed",
                error.to_string(),
            );
            self.status = format!(
                "{}{error}",
                language.localized(copy::app::COULD_NOT_SAVE_THE_POLICY_SELECTION)
            );
            cx.notify();
            return None;
        }
        let catalog_selection = self
            .policy_groups()
            .find(|group| group.name == group_name)
            .and_then(|group| {
                group
                    .nodes
                    .iter()
                    .find(|node| node.name == node_name)
                    .map(|node| (group.id.clone(), node.id.clone()))
            });
        if let Some(catalog) = self.catalog.as_mut() {
            let _ = catalog.apply_selector_target(group_name, node_name);
        }
        if self.workspace.selected_group.as_ref() == Some(group_id) {
            self.workspace.select_node(node_id.clone());
        } else if let Some((catalog_group_id, catalog_node_id)) = catalog_selection
            && self.workspace.selected_group.as_ref() == Some(&catalog_group_id)
        {
            self.workspace.select_node(catalog_node_id);
        }
        record_operation(
            operation,
            LogLevel::Info,
            "policy.node.saved",
            format!("group={group_name}"),
        );
        Some(previous)
    }

    fn finish_policy_node_selection(
        &mut self,
        request: PolicySelectionRequest,
        operation: u64,
        result: Result<mihomo::ManagedPolicyRuntimeSnapshot, mihomo::LoadError>,
        cx: &mut Context<Self>,
    ) {
        let PolicySelectionRequest {
            group_id,
            group_name,
            node_id,
            node_name,
        } = request;
        self.policy_selection_busy = None;
        match result {
            Ok(snapshot) => {
                let current = snapshot
                    .current
                    .clone()
                    .unwrap_or_else(|| node_name.clone());
                if let Some(catalog) = self.catalog.as_mut() {
                    let _ = catalog.apply_selector_target(&group_name, &current);
                }
                if self.workspace.selected_group.as_ref() == Some(&group_id) {
                    self.workspace.select_node(node_id);
                }
                if let Some(stored_group) = self
                    .managed_policies
                    .groups
                    .iter()
                    .find(|group| group.name == group_name)
                {
                    self.managed_policies.runtime_generation =
                        self.managed_policies.runtime_generation.wrapping_add(1);
                    self.managed_policies.runtime_states.insert(
                        stored_group.id.clone(),
                        ManagedPolicyRuntimeState::Ready {
                            generation: self.managed_policies.runtime_generation,
                            current: snapshot.current,
                            candidates: snapshot.candidates,
                        },
                    );
                }
                record_operation(
                    operation,
                    LogLevel::Info,
                    "policy.node.succeeded",
                    format!("group={group_name}"),
                );
                self.status =
                    copy::app::policy_selection_applied(self.language(), &group_name, &current);
            }
            Err(error) => {
                record_operation(
                    operation,
                    LogLevel::Error,
                    "policy.node.failed",
                    error.to_string(),
                );
                self.status = copy::app::policy_selection_apply_failed(
                    self.language(),
                    &group_name,
                    &node_name,
                    &error.to_string(),
                );
            }
        }
        cx.notify();
    }

    fn global_target_identity(&self) -> Option<&NodeIdentity> {
        self.managed_policies.node_selections.global()
    }

    fn global_target(&self) -> Option<&str> {
        self.global_target_identity()
            .map(|identity| identity.node_name.as_str())
            .or_else(|| self.runtime_global_target())
    }

    fn runtime_global_target(&self) -> Option<&str> {
        self.policy_groups()
            .find(|group| group.name.eq_ignore_ascii_case("GLOBAL"))
            .map(|group| group.target.as_str())
    }
}
