#![allow(clippy::unreadable_literal)]

//! Manis UI 2.0 tokens: "Obsidian & Alabaster — Swiss Precision Routing".
//!
//! `design-system/manis/tokens.json` is the single source of truth for both the desktop GPUI
//! application and the Android Compose application. Keep this file a literal projection of it;
//! a colour that exists here but not there is a design regression, not a local tweak.

use gpui::{FontWeight, Pixels, Rgba, px, rgb, rgba};
use gpui_component::{Theme as ComponentTheme, ThemeMode};

#[derive(Clone, Copy)]
pub(crate) struct Theme {
    pub window_backdrop: Rgba,
    /// The alabaster/obsidian workspace canvas. Page background.
    pub surface_base: Rgba,
    /// Recessed wells: segmented tracks, input backgrounds, inset strips.
    pub surface_low: Rgba,
    /// Hover fills and pressed surfaces.
    pub surface_high: Rgba,
    /// Dialogs, popovers, bottom sheets, utility editors.
    pub surface_overlay: Rgba,
    /// Navigation rail, command bar, mobile bottom bar.
    pub surface_chrome: Rgba,
    /// Opaque content cards: policy groups, subscriptions, settings groups.
    pub surface_elevated: Rgba,
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
    pub button_bg: Rgba,
    pub button_hover: Rgba,
    pub button_active: Rgba,
    pub focus_ring: Rgba,
    pub modal_scrim: Rgba,
    /// Royal Violet, reserved for the rule -> policy group -> exit chain.
    pub route_trace: Rgba,
    pub route_soft: Rgba,
    pub status_success: Rgba,
    pub status_success_soft: Rgba,
    pub status_warning: Rgba,
    pub status_warning_soft: Rgba,
    pub status_error: Rgba,
    pub status_error_soft: Rgba,
    /// Destructive buttons own an independent fill/label pair.
    pub danger_fill: Rgba,
    pub danger_hover: Rgba,
    pub danger_active: Rgba,
    pub danger_on: Rgba,
    pub log_info: Rgba,
    pub log_debug: Rgba,
    pub log_trace: Rgba,
}

impl Theme {
    pub(crate) fn light() -> Self {
        Self {
            // Warm architectural plaster: canvas #F7F6F3, white cards, a 1px warm stone hairline.
            window_backdrop: rgb(0xf7f6f3),
            surface_base: rgb(0xf7f6f3),
            surface_low: rgb(0xf1efea),
            surface_high: rgb(0xe6e3dc),
            surface_overlay: rgb(0xffffff),
            surface_chrome: rgb(0xefece6),
            surface_elevated: rgb(0xffffff),
            text_primary: rgb(0x141413),
            text_secondary: rgb(0x4e4d49),
            text_tertiary: rgb(0x5e5c57),
            outline_subtle: rgb(0xe5e2dc),
            outline_strong: rgb(0x8b8881),
            action_primary: rgb(0x2b4eff),
            action_primary_hover: rgb(0x1f3ee0),
            action_primary_active: rgb(0x1732bd),
            action_on_primary: rgb(0xffffff),
            action_soft: rgb(0xeef2ff),
            button_bg: rgb(0xf4f3ef),
            button_hover: rgb(0xeae7e0),
            button_active: rgb(0xdfdcd3),
            focus_ring: rgb(0x2b4eff),
            modal_scrim: rgba(0x14141359),
            route_trace: rgb(0x6d28d9),
            route_soft: rgb(0xf3eeff),
            status_success: rgb(0x0e7a4b),
            status_success_soft: rgb(0xe8f7f0),
            status_warning: rgb(0x9a5504),
            status_warning_soft: rgb(0xfef5e7),
            status_error: rgb(0xc81e1e),
            status_error_soft: rgb(0xfef2f2),
            danger_fill: rgb(0xc81e1e),
            danger_hover: rgb(0xa61717),
            danger_active: rgb(0x861212),
            danger_on: rgb(0xffffff),
            log_info: rgb(0x1d4ed8),
            log_debug: rgb(0x6d28d9),
            log_trace: rgb(0x0f766e),
        }
    }

    pub(crate) fn dark() -> Self {
        Self {
            // True neutral obsidian: no cold-blue mud, no green cast.
            window_backdrop: rgb(0x0d0d0f),
            surface_base: rgb(0x0d0d0f),
            surface_low: rgb(0x17171b),
            surface_high: rgb(0x25252c),
            surface_overlay: rgb(0x212127),
            surface_chrome: rgb(0x141417),
            surface_elevated: rgb(0x1c1c21),
            text_primary: rgb(0xf4f4f6),
            text_secondary: rgb(0xb4b4be),
            text_tertiary: rgb(0x9a9aa6),
            outline_subtle: rgb(0x2b2b33),
            outline_strong: rgb(0x787886),
            action_primary: rgb(0x6b8aff),
            action_primary_hover: rgb(0x859fff),
            action_primary_active: rgb(0xa1b5ff),
            action_on_primary: rgb(0x0a0d18),
            action_soft: rgb(0x1b2342),
            button_bg: rgb(0x24242c),
            button_hover: rgb(0x2e2e38),
            button_active: rgb(0x383844),
            focus_ring: rgb(0x859fff),
            modal_scrim: rgba(0x00000080),
            route_trace: rgb(0xc4b5fd),
            route_soft: rgb(0x271f3d),
            status_success: rgb(0x4ade80),
            status_success_soft: rgb(0x122b20),
            status_warning: rgb(0xfbbf24),
            status_warning_soft: rgb(0x2e2312),
            status_error: rgb(0xf87171),
            status_error_soft: rgb(0x331719),
            danger_fill: rgb(0xc81e1e),
            danger_hover: rgb(0xa61717),
            danger_active: rgb(0x861212),
            danger_on: rgb(0xffffff),
            log_info: rgb(0x93c5fd),
            log_debug: rgb(0xd8b4fe),
            log_trace: rgb(0x5eead4),
        }
    }

    /// The status colour a measured latency earns, or the neutral token when it is unknown.
    pub(crate) fn latency_color(self, milliseconds: Option<u64>) -> Rgba {
        match milliseconds {
            Some(value) if value < 120 => self.status_success,
            Some(value) if value <= 350 => self.status_warning,
            Some(_) => self.status_error,
            None => self.text_tertiary,
        }
    }
}

/// The sans stack resolves to the platform UI face; the mono stack keeps every number tabular.
///
/// A concrete face is named per platform: the generic `monospace` alias resolves to a
/// proportional face on some font-kit backends, which breaks the tabular guarantee.
#[cfg(target_os = "macos")]
pub(crate) const MONO_FAMILY: &str = "Menlo";
#[cfg(target_os = "windows")]
pub(crate) const MONO_FAMILY: &str = "Consolas";
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub(crate) const MONO_FAMILY: &str = "DejaVu Sans Mono";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TextRole {
    PageTitle,
    SectionTitle,
    Body,
    Label,
    /// Latency, traffic, IPs, rule numbers, timestamps: always monospaced and tabular.
    Data,
    Metadata,
}

impl TextRole {
    pub(crate) fn size(self) -> Pixels {
        match self {
            Self::PageTitle => px(22.0),
            Self::SectionTitle => px(15.0),
            Self::Body => px(13.5),
            Self::Label => px(13.0),
            Self::Data => px(12.0),
            Self::Metadata => px(11.5),
        }
    }

    pub(crate) fn line_height(self) -> Pixels {
        match self {
            Self::PageTitle => px(30.0),
            Self::SectionTitle => px(22.0),
            Self::Body => px(20.0),
            Self::Label | Self::Data => px(18.0),
            Self::Metadata => px(16.0),
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
    Xxl,
}

impl Space {
    pub(crate) fn px(self) -> Pixels {
        match self {
            Self::Xs => px(4.0),
            Self::Sm => px(8.0),
            Self::Md => px(12.0),
            Self::Lg => px(16.0),
            Self::Xl => px(24.0),
            Self::Xxl => px(32.0),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Radius {
    /// Region codes, protocol/property pills.
    Badge,
    Control,
    Row,
    Card,
    Pane,
    Sheet,
}

impl Radius {
    pub(crate) fn px(self) -> Pixels {
        match self {
            Self::Badge => px(5.0),
            Self::Control => px(7.0),
            Self::Row => px(8.0),
            Self::Card | Self::Pane => px(12.0),
            Self::Sheet => px(16.0),
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
    pub(crate) fn height(self) -> Pixels {
        match self {
            Self::Compact | Self::Icon => px(28.0),
            Self::Standard => px(32.0),
        }
    }

    pub(crate) fn min_pointer_target(self) -> Pixels {
        self.height()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LayoutMetric {
    MinWindowWidth,
    MinWindowHeight,
    WideNavigation,
    MediumNavigation,
    CompactNavigation,
    CommandBarHeight,
    BrandRowHeight,
    ContentMaxWidth,
}

impl LayoutMetric {
    pub(crate) fn px(self) -> Pixels {
        match self {
            Self::MinWindowWidth => px(640.0),
            Self::MinWindowHeight => px(560.0),
            Self::WideNavigation => px(232.0),
            Self::MediumNavigation => px(68.0),
            Self::CompactNavigation | Self::CommandBarHeight | Self::BrandRowHeight => px(56.0),
            Self::ContentMaxWidth => px(1160.0),
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
    component.radius = Radius::Control.px();
    component.radius_lg = Radius::Card.px();
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
    component.button = theme.button_bg.into();
    component.button_foreground = theme.text_primary.into();
    component.button_hover = theme.button_hover.into();
    component.button_active = theme.button_active.into();
    component.button_primary = theme.action_primary.into();
    component.button_primary_hover = theme.action_primary_hover.into();
    component.button_primary_active = theme.action_primary_active.into();
    component.button_primary_foreground = theme.action_on_primary.into();
    component.button_secondary = theme.button_bg.into();
    component.button_secondary_hover = theme.button_hover.into();
    component.button_secondary_active = theme.button_active.into();
    component.button_secondary_foreground = theme.text_primary.into();
    // Destructive fills have their own contrast pair; status text and an inverted
    // dark-mode primary button must never determine a destructive button's label.
    component.button_danger = theme.danger_fill.into();
    component.button_danger_hover = theme.danger_hover.into();
    component.button_danger_active = theme.danger_active.into();
    component.button_danger_foreground = theme.danger_on.into();
    component.secondary = theme.surface_low.into();
    component.secondary_hover = theme.surface_high.into();
    component.secondary_active = theme.button_active.into();
    component.secondary_foreground = theme.text_primary.into();
    component.danger = theme.status_error.into();
    component.danger_hover = theme.danger_hover.into();
    component.danger_active = theme.danger_active.into();
    component.danger_foreground = theme.danger_on.into();
    // Compound components must not paint their upstream opaque card underneath Manis'
    // rounded shells. Their titles and rows own the visible material instead.
    component.accordion = rgba(0x00000000).into();
    component.group_box = theme.surface_elevated.into();
    component.group_box_foreground = theme.text_primary.into();
    component.colors.list = theme.surface_elevated.into();
    component.colors.list_even = theme.surface_elevated.into();
    component.colors.list_head = theme.surface_low.into();
    component.colors.list_hover = theme.surface_high.into();
    component.colors.list_active = theme.action_soft.into();
    component.list_active_border = theme.outline_subtle.into();
    component.table = theme.surface_elevated.into();
    component.table_even = theme.surface_elevated.into();
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
    component.sidebar_accent_foreground = theme.action_primary.into();
    component.sidebar_primary = theme.action_primary.into();
    component.sidebar_primary_foreground = theme.action_on_primary.into();
    component.tab_bar = theme.surface_chrome.into();
    component.tab_bar_segmented = theme.surface_low.into();
    component.tab = theme.surface_chrome.into();
    component.tab_foreground = theme.text_secondary.into();
    component.tab_active = theme.surface_elevated.into();
    component.tab_active_foreground = theme.text_primary.into();
    component.progress_bar = theme.action_primary.into();
    component.slider_bar = theme.action_primary.into();
    component.slider_thumb = theme.action_primary.into();
    component.switch = if theme.surface_base == rgb(0xf7f6f3) {
        rgb(0xd0cdc6)
    } else {
        rgb(0x3a3a45)
    }
    .into();
    component.switch_thumb = theme.surface_elevated.into();
    component.scrollbar = rgba(0x00000000).into();
    component.scrollbar_thumb = theme.outline_strong.into();
    component.scrollbar_thumb_hover = theme.text_secondary.into();
    component.skeleton = theme.action_soft.into();
    component.description_list_label = theme.surface_low.into();
    component.description_list_label_foreground = theme.text_secondary.into();
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
        assert_px(Space::Xxl.px(), 32.0);

        assert_px(Radius::Badge.px(), 5.0);
        assert_px(Radius::Control.px(), 7.0);
        assert_px(Radius::Row.px(), 8.0);
        assert_px(Radius::Card.px(), 12.0);
        assert_px(Radius::Pane.px(), 12.0);
        assert_px(Radius::Sheet.px(), 16.0);

        assert_px(ControlSize::Compact.height(), 28.0);
        assert_px(ControlSize::Standard.height(), 32.0);
        assert_px(ControlSize::Icon.min_pointer_target(), 28.0);
    }

    fn surfaces(theme: Theme) -> [gpui::Rgba; 8] {
        [
            theme.window_backdrop,
            theme.surface_base,
            theme.surface_low,
            theme.surface_high,
            theme.surface_overlay,
            theme.surface_chrome,
            theme.surface_elevated,
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
    fn content_surfaces_are_opaque_and_distinct_in_both_themes() {
        for theme in [Theme::light(), Theme::dark()] {
            for surface in surfaces(theme) {
                assert_opaque(surface);
            }
            assert_ne!(theme.surface_base, theme.surface_chrome);
            assert_ne!(theme.surface_base, theme.surface_low);
            assert_ne!(theme.surface_low, theme.surface_high);
            assert_ne!(theme.surface_high, theme.action_soft);
            assert_ne!(theme.surface_base, theme.surface_elevated);
            // Only the scrim dims an already opaque application; dialog content never does.
            assert!((0.2..=0.6).contains(&theme.modal_scrim.a));
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
                for surface in [
                    theme.surface_base,
                    theme.surface_low,
                    theme.surface_overlay,
                    theme.surface_elevated,
                ] {
                    assert_contrast(status, surface, 4.5);
                }
            }
            assert_contrast(theme.route_trace, theme.route_soft, 4.5);
            // Level tags use a 10% tint of their own colour. TRACE has no tinted tag in the
            // design, so it is only required to read on a plain surface.
            for level in [
                theme.log_info,
                theme.log_debug,
                theme.status_warning,
                theme.status_error,
            ] {
                assert_contrast(level, theme.surface_base.blend(level.opacity(0.1)), 4.5);
            }
            for surface in [
                theme.surface_base,
                theme.surface_low,
                theme.surface_elevated,
            ] {
                assert_contrast(theme.log_trace, surface, 4.5);
            }
        }
    }

    #[test]
    fn soft_status_fills_carry_their_status_text() {
        for theme in [Theme::light(), Theme::dark()] {
            for (foreground, background) in [
                (theme.status_success, theme.status_success_soft),
                (theme.status_warning, theme.status_warning_soft),
                (theme.status_error, theme.status_error_soft),
                (theme.route_trace, theme.route_soft),
                (theme.action_primary, theme.action_soft),
                (theme.danger_on, theme.danger_fill),
            ] {
                assert_contrast(foreground, background, 4.5);
            }
        }
    }

    #[test]
    fn controls_and_keyboard_focus_remain_visible() {
        for theme in [Theme::light(), Theme::dark()] {
            // `border.strong` is specified for interactive input boundaries on content
            // surfaces. The chrome carries navigation only, so it keeps the non-text floor.
            for surface in [theme.surface_base, theme.surface_overlay] {
                assert_contrast(theme.outline_strong, surface, 3.0);
            }
            assert_contrast(theme.outline_strong, theme.surface_chrome, 2.9);
            for surface in [
                theme.surface_base,
                theme.surface_overlay,
                theme.surface_chrome,
            ] {
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
    fn button_states_are_distinct_and_readable() {
        for theme in [Theme::light(), Theme::dark()] {
            let fills = [theme.button_bg, theme.button_hover, theme.button_active];
            for fill in fills {
                assert_opaque(fill);
                assert_contrast(theme.text_primary, fill, 4.5);
            }
            assert_ne!(fills[0], fills[1]);
            assert_ne!(fills[1], fills[2]);
            let mut component = ComponentTheme::default();
            project_component_palette(&mut component, theme);
            assert_eq!(component.button, component.button_secondary);
            assert_ne!(component.button_primary, component.button_secondary);
            assert_eq!(component.button_primary, theme.action_primary.into());
            assert_eq!(component.button_hover, component.button_secondary_hover);
            assert_eq!(component.button_active, component.button_secondary_active);
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
    fn latency_colors_follow_the_design_bands() {
        for theme in [Theme::light(), Theme::dark()] {
            assert_eq!(theme.latency_color(None), theme.text_tertiary);
            assert_eq!(theme.latency_color(Some(38)), theme.status_success);
            assert_eq!(theme.latency_color(Some(119)), theme.status_success);
            assert_eq!(theme.latency_color(Some(120)), theme.status_warning);
            assert_eq!(theme.latency_color(Some(350)), theme.status_warning);
            assert_eq!(theme.latency_color(Some(351)), theme.status_error);
        }
    }

    #[test]
    fn layout_metrics_preserve_the_adaptive_breakpoints() {
        assert_px(LayoutMetric::MinWindowWidth.px(), 640.0);
        assert_px(LayoutMetric::MinWindowHeight.px(), 560.0);
        assert_px(LayoutMetric::WideNavigation.px(), 232.0);
        assert_px(LayoutMetric::MediumNavigation.px(), 68.0);
        assert_px(LayoutMetric::CompactNavigation.px(), 56.0);
        assert_px(LayoutMetric::CommandBarHeight.px(), 56.0);
        assert_px(LayoutMetric::BrandRowHeight.px(), 56.0);
        assert_px(LayoutMetric::ContentMaxWidth.px(), 1160.0);
    }
}
