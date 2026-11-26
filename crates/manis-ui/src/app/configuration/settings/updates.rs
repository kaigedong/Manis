use gpui::{
    Div, FontWeight, InteractiveElement, IntoElement, ParentElement, Stateful,
    StatefulInteractiveElement, Styled, div, img, prelude::FluentBuilder as _, px,
};
use gpui_component::{Disableable, Icon, IconName, Sizable, button::Button, spinner::Spinner};

use super::{
    ActionRole, AppUpdateState, Context, ControlSize, Language, ManisApp, SettingsGroupKey,
    TextRole, Theme, app_update, copy, settings_group_card, settings_icon_badge,
    settings_inset_divider, style_action_button,
};

impl ManisApp {
    #[expect(
        clippy::too_many_lines,
        reason = "Keeps the About & Updates inset card, zero-shift status slot, GitHub row, and licenses together"
    )]
    pub(in crate::app) fn app_update_panel(
        &self,
        theme: Theme,
        _compact: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let language = self.language();
        let is_checking = matches!(self.app_update_state, AppUpdateState::Checking);
        let is_downloading = matches!(self.app_update_state, AppUpdateState::Downloading { .. });

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
            .when(is_downloading, |el| {
                el.opacity(0.45)
                    .child(Icon::new(IconName::RefreshCw).size(px(13.0)))
            })
            .when(!is_checking && !is_downloading, |el| {
                el.cursor_pointer()
                    .hover(move |style| style.bg(theme.surface_low).text_color(theme.text_primary))
                    .on_click(cx.listener(|this, _, _window, cx| {
                        this.check_for_app_update(cx);
                    }))
                    .child(Icon::new(IconName::RefreshCw).size(px(13.0)))
            });

        let install_action = if app_update::supports_self_install() {
            self.app_update_install_action(language, cx)
        } else {
            None
        };

        let status_color = match &self.app_update_state {
            AppUpdateState::Available(_) | AppUpdateState::Downloading { .. } => {
                theme.action_primary
            }
            AppUpdateState::Ready { .. } | AppUpdateState::Current => theme.status_success,
            AppUpdateState::Failed(_) | AppUpdateState::InstallFailed(_) => theme.status_error,
            AppUpdateState::Idle | AppUpdateState::Checking => theme.text_tertiary,
        };

        let brand_mark = div()
            .size(px(36.0))
            .flex_shrink_0()
            .rounded(px(10.0))
            .overflow_hidden()
            .bg(theme.surface_low)
            .p(px(4.0))
            .child(img(crate::assets::BRAND_MARK_PATH).size_full());

        let version_row = div()
            .w_full()
            .min_h(px(66.0))
            .px(px(16.0))
            .py(px(12.0))
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
                    .child(brand_mark)
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .flex()
                            .flex_col()
                            .gap(px(2.0))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.0))
                                    .child(
                                        div()
                                            .debug_selector(|| "manis-current-version".to_owned())
                                            .text_size(px(14.0))
                                            .line_height(px(20.0))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(theme.text_primary)
                                            .child(copy::app_update::current_version(
                                                language,
                                                app_update::current_version(),
                                            )),
                                    )
                                    .child(check_trigger),
                            )
                            // Fixed height prevents layout shift as update states transition.
                            .child(
                                div()
                                    .h(px(18.0))
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .text_size(px(12.0))
                                    .line_height(px(18.0))
                                    .text_color(status_color)
                                    .child(self.app_update_status(language)),
                            ),
                    ),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .h(px(30.0))
                    .flex()
                    .items_center()
                    .when_some(install_action, ParentElement::child),
            );

        let github_row = div()
            .id("app-update-github")
            .debug_selector(|| "app-update-github".to_owned())
            .w_full()
            .min_h(px(52.0))
            .px(px(16.0))
            .py(px(10.0))
            .cursor_pointer()
            .hover(move |style| style.bg(theme.surface_low))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .on_click(|_, _, cx| cx.open_url(app_update::REPOSITORY_URL))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .child(settings_icon_badge(IconName::Github, theme))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .flex()
                            .flex_col()
                            .gap(px(1.0))
                            .child(
                                div()
                                    .text_size(px(13.5))
                                    .line_height(px(19.0))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(theme.text_primary)
                                    .child("GitHub"),
                            )
                            .child(
                                div()
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .text_size(TextRole::Metadata.size())
                                    .line_height(TextRole::Metadata.line_height())
                                    .text_color(theme.text_tertiary)
                                    .child(app_update::REPOSITORY_URL),
                            ),
                    ),
            )
            .child(
                Icon::new(IconName::ExternalLink)
                    .size(px(14.0))
                    .text_color(theme.text_tertiary),
            );

        let licenses_row = div()
            .w_full()
            .px(px(16.0))
            .py(px(12.0))
            .flex()
            .flex_col()
            .gap(px(3.0))
            .child(
                div()
                    .text_size(TextRole::Metadata.size())
                    .line_height(TextRole::Metadata.line_height())
                    .text_color(theme.text_tertiary)
                    .child(language.localized(copy::app_update::APP_LICENSE)),
            )
            .child(
                div()
                    .text_size(TextRole::Metadata.size())
                    .line_height(TextRole::Metadata.line_height())
                    .text_color(theme.text_tertiary)
                    .child(language.localized(copy::app_update::CORE_LICENSE)),
            );

        self.collapsible_settings_section(
            SettingsGroupKey::Updates,
            language.localized(copy::app_update::ABOUT_AND_UPDATES),
            Some(format!("v{}", app_update::current_version()).into()),
            None,
            |_cx| {
                settings_group_card("configuration-app-update", theme)
                    .debug_selector(|| "app-update-panel".to_owned())
                    .child(version_row)
                    .child(settings_inset_divider(theme, 16.0))
                    .child(github_row)
                    .child(settings_inset_divider(theme, 16.0))
                    .child(licenses_row)
                    .into_any_element()
            },
            theme,
            cx,
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
            AppUpdateState::Downloading { progress, .. } => Some(
                style_action_button(
                    Button::new("app-update-downloading")
                        .icon(IconName::ArrowUp)
                        .label(copy::app_update::downloading_progress_pill(
                            language, progress,
                        )),
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
            AppUpdateState::Downloading { progress, .. } => {
                copy::app_update::downloading_progress_pill(language, *progress)
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
