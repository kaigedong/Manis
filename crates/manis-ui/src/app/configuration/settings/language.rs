use gpui::IntoElement as _;
use gpui_component::IconName;

use super::{
    Context, Div, FluentBuilder, FontWeight, InteractiveElement, Language, LanguagePreference,
    ManisApp, ParentElement, Role, SettingsGroupKey, Stateful, StatefulInteractiveElement, Styled,
    Theme, copy, div, language_preference_label, px, save_language_preference_in,
    settings_group_card, settings_row,
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

        self.collapsible_settings_section(
            SettingsGroupKey::Language,
            language.localized(copy::configuration::GENERAL),
            Some(current_language.into()),
            None,
            |cx| {
                let segmented = div()
                    .p(px(3.0))
                    .rounded(px(9.0))
                    .bg(theme.surface_low)
                    .flex()
                    .items_center()
                    .gap(px(2.0))
                    .when(compact, gpui::Styled::w_full)
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
                                compact,
                                language,
                                theme,
                                cx,
                            )
                        }),
                    )
                    .into_any_element();

                settings_group_card("configuration-language", theme)
                    .child(if compact {
                        div()
                            .w_full()
                            .px(px(16.0))
                            .py(px(12.0))
                            .flex()
                            .flex_col()
                            .gap(px(10.0))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .child(
                                        div()
                                            .text_size(px(13.5))
                                            .line_height(px(19.0))
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(theme.text_primary)
                                            .child(language.localized(
                                                copy::configuration::INTERFACE_LANGUAGE,
                                            )),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(12.0))
                                            .text_color(theme.text_tertiary)
                                            .child(current_language),
                                    ),
                            )
                            .child(segmented)
                    } else {
                        settings_row(
                            IconName::Globe,
                            language.localized(copy::configuration::INTERFACE_LANGUAGE),
                            Some(
                                format!(
                                    "{} · {current_language}",
                                    language.localized(copy::configuration::CURRENT)
                                )
                                .into(),
                            ),
                            segmented,
                            theme,
                        )
                    })
                    .into_any_element()
            },
            theme,
            cx,
        )
    }

    fn language_option(
        preference: LanguagePreference,
        selected: bool,
        compact: bool,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let label = language_preference_label(preference, language);
        let selected_bg = if theme.is_dark() {
            theme.surface_high
        } else {
            theme.surface_elevated
        };
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
            .when(compact, gpui::Styled::flex_1)
            .h(px(28.0))
            .px(px(11.0))
            .rounded(px(7.0))
            .bg(if selected {
                selected_bg
            } else {
                gpui::rgba(0x0000_0000)
            })
            .when(selected && !theme.is_dark(), gpui::Styled::shadow_xs)
            .flex()
            .items_center()
            .justify_center()
            .text_size(px(12.5))
            .line_height(px(16.0))
            .font_weight(if selected {
                FontWeight::SEMIBOLD
            } else {
                FontWeight::MEDIUM
            })
            .text_color(if selected {
                theme.text_primary
            } else {
                theme.text_secondary
            })
            .hover(move |style| {
                style
                    .bg(if selected {
                        selected_bg
                    } else {
                        theme.button_hover
                    })
                    .text_color(theme.text_primary)
            })
            .on_click(cx.listener(move |this, _, _, cx| {
                this.set_language_preference(preference, cx);
            }))
            .child(label)
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
                    .localized(
                        copy::configuration::LANGUAGE_CHANGED_FOR_THIS_SESSION_DATA_DIRECTORY_UNAVAILABLE,
                    )
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
