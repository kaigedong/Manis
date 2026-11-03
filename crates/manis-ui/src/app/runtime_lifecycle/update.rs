use super::{
    AppUpdateError, AvailableUpdate, Context, Duration, Language, LogLevel, ManisApp, app_update,
    copy, core_update, mihomo, record_event,
};
use crate::kernel::KernelRuntime;

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
    Downloading(AvailableUpdate),
    Ready {
        update: AvailableUpdate,
        package: std::path::PathBuf,
    },
    Current,
    Failed(AppUpdateError),
    InstallFailed(String),
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
        let runtime = KernelRuntime::prepare_with_language(Some(store_dir));
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

    pub(in crate::app) fn check_for_app_update(&mut self, cx: &mut Context<Self>) {
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

    pub(in crate::app) fn start_app_update_download(&mut self, cx: &mut Context<Self>) {
        let AppUpdateState::Available(update) = self.app_update_state.clone() else {
            return;
        };
        self.app_update_state = AppUpdateState::Downloading(update.clone());
        cx.notify();
        let executor = cx.background_executor().clone();
        let download = update.clone();
        cx.spawn(async move |this, cx| {
            let package = executor
                .spawn(async move { app_update::download_package(&download) })
                .await;
            this.update(cx, |this, cx| {
                this.finish_app_update_download(update, package);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn finish_app_update_download(
        &mut self,
        update: AvailableUpdate,
        result: Result<std::path::PathBuf, AppUpdateError>,
    ) {
        self.app_update_state = match result {
            Ok(package) => {
                record_event(
                    LogLevel::Info,
                    "app.update.downloaded",
                    format!("version={}", update.version),
                );
                AppUpdateState::Ready { update, package }
            }
            Err(error) => {
                record_event(
                    LogLevel::Warn,
                    "app.update.download.failed",
                    error.to_string(),
                );
                AppUpdateState::Failed(error)
            }
        };
    }

    pub(in crate::app) fn install_app_update(&mut self, cx: &mut Context<Self>) {
        let AppUpdateState::Ready { package, .. } = self.app_update_state.clone() else {
            return;
        };
        match app_update::install_package(&package) {
            Ok(()) => {
                record_event(LogLevel::Info, "app.update.install", "restarting");
                cx.quit();
            }
            Err(message) => {
                record_event(
                    LogLevel::Error,
                    "app.update.install.failed",
                    message.clone(),
                );
                self.app_update_state = AppUpdateState::InstallFailed(message);
                cx.notify();
            }
        }
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
                    AppUpdateState::Available(_) | AppUpdateState::Ready { .. } => previous,
                    AppUpdateState::Idle
                    | AppUpdateState::Checking
                    | AppUpdateState::Current
                    | AppUpdateState::Downloading(_)
                    | AppUpdateState::InstallFailed(_)
                    | AppUpdateState::Failed(_) => AppUpdateState::Failed(error),
                }
            }
        };
    }
}
