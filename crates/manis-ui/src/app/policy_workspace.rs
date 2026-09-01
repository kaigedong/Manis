use super::{
    GroupBenchmarkNodeState, GroupBenchmarkState, ManisApp, compact_proxy_mode_label,
    controller_status_label, policy_kind_label, policy_presentation::PolicyGroupIconView,
    proxy_mode_label, routing_mode_label, status_bar_values,
};
use crate::{
    assets, brand,
    components::{ActionRole, StatusTone, action_button, empty_state, page_heading, status_badge},
    diagnostics::{UiEvent, trace_ui},
    localization::{CountNoun, Language, Message, copy},
    mihomo::{ControllerState, LiveStreamPhase},
    theme::{ControlSize, LayoutMetric, Radius, Space, TextRole, Theme},
};
use gpui::{
    AnyElement, Context, Div, ParentElement, Role, Stateful, Styled, Toggled, div, img, prelude::*,
    px,
};
use gpui_component::{
    Disableable, IconName, Selectable, Sizable,
    button::{Button, ButtonGroup, ButtonVariant, ButtonVariants},
    status_bar::StatusBar,
};
use manis_core::{
    ManagedPolicyGroup, ManagedPolicyIcon, ManagedPolicyStrategy, PolicyGroup, PolicyGroupId,
    PolicyNode, PrimaryWorkspace, ProxyMode, RoutingMode, WindowSizeClass,
};

mod cards;
mod chrome;
mod model;
mod status;
pub(in crate::app) use model::{
    OfflinePolicyCardView, PolicyListCardView, PolicyNodeRowContext, PolicySelectionRequest,
};

fn platform_chrome_left_padding() -> gpui::Pixels {
    if cfg!(target_os = "macos") {
        // A transparent macOS title bar extends application content underneath the traffic
        // lights. Reserve their native control area before rendering the Manis brand.
        px(78.0)
    } else {
        Space::Lg.px()
    }
}
