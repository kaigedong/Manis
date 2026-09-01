use super::{
    Context, ControllerReadiness, ControllerState, LogLevel, ManisApp, ProxyMode, ProxyModeBlock,
    ProxyPorts, TunSupport, UiEvent, apply_proxy_mode_transition, begin_operation,
    controller_state_label, copy, proxy_mode_block, proxy_mode_label, record_operation, trace_ui,
};

impl ManisApp {
    pub(crate) fn proxy_mode_block(&self, requested: ProxyMode) -> Option<ProxyModeBlock> {
        if self.configuration_transfer.active {
            return Some(ProxyModeBlock::Busy);
        }
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

    pub(in crate::app) fn apply_proxy_mode(
        &mut self,
        requested: ProxyMode,
        cx: &mut Context<Self>,
    ) {
        if self.configuration_transfer.active {
            return;
        }
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
}
