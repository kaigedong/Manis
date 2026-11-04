use gpui::{InteractiveElement, IntoElement, ParentElement, Stateful, Styled, div, px};
use gpui_component::{Disableable, IconName, button::Button};

use super::{
    ActionRole, AppUpdateState, Context, ControlSize, Div, Language, ManisApp, Space, TextRole,
    Theme, app_update, copy, panel_surface, section_heading, style_action_button,
};

impl ManisApp {
    pub(in crate::app) fn app_update_panel(
        &self,
        theme: Theme,
        compact: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let language = self.language();
        let github_label = language.localized(copy::app_update::OPEN_GITHUB);
        let mut actions = div().flex().items_center().gap(Space::Sm.px());
        if app_update::supports_self_install() {
            actions = actions.child(self.app_update_action(language, cx));
        }
        actions = actions.child(
            style_action_button(
                Button::new("app-update-github")
                    .debug_selector(|| "app-update-github".to_owned())
                    .accessibility_label(github_label)
                    .label(github_label)
                    .icon(IconName::ExternalLink),
                ActionRole::Secondary,
                ControlSize::Compact,
            )
            .on_click(|_, _, cx| cx.open_url(app_update::RELEASES_URL)),
        );
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
                    .child(actions),
            )
            .child(
                div()
                    .mt(Space::Sm.px())
                    .text_size(TextRole::Metadata.size())
                    .line_height(TextRole::Metadata.line_height())
                    .text_color(theme.text_secondary)
                    .child(language.localized(copy::app_update::APP_LICENSE)),
            )
            .child(
                div()
                    .text_size(TextRole::Metadata.size())
                    .line_height(TextRole::Metadata.line_height())
                    .text_color(theme.text_secondary)
                    .child(language.localized(copy::app_update::CORE_LICENSE)),
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

    fn app_update_action(&self, language: Language, cx: &mut Context<Self>) -> gpui::AnyElement {
        match self.app_update_state.clone() {
            AppUpdateState::Available(_) => style_action_button(
                Button::new("app-update-download")
                    .debug_selector(|| "app-update-download".to_owned())
                    .accessibility_label(language.localized(copy::app_update::DOWNLOAD))
                    .label(language.localized(copy::app_update::DOWNLOAD))
                    .icon(IconName::ArrowDown),
                ActionRole::Primary,
                ControlSize::Compact,
            )
            .on_click(cx.listener(|this, _, _window, cx| {
                this.start_app_update_download(cx);
            }))
            .into_any_element(),
            AppUpdateState::Downloading(_) => style_action_button(
                Button::new("app-update-downloading")
                    .label(language.localized(copy::app_update::DOWNLOADING)),
                ActionRole::Secondary,
                ControlSize::Compact,
            )
            .disabled(true)
            .into_any_element(),
            AppUpdateState::Ready { .. } => style_action_button(
                Button::new("app-update-install")
                    .debug_selector(|| "app-update-install".to_owned())
                    .accessibility_label(language.localized(copy::app_update::INSTALL_AND_RESTART))
                    .label(language.localized(copy::app_update::INSTALL_AND_RESTART))
                    .icon(IconName::Check),
                ActionRole::Primary,
                ControlSize::Compact,
            )
            .on_click(cx.listener(|this, _, _window, cx| {
                this.install_app_update(cx);
            }))
            .into_any_element(),
            AppUpdateState::Idle
            | AppUpdateState::Checking
            | AppUpdateState::Current
            | AppUpdateState::Failed(_)
            | AppUpdateState::InstallFailed(_) => style_action_button(
                Button::new("app-update-check")
                    .debug_selector(|| "app-update-check".to_owned())
                    .accessibility_label(language.localized(copy::app_update::CHECK_NOW))
                    .label(language.localized(copy::app_update::CHECK_NOW))
                    .icon(IconName::Redo2),
                ActionRole::Secondary,
                ControlSize::Compact,
            )
            .on_click(cx.listener(|this, _, _window, cx| {
                this.check_for_app_update(cx);
            }))
            .into_any_element(),
        }
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
            AppUpdateState::Downloading(_) => {
                language.localized(copy::app_update::DOWNLOADING).to_owned()
            }
            AppUpdateState::Ready { update, .. } => {
                copy::app_update::ready_to_install(language, &update.version)
            }
            AppUpdateState::Current => language.localized(copy::app_update::UP_TO_DATE).to_owned(),
            AppUpdateState::Failed(error) => copy::app_update::error(language, *error).to_owned(),
            AppUpdateState::InstallFailed(message) => {
                copy::app_update::install_failed(language, message)
            }
        }
    }
}
