use std::sync::Arc;
use std::time::Duration;

use super::update::DOWNLOAD_POLL_INTERVAL;
use super::{
    Context, ControllerRuntime, ControllerState, LiveStreamPhase, LiveStreamStatus, LoadedSnapshot,
    LogLevel, ManagedPolicyRuntimeState, ManisApp, MihomoCoreUpdateOutcome, MihomoCoreUpdatePhase,
    ProxyMode, SharedDownloadProgress, UiEvent, begin_operation, copy, core_update,
    install_downloaded_mihomo_core, mihomo, record_event, record_operation, trace_ui,
};
use crate::app::AutoConnectStage;
use crate::app_update::DownloadProgress;
use crate::mihomo::RuntimeProfileSource;

/// One automatic retry covers a login-time start that raced the network coming up.
const AUTO_CONNECT_MAX_RETRIES: u8 = 1;
const AUTO_CONNECT_RETRY_DELAY: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AutoConnectDecision {
    Skip,
    WaitForCore,
    Connect,
}

/// Why an automatic start may or may not run. Keeping this an enum avoids a row of booleans and
/// makes the order in which the reasons are checked explicit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AutoConnectGate {
    Disabled,
    Fixture,
    Busy,
    NoProfile,
    Ready,
}

/// Decides what an automatic start should do, so the policy is testable without a window.
fn auto_connect_decision(gate: AutoConnectGate, core_ready: bool) -> AutoConnectDecision {
    match gate {
        AutoConnectGate::Ready if core_ready => AutoConnectDecision::Connect,
        AutoConnectGate::Ready => AutoConnectDecision::WaitForCore,
        AutoConnectGate::Disabled
        | AutoConnectGate::Fixture
        | AutoConnectGate::Busy
        | AutoConnectGate::NoProfile => AutoConnectDecision::Skip,
    }
}

impl ManisApp {
    /// Starts the managed kernel when the app opens, unless the user turned that off.
    ///
    /// A missing core does not fail the start: the update that installs it calls back here, and a
    /// start that fails once (for example while the network is still coming up at login) is
    /// retried once before the failure is left on screen with the manual retry button.
    pub(in crate::app) fn maybe_auto_connect(&mut self, cx: &mut Context<Self>) {
        // A direct-only bootstrap profile compiles even on a fresh install, so require an actual
        // subscription or single node before starting the kernel.
        let has_sources =
            !self.imported_subscriptions.is_empty() || !self.saved_single_nodes.is_empty();
        let gate = if !self.auto_connect.enabled {
            AutoConnectGate::Disabled
        } else if self.runtime.is_fixture() {
            AutoConnectGate::Fixture
        } else if self.configuration_transfer.active
            || matches!(
                self.controller,
                ControllerState::Connecting { .. } | ControllerState::Connected { .. }
            )
        {
            AutoConnectGate::Busy
        } else if !has_sources
            || !matches!(
                self.runtime.profile_source(),
                RuntimeProfileSource::SavedSources
            )
        {
            AutoConnectGate::NoProfile
        } else {
            AutoConnectGate::Ready
        };
        let decision = auto_connect_decision(gate, self.mihomo_core_update_state.is_installed());
        match decision {
            AutoConnectDecision::Skip => {}
            AutoConnectDecision::WaitForCore => {
                self.auto_connect.stage = AutoConnectStage::WaitingForCore;
            }
            AutoConnectDecision::Connect => {
                self.auto_connect.stage = AutoConnectStage::Connecting;
                self.auto_connect.attempts = 0;
                record_event(LogLevel::Info, "auto_connect.started", "opening the app");
                self.connect_mihomo(cx);
            }
        }
    }

    fn after_auto_connect_failure(&mut self, cx: &mut Context<Self>) {
        if self.auto_connect.stage != AutoConnectStage::Connecting {
            return;
        }
        if self.configuration_transfer.active {
            // A config transfer stopped the core on purpose; do not fight it.
            self.auto_connect.stage = AutoConnectStage::Idle;
            return;
        }
        if self.auto_connect.attempts >= AUTO_CONNECT_MAX_RETRIES {
            self.auto_connect.stage = AutoConnectStage::Idle;
            return;
        }
        self.auto_connect.attempts += 1;
        record_event(
            LogLevel::Info,
            "auto_connect.retrying",
            "the automatic start failed once; trying again",
        );
        let timer = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            timer.timer(AUTO_CONNECT_RETRY_DELAY).await;
            this.update(cx, |this, cx| {
                if this.auto_connect.stage != AutoConnectStage::Connecting {
                    return;
                }
                if this.configuration_transfer.active
                    || matches!(
                        this.controller,
                        ControllerState::Connecting { .. } | ControllerState::Connected { .. }
                    )
                {
                    this.auto_connect.stage = AutoConnectStage::Idle;
                    return;
                }
                this.connect_mihomo(cx);
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    #[cfg(not(test))]
    pub(in crate::app) fn probe_mihomo_core_version(&mut self, cx: &mut Context<Self>) {
        if self.runtime.is_fixture()
            || !self.mihomo_core_update_state.is_installed()
            || self.mihomo_core_update_state.current_version().is_some()
        {
            return;
        }
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let version = executor
                .spawn(async { core_update::probe_installed_core_version() })
                .await;
            if let Some(version) = version {
                this.update(cx, |this, cx| {
                    if this.mihomo_core_update_state.is_installed() {
                        this.mihomo_core_update_state
                            .record_installed_version(&version);
                        cx.notify();
                    }
                })
                .ok();
            }
        })
        .detach();
    }

    /// Automatically checks for a newer Mihomo release once when entering Runtime settings.
    pub(in crate::app) fn maybe_auto_check_mihomo_core_update(&mut self, cx: &mut Context<Self>) {
        if self.mihomo_core_update_state.is_installed()
            && matches!(
                self.mihomo_core_update_state.phase,
                MihomoCoreUpdatePhase::Idle
            )
        {
            self.check_for_mihomo_core_update(cx);
        }
    }

    pub(in crate::app) fn check_for_mihomo_core_update(&mut self, cx: &mut Context<Self>) {
        if self.runtime.is_fixture()
            || self.mihomo_core_update_state.is_checking()
            || self.mihomo_core_update_state.is_busy()
        {
            return;
        }
        let previous = std::mem::replace(
            &mut self.mihomo_core_update_state.phase,
            MihomoCoreUpdatePhase::Checking,
        );
        let known_version = self
            .mihomo_core_update_state
            .current_version()
            .map(str::to_owned);
        let is_installed = self.mihomo_core_update_state.is_installed();
        cx.notify();

        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let (probed, checked) = executor
                .spawn(async move {
                    let probed = if known_version.is_none() && is_installed {
                        core_update::probe_installed_core_version()
                    } else {
                        None
                    };
                    let effective_current = known_version.or_else(|| probed.clone());
                    let checked = core_update::check_for_core_update(effective_current.as_deref());
                    (probed, checked)
                })
                .await;
            this.update(cx, |this, cx| {
                if let Some(version) = probed
                    && this.mihomo_core_update_state.is_installed()
                {
                    this.mihomo_core_update_state
                        .record_installed_version(&version);
                }
                this.finish_mihomo_core_update_check(checked, previous);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(in crate::app) fn finish_mihomo_core_update_check(
        &mut self,
        result: Result<core_update::CoreUpdateCheck, core_update::CoreUpdateError>,
        previous: MihomoCoreUpdatePhase,
    ) {
        self.mihomo_core_update_state.phase = match result {
            Ok(core_update::CoreUpdateCheck::Available(asset)) => match previous {
                MihomoCoreUpdatePhase::ReadyToInstall {
                    asset: ready,
                    package,
                } if ready.version == asset.version => MihomoCoreUpdatePhase::ReadyToInstall {
                    asset: ready,
                    package,
                },
                other => {
                    if !matches!(&other, MihomoCoreUpdatePhase::Available(known) if known.version == asset.version)
                    {
                        record_event(
                            LogLevel::Info,
                            "core.update.available",
                            format!("version={}", asset.version),
                        );
                    }
                    MihomoCoreUpdatePhase::Available(asset)
                }
            },
            Ok(core_update::CoreUpdateCheck::Current { version }) => {
                MihomoCoreUpdatePhase::Current {
                    latest_version: version,
                }
            }
            Err(error) => {
                record_event(
                    LogLevel::Warn,
                    "core.update.check.failed",
                    error.to_string(),
                );
                match previous {
                    MihomoCoreUpdatePhase::Available(_)
                    | MihomoCoreUpdatePhase::ReadyToInstall { .. } => previous,
                    _ => MihomoCoreUpdatePhase::Failed(
                        error.localized_message(self.language()).to_owned(),
                    ),
                }
            }
        };
    }

    pub(in crate::app) fn update_mihomo_core(&mut self, cx: &mut Context<Self>) {
        if self.configuration_transfer.active || self.mihomo_core_update_state.is_busy() {
            return;
        }
        if self.subscription_store_dir.is_none() {
            self.language()
                .localized(
                    copy::app::THE_MANIS_DATA_DIRECTORY_IS_UNAVAILABLE_MIHOMO_CANNOT_BE_UPDATED,
                )
                .clone_into(&mut self.status);
            cx.notify();
            return;
        }

        if let MihomoCoreUpdatePhase::ReadyToInstall { asset, package } =
            self.mihomo_core_update_state.phase.clone()
        {
            if self.proxy_mode != ProxyMode::Off {
                self.language()
                    .localized(copy::app::TURN_OFF_THE_ACTIVE_PROXY_MODE_BEFORE_UPDATING_MIHOMO)
                    .clone_into(&mut self.status);
                cx.notify();
                return;
            }
            self.install_staged_mihomo_core(asset, package, cx);
            return;
        }

        let known_asset = match self.mihomo_core_update_state.phase.clone() {
            MihomoCoreUpdatePhase::Available(asset) => Some(asset),
            _ => None,
        };
        self.start_mihomo_core_download(known_asset, cx);
    }

    fn start_mihomo_core_download(
        &mut self,
        known_asset: Option<core_update::ReleaseAsset>,
        cx: &mut Context<Self>,
    ) {
        let language = self.language();
        self.mihomo_core_update_state.phase = MihomoCoreUpdatePhase::Downloading {
            version: known_asset.as_ref().map(|asset| asset.version.clone()),
            progress: DownloadProgress::default(),
        };
        language
            .localized(copy::app::DOWNLOADING_AND_VERIFYING_THE_STABLE_MIHOMO_RELEASE)
            .clone_into(&mut self.status);
        cx.notify();

        let executor = cx.background_executor().clone();
        let timer = executor.clone();
        cx.spawn(async move |this, cx| {
            let asset = match known_asset {
                Some(asset) => asset,
                None => match executor
                    .spawn(async { core_update::fetch_current_release_asset() })
                    .await
                {
                    Ok(asset) => {
                        let version = asset.version.clone();
                        let _ = this.update(cx, |this, cx| {
                            if let MihomoCoreUpdatePhase::Downloading { version: slot, .. } =
                                &mut this.mihomo_core_update_state.phase
                            {
                                *slot = Some(version);
                                cx.notify();
                            }
                        });
                        asset
                    }
                    Err(error) => {
                        record_event(
                            LogLevel::Error,
                            "core.update.fetch.failed",
                            error.to_string(),
                        );
                        let message = error.localized_message(language).to_owned();
                        this.update(cx, |this, cx| {
                            this.mihomo_core_update_state.phase =
                                MihomoCoreUpdatePhase::Failed(message.clone());
                            this.status = message;
                            cx.notify();
                        })
                        .ok();
                        return;
                    }
                },
            };

            let shared = Arc::new(SharedDownloadProgress::new());
            let progress_writer = Arc::clone(&shared);
            let download_asset = asset.clone();
            let download_task = executor.spawn(async move {
                let result = core_update::download_asset_package_with_progress(
                    &download_asset,
                    |progress| progress_writer.store(progress),
                );
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
                            if let MihomoCoreUpdatePhase::Downloading { progress, .. } =
                                &mut this.mihomo_core_update_state.phase
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

            let downloaded = download_task.await;
            this.update(cx, |this, cx| {
                this.finish_mihomo_core_download(asset, downloaded, cx);
            })
            .ok();
        })
        .detach();
    }

    fn finish_mihomo_core_download(
        &mut self,
        asset: core_update::ReleaseAsset,
        result: Result<Vec<u8>, core_update::CoreUpdateError>,
        cx: &mut Context<Self>,
    ) {
        let language = self.language();
        match result {
            Ok(bytes) => {
                let package = Arc::new(bytes);
                if self.proxy_mode == ProxyMode::Off && !self.configuration_transfer.active {
                    self.install_staged_mihomo_core(asset, package, cx);
                } else {
                    let version = asset.version.clone();
                    self.mihomo_core_update_state.phase =
                        MihomoCoreUpdatePhase::ReadyToInstall { asset, package };
                    self.status = copy::core_update::ready_to_install(language, &version, true);
                    cx.notify();
                }
            }
            Err(error) => {
                record_event(
                    LogLevel::Error,
                    "core.update.download.failed",
                    error.to_string(),
                );
                let message = error.localized_message(language).to_owned();
                self.mihomo_core_update_state.phase =
                    MihomoCoreUpdatePhase::Failed(message.clone());
                self.status = message;
                cx.notify();
            }
        }
    }

    fn install_staged_mihomo_core(
        &mut self,
        asset: core_update::ReleaseAsset,
        package: Arc<Vec<u8>>,
        cx: &mut Context<Self>,
    ) {
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
        self.mihomo_core_update_state.phase = MihomoCoreUpdatePhase::Installing {
            version: asset.version.clone(),
        };
        self.live_generation = self.live_generation.wrapping_add(1);
        self.live_runtime = None;
        self.controller = ControllerState::Disconnected;
        self.status = copy::core_update::installing_version(language, &asset.version);

        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let outcome = executor
                .spawn(async move {
                    install_downloaded_mihomo_core(
                        &previous, &store_dir, language, reconnect, &asset, &package,
                    )
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
                self.mihomo_core_update_state
                    .record_installed_version(&version);
                self.mihomo_core_update_state.phase = MihomoCoreUpdatePhase::Updated {
                    version: version.clone(),
                };
                self.apply_core_update_snapshot(snapshot, cx);
                self.status = copy::app::mihomo_installed(language, &version);
            }
            MihomoCoreUpdateOutcome::Failed { message, recovered } => {
                if core_update::managed_core_binary_path().is_err() {
                    self.mihomo_core_update_state.installed_version = None;
                } else if self.mihomo_core_update_state.installed_version.is_none() {
                    self.mihomo_core_update_state.installed_version = Some(String::new());
                }
                self.mihomo_core_update_state.phase =
                    MihomoCoreUpdatePhase::Failed(message.clone());
                self.apply_core_update_snapshot(recovered, cx);
                self.status = format!(
                    "{}{message}",
                    language
                        .localized(copy::app::MIHOMO_UPDATE_FAILED_THE_PREVIOUS_CORE_WAS_RESTORED)
                );
            }
        }
        if self.auto_connect.stage == AutoConnectStage::WaitingForCore {
            self.maybe_auto_connect(cx);
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
                        this.auto_connect.stage = AutoConnectStage::Idle;
                        this.auto_connect.attempts = 0;
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
                        this.after_auto_connect_failure(cx);
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
        self.ingest_snapshot_node_latencies(&snapshot);
        let mut catalog = snapshot.catalog;
        for (group, target) in self.managed_policies.node_selections.iter_policy_targets() {
            if let Some(catalog) = catalog.as_mut() {
                let _ = catalog.apply_selector_target(group, target);
            }
        }
        if let Some(catalog) = catalog.as_mut() {
            self.apply_completed_policy_benchmarks(catalog);
        }
        let mut providers = snapshot.providers;
        self.apply_shared_latencies_to_providers(&mut providers);
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
        self.source_providers = providers;
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
        if self.mihomo_core_update_state.is_installed() && !snapshot.version.trim().is_empty() {
            self.mihomo_core_update_state
                .record_installed_version(&snapshot.version);
        }
        self.controller = ControllerState::Connected {
            endpoint,
            version: snapshot.version,
            active_connections: snapshot.active_connections,
            download_total: snapshot.download_total,
            upload_total: snapshot.upload_total,
        };
    }

    pub(in crate::app) fn policy_catalog_is_incomplete(&self) -> bool {
        self.catalog.as_ref().map_or_else(
            || {
                self.imported_subscriptions
                    .iter()
                    .any(|source| source.enabled)
            },
            |catalog| {
                catalog.iter().any(|group| {
                    group.nodes.is_empty() || group.target.as_deref() == Some("COMPATIBLE")
                })
            },
        )
    }

    pub(in crate::app) fn apply_refreshed_policy_snapshot(&mut self, snapshot: LoadedSnapshot) {
        self.ingest_snapshot_node_latencies(&snapshot);
        let mut providers = snapshot.providers;
        self.apply_shared_latencies_to_providers(&mut providers);
        let Some(mut catalog) = snapshot.catalog else {
            self.source_providers = providers;
            return;
        };
        for (group, target) in self.managed_policies.node_selections.iter_policy_targets() {
            let _ = catalog.apply_selector_target(group, target);
        }
        self.apply_completed_policy_benchmarks(&mut catalog);
        let selected_group = catalog.select(self.workspace.selected_group.as_ref());
        let selected_node = selected_group
            .target
            .as_deref()
            .and_then(|target| selected_group.nodes.iter().find(|node| node.name == target))
            .or_else(|| selected_group.nodes.first())
            .map(|node| node.id.clone());
        let selected_group_id = selected_group.id.clone();
        self.catalog = Some(catalog);
        self.source_providers = providers;
        self.workspace
            .replace_source_selection(selected_group_id, selected_node);
    }

    fn sync_catalog_and_workspace_target(&mut self, group: &str, current: &str) {
        if let Some(catalog) = self.catalog.as_mut()
            && catalog.apply_selector_target(group, current)
            && let Some((group_id, node_id)) = catalog
                .iter()
                .find(|candidate| candidate.name == group)
                .and_then(|candidate| {
                    candidate
                        .nodes
                        .iter()
                        .find(|node| node.name == current)
                        .map(|node| (candidate.id.clone(), node.id.clone()))
                })
        {
            self.workspace.set_selection_for(group_id, node_id);
        }
    }

    pub(in crate::app) fn sync_saved_node_selections(&mut self, cx: &mut Context<Self>) {
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
                            this.sync_catalog_and_workspace_target(&group, current);
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

#[cfg(test)]
mod tests {
    use super::{AutoConnectDecision, AutoConnectGate, auto_connect_decision};

    #[test]
    fn skips_the_automatic_start_when_it_cannot_succeed() {
        for gate in [
            AutoConnectGate::Disabled,
            AutoConnectGate::Fixture,
            AutoConnectGate::Busy,
            AutoConnectGate::NoProfile,
        ] {
            assert_eq!(
                auto_connect_decision(gate, true),
                AutoConnectDecision::Skip,
                "{gate:?}"
            );
        }
    }

    #[test]
    fn waits_for_the_core_before_connecting() {
        assert_eq!(
            auto_connect_decision(AutoConnectGate::Ready, false),
            AutoConnectDecision::WaitForCore
        );
        assert_eq!(
            auto_connect_decision(AutoConnectGate::Ready, true),
            AutoConnectDecision::Connect
        );
    }
}
