//! Typed, domain-scoped product copy.
//!
//! Keeping translations here makes UI code describe intent instead of embedding
//! two language variants at every render or state transition site.

pub(crate) mod activity;
pub(crate) mod app;
pub(crate) mod app_update;
pub(crate) mod backup;
pub(crate) mod common;
pub(crate) mod configuration;
pub(crate) mod core_update;
pub(crate) mod kernel;
pub(crate) mod logs;
pub(crate) mod nodes;
pub(crate) mod subscription_input;
pub(crate) mod system_proxy;
pub(crate) mod tray;
