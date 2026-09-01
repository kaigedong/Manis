use super::{
    Context, ControllerState, Duration, LiveRuntimeSession, LiveStreamPhase, LiveStreamStatus,
    LogLevel, ManagedRuntimeHealth, ManisApp, ProxyMode, Task, begin_operation, copy,
    record_operation,
};

impl ManisApp {
    pub(in crate::app) fn start_live_runtime(
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

    pub(in crate::app) fn poll_live_runtime(&mut self, generation: u64, cx: &mut Context<Self>) {
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

    pub(in crate::app) fn fail_safe_stopped_managed_kernel(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
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
    pub(in crate::app) fn shutdown_for_quit(&mut self, _cx: &mut Context<Self>) -> Task<()> {
        let language = self.language();
        let operation = begin_operation(
            "app.shutdown.requested",
            format!("proxy_mode={:?}", self.proxy_mode),
        );
        if let Ok(mut dns) = self.tun_dns.lock()
            && let Err(error) = dns.shutdown_with_language(language)
        {
            record_operation(
                operation,
                LogLevel::Error,
                "tun_dns.shutdown.failed",
                error.to_string(),
            );
        }
        if self.proxy_mode == ProxyMode::Tun {
            match self.runtime.set_tun_enabled(false) {
                Ok(()) => record_operation(
                    operation,
                    LogLevel::Info,
                    "tun.shutdown.succeeded",
                    "controller accepted disable request",
                ),
                Err(error) => record_operation(
                    operation,
                    LogLevel::Error,
                    "tun.shutdown.failed",
                    error.to_string(),
                ),
            }
        }
        if let Ok(mut system) = self.system_proxy.lock()
            && let Err(error) = system.shutdown_with_language(language)
        {
            record_operation(
                operation,
                LogLevel::Error,
                "system_proxy.shutdown.failed",
                error.to_string(),
            );
        }
        match self.runtime.stop_managed() {
            Ok(()) => record_operation(
                operation,
                LogLevel::Info,
                "kernel.shutdown.succeeded",
                "managed kernel stop completed",
            ),
            Err(error) => {
                record_operation(operation, LogLevel::Error, "kernel.shutdown.failed", error);
            }
        }
        Task::ready(())
    }

    #[cfg(not(test))]
    pub(in crate::app) fn recover_stale_system_proxy(&mut self) {
        let language = self.language();
        match self.system_proxy.lock() {
            Ok(mut system) => {
                if let Err(error) = system.recover_stale_with_language(language) {
                    self.status = format!(
                        "{}{error}",
                        language.localized(copy::app::SYSTEM_PROXY_RECOVERY_NEEDS_ATTENTION)
                    );
                }
            }
            Err(_poisoned) => {
                language
                    .localized(copy::app::SYSTEM_PROXY_RECOVERY_STATE_IS_UNAVAILABLE)
                    .clone_into(&mut self.status);
            }
        }
        match self.tun_dns.lock() {
            Ok(mut dns) => {
                if let Err(error) = dns.recover_stale_with_language(language) {
                    self.status = format!(
                        "{}{error}",
                        language.localized(copy::app::TUN_DNS_RECOVERY_NEEDS_ATTENTION)
                    );
                }
            }
            Err(_poisoned) => {
                language
                    .localized(copy::app::TUN_DNS_RECOVERY_STATE_IS_UNAVAILABLE)
                    .clone_into(&mut self.status);
            }
        }
    }
}
