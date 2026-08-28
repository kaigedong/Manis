#![allow(clippy::unreadable_literal)]

use gpui::{Rgba, rgb};
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
