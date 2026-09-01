use super::{
    Context, ControllerRuntime, ControllerState, LogLevel, ManisApp, NodeIdentity, RoutingMode,
    UiEvent, begin_operation, controller_state_label, copy, mihomo, record_operation, trace_ui,
};

impl ManisApp {
    pub(in crate::app) fn select_global_node(
        &mut self,
        selected: NodeIdentity,
        cx: &mut Context<Self>,
    ) {
        if self.configuration_transfer.active {
            return;
        }
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
}
