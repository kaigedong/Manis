use super::{
    AppUpdateError, AvailableUpdate, Context, Duration, KernelKind, KernelRuntime, Language,
    LogLevel, ManisApp, app_update, copy, core_update, mihomo, record_event,
};

pub(in crate::app) enum MihomoCoreUpdateOutcome {
    Installed {
        version: String,
        runtime: KernelRuntime,
        snapshot: Option<mihomo::RuntimeSnapshot>,
    },
    Failed {
        message: String,
        recovered: Option<mihomo::RuntimeSnapshot>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::app) enum AppUpdateState {
    Idle,
    Checking,
    Available(AvailableUpdate),
    Current,
    Failed(AppUpdateError),
}

pub(super) fn perform_mihomo_core_update(
    previous: &KernelRuntime,
    store_dir: &std::path::Path,
    language: Language,
    reconnect: bool,
) -> MihomoCoreUpdateOutcome {
    if let Err(message) = previous.stop_managed() {
        return MihomoCoreUpdateOutcome::Failed {
            message,
            recovered: None,
        };
    }

    let mut prepared = None;
    let install = core_update::install_latest_core_update(|| {
        let runtime =
            KernelRuntime::prepare_with_language(KernelKind::Mihomo, Some(store_dir), language)
                .map_err(|_message| core_update::CoreUpdateError::PublishFailed)?;
        let snapshot = reconnect
            .then(|| runtime.connect())
            .transpose()
            .map_err(|_error| core_update::CoreUpdateError::PublishFailed)?;
        #[cfg(target_os = "macos")]
        crate::macos_privileged::MacosPrivilegedProcessSpawner::sync_managed_core_if_available()
            .map_err(|_error| core_update::CoreUpdateError::PublishFailed)?;
        prepared = Some((runtime, snapshot));
        Ok(())
    });

    match (install, prepared) {
        (Ok(installed), Some((runtime, snapshot))) => MihomoCoreUpdateOutcome::Installed {
            version: installed.version,
            runtime,
            snapshot,
        },
        (Ok(_installed), None) => MihomoCoreUpdateOutcome::Failed {
            message: core_update::CoreUpdateError::PublishFailed
                .localized_message(language)
                .to_owned(),
            recovered: reconnect.then(|| previous.connect()).and_then(Result::ok),
        },
        (Err(error), _) => {
            record_event(LogLevel::Error, "core.update.failed", error.to_string());
            MihomoCoreUpdateOutcome::Failed {
                message: error.localized_message(language).to_owned(),
                recovered: reconnect.then(|| previous.connect()).and_then(Result::ok),
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(in crate::app) enum KernelSwitchState {
    #[default]
    Idle,
    Preparing,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(in crate::app) enum MihomoCoreUpdateState {
    #[default]
    Missing,
    Ready(String),
    Updating,
}

impl MihomoCoreUpdateState {
    pub(in crate::app) const fn is_busy(&self) -> bool {
        matches!(self, Self::Updating)
    }
}

impl KernelSwitchState {
    pub(in crate::app) const fn is_busy(self) -> bool {
        matches!(self, Self::Preparing)
    }
}

impl ManisApp {
    pub(in crate::app) fn initial_mihomo_core_update_state() -> MihomoCoreUpdateState {
        #[cfg(test)]
        {
            MihomoCoreUpdateState::Missing
        }
        #[cfg(not(test))]
        {
            core_update::managed_core_binary_path()
                .map_or(MihomoCoreUpdateState::Missing, |_path| {
                    MihomoCoreUpdateState::Ready(String::new())
                })
        }
    }

    pub(in crate::app) fn start_app_update_polling(cx: &mut Context<Self>) {
        let timer = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            loop {
                if this.update(cx, Self::check_for_app_update).is_err() {
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

    pub(in crate::app) fn finish_app_update_check(
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
                    AppUpdateState::Idle
                    | AppUpdateState::Checking
                    | AppUpdateState::Current
                    | AppUpdateState::Failed(_) => AppUpdateState::Failed(error),
                }
            }
        };
    }
}
