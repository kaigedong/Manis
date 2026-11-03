use super::{
    Context, ControllerState, LogLevel, ManisApp, PreferencePersistence, RoutingMode,
    RoutingModeApplyResult, UiEvent, begin_operation, controller_state_label, copy, mihomo,
    record_operation, routing_mode_label, trace_ui,
};

impl ManisApp {
    pub(in crate::app) fn apply_routing_mode(
        &mut self,
        requested: RoutingMode,
        cx: &mut Context<Self>,
    ) {
        if self.configuration_transfer.active {
            return;
        }
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
                    let persistence = match store_dir.as_deref() {
                        Some(directory) => match mihomo::save_routing_mode_in(directory, requested)
                        {
                            Ok(()) => PreferencePersistence::Saved,
                            Err(error) => PreferencePersistence::Failed(error),
                        },
                        None => PreferencePersistence::Skipped,
                    };
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

    pub(in crate::app) fn finish_routing_mode_change(
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
                    format!(
                        "active={requested:?} persisted={}",
                        !matches!(persistence, PreferencePersistence::Failed(_))
                    ),
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
                if let PreferencePersistence::Failed(_error) = persistence {
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
}
