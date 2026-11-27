use gpui::{
    AnyElement, Context, Div, FontWeight, InteractiveElement, IntoElement, KeyDownEvent,
    MouseButton, ParentElement, Role, SharedString, Stateful, StatefulInteractiveElement, Styled,
    Window, div, prelude::*, px,
};
use gpui_component::{Icon, IconName};
use manis_core::WindowSizeClass;

use crate::app::{AppUpdateState, ConfigurationSection, ManisApp, MihomoCoreUpdatePhase};
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
pub(super) mod proxy_dns;
mod routing;
mod startup;
mod updates;

pub(in crate::app) use proxy_dns::ProxyDnsEditorState;

/// Expansion state for collapsible desktop settings cards (collapsed by default).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(in crate::app) struct SettingsDisclosureState {
    proxy_dns: bool,
    proxy_sources: bool,
    rule_sources: bool,
}

impl SettingsDisclosureState {
    const fn is_expanded(self, section: SettingsDisclosureSection) -> bool {
        match section {
            SettingsDisclosureSection::ProxyDns => self.proxy_dns,
            SettingsDisclosureSection::ProxySources => self.proxy_sources,
            SettingsDisclosureSection::RuleSources => self.rule_sources,
        }
    }

    const fn toggle(&mut self, section: SettingsDisclosureSection) {
        match section {
            SettingsDisclosureSection::ProxyDns => self.proxy_dns = !self.proxy_dns,
            SettingsDisclosureSection::ProxySources => self.proxy_sources = !self.proxy_sources,
            SettingsDisclosureSection::RuleSources => self.rule_sources = !self.rule_sources,
        }
    }

    const fn expand(&mut self, section: SettingsDisclosureSection) {
        match section {
            SettingsDisclosureSection::ProxyDns => self.proxy_dns = true,
            SettingsDisclosureSection::ProxySources => self.proxy_sources = true,
            SettingsDisclosureSection::RuleSources => self.rule_sources = true,
        }
    }
}

/// Settings sections that render as collapsed-by-default inset disclosure cards.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SettingsDisclosureSection {
    ProxyDns,
    ProxySources,
    RuleSources,
}

impl SettingsDisclosureSection {
    const fn section_id(self) -> &'static str {
        match self {
            Self::ProxyDns => "configuration-proxy-dns-section",
            Self::ProxySources => "configuration-proxy-sources-section",
            Self::RuleSources => "configuration-rule-sources-section",
        }
    }

    const fn card_id(self) -> &'static str {
        match self {
            Self::ProxyDns => "configuration-proxy-dns",
            Self::ProxySources => "configuration-source",
            Self::RuleSources => "configuration-rule-sources",
        }
    }

    const fn toggle_id(self) -> &'static str {
        match self {
            Self::ProxyDns => "settings-disclosure-proxy-dns",
            Self::ProxySources => "settings-disclosure-proxy-sources",
            Self::RuleSources => "settings-disclosure-rule-sources",
        }
    }

    const fn body_id(self) -> &'static str {
        match self {
            Self::ProxyDns => "configuration-proxy-dns-body",
            Self::ProxySources => "configuration-source-body",
            Self::RuleSources => "configuration-rule-sources-body",
        }
    }
}

/// Header configuration for a collapsible inset settings disclosure card.
pub(super) struct SettingsDisclosureHeader {
    pub(super) section: SettingsDisclosureSection,
    pub(super) icon: IconName,
    pub(super) title: SharedString,
    pub(super) summary: SharedString,
    pub(super) has_issue: bool,
    pub(super) expanded_action: Option<AnyElement>,
}

impl ManisApp {
    pub(super) const fn is_settings_disclosure_expanded(
        &self,
        section: SettingsDisclosureSection,
    ) -> bool {
        self.settings_disclosure.is_expanded(section)
    }

    pub(super) fn toggle_settings_disclosure(
        &mut self,
        section: SettingsDisclosureSection,
        cx: &mut Context<Self>,
    ) {
        self.settings_disclosure.toggle(section);
        cx.notify();
    }

    pub(super) const fn expand_settings_disclosure(&mut self, section: SettingsDisclosureSection) {
        self.settings_disclosure.expand(section);
    }

    pub(super) const fn expand_configuration_section_disclosure(
        &mut self,
        section: ConfigurationSection,
    ) {
        match section {
            ConfigurationSection::ProxySources => {
                self.expand_settings_disclosure(SettingsDisclosureSection::ProxySources);
            }
            ConfigurationSection::RuleSources => {
                self.expand_settings_disclosure(SettingsDisclosureSection::RuleSources);
            }
            ConfigurationSection::General
            | ConfigurationSection::Runtime
            | ConfigurationSection::Updates => {}
        }
    }

    /// Renders a collapsed-by-default inset settings card modeled after mobile grouped disclosure
    /// rows: icon badge, title, right-aligned count summary, and chevron that expands in place.
    pub(super) fn settings_disclosure_card(
        &self,
        header: SettingsDisclosureHeader,
        body: impl FnOnce(&mut Context<Self>) -> AnyElement,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let section = header.section;
        let expanded = self.is_settings_disclosure_expanded(section);
        let body_id = section.body_id();
        let header_row = Self::settings_disclosure_header_row(header, expanded, theme, cx);

        div().id(section.section_id()).w_full().child(
            settings_group_card(section.card_id(), theme)
                .child(header_row)
                .when(expanded, |card| {
                    card.child(
                        div()
                            .w_full()
                            .h(px(1.0))
                            .bg(gpui::Hsla::from(theme.outline_subtle).opacity(0.55)),
                    )
                    .child(
                        div()
                            .id(body_id)
                            .debug_selector(move || body_id.to_owned())
                            .w_full()
                            .child(body(cx)),
                    )
                }),
        )
    }

    fn settings_disclosure_header_row(
        header: SettingsDisclosureHeader,
        expanded: bool,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let section = header.section;
        let toggle_id = section.toggle_id();
        let title = header.title;

        div()
            .id(toggle_id)
            .debug_selector(move || toggle_id.to_owned())
            .role(Role::Button)
            .aria_label(title.clone())
            .aria_expanded(expanded)
            .tab_stop(true)
            .focusable()
            .map(crate::components::primary_button_interaction)
            .cursor_pointer()
            .w_full()
            .min_h(px(54.0))
            .px(px(16.0))
            .py(px(11.0))
            .hover(move |row| row.bg(theme.surface_low))
            .focus_visible(move |row| row.bg(theme.surface_low))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .child(settings_icon_badge(header.icon, theme))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .overflow_x_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(px(13.5))
                            .line_height(px(19.0))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.text_primary)
                            .child(title),
                    ),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .when(!expanded && header.has_issue, |row| {
                        row.child(div().size(px(6.0)).rounded_full().bg(theme.status_error))
                    })
                    .child(
                        div()
                            .text_size(px(12.5))
                            .line_height(px(18.0))
                            .text_color(if !expanded && header.has_issue {
                                theme.status_error
                            } else {
                                theme.text_tertiary
                            })
                            .child(header.summary),
                    )
                    .when_some(
                        header.expanded_action.filter(|_| expanded),
                        |row, action| {
                            row.child(
                                div()
                                    .flex_shrink_0()
                                    .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                        cx.stop_propagation();
                                    })
                                    .child(action),
                            )
                        },
                    )
                    .child(
                        Icon::new(if expanded {
                            IconName::ChevronDown
                        } else {
                            IconName::ChevronRight
                        })
                        .size(px(14.0))
                        .flex_shrink_0()
                        .text_color(gpui::Hsla::from(theme.text_tertiary).opacity(0.75)),
                    ),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.toggle_settings_disclosure(section, cx);
            }))
            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, _, cx| {
                if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                    this.toggle_settings_disclosure(section, cx);
                    cx.stop_propagation();
                }
            }))
    }
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
