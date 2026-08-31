#![allow(clippy::unreadable_literal)]

use gpui::{FontWeight, Pixels, Rgba, px, rgb, rgba};
use gpui_component::{Theme as ComponentTheme, ThemeMode};

#[derive(Clone, Copy)]
pub(crate) struct Theme {
    pub window_backdrop: Rgba,
    pub surface_base: Rgba,
    pub surface_low: Rgba,
    pub surface_high: Rgba,
    pub surface_overlay: Rgba,
    pub surface_chrome: Rgba,
    pub text_primary: Rgba,
    pub text_secondary: Rgba,
    pub text_tertiary: Rgba,
    pub outline_subtle: Rgba,
    pub outline_strong: Rgba,
    pub action_primary: Rgba,
    pub action_primary_hover: Rgba,
    pub action_primary_active: Rgba,
    pub action_on_primary: Rgba,
    pub action_soft: Rgba,
    pub focus_ring: Rgba,
    pub modal_scrim: Rgba,
    pub route_trace: Rgba,
    pub route_soft: Rgba,
    pub status_success: Rgba,
    pub status_warning: Rgba,
    pub status_error: Rgba,
}

impl Theme {
    pub(crate) fn light() -> Self {
        Self {
            // Surface hierarchy comes from neutral luminance, never desktop bleed-through.
            // See docs/interface-design.md for the public design references and contrast policy.
            window_backdrop: rgb(0xffffff),
            surface_base: rgb(0xffffff),
            surface_low: rgb(0xf9f9f9),
            surface_high: rgb(0xf3f3f3),
            surface_overlay: rgb(0xffffff),
            surface_chrome: rgb(0xf9f9f9),
            text_primary: rgb(0x0d0d0d),
            text_secondary: rgb(0x5d5d5d),
            text_tertiary: rgb(0x666666),
            outline_subtle: rgb(0xe5e5e5),
            outline_strong: rgb(0x858585),
            action_primary: rgb(0x181818),
            action_primary_hover: rgb(0x303030),
            action_primary_active: rgb(0x414141),
            action_on_primary: rgb(0xffffff),
            action_soft: rgb(0xededed),
            focus_ring: rgb(0x2563eb),
            modal_scrim: rgba(0x00000040),
            route_trace: rgb(0x923b0f),
            route_soft: rgb(0xfff5f0),
            status_success: rgb(0x00692a),
            status_warning: rgb(0x875500),
            status_error: rgb(0xb42318),
        }
    }

    pub(crate) fn dark() -> Self {
        Self {
            window_backdrop: rgb(0x212121),
            surface_base: rgb(0x212121),
            surface_low: rgb(0x282828),
            surface_high: rgb(0x303030),
            surface_overlay: rgb(0x303030),
            surface_chrome: rgb(0x181818),
            text_primary: rgb(0xf3f3f3),
            text_secondary: rgb(0xb9b9b9),
            text_tertiary: rgb(0xababab),
            outline_subtle: rgb(0x3d3d3d),
            outline_strong: rgb(0x858585),
            action_primary: rgb(0xf3f3f3),
            action_primary_hover: rgb(0xededed),
            action_primary_active: rgb(0xdcdcdc),
            action_on_primary: rgb(0x0d0d0d),
            action_soft: rgb(0x393939),
            focus_ring: rgb(0x8cb4ff),
            modal_scrim: rgba(0x00000066),
            route_trace: rgb(0xffb790),
            route_soft: rgb(0x362820),
            status_success: rgb(0x73d499),
            status_warning: rgb(0xf0c36a),
            status_error: rgb(0xff9a96),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TextRole {
    PageTitle,
    SectionTitle,
    Body,
    Label,
    Metadata,
    Data,
}

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
            Self::PageTitle | Self::SectionTitle => FontWeight::SEMIBOLD,
            Self::Label | Self::Data => FontWeight::MEDIUM,
            Self::Body | Self::Metadata => FontWeight::NORMAL,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Space {
    Xs,
    Sm,
    Md,
    Lg,
    Xl,
}

impl Space {
    pub(crate) fn px(self) -> Pixels {
        match self {
            Self::Xs => px(4.0),
            Self::Sm => px(8.0),
            Self::Md => px(12.0),
            Self::Lg => px(16.0),
            Self::Xl => px(24.0),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Radius {
    Control,
    Row,
    Pane,
}

impl Radius {
    pub(crate) fn px(self) -> Pixels {
        match self {
            Self::Control | Self::Row => px(8.0),
            Self::Pane => px(12.0),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ControlSize {
    Compact,
    Standard,
    Icon,
}

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

    pub(crate) fn min_pointer_target(self) -> Pixels {
        match self {
            Self::Icon => px(32.0),
            Self::Compact => px(34.0),
            Self::Standard => px(38.0),
        }
    }
}

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

    project_component_palette(ComponentTheme::global_mut(cx), theme);
    ComponentTheme::sync_base(cx);
}

fn project_component_palette(component: &mut ComponentTheme, theme: Theme) {
    component.background = theme.surface_base.into();
    component.foreground = theme.text_primary.into();
    component.border = theme.outline_subtle.into();
    component.input = theme.outline_strong.into();
    component.popover = theme.surface_overlay.into();
    component.popover_foreground = theme.text_primary.into();
    component.muted = theme.surface_low.into();
    component.muted_foreground = theme.text_secondary.into();
    component.accent = theme.action_soft.into();
    component.accent_foreground = theme.text_primary.into();
    component.primary = theme.action_primary.into();
    component.primary_hover = theme.action_primary_hover.into();
    component.primary_active = theme.action_primary_active.into();
    component.primary_foreground = theme.action_on_primary.into();
    component.ring = theme.focus_ring.into();
    component.caret = theme.text_primary.into();
    component.selection = theme.action_soft.into();
    component.overlay = theme.modal_scrim.into();
    component.button = theme.surface_base.into();
    component.button_foreground = theme.text_primary.into();
    component.button_hover = theme.surface_high.into();
    component.button_active = theme.action_soft.into();
    component.button_primary = theme.action_primary.into();
    component.button_primary_hover = theme.action_primary_hover.into();
    component.button_primary_active = theme.action_primary_active.into();
    component.button_primary_foreground = theme.action_on_primary.into();
    component.button_secondary = theme.surface_base.into();
    component.button_secondary_hover = theme.surface_high.into();
    component.button_secondary_active = theme.action_soft.into();
    component.button_secondary_foreground = theme.text_primary.into();
    // Destructive fills have their own contrast pair; status text and an inverted
    // dark-mode primary button must never determine a destructive button's label.
    component.button_danger = rgb(0xba2623).into();
    component.button_danger_hover = rgb(0x911e1b).into();
    component.button_danger_active = rgb(0x6e1615).into();
    component.button_danger_foreground = rgb(0xffffff).into();
    component.secondary = theme.surface_base.into();
    component.secondary_hover = theme.surface_high.into();
    component.secondary_active = theme.action_soft.into();
    component.secondary_foreground = theme.text_primary.into();
    component.danger = theme.status_error.into();
    component.danger_hover = component.button_danger_hover;
    component.danger_active = component.button_danger_active;
    component.danger_foreground = theme.action_on_primary.into();
    // Compound components must not paint their upstream opaque card underneath Manis'
    // rounded shells. Their titles and rows own the visible material instead.
    component.accordion = rgba(0x00000000).into();
    component.group_box = theme.surface_base.into();
    component.group_box_foreground = theme.text_primary.into();
    component.colors.list = theme.surface_base.into();
    component.colors.list_even = theme.surface_base.into();
    component.colors.list_head = theme.surface_low.into();
    component.colors.list_hover = theme.surface_high.into();
    component.colors.list_active = theme.action_soft.into();
    component.list_active_border = theme.outline_subtle.into();
    component.table = theme.surface_base.into();
    component.table_even = theme.surface_base.into();
    component.table_head = theme.surface_low.into();
    component.table_hover = theme.surface_high.into();
    component.table_active = theme.action_soft.into();
    component.table_active_border = theme.outline_subtle.into();
    component.table_head_foreground = theme.text_secondary.into();
    component.table_foot = theme.surface_low.into();
    component.table_foot_foreground = theme.text_secondary.into();
    component.table_row_border = theme.outline_subtle.into();
    project_component_chrome(component, theme);
    sync_component_color_tokens(component);
}

fn project_component_chrome(component: &mut ComponentTheme, theme: Theme) {
    component.title_bar = theme.surface_chrome.into();
    component.title_bar_border = theme.outline_subtle.into();
    component.status_bar = theme.surface_chrome.into();
    component.status_bar_border = theme.outline_subtle.into();
    component.sidebar = theme.surface_chrome.into();
    component.sidebar_foreground = theme.text_primary.into();
    component.sidebar_border = theme.outline_subtle.into();
    component.sidebar_accent = theme.action_soft.into();
    component.sidebar_accent_foreground = theme.text_primary.into();
    component.sidebar_primary = theme.action_primary.into();
    component.sidebar_primary_foreground = theme.action_on_primary.into();
    component.tab_bar = theme.surface_base.into();
    component.tab_bar_segmented = theme.surface_low.into();
    component.tab = theme.surface_base.into();
    component.tab_foreground = theme.text_secondary.into();
    component.tab_active = theme.action_soft.into();
    component.tab_active_foreground = theme.text_primary.into();
    component.progress_bar = theme.action_primary.into();
    component.slider_bar = theme.action_primary.into();
    component.slider_thumb = theme.action_primary.into();
    component.switch = theme.outline_strong.into();
    component.switch_thumb = theme.surface_base.into();
    component.scrollbar = rgba(0x00000000).into();
    component.scrollbar_thumb = theme.outline_strong.into();
    component.scrollbar_thumb_hover = theme.text_secondary.into();
    component.skeleton = theme.action_soft.into();
    component.description_list_label = theme.surface_low.into();
    component.description_list_label_foreground = theme.text_secondary.into();
    component.tiles = theme.surface_base.into();
}

/// gpui-component keeps legacy colors and renderable tokens separately. Root and newer
/// components read `tokens`, so every palette projection must update both representations.
fn sync_component_color_tokens(component: &mut ComponentTheme) {
    component.tokens = component.colors.into();
}

#[cfg(test)]
mod tests {
    use super::{
        ComponentTheme, ControlSize, LayoutMetric, Radius, Space, TextRole, Theme,
        project_component_palette,
    };

    fn assert_px(value: gpui::Pixels, expected: f32) {
        assert!((value.as_f32() - expected).abs() < f32::EPSILON);
    }

    #[test]
    fn typography_roles_form_a_stable_reading_scale() {
        assert!(TextRole::PageTitle.size().as_f32() > TextRole::SectionTitle.size().as_f32());
        assert!(TextRole::SectionTitle.size().as_f32() > TextRole::Body.size().as_f32());
        assert!(TextRole::Body.size().as_f32() > TextRole::Metadata.size().as_f32());
        assert!(TextRole::PageTitle.line_height().as_f32() > TextRole::PageTitle.size().as_f32());
    }

    #[test]
    fn spacing_radius_and_control_tokens_match_the_design_system() {
        assert_px(Space::Xs.px(), 4.0);
        assert_px(Space::Sm.px(), 8.0);
        assert_px(Space::Md.px(), 12.0);
        assert_px(Space::Lg.px(), 16.0);
        assert_px(Space::Xl.px(), 24.0);

        assert_px(Radius::Control.px(), 8.0);
        assert_px(Radius::Row.px(), 8.0);
        assert_px(Radius::Pane.px(), 12.0);

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

    fn surfaces(theme: Theme) -> [gpui::Rgba; 7] {
        [
            theme.window_backdrop,
            theme.surface_base,
            theme.surface_low,
            theme.surface_high,
            theme.surface_overlay,
            theme.surface_chrome,
            theme.action_soft,
        ]
    }

    fn assert_opaque(color: gpui::Rgba) {
        assert!((color.a - 1.0).abs() < f32::EPSILON, "{color:?}");
    }

    fn luminance(color: gpui::Rgba) -> f32 {
        let linear = |channel: f32| {
            if channel <= 0.04045 {
                channel / 12.92
            } else {
                ((channel + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * linear(color.r) + 0.7152 * linear(color.g) + 0.0722 * linear(color.b)
    }

    fn assert_contrast(foreground: gpui::Rgba, background: gpui::Rgba, minimum: f32) {
        assert_opaque(foreground);
        assert_opaque(background);
        let first = luminance(foreground);
        let second = luminance(background);
        let ratio = (first.max(second) + 0.05) / (first.min(second) + 0.05);
        assert!(
            ratio >= minimum,
            "{foreground:?} on {background:?}: {ratio}:1 < {minimum}:1"
        );
    }

    #[test]
    fn content_surfaces_are_opaque_and_neutral_in_both_themes() {
        for theme in [Theme::light(), Theme::dark()] {
            for surface in surfaces(theme) {
                assert_opaque(surface);
                assert!((surface.r - surface.g).abs() < f32::EPSILON);
                assert!((surface.g - surface.b).abs() < f32::EPSILON);
            }
            assert_ne!(theme.surface_base, theme.surface_chrome);
            assert_ne!(theme.surface_base, theme.surface_low);
            assert_ne!(theme.surface_low, theme.surface_high);
            assert_ne!(theme.surface_high, theme.action_soft);
            // Only the scrim dims an already opaque application; dialog content never does.
            assert!((0.2..=0.4).contains(&theme.modal_scrim.a));
        }
    }

    #[test]
    fn all_normal_text_roles_meet_aa_on_every_content_surface() {
        for theme in [Theme::light(), Theme::dark()] {
            for surface in surfaces(theme) {
                for foreground in [
                    theme.text_primary,
                    theme.text_secondary,
                    theme.text_tertiary,
                ] {
                    assert_contrast(foreground, surface, 4.5);
                }
            }
            for status in [
                theme.status_success,
                theme.status_warning,
                theme.status_error,
            ] {
                for surface in [theme.surface_base, theme.surface_low, theme.surface_overlay] {
                    assert_contrast(status, surface, 4.5);
                }
            }
            assert_contrast(theme.route_trace, theme.route_soft, 4.5);
        }
    }

    #[test]
    fn controls_and_keyboard_focus_remain_visible() {
        for theme in [Theme::light(), Theme::dark()] {
            for surface in [
                theme.surface_base,
                theme.surface_overlay,
                theme.surface_chrome,
            ] {
                assert_contrast(theme.outline_strong, surface, 3.0);
                assert_contrast(theme.focus_ring, surface, 3.0);
            }
            for fill in [
                theme.action_primary,
                theme.action_primary_hover,
                theme.action_primary_active,
            ] {
                assert_contrast(theme.action_on_primary, fill, 4.5);
            }
            assert_ne!(theme.action_primary, theme.action_primary_hover);
            assert_ne!(theme.action_primary_hover, theme.action_primary_active);
        }
    }

    #[test]
    fn component_render_tokens_follow_projected_palette() {
        // Reuse the same component to catch stale tokens after switching themes.
        let mut component = ComponentTheme::default();
        for theme in [Theme::light(), Theme::dark(), Theme::light()] {
            project_component_palette(&mut component, theme);
            for (token, color) in [
                (component.tokens.background, component.background),
                (component.tokens.popover, component.popover),
                (component.tokens.table, component.table),
                (component.tokens.list, component.colors.list),
                (component.tokens.tab_bar, component.tab_bar),
                (component.tokens.sidebar, component.sidebar),
                (component.tokens.title_bar, component.title_bar),
                (component.tokens.button_primary, component.button_primary),
                (component.tokens.button_danger, component.button_danger),
            ] {
                assert_eq!(token.color, color);
                assert_eq!(token.background, gpui::Background::from(color));
                assert_opaque(color.into());
            }
            assert_eq!(component.background, theme.surface_base.into());
            assert_eq!(component.popover, theme.surface_overlay.into());
            assert_eq!(component.tokens.ring.color, theme.focus_ring.into());
            assert_eq!(component.tokens.overlay.color, theme.modal_scrim.into());
            assert!(component.tokens.accordion.color.a.abs() < f32::EPSILON);
            for fill in [
                component.button_danger,
                component.button_danger_hover,
                component.button_danger_active,
            ] {
                assert_contrast(component.button_danger_foreground.into(), fill.into(), 4.5);
            }
        }
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
