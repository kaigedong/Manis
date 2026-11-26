use gpui::{
    AnyElement, Context, Div, FontWeight, InteractiveElement, IntoElement, KeyDownEvent,
    ParentElement, Role, SharedString, Stateful, StatefulInteractiveElement, Styled, Window, div,
    prelude::*, px,
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

/// Collapsible groups within the desktop settings workspace.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SettingsGroupKey {
    Language,
    Startup,
    Transfer,
    Runtime,
    ProxySources,
    RuleSources,
    Updates,
}

impl SettingsGroupKey {
    const fn storage_key(self) -> &'static str {
        match self {
            Self::Language => "settings-group:language",
            Self::Startup => "settings-group:startup",
            Self::Transfer => "settings-group:transfer",
            Self::Runtime => "settings-group:runtime",
            Self::ProxySources => "settings-group:proxy-sources",
            Self::RuleSources => "settings-group:rule-sources",
            Self::Updates => "settings-group:updates",
        }
    }

    const fn section_id(self) -> &'static str {
        match self {
            Self::Language => "configuration-language-section",
            Self::Startup => "configuration-startup-section",
            Self::Transfer => "configuration-transfer-section",
            Self::Runtime => "configuration-kernel-section",
            Self::ProxySources => "configuration-proxy-sources-section",
            Self::RuleSources => "configuration-rule-sources-section",
            Self::Updates => "configuration-app-update-section",
        }
    }

    const fn toggle_id(self) -> &'static str {
        match self {
            Self::Language => "settings-group-toggle-language",
            Self::Startup => "settings-group-toggle-startup",
            Self::Transfer => "settings-group-toggle-transfer",
            Self::Runtime => "settings-group-toggle-runtime",
            Self::ProxySources => "settings-group-toggle-proxy-sources",
            Self::RuleSources => "settings-group-toggle-rule-sources",
            Self::Updates => "settings-group-toggle-updates",
        }
    }
}

impl ManisApp {
    pub(super) fn is_settings_group_expanded(&self, group: SettingsGroupKey) -> bool {
        !self.node_workspace.is_group_collapsed(group.storage_key())
    }

    pub(super) fn toggle_settings_group(
        &mut self,
        group: SettingsGroupKey,
        cx: &mut Context<Self>,
    ) {
        self.node_workspace.toggle_group(group.storage_key());
        cx.notify();
    }

    pub(super) fn expand_settings_group(&mut self, group: SettingsGroupKey) {
        if self.node_workspace.is_group_collapsed(group.storage_key()) {
            self.node_workspace.toggle_group(group.storage_key());
        }
    }

    pub(super) fn expand_configuration_section_groups(&mut self, section: ConfigurationSection) {
        match section {
            ConfigurationSection::General => {
                self.expand_settings_group(SettingsGroupKey::Language);
                self.expand_settings_group(SettingsGroupKey::Startup);
                self.expand_settings_group(SettingsGroupKey::Transfer);
            }
            ConfigurationSection::Runtime => self.expand_settings_group(SettingsGroupKey::Runtime),
            ConfigurationSection::ProxySources => {
                self.expand_settings_group(SettingsGroupKey::ProxySources);
            }
            ConfigurationSection::RuleSources => {
                self.expand_settings_group(SettingsGroupKey::RuleSources);
            }
            ConfigurationSection::Updates => self.expand_settings_group(SettingsGroupKey::Updates),
        }
    }

    /// Renders a settings section with a clickable disclosure header above its inset group card.
    #[expect(
        clippy::too_many_arguments,
        reason = "Keeps the shared collapsible section builder explicit at each settings call site"
    )]
    pub(super) fn collapsible_settings_section(
        &self,
        group: SettingsGroupKey,
        title: impl Into<SharedString>,
        detail: Option<SharedString>,
        action: Option<AnyElement>,
        body: impl FnOnce(&mut Context<Self>) -> AnyElement,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let title = title.into();
        let expanded = self.is_settings_group_expanded(group);
        let toggle_id = group.toggle_id();

        let trigger = div()
            .id(toggle_id)
            .debug_selector(move || toggle_id.to_owned())
            .role(Role::Button)
            .aria_label(title.clone())
            .aria_expanded(expanded)
            .tab_stop(true)
            .focusable()
            .map(crate::components::primary_button_interaction)
            .cursor_pointer()
            .ml(px(-6.0))
            .px(px(6.0))
            .py(px(3.0))
            .rounded(px(7.0))
            .hover(move |row| row.bg(theme.surface_low))
            .flex()
            .items_center()
            .gap(px(6.0))
            .child(
                Icon::new(if expanded {
                    IconName::ChevronDown
                } else {
                    IconName::ChevronRight
                })
                .size(px(13.0))
                .flex_shrink_0()
                .text_color(theme.text_tertiary),
            )
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
                        .ml(px(2.0))
                        .font_family(crate::theme::MONO_FAMILY)
                        .text_size(px(11.5))
                        .line_height(px(16.0))
                        .text_color(theme.text_tertiary)
                        .child(detail),
                )
            })
            .on_click(cx.listener(move |this, _, _, cx| {
                this.toggle_settings_group(group, cx);
            }))
            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, _, cx| {
                if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                    this.toggle_settings_group(group, cx);
                    cx.stop_propagation();
                }
            }));

        let header = div()
            .w_full()
            .px(px(4.0))
            .mb(if expanded { px(8.0) } else { px(0.0) })
            .pb(if expanded { px(0.0) } else { px(10.0) })
            .when(!expanded, |row| {
                row.border_b_1()
                    .border_color(gpui::Hsla::from(theme.outline_subtle).opacity(0.45))
            })
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
                    .child(trigger),
            )
            .when_some(action, |row, action| {
                row.child(div().flex_shrink_0().child(action))
            });

        div()
            .id(group.section_id())
            .w_full()
            .child(header)
            .when(expanded, |section| section.child(body(cx)))
    }
}

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
        .debug_selector(move || id.to_owned())
        .w_full()
        .rounded(px(14.0))
        .overflow_hidden()
        .border_1()
        .border_color(border)
        .bg(theme.surface_elevated)
        .when(!theme.is_dark(), gpui::Styled::shadow_xs)
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
