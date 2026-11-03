use super::{
    ControllerReadiness, ManagedPolicyRuntimeState, ManisApp, PolicySelectionRequest,
    PreferencePersistence, ProxyModeBlock, RoutingModeApplyResult, TunSupport,
    apply_proxy_mode_transition, controller_state_label, policy_target_is_selectable,
    proxy_mode_block, proxy_mode_label, routing_mode_label,
};
use crate::{
    diagnostics::{LogLevel, UiEvent, begin_operation, record_operation, trace_ui},
    localization::copy,
    mihomo::{self, ControllerRuntime, ControllerState},
    system_proxy::ProxyPorts,
};
use gpui::Context;
use manis_core::{
    ManagedPolicyStrategy, NodeIdentity, PolicyGroupId, ProxyId, ProxyMode, RoutingMode,
};
use std::collections::BTreeSet;

mod global_node;
mod policy_node;
mod proxy_mode;
mod routing_mode;
