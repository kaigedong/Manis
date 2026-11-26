use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use std::time::Duration;

use super::{
    AppUpdateError, AvailableUpdate, Context, Language, LogLevel, ManisApp, app_update, copy,
    core_update, mihomo, record_event,
};
use crate::{app_update::DownloadProgress, kernel::KernelRuntime};

const DOWNLOAD_POLL_INTERVAL: Duration = Duration::from_millis(100);

struct SharedDownloadProgress {
    downloaded_bytes: AtomicU64,
    total_bytes: AtomicU64,
    bytes_per_second: AtomicU64,
    finished: AtomicBool,
}

impl SharedDownloadProgress {
    fn new() -> Self {
        Self {
            downloaded_bytes: AtomicU64::new(0),
            total_bytes: AtomicU64::new(0),
            bytes_per_second: AtomicU64::new(0),
            finished: AtomicBool::new(false),
        }
    }

    fn store(&self, progress: DownloadProgress) {
        self.downloaded_bytes
            .store(progress.downloaded_bytes, Ordering::Relaxed);
        self.total_bytes
            .store(progress.total_bytes.unwrap_or(0), Ordering::Relaxed);
        self.bytes_per_second
            .store(progress.bytes_per_second, Ordering::Relaxed);
    }

    fn load(&self) -> DownloadProgress {
        let total = self.total_bytes.load(Ordering::Relaxed);
        DownloadProgress {
            downloaded_bytes: self.downloaded_bytes.load(Ordering::Relaxed),
            total_bytes: (total > 0).then_some(total),
            bytes_per_second: self.bytes_per_second.load(Ordering::Relaxed),
        }
    }

    fn mark_finished(&self) {
        self.finished.store(true, Ordering::Release);
    }

    fn is_finished(&self) -> bool {
        self.finished.load(Ordering::Acquire)
    }
}

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
    Downloading {
        update: AvailableUpdate,
        progress: DownloadProgress,
    },
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

    /// Triggers an update check automatically when the user enters the About & Updates view.
    pub(in crate::app) fn maybe_auto_check_app_update(&mut self, cx: &mut Context<Self>) {
        if matches!(
            self.app_update_state,
            AppUpdateState::Idle | AppUpdateState::Current | AppUpdateState::Failed(_)
        ) {
            self.check_for_app_update(cx);
        }
    }

    pub(in crate::app) fn check_for_app_update(&mut self, cx: &mut Context<Self>) {
        if self.runtime.is_fixture()
            || matches!(
                self.app_update_state,
                AppUpdateState::Checking | AppUpdateState::Downloading { .. }
            )
        {
            return;
        }
        self.app_update_dismissed_version = None;
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

    pub(in crate::app) fn dismiss_app_update_banner(&mut self, cx: &mut Context<Self>) {
        let version = match &self.app_update_state {
            AppUpdateState::Available(update) | AppUpdateState::Ready { update, .. } => {
                Some(update.version.clone())
            }
            AppUpdateState::Idle
            | AppUpdateState::Checking
            | AppUpdateState::Downloading { .. }
            | AppUpdateState::Current
            | AppUpdateState::Failed(_)
            | AppUpdateState::InstallFailed(_) => None,
        };
        if let Some(version) = version {
            self.app_update_dismissed_version = Some(version);
            cx.notify();
        }
    }

    pub(in crate::app) fn is_app_update_banner_dismissed(&self, version: &str) -> bool {
        self.app_update_dismissed_version.as_deref() == Some(version)
    }

    pub(in crate::app) fn start_app_update_download(&mut self, cx: &mut Context<Self>) {
        let AppUpdateState::Available(update) = self.app_update_state.clone() else {
            return;
        };
        self.app_update_dismissed_version = None;
        self.app_update_state = AppUpdateState::Downloading {
            update: update.clone(),
            progress: DownloadProgress::default(),
        };
        cx.notify();
        let executor = cx.background_executor().clone();
        let timer = executor.clone();
        let shared = Arc::new(SharedDownloadProgress::new());
        let progress_writer = Arc::clone(&shared);
        let download = update.clone();
        cx.spawn(async move |this, cx| {
            let download_task = executor.spawn(async move {
                let result = app_update::download_package_with_progress(&download, |progress| {
                    progress_writer.store(progress);
                });
                progress_writer.mark_finished();
                result
            });

            let mut last_seen = DownloadProgress::default();
            while !shared.is_finished() {
                timer.timer(DOWNLOAD_POLL_INTERVAL).await;
                let latest = shared.load();
                if latest != last_seen {
                    last_seen = latest;
                    let still_alive = this
                        .update(cx, |this, cx| {
                            if let AppUpdateState::Downloading { progress, .. } =
                                &mut this.app_update_state
                            {
                                *progress = latest;
                                cx.notify();
                            }
                        })
                        .is_ok();
                    if !still_alive {
                        break;
                    }
                }
            }

            let package = download_task.await;
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
                self.app_update_dismissed_version = None;
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
            Ok(Some(update)) => match previous {
                AppUpdateState::Ready {
                    update: ready,
                    package,
                } if ready == update => AppUpdateState::Ready {
                    update: ready,
                    package,
                },
                other => {
                    if !matches!(&other, AppUpdateState::Available(known) if known == &update) {
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
            },
            Ok(None) => AppUpdateState::Current,
            Err(error) => {
                record_event(LogLevel::Warn, "app.update.check.failed", error.to_string());
                // A temporary network failure must not hide an already discovered release.
                match previous {
                    AppUpdateState::Available(_) | AppUpdateState::Ready { .. } => previous,
                    AppUpdateState::Idle
                    | AppUpdateState::Checking
                    | AppUpdateState::Current
                    | AppUpdateState::Downloading { .. }
                    | AppUpdateState::InstallFailed(_)
                    | AppUpdateState::Failed(_) => AppUpdateState::Failed(error),
                }
            }
        };
    }
}
