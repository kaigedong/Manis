//! Platform-neutral Manis desktop domain layer.
//!
//! This crate owns everything the NSH shells and the headless engine need
//! without pulling a UI toolkit: branding paths, the stored workspace,
//! subscription and manual-rule persistence, the Mihomo controller transport
//! and profile compiler, the managed kernel lifecycle, system-proxy and
//! privilege helpers, update checks and the NSH host bridge.
//!
//! `GTK`, `AppKit` and `Win32` live in `manis-nsh-desktop`; the TSX pages live in
//! `nsh/`. Both talk to this crate through the re-exported bridge functions and
//! [`run_engine`].
//!
//! The saved-workspace and controller store keeps read/write APIs for NSH
//! commands that are declared in the shared capability list but not wired in
//! this crate yet (managed policy edits, QX rule source edits, import preview,
//! routing-rule reordering, node-selection persistence). They are covered by
//! unit tests, so dead-code analysis is disabled at the crate boundary until
//! the matching host handlers land.

#![allow(dead_code)]

mod app_update;
mod autostart;
mod brand;
mod config_toml;
mod core_update;
mod diagnostics;
mod direct_rule;
mod engine;
mod kernel;
#[cfg(target_os = "linux")]
mod linux_privileged;
#[cfg(target_os = "macos")]
mod macos_privileged;
mod manual_rule;
mod mihomo;
mod nsh;
mod rule_source;
mod stored_workspace;
mod subscription;

pub use engine::{EngineError, run as run_engine};
pub use nsh::{
    nsh_add_node, nsh_add_subscription, nsh_apply_profile, nsh_check_update, nsh_controller_socket,
    nsh_delete_node, nsh_delete_rule, nsh_delete_subscription, nsh_logs, nsh_root_data,
    nsh_save_rule, nsh_select_global_node, nsh_select_group_node, nsh_set_autostart,
    nsh_set_routing_mode, nsh_set_tun, nsh_toggle_node, nsh_toggle_rule, nsh_toggle_subscription,
};

/// Returns the version embedded by the packaging workflow.
#[must_use]
pub fn version() -> &'static str {
    app_update::current_version()
}
