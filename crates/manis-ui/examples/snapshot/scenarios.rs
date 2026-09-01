#[path = "scenarios/buttons.rs"]
mod buttons;
#[path = "scenarios/common.rs"]
mod common;
#[path = "scenarios/configuration.rs"]
mod configuration;
#[path = "scenarios/policy.rs"]
mod policy;
#[path = "scenarios/runtime.rs"]
mod runtime;
#[path = "scenarios/sources.rs"]
mod sources;

pub(crate) use buttons::capture_buttons;
pub(crate) use common::{capture, snapshot_hex};
pub(crate) use configuration::{
    capture_app_updates, capture_configuration, capture_configuration_sections,
    capture_configuration_transfer, capture_localization,
};
pub(crate) use policy::{
    capture_automatic_policy, capture_managed_policy_settings, capture_proxy_candidate,
    capture_routing_rules,
};
#[cfg(test)]
pub(crate) use runtime::validate_live_output;
pub(crate) use runtime::{
    capture_appearance, capture_compact_flow, capture_connected, capture_data_page_coverage,
    capture_live_when_configured, capture_log_colors, capture_medium_sheet, capture_merged_nodes,
    capture_navigation_icons, capture_nodes_toolbar, capture_stream_status,
};
pub(crate) use sources::{capture_remote_subscription_preview, capture_source_cards};
