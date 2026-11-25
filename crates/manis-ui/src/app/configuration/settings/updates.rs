use gpui::{
    FontWeight, InteractiveElement, IntoElement, ParentElement, Stateful,
    StatefulInteractiveElement, Styled, div, prelude::FluentBuilder as _, px,
};
use gpui_component::{Disableable, Icon, IconName, Sizable, button::Button, spinner::Spinner};

use super::{
    ActionRole, AppUpdateState, Context, ControlSize, Div, Language, ManisApp, Space, TextRole,
    Theme, app_update, copy, panel_surface, section_header, style_action_button,
};

impl ManisApp {
    #[expect(
        clippy::too_many_lines,
        reason = "Keeps the About & Updates panel layout, inline refresh control, and GitHub link together"
    )]
    pub(in crate::app) fn app_update_panel(
        &self,
        theme: Theme,
        compact: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let language = self.language();
        let is_checking = matches!(self.app_update_state, AppUpdateState::Checking);

        let check_trigger = div()
            .id("app-update-check")
            .debug_selector(|| "app-update-check".to_owned())
            .size(px(24.0))
            .rounded(px(6.0))
            .flex()
            .items_center()
            .justify_center()
            .text_color(theme.text_secondary)
            .when(is_checking, |el| {
                el.child(
                    Spinner::new()
                        .icon(IconName::RefreshCw)
                        .xsmall()
                        .color(theme.action_primary.into()),
                )
            })
            .when(!is_checking, |el| {
                el.cursor_pointer()
                    .hover(move |style| style.bg(theme.surface_low).text_color(theme.text_primary))
                    .on_click(cx.listener(|this, _, _window, cx| {
                        this.check_for_app_update(cx);
                    }))
                    .child(Icon::new(IconName::RefreshCw).size(px(13.0)))
            });

        let version_row = div()
            .flex()
            .items_center()
            .gap(px(6.0))
            .child(
                div()
                    .debug_selector(|| "manis-current-version".to_owned())
                    .text_size(TextRole::Body.size())
                    .line_height(TextRole::Body.line_height())
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.text_primary)
                    .child(copy::app_update::current_version(
                        language,
                        app_update::current_version(),
                    )),
            )
            .child(check_trigger);

        let github_link = div()
            .id("app-update-github")
            .debug_selector(|| "app-update-github".to_owned())
            .flex()
            .items_center()
            .gap(px(6.0))
            .px(px(6.0))
            .py(px(2.0))
            .ml(px(-6.0))
            .rounded(px(6.0))
            .cursor_pointer()
            .text_size(TextRole::Metadata.size())
            .line_height(TextRole::Metadata.line_height())
            .text_color(theme.text_secondary)
            .hover(move |style| style.bg(theme.surface_low).text_color(theme.action_primary))
            .on_click(|_, _, cx| cx.open_url(app_update::REPOSITORY_URL))
            .child(Icon::new(IconName::Github).size(px(14.0)))
            .child(app_update::REPOSITORY_URL);

        let info_block = div()
            .flex()
            .flex_col()
            .items_start()
            .gap(Space::Xs.px())
            .child(version_row)
            .child(github_link);

        let install_action = if app_update::supports_self_install() {
            self.app_update_install_action(language, cx)
        } else {
            None
        };

        panel_surface("configuration-app-update", compact, theme)
            .debug_selector(|| "app-update-panel".to_owned())
            .child(section_header(
                language.localized(copy::app_update::ABOUT_AND_UPDATES),
                "",
                None,
                theme,
            ))
            .child(
                div()
                    .mt(Space::Md.px())
                    .min_h(px(52.0))
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .justify_between()
                    .gap(Space::Md.px())
                    .child(info_block)
                    .when_some(install_action, ParentElement::child),
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
                    .min_h(px(24.0))
                    .text_size(TextRole::Metadata.size())
                    .line_height(TextRole::Metadata.line_height())
                    .text_color(theme.text_secondary)
                    .child(self.app_update_status(language)),
            )
    }

    fn app_update_install_action(
        &self,
        language: Language,
        cx: &mut Context<Self>,
    ) -> Option<gpui::AnyElement> {
        match self.app_update_state.clone() {
            AppUpdateState::Available(_) => Some(
                style_action_button(
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
            ),
            AppUpdateState::Downloading(_) => Some(
                style_action_button(
                    Button::new("app-update-downloading")
                        .label(language.localized(copy::app_update::DOWNLOADING)),
                    ActionRole::Secondary,
                    ControlSize::Compact,
                )
                .disabled(true)
                .into_any_element(),
            ),
            AppUpdateState::Ready { .. } => Some(
                style_action_button(
                    Button::new("app-update-install")
                        .debug_selector(|| "app-update-install".to_owned())
                        .accessibility_label(
                            language.localized(copy::app_update::INSTALL_AND_RESTART),
                        )
                        .label(language.localized(copy::app_update::INSTALL_AND_RESTART))
                        .icon(IconName::Check),
                    ActionRole::Primary,
                    ControlSize::Compact,
                )
                .on_click(cx.listener(|this, _, _window, cx| {
                    this.install_app_update(cx);
                }))
                .into_any_element(),
            ),
            AppUpdateState::Idle
            | AppUpdateState::Checking
            | AppUpdateState::Current
            | AppUpdateState::Failed(_)
            | AppUpdateState::InstallFailed(_) => None,
        }
    }

    fn app_update_status(&self, language: Language) -> String {
        match &self.app_update_state {
            AppUpdateState::Idle => String::new(),
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
