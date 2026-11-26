use gpui::{
    FontWeight, InteractiveElement, KeyDownEvent, ParentElement, Role, Stateful,
    StatefulInteractiveElement, Styled, div, prelude::FluentBuilder, px,
};
use gpui_component::IconName;

use super::{
    ConfigurationSection, Context, CountNoun, Div, ManisApp, Radius, Space, TextRole, Theme,
    configuration_section_label, settings_nav_icon_badge,
};

pub(in crate::app) fn configuration_section_at_scroll(
    section_tops: &[gpui::Pixels],
    scroll_top: gpui::Pixels,
    at_bottom: bool,
) -> ConfigurationSection {
    // The last section may be shorter than the viewport and never reach its top edge.
    let index = if at_bottom {
        section_tops.len().saturating_sub(1)
    } else {
        section_tops
            .iter()
            .rposition(|top| *top <= scroll_top + px(1.0))
            .unwrap_or(0)
    };
    ConfigurationSection::ALL
        .get(index)
        .copied()
        .unwrap_or_default()
}

const fn configuration_section_index(section: ConfigurationSection) -> usize {
    match section {
        ConfigurationSection::General => 0,
        ConfigurationSection::Runtime => 1,
        ConfigurationSection::ProxySources => 2,
        ConfigurationSection::RuleSources => 3,
        ConfigurationSection::Updates => 4,
    }
}

const fn configuration_section_icon(section: ConfigurationSection) -> IconName {
    match section {
        ConfigurationSection::General => IconName::Settings2,
        ConfigurationSection::Runtime => IconName::Cpu,
        ConfigurationSection::ProxySources => IconName::Globe,
        ConfigurationSection::RuleSources => IconName::Map,
        ConfigurationSection::Updates => IconName::Info,
    }
}

impl ManisApp {
    pub(in crate::app) fn scroll_to_configuration_section(
        &mut self,
        section: ConfigurationSection,
        cx: &mut Context<Self>,
    ) {
        let index = configuration_section_index(section);
        self.expand_configuration_section_groups(section);
        self.configuration_section = section;
        self.configuration_scroll.scroll_to_top_of_item(index);
        self.configuration_navigation_scroll.scroll_to_item(index);
        if section == ConfigurationSection::Updates {
            self.maybe_auto_check_app_update(cx);
        }
        cx.notify();
    }

    pub(in crate::app) fn sync_configuration_directory(&mut self, cx: &mut Context<Self>) {
        let scroll = &self.configuration_scroll;
        let section_tops: Vec<_> = (0..ConfigurationSection::ALL.len())
            .filter_map(|index| scroll.bounds_for_item(index).map(|bounds| bounds.top()))
            .collect();
        if section_tops.is_empty() {
            return;
        }
        let at_bottom = scroll.max_offset().y > px(0.0)
            && -scroll.offset().y >= scroll.max_offset().y - px(1.0);
        let section = configuration_section_at_scroll(
            &section_tops,
            scroll.bounds().top() - scroll.offset().y,
            at_bottom,
        );
        if self.configuration_section != section {
            self.configuration_section = section;
            self.configuration_navigation_scroll
                .scroll_to_item(configuration_section_index(section));
            if section == ConfigurationSection::Updates {
                self.maybe_auto_check_app_update(cx);
            }
            cx.notify();
        }
    }

    /// The five-section settings rail: flush modern sidebar beside the document, or a recessed
    /// pill bar on compact viewports.
    pub(in crate::app) fn configuration_navigation(
        &self,
        theme: Theme,
        compact: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let items =
            div()
                .id("configuration-navigation-items")
                .flex()
                .gap(px(4.0))
                .when(compact, |items| {
                    items.grid().grid_cols(3).gap(Space::Xs.px())
                })
                .when(!compact, gpui::Styled::flex_col)
                .children(ConfigurationSection::ALL.into_iter().map(|section| {
                    self.configuration_navigation_item(section, theme, compact, cx)
                }));
        if compact {
            return div()
                .id("configuration-navigation")
                .w_full()
                .flex_shrink_0()
                .px(px(12.0))
                .pt(px(12.0))
                .child(
                    div()
                        .w_full()
                        .p(px(4.0))
                        .rounded(px(12.0))
                        .bg(theme.surface_low)
                        .child(items),
                );
        }
        div()
            .id("configuration-navigation-host")
            .w(px(224.0))
            .h_full()
            .flex_shrink_0()
            .border_r_1()
            .border_color(gpui::Hsla::from(theme.outline_subtle).opacity(0.55))
            .bg(theme.surface_base)
            .px(px(12.0))
            .pt(px(22.0))
            .pb(px(16.0))
            .flex()
            .flex_col()
            .child(
                div()
                    .id("configuration-navigation")
                    .w_full()
                    .flex()
                    .flex_col()
                    .child(items),
            )
    }

    /// Quiet right-aligned metadata figure for each settings category.
    fn configuration_navigation_badge(
        &self,
        section: ConfigurationSection,
        language: super::Language,
    ) -> Option<String> {
        match section {
            ConfigurationSection::General => Some(self.language().display_name().to_owned()),
            ConfigurationSection::Runtime => Some(self.runtime.kind().display_name().to_owned()),
            ConfigurationSection::ProxySources => Some(language.count(
                CountNoun::Source,
                self.imported_subscriptions.len() + self.saved_single_nodes.len(),
            )),
            ConfigurationSection::RuleSources => {
                Some(language.count(CountNoun::Source, self.rule_sources.sources.len()))
            }
            ConfigurationSection::Updates => match &self.app_update_state {
                crate::app::AppUpdateState::Available(update)
                | crate::app::AppUpdateState::Ready { update, .. } => Some(update.version.clone()),
                _ => None,
            },
        }
    }

    fn configuration_navigation_meta(value: String, selected: bool, theme: Theme) -> Div {
        div()
            .ml_auto()
            .flex_shrink_0()
            .font_family(crate::theme::MONO_FAMILY)
            .text_size(px(11.0))
            .line_height(px(16.0))
            .text_color(if selected {
                theme.action_primary
            } else {
                theme.text_tertiary
            })
            .child(value)
    }

    fn configuration_navigation_item(
        &self,
        section: ConfigurationSection,
        theme: Theme,
        compact: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let language = self.language();
        let selected = self.configuration_section == section;
        let metadata = self.configuration_navigation_badge(section, language);
        let compact_bg = if selected {
            theme.surface_elevated
        } else {
            gpui::rgba(0x0000_0000)
        };
        div()
            .id(format!("configuration-nav-{}", section.key()))
            .role(Role::Button)
            .aria_label(configuration_section_label(section, language))
            .aria_toggled(if selected {
                gpui::Toggled::True
            } else {
                gpui::Toggled::False
            })
            .tab_stop(true)
            .focusable()
            .map(crate::components::primary_button_interaction)
            .cursor_pointer()
            .min_w(px(0.0))
            .h(if compact { px(34.0) } else { px(38.0) })
            .px(if compact { Space::Sm.px() } else { px(10.0) })
            .flex()
            .items_center()
            .gap(px(10.0))
            .when(compact, gpui::Styled::justify_center)
            .rounded(Radius::Control.px())
            .border_1()
            .border_color(gpui::rgba(0x0000_0000))
            .bg(if compact {
                compact_bg
            } else if selected {
                theme.action_soft
            } else {
                gpui::rgba(0x0000_0000)
            })
            .when(
                compact && selected && !theme.is_dark(),
                gpui::Styled::shadow_xs,
            )
            .hover(move |row| {
                row.bg(if selected {
                    if compact {
                        compact_bg
                    } else {
                        theme.action_soft
                    }
                } else {
                    theme.surface_high
                })
            })
            .focus_visible(move |row| row.border_color(theme.focus_ring))
            .when(!compact, |row| {
                row.child(settings_nav_icon_badge(
                    configuration_section_icon(section),
                    selected,
                    theme,
                ))
            })
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .overflow_hidden()
                    .text_ellipsis()
                    .text_size(TextRole::Label.size())
                    .line_height(TextRole::Label.line_height())
                    .font_weight(if selected {
                        FontWeight::SEMIBOLD
                    } else {
                        FontWeight::MEDIUM
                    })
                    .text_color(if selected {
                        if compact {
                            theme.text_primary
                        } else {
                            theme.action_primary
                        }
                    } else {
                        theme.text_secondary
                    })
                    .child(configuration_section_label(section, language)),
            )
            .when_some(metadata.filter(|_| !compact), |row, metadata| {
                row.child(Self::configuration_navigation_meta(
                    metadata, selected, theme,
                ))
            })
            .on_click(cx.listener(move |this, _, _, cx| {
                this.scroll_to_configuration_section(section, cx);
            }))
            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, _, cx| {
                if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                    this.scroll_to_configuration_section(section, cx);
                    cx.stop_propagation();
                }
            }))
    }
}
