use super::{
    GroupBenchmarkState, ManagedPolicyRuntimeState, ManisApp, PolicyBenchmarkRun,
    benchmark_timestamp,
};
use crate::{
    app_update::{self, AppUpdateError, AvailableUpdate},
    core_update,
    diagnostics::{LogLevel, UiEvent, begin_operation, record_event, record_operation, trace_ui},
    localization::{Language, copy},
    mihomo::{
        self, ControllerRuntime, ControllerState, LiveRuntimeSession, LiveStreamPhase,
        LiveStreamStatus, LoadedSnapshot, ManagedRuntimeHealth,
    },
};
use gpui::{Context, Task};
use manis_core::ProxyMode;
use std::time::Duration;

mod benchmark;
#[path = "runtime_lifecycle/kernel.rs"]
mod kernel_flow;
mod live;
mod model;
mod update;
pub(super) use model::LifecycleSubscriptions;

pub(super) use update::{
    AppUpdateState, MihomoCoreUpdateOutcome, MihomoCoreUpdatePhase, MihomoCoreUpdateState,
};
use update::{SharedDownloadProgress, install_downloaded_mihomo_core};
