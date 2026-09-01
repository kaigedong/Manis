use gpui::{InteractiveElement, ParentElement, Stateful, Styled, div, px};
use gpui_component::{IconName, button::Button};

use super::{
    ActionRole, AppUpdateState, ControlSize, Div, Language, ManisApp, Space, TextRole, Theme,
    app_update, copy, panel_surface, section_heading, style_action_button,
};

impl ManisApp {
    pub(in crate::app) fn app_update_panel(&self, theme: Theme, compact: bool) -> Stateful<Div> {
        let language = self.language();
        let label = language.localized(copy::app_update::OPEN_GITHUB);
        panel_surface("configuration-app-update", compact, theme)
            .debug_selector(|| "app-update-panel".to_owned())
            .child(section_heading(
                language.localized(copy::app_update::APP_UPDATES),
                language.localized(copy::app_update::CHECK_AUTOMATICALLY_DETAIL),
                None,
                theme,
            ))
            .child(
                div()
                    .mt(Space::Md.px())
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .justify_between()
                    .gap(Space::Md.px())
                    .child(Self::version_information(language, theme))
                    .child(
                        style_action_button(
                            Button::new("app-update-github")
                                .debug_selector(|| "app-update-github".to_owned())
                                .accessibility_label(label)
                                .label(label)
                                .icon(IconName::ExternalLink),
                            ActionRole::Secondary,
                            ControlSize::Compact,
                        )
                        .on_click(|_, _, cx| cx.open_url(app_update::RELEASES_URL)),
                    ),
            )
            .child(
                div()
                    .mt(Space::Sm.px())
                    .min_h(px(40.0))
                    .text_size(TextRole::Metadata.size())
                    .line_height(TextRole::Metadata.line_height())
                    .text_color(theme.text_secondary)
                    .child(self.app_update_status(language)),
            )
    }

    fn app_update_status(&self, language: Language) -> String {
        match &self.app_update_state {
            AppUpdateState::Idle => language
                .localized(copy::app_update::CHECK_PENDING)
                .to_owned(),
            AppUpdateState::Checking => language.localized(copy::app_update::CHECKING).to_owned(),
            AppUpdateState::Available(update) => {
                copy::app_update::available_version(language, &update.version)
            }
            AppUpdateState::Current => language.localized(copy::app_update::UP_TO_DATE).to_owned(),
            AppUpdateState::Failed(error) => copy::app_update::error(language, *error).to_owned(),
        }
    }
}
