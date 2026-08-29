#![allow(clippy::unreadable_literal)]

use gpui::{FontWeight, Pixels, Rgba, px, rgb};
use gpui_component::{Theme as ComponentTheme, ThemeMode};

#[derive(Clone, Copy)]
pub(crate) struct Theme {
    pub surface_base: Rgba,
    pub surface_low: Rgba,
    pub surface_high: Rgba,
    pub surface_chrome: Rgba,
    pub text_primary: Rgba,
    pub text_secondary: Rgba,
    pub text_tertiary: Rgba,
    pub outline_subtle: Rgba,
    pub outline_strong: Rgba,
    pub action_primary: Rgba,
    pub action_on_primary: Rgba,
    pub action_soft: Rgba,
    pub route_trace: Rgba,
    pub route_soft: Rgba,
    pub status_success: Rgba,
    pub status_warning: Rgba,
    pub status_error: Rgba,
}

impl Theme {
    pub(crate) fn light() -> Self {
        Self {
            surface_base: rgb(0xf4f7f5),
            surface_low: rgb(0xedf2ef),
            surface_high: rgb(0xffffff),
            surface_chrome: rgb(0xe7eeea),
            text_primary: rgb(0x152321),
            text_secondary: rgb(0x5f6e69),
            text_tertiary: rgb(0x84918d),
            outline_subtle: rgb(0xcbd6d2),
            outline_strong: rgb(0x9fafa9),
            action_primary: rgb(0x176c62),
            action_on_primary: rgb(0xffffff),
            action_soft: rgb(0xd5ebe6),
            route_trace: rgb(0xd46642),
            route_soft: rgb(0xf8e5dc),
            status_success: rgb(0x24795f),
            status_warning: rgb(0x9a6700),
            status_error: rgb(0xb54f49),
        }
    }

    pub(crate) fn dark() -> Self {
        Self {
            surface_base: rgb(0x0e1715),
            surface_low: rgb(0x111d1a),
            surface_high: rgb(0x172521),
            surface_chrome: rgb(0x13211e),
            text_primary: rgb(0xe3eeea),
            text_secondary: rgb(0xa4b4ae),
            text_tertiary: rgb(0x7d8e88),
            outline_subtle: rgb(0x2b3d37),
            outline_strong: rgb(0x435851),
            action_primary: rgb(0x79d7c6),
            action_on_primary: rgb(0x082a24),
            action_soft: rgb(0x1b4038),
            route_trace: rgb(0xf39b75),
            route_soft: rgb(0x402820),
            status_success: rgb(0x79d7b0),
            status_warning: rgb(0xe0a83a),
            status_error: rgb(0xef8c84),
        }
    }
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TextRole {
    PageTitle,
    SectionTitle,
    Body,
    Label,
    Metadata,
    Data,
}

#[allow(dead_code)]
impl TextRole {
    pub(crate) fn size(self) -> Pixels {
        match self {
            Self::PageTitle => px(20.0),
            Self::SectionTitle => px(16.0),
            Self::Body => px(13.0),
            Self::Label | Self::Data => px(12.0),
            Self::Metadata => px(11.0),
        }
    }

    pub(crate) fn line_height(self) -> Pixels {
        match self {
            Self::PageTitle => px(26.0),
            Self::SectionTitle => px(22.0),
            Self::Body => px(20.0),
            Self::Label | Self::Metadata => px(16.0),
            Self::Data => px(18.0),
        }
    }

    pub(crate) fn weight(self) -> FontWeight {
        match self {
            Self::PageTitle => FontWeight::BOLD,
            Self::SectionTitle | Self::Label | Self::Data => FontWeight::SEMIBOLD,
            Self::Body | Self::Metadata => FontWeight::NORMAL,
        }
    }

    pub(crate) fn uses_monospace(self) -> bool {
        matches!(self, Self::Data)
    }
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Space {
    None,
    Xs,
    Sm,
    Md,
    Lg,
    Xl,
    Xxl,
}

#[allow(dead_code)]
impl Space {
    pub(crate) fn px(self) -> Pixels {
        match self {
            Self::None => px(0.0),
            Self::Xs => px(4.0),
            Self::Sm => px(8.0),
            Self::Md => px(12.0),
            Self::Lg => px(16.0),
            Self::Xl => px(24.0),
            Self::Xxl => px(32.0),
        }
    }
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Radius {
    Control,
    Row,
    Pane,
    Window,
}

#[allow(dead_code)]
impl Radius {
    pub(crate) fn px(self) -> Pixels {
        match self {
            Self::Control | Self::Row => px(8.0),
            Self::Pane => px(12.0),
            Self::Window => px(18.0),
        }
    }
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ControlSize {
    Compact,
    Standard,
    Icon,
}

#[allow(dead_code)]
impl ControlSize {
    pub(crate) const fn component_size(self) -> gpui_component::Size {
        match self {
            Self::Compact | Self::Icon => gpui_component::Size::Small,
            Self::Standard => gpui_component::Size::Medium,
        }
    }

    pub(crate) fn height(self) -> Pixels {
        match self {
            Self::Compact => px(34.0),
            Self::Standard => px(38.0),
            Self::Icon => px(32.0),
        }
    }

    pub(crate) fn icon(self) -> Pixels {
        match self {
            Self::Compact | Self::Icon => px(16.0),
            Self::Standard => px(18.0),
        }
    }

    pub(crate) fn min_pointer_target(self) -> Pixels {
        match self {
            Self::Icon => px(32.0),
            Self::Compact => px(34.0),
            Self::Standard => px(38.0),
        }
    }
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LayoutMetric {
    MinWindowWidth,
    MinWindowHeight,
    WideNavigation,
    MediumNavigation,
    CompactNavigation,
    WidePolicyList,
    MediumPolicyList,
}

#[allow(dead_code)]
impl LayoutMetric {
    pub(crate) fn px(self) -> Pixels {
        match self {
            Self::MinWindowWidth => px(640.0),
            Self::MinWindowHeight => px(560.0),
            Self::WideNavigation => px(220.0),
            Self::MediumNavigation => px(66.0),
            Self::CompactNavigation => px(56.0),
            Self::WidePolicyList => px(326.0),
            Self::MediumPolicyList => px(292.0),
        }
    }
}

/// Projects the Manis palette into gpui-component's semantic theme.
///
/// Component behavior and layout stay upstream-owned, while the controls still
/// look native to Manis instead of switching to the library's default palette.
pub(crate) fn sync_component_theme(
    theme: Theme,
    dark: bool,
    window: Option<&mut gpui::Window>,
    cx: &mut gpui::App,
) {
    ComponentTheme::change(
        if dark {
            ThemeMode::Dark
        } else {
            ThemeMode::Light
        },
        window,
        cx,
    );

    let component = ComponentTheme::global_mut(cx);
    component.background = theme.surface_base.into();
    component.foreground = theme.text_primary.into();
    component.border = theme.outline_subtle.into();
    component.input = theme.outline_strong.into();
    component.popover = theme.surface_high.into();
    component.popover_foreground = theme.text_primary.into();
    component.muted = theme.surface_low.into();
    component.muted_foreground = theme.text_secondary.into();
    component.accent = theme.action_soft.into();
    component.accent_foreground = theme.text_primary.into();
    component.primary = theme.action_primary.into();
    component.primary_foreground = theme.action_on_primary.into();
    component.ring = theme.action_primary.into();
    component.button = theme.surface_low.into();
    component.button_foreground = theme.text_primary.into();
    component.button_hover = theme.action_soft.into();
    component.button_active = theme.action_soft.into();
    component.secondary = theme.surface_low.into();
    component.secondary_foreground = theme.text_primary.into();
    component.danger = theme.status_error.into();
    component.danger_foreground = theme.action_on_primary.into();
    ComponentTheme::sync_base(cx);
}

#[cfg(test)]
mod tests {
    use super::{ControlSize, LayoutMetric, Radius, Space, TextRole};

    fn assert_px(value: gpui::Pixels, expected: f32) {
        assert!((value.as_f32() - expected).abs() < f32::EPSILON);
    }

    #[test]
    fn typography_roles_form_a_stable_reading_scale() {
        assert!(TextRole::PageTitle.size().as_f32() > TextRole::SectionTitle.size().as_f32());
        assert!(TextRole::SectionTitle.size().as_f32() > TextRole::Body.size().as_f32());
        assert!(TextRole::Body.size().as_f32() > TextRole::Metadata.size().as_f32());
        assert!(TextRole::PageTitle.line_height().as_f32() > TextRole::PageTitle.size().as_f32());
        assert!(TextRole::Data.uses_monospace());
        assert!(!TextRole::Body.uses_monospace());
    }

    #[test]
    fn spacing_radius_and_control_tokens_match_the_design_system() {
        assert_px(Space::None.px(), 0.0);
        assert_px(Space::Xs.px(), 4.0);
        assert_px(Space::Sm.px(), 8.0);
        assert_px(Space::Md.px(), 12.0);
        assert_px(Space::Lg.px(), 16.0);
        assert_px(Space::Xl.px(), 24.0);
        assert_px(Space::Xxl.px(), 32.0);

        assert_px(Radius::Control.px(), 8.0);
        assert_px(Radius::Row.px(), 8.0);
        assert_px(Radius::Pane.px(), 12.0);
        assert_px(Radius::Window.px(), 18.0);

        assert_px(ControlSize::Compact.height(), 34.0);
        assert_px(ControlSize::Standard.height(), 38.0);
        assert_px(ControlSize::Icon.min_pointer_target(), 32.0);
        assert_eq!(
            ControlSize::Compact.component_size(),
            gpui_component::Size::Small
        );
        assert_eq!(
            ControlSize::Standard.component_size(),
            gpui_component::Size::Medium
        );
    }

    #[test]
    fn layout_metrics_preserve_the_adaptive_breakpoints() {
        assert_px(LayoutMetric::MinWindowWidth.px(), 640.0);
        assert_px(LayoutMetric::MinWindowHeight.px(), 560.0);
        assert_px(LayoutMetric::WideNavigation.px(), 220.0);
        assert_px(LayoutMetric::MediumNavigation.px(), 66.0);
        assert_px(LayoutMetric::CompactNavigation.px(), 56.0);
        assert_px(LayoutMetric::WidePolicyList.px(), 326.0);
        assert_px(LayoutMetric::MediumPolicyList.px(), 292.0);
    }
}
