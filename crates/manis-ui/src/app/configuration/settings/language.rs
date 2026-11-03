use super::{
    Context, Div, FluentBuilder, FontWeight, InteractiveElement, IntoElement, Language,
    LanguagePreference, ManisApp, ParentElement, Radius, Role, Space, Stateful,
    StatefulInteractiveElement, StatusTone, Styled, TextRole, Theme, copy, div,
    language_preference_label, panel_surface, px, save_language_preference_in, section_heading,
    status_badge,
};

impl ManisApp {
    pub(in crate::app) fn language_panel(
        &self,
        theme: Theme,
        compact: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let language = self.language();
        let current_preference = self.language_preference();
        let current_language = language.display_name();
        panel_surface("configuration-language", compact, theme)
            .child(section_heading(
                language.localized(copy::configuration::INTERFACE_LANGUAGE),
                "",
                Some(
                    status_badge(
                        format!(
                            "{} · {current_language}",
                            language.localized(copy::configuration::CURRENT)
                        ),
                        StatusTone::Neutral,
                        theme,
                    )
                    .into_any_element(),
                ),
                theme,
            ))
            .child(
                div()
                    .mt(Space::Md.px())
                    .grid()
                    .gap(Space::Sm.px())
                    .grid_cols(if compact { 1 } else { 3 })
                    .children(
                        [
                            LanguagePreference::FollowSystem,
                            LanguagePreference::English,
                            LanguagePreference::SimplifiedChinese,
                        ]
                        .into_iter()
                        .map(|preference| {
                            Self::language_option(
                                preference,
                                preference == current_preference,
                                language,
                                theme,
                                cx,
                            )
                        }),
                    ),
            )
    }

    fn language_option(
        preference: LanguagePreference,
        selected: bool,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let label = language_preference_label(preference, language);
        div()
            .id(format!(
                "language-option-{}",
                preference.persistence_key().replace('-', "_")
            ))
            .role(Role::Button)
            .aria_label(format!(
                "{}: {label}",
                language.localized(copy::configuration::SELECT_LANGUAGE)
            ))
            .aria_toggled(if selected {
                gpui::Toggled::True
            } else {
                gpui::Toggled::False
            })
            .tab_stop(true)
            .focusable()
            .map(crate::components::primary_button_interaction)
            .cursor_pointer()
            .min_h(px(52.0))
            .px(Space::Md.px())
            .py(Space::Sm.px())
            .rounded(Radius::Row.px())
            .border_1()
            .border_color(if selected {
                theme.action_primary
            } else {
                theme.outline_subtle
            })
            .bg(if selected {
                theme.action_soft
            } else {
                theme.surface_low
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        div()
                            .text_size(TextRole::Label.size())
                            .line_height(TextRole::Label.line_height())
                            .font_weight(TextRole::Label.weight())
                            .text_color(if selected {
                                theme.action_primary
                            } else {
                                theme.text_primary
                            })
                            .child(label),
                    )
                    .when(selected, |row| {
                        row.child(
                            div()
                                .text_size(TextRole::Metadata.size())
                                .line_height(TextRole::Metadata.line_height())
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(theme.action_primary)
                                .child(language.localized(copy::configuration::SELECTED)),
                        )
                    }),
            )
            .hover(move |style| {
                style.bg(if selected {
                    theme.action_soft
                } else {
                    theme.button_hover
                })
            })
            .active(move |style| {
                style.bg(if selected {
                    theme.action_soft
                } else {
                    theme.button_active
                })
            })
            .on_click(cx.listener(move |this, _, _, cx| {
                this.set_language_preference(preference, cx);
            }))
    }

    pub(in crate::app) fn set_language_preference(
        &mut self,
        preference: LanguagePreference,
        cx: &mut Context<Self>,
    ) {
        if self.configuration_transfer.active {
            return;
        }
        self.localizer.set_preference(preference);
        let language = self.language();
        match self.subscription_store_dir.as_ref() {
            Some(store_dir) => match save_language_preference_in(store_dir, preference) {
                Ok(_path) => {
                    self.status = format!(
                        "{} · {}",
                        language.localized(copy::configuration::LANGUAGE_SAVED),
                        language_preference_label(preference, language)
                    );
                }
                Err(error) => {
                    self.status = format!(
                        "{}: {}",
                        language.localized(
                            copy::configuration::LANGUAGE_CHANGED_BUT_COULD_NOT_BE_SAVED
                        ),
                        copy::configuration::language_preference_error(language, &error)
                    );
                }
            },
            None => {
                language
                    .localized(copy::configuration::LANGUAGE_CHANGED_FOR_THIS_SESSION_DATA_DIRECTORY_UNAVAILABLE)
                    .clone_into(&mut self.status);
            }
        }
        if let Some(input) = self.proxy_source_editor.input.as_ref() {
            input.update(cx, |input, cx| input.set_language(language, cx));
        }
        if let Some(input) = self.proxy_source_editor.name_input.as_ref() {
            input.update(cx, |input, cx| {
                input.set_placeholder(
                    language.localized(copy::common::FOR_EXAMPLE_MY_SUBSCRIPTION),
                    cx,
                );
            });
        }
        if let Some(input) = self.inputs.policy_group_name.as_ref() {
            input.update(cx, |input, cx| {
                input.set_placeholder(
                    language.localized(copy::common::FOR_EXAMPLE_HONG_KONG_AUTO),
                    cx,
                );
            });
        }
        if let Some(input) = self.inputs.policy_group_filter.as_ref() {
            input.update(cx, |input, cx| {
                input.set_placeholder(language.localized(copy::common::FOR_EXAMPLE_HONG_KONG), cx);
            });
        }
        cx.notify();
    }
}
