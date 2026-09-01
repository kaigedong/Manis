#[path = "runtime/appearance.rs"]
mod appearance;
#[path = "runtime/connected.rs"]
mod connected;
#[path = "runtime/live.rs"]
mod live;
#[path = "runtime/logs.rs"]
mod logs;
#[path = "runtime/navigation.rs"]
mod navigation;
#[path = "runtime/nodes.rs"]
mod nodes;

pub(crate) use appearance::capture_appearance;
pub(crate) use connected::{capture_connected, capture_data_page_coverage, capture_stream_status};
pub(crate) use live::capture_live_when_configured;
#[cfg(test)]
pub(crate) use live::validate_live_output;
pub(crate) use logs::capture_log_colors;
pub(crate) use navigation::capture_navigation_icons;
pub(crate) use nodes::{
    capture_compact_flow, capture_medium_sheet, capture_merged_nodes, capture_nodes_toolbar,
};
