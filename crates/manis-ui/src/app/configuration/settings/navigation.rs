use gpui::{
    FontWeight, InteractiveElement, IntoElement, KeyDownEvent, ParentElement, Role, Stateful,
    StatefulInteractiveElement, Styled, div, prelude::FluentBuilder, px,
};

use super::{
    ConfigurationSection, Context, CountNoun, Div, ManisApp, Radius, Space, StatusTone, TextRole,
    Theme, configuration_section_detail, configuration_section_label, copy, page_heading,
    status_badge,
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
        ConfigurationSection::Advanced => 4,
        ConfigurationSection::Updates => 5,
    }
}

impl ManisApp {
    pub(in crate::app) fn scroll_to_configuration_section(
        &mut self,
        section: ConfigurationSection,
        cx: &mut Context<Self>,
    ) {
        let index = configuration_section_index(section);
        self.configuration_section = section;
        self.configuration_scroll.scroll_to_top_of_item(index);
        self.configuration_navigation_scroll.scroll_to_item(index);
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
            cx.notify();
        }
    }

    pub(in crate::app) fn configuration_navigation(
        &self,
        theme: Theme,
        compact: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let language = self.language();
        let navigation = div()
            .flex_shrink_0()
            .bg(theme.surface_chrome)
            .when(compact, |navigation| {
                navigation
                    .w_full()
                    .px(Space::Md.px())
                    .py(Space::Sm.px())
                    .border_b_1()
            })
            .when(!compact, |navigation| {
                navigation
                    .w(px(228.0))
                    .h_full()
                    .p(Space::Md.px())
                    .border_r_1()
                    .flex()
                    .flex_col()
            })
            .border_color(theme.outline_subtle)
            .when(!compact, |navigation| {
                navigation.child(
                    div()
                        .px(Space::Sm.px())
                        .pb(Space::Sm.px())
                        .text_size(TextRole::Metadata.size())
                        .line_height(TextRole::Metadata.line_height())
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme.text_tertiary)
                        .child(language.localized(copy::configuration::SETTINGS)),
                )
            });
        let items =
            div()
                .id("configuration-navigation-items")
                .flex()
                .gap(if compact {
                    Space::Xs.px()
                } else {
                    Space::Sm.px()
                })
                .when(compact, |items| {
                    items
                        .overflow_x_scroll()
                        .track_scroll(&self.configuration_navigation_scroll)
                })
                .when(!compact, gpui::Styled::flex_col)
                .children(ConfigurationSection::ALL.into_iter().map(|section| {
                    self.configuration_navigation_item(section, theme, compact, cx)
                }));
        navigation.child(items).when(!compact, |navigation| {
            navigation.child(
                div()
                    .mt_auto()
                    .px(Space::Sm.px())
                    .pt(Space::Lg.px())
                    .text_size(TextRole::Metadata.size())
                    .line_height(TextRole::Metadata.line_height())
                    .text_color(theme.text_tertiary)
                    .child(language.localized(copy::configuration::CHANGES_ARE_STORED_LOCALLY)),
            )
        })
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
        let metadata = match section {
            ConfigurationSection::General => self.language().display_name().to_owned(),
            ConfigurationSection::Runtime => self.runtime.kind().display_name().to_owned(),
            ConfigurationSection::ProxySources => language.count(
                CountNoun::Source,
                self.imported_subscriptions.len() + self.saved_single_nodes.len(),
            ),
            ConfigurationSection::RuleSources => {
                language.count(CountNoun::Source, self.rule_sources.sources.len())
            }
            ConfigurationSection::Advanced => language
                .localized(copy::configuration::MANAGED_SECTION_SUMMARY)
                .to_owned(),
            ConfigurationSection::Updates => String::new(),
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
            .min_w(if compact { px(104.0) } else { px(0.0) })
            .px(Space::Md.px())
            .py(Space::Sm.px())
            .rounded(Radius::Control.px())
            .border_1()
            .border_color(gpui::rgba(0x0000_0000))
            .bg(if selected {
                theme.action_soft
            } else {
                gpui::rgba(0x0000_0000)
            })
            .hover(move |row| {
                row.bg(if selected {
                    theme.action_soft
                } else {
                    theme.surface_high
                })
            })
            .focus_visible(move |row| row.border_color(theme.focus_ring))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(Space::Sm.px())
                    .child(
                        div()
                            .text_size(TextRole::Label.size())
                            .line_height(TextRole::Label.line_height())
                            .font_weight(if selected {
                                FontWeight::SEMIBOLD
                            } else {
                                FontWeight::NORMAL
                            })
                            .text_color(if selected {
                                theme.action_primary
                            } else {
                                theme.text_primary
                            })
                            .child(configuration_section_label(section, language)),
                    )
                    .when(!compact, |row| {
                        row.child(
                            div()
                                .text_size(TextRole::Metadata.size())
                                .line_height(TextRole::Metadata.line_height())
                                .text_color(theme.text_tertiary)
                                .child(metadata),
                        )
                    }),
            )
            .when(!compact, |item| {
                item.child(
                    div()
                        .mt(px(2.0))
                        .text_size(TextRole::Metadata.size())
                        .line_height(TextRole::Metadata.line_height())
                        .text_color(theme.text_secondary)
                        .child(configuration_section_detail(section, language)),
                )
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

    pub(in crate::app) fn workspace_header(
        title: &'static str,
        detail: &'static str,
        badge: &'static str,
        badge_tone: StatusTone,
        theme: Theme,
        compact: bool,
    ) -> Div {
        div()
            .px(if compact {
                Space::Lg.px()
            } else {
                Space::Xl.px()
            })
            .py(Space::Md.px())
            .flex_shrink_0()
            .border_b_1()
            .border_color(theme.outline_subtle)
            .flex()
            .items_start()
            .justify_between()
            .gap(Space::Lg.px())
            .child(page_heading(
                title,
                if compact { "" } else { detail },
                Some(status_badge(badge, badge_tone, theme).into_any_element()),
                theme,
            ))
    }
}
