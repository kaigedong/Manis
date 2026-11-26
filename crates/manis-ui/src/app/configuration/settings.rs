use gpui::{
    AnyElement, Context, Div, FontWeight, IntoElement, ParentElement, Role, SharedString, Stateful,
    Styled, Window, div, prelude::*, px,
};
use gpui_component::{Icon, IconName};
use manis_core::WindowSizeClass;

use crate::app::{AppUpdateState, ConfigurationSection, ManisApp, MihomoCoreUpdateState};
use crate::{
    app_update,
    components::{ActionRole, action_button, style_action_button},
    localization::{
        CountNoun, Language, LanguagePreference, Message, copy, save_language_preference_in,
    },
    theme::{ControlSize, Radius, Space, TextRole, Theme},
};

use super::{configuration_section_label, language_preference_label};

mod kernel;
mod language;
pub(in crate::app) mod navigation;
mod routing;
mod startup;
mod updates;

/// A calm inset-grouped settings card: pure elevated surface with crisp rounded corners and
/// whisper-thin separation instead of heavy boxy frames.
pub(super) fn settings_group_card(id: &'static str, theme: Theme) -> Stateful<Div> {
    let border = if theme.is_dark() {
        gpui::Hsla::from(theme.outline_subtle).opacity(0.85)
    } else {
        gpui::Hsla::from(theme.outline_subtle).opacity(0.6)
    };
    div()
        .id(id)
        .w_full()
        .rounded(px(14.0))
        .overflow_hidden()
        .border_1()
        .border_color(border)
        .bg(theme.surface_elevated)
        .when(!theme.is_dark(), gpui::Styled::shadow_xs)
}

/// External section heading that floats quietly above a settings group card.
pub(super) fn settings_section_header(
    title: impl Into<SharedString>,
    detail: Option<SharedString>,
    action: Option<AnyElement>,
    theme: Theme,
) -> Div {
    let title = title.into();
    div()
        .w_full()
        .px(px(4.0))
        .mb(px(8.0))
        .min_h(px(28.0))
        .flex()
        .items_center()
        .justify_between()
        .gap(Space::Md.px())
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(
                    div()
                        .text_size(px(13.0))
                        .line_height(px(18.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme.text_secondary)
                        .child(title),
                )
                .when_some(detail, |row, detail| {
                    row.child(
                        div()
                            .font_family(crate::theme::MONO_FAMILY)
                            .text_size(px(11.5))
                            .line_height(px(16.0))
                            .text_color(theme.text_tertiary)
                            .child(detail),
                    )
                }),
        )
        .when_some(action, |row, action| {
            row.child(div().flex_shrink_0().child(action))
        })
}

/// Uniform rounded-square icon badge anchored to the left of modern settings rows.
pub(super) fn settings_icon_badge(icon: impl Into<Icon>, theme: Theme) -> Div {
    let (bg, fg) = if theme.is_dark() {
        (theme.action_soft, theme.action_primary)
    } else {
        (theme.action_primary, theme.action_on_primary)
    };
    div()
        .size(px(28.0))
        .flex_shrink_0()
        .rounded(Radius::Row.px())
        .bg(bg)
        .text_color(fg)
        .flex()
        .items_center()
        .justify_center()
        .child(icon.into().size(px(14.0)))
}

/// Neutral icon badge variant for secondary/navigation chrome.
pub(super) fn settings_nav_icon_badge(icon: IconName, selected: bool, theme: Theme) -> Div {
    let (bg, fg) = if selected {
        if theme.is_dark() {
            (theme.action_soft, theme.action_primary)
        } else {
            (theme.action_primary, theme.action_on_primary)
        }
    } else {
        (theme.surface_low, theme.text_secondary)
    };
    div()
        .size(px(24.0))
        .flex_shrink_0()
        .rounded(px(7.0))
        .bg(bg)
        .text_color(fg)
        .flex()
        .items_center()
        .justify_center()
        .child(Icon::new(icon).size(px(13.0)))
}

/// Inset hairline divider between rows inside a settings group card.
pub(super) fn settings_inset_divider(theme: Theme, start_indent: f32) -> Div {
    div()
        .ml(px(start_indent))
        .mr(px(16.0))
        .h(px(1.0))
        .bg(gpui::Hsla::from(theme.outline_subtle).opacity(0.55))
}

/// One breathable row inside a grouped settings card: icon badge, title (+ optional subtitle),
/// and a right-aligned control.
pub(super) fn settings_row(
    icon: impl Into<Icon>,
    title: impl Into<SharedString>,
    subtitle: Option<SharedString>,
    trailing: AnyElement,
    theme: Theme,
) -> Div {
    let title = title.into();
    div()
        .w_full()
        .min_h(px(54.0))
        .px(px(16.0))
        .py(px(11.0))
        .flex()
        .items_center()
        .justify_between()
        .gap(px(16.0))
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .flex()
                .items_center()
                .gap(px(12.0))
                .child(settings_icon_badge(icon, theme))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .flex()
                        .flex_col()
                        .gap(px(2.0))
                        .child(
                            div()
                                .text_size(px(13.5))
                                .line_height(px(19.0))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(theme.text_primary)
                                .child(title),
                        )
                        .when_some(subtitle, |col, subtitle| {
                            col.child(
                                div()
                                    .text_size(px(12.0))
                                    .line_height(px(16.0))
                                    .text_color(theme.text_tertiary)
                                    .child(subtitle),
                            )
                        }),
                ),
        )
        .child(div().flex_shrink_0().child(trailing))
}
