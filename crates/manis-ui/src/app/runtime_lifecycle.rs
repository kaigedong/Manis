use super::{GroupBenchmarkState, ManagedPolicyRuntimeState, ManisApp, PolicyBenchmarkRun};
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

use update::perform_mihomo_core_update;
pub(super) use update::{AppUpdateState, MihomoCoreUpdateOutcome, MihomoCoreUpdateState};
