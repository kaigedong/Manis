use super::{
    Div, IntoElement, ManisApp, ParentElement, Space, Stateful, StatusTone, Styled, TextRole,
    Theme, copy, div, panel_surface, proxy_mode_label, px, routing_mode_label, section_heading,
    status_badge,
};
impl ManisApp {
    pub(in crate::app) fn advanced_configuration_panel(
        &self,
        theme: Theme,
        compact: bool,
    ) -> Stateful<Div> {
        let language = self.language();
        let profile_source = self.runtime.profile_source();
        let profile_detail = copy::configuration::profile_source_detail(language, profile_source);
        panel_surface("configuration-advanced", compact, theme)
            .child(section_heading(
                language.localized(copy::configuration::ADVANCED_SETTINGS),
                language.localized(copy::configuration::CURRENT_MANAGED_NETWORK_BEHAVIOR),
                Some(
                    status_badge(
                        language.localized(copy::configuration::MANAGED),
                        StatusTone::Neutral,
                        theme,
                    )
                    .into_any_element(),
                ),
                theme,
            ))
            .child(Self::advanced_configuration_row(
                language.localized(copy::configuration::PROXY_MODE),
                proxy_mode_label(language, self.proxy_mode),
                language.localized(copy::configuration::CHANGED_FROM_THE_MAIN_TOOLBAR),
                theme,
            ))
            .child(Self::advanced_configuration_row(
                language.localized(copy::configuration::ROUTING_MODE),
                routing_mode_label(language, self.routing_mode),
                language.localized(copy::configuration::DIRECT_GLOBAL_OR_ORDERED_RULES),
                theme,
            ))
            .child(Self::advanced_configuration_row(
                language.localized(copy::configuration::PROCESS_IDENTIFICATION),
                language.localized(copy::configuration::ALWAYS),
                language.localized(copy::configuration::USED_TO_IMPROVE_NETWORK_ACTIVITY),
                theme,
            ))
            .child(Self::advanced_configuration_row(
                language.localized(copy::configuration::DNS_AND_TUN),
                language.localized(copy::configuration::AUTOMATIC),
                profile_detail,
                theme,
            ))
    }

    fn advanced_configuration_row(
        label: &'static str,
        value: &'static str,
        detail: &'static str,
        theme: Theme,
    ) -> Div {
        div()
            .mt(Space::Md.px())
            .pt(Space::Md.px())
            .border_t_1()
            .border_color(theme.outline_subtle)
            .flex()
            .items_center()
            .justify_between()
            .gap(Space::Lg.px())
            .child(
                div()
                    .min_w(px(0.0))
                    .child(
                        div()
                            .text_size(TextRole::Label.size())
                            .line_height(TextRole::Label.line_height())
                            .font_weight(TextRole::Label.weight())
                            .text_color(theme.text_primary)
                            .child(label),
                    )
                    .child(
                        div()
                            .mt(Space::Xs.px())
                            .text_size(TextRole::Metadata.size())
                            .line_height(TextRole::Metadata.line_height())
                            .text_color(theme.text_secondary)
                            .child(detail),
                    ),
            )
            .child(status_badge(value, StatusTone::Neutral, theme))
    }
}
