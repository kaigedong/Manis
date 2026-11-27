use gpui::{
    AnyElement, Div, FontWeight, InteractiveElement, IntoElement, ParentElement, Stateful,
    StatefulInteractiveElement, Styled, div, prelude::FluentBuilder as _, px,
};
use gpui_component::{Disableable, Icon, IconName, Sizable, button::Button, spinner::Spinner};
use manis_core::ProxyMode;

use super::{
    ActionRole, Context, ControlSize, Language, ManisApp, MihomoCoreUpdatePhase, Theme, copy,
    settings_group_card, settings_icon_badge, settings_section_header, style_action_button,
};

impl ManisApp {
    pub(in crate::app) fn kernel_panel(
        &self,
        theme: Theme,
        _compact: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let language = self.language();
        let runtime_name = self.runtime.kind().display_name();
        div()
            .id("configuration-kernel-section")
            .w_full()
            .child(settings_section_header(
                language.localized(copy::configuration::RUNTIME),
                Some(runtime_name.into()),
                None,
                theme,
            ))
            .child(
                settings_group_card("configuration-kernel", theme)
                    .debug_selector(|| "kernel-update-panel".to_owned())
                    .child(self.mihomo_core_update_row(language, theme, cx)),
            )
    }

    fn mihomo_core_update_row(
        &self,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        let is_checking = self.mihomo_core_update_state.is_checking();
        let is_busy = self.mihomo_core_update_state.is_busy();
        let check_trigger =
            self.mihomo_core_check_trigger(language, theme, is_checking, is_busy, cx);
        let action = self.mihomo_core_update_action(language, cx);
        let status_color = self.mihomo_core_status_color(theme);

        div()
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
                    .child(settings_icon_badge(IconName::Cpu, theme))
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
                                            .debug_selector(|| "mihomo-current-version".to_owned())
                                            .text_size(px(14.0))
                                            .line_height(px(20.0))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(theme.text_primary)
                                            .child(self.mihomo_core_version_heading(language)),
                                    )
                                    .child(check_trigger),
                            )
                            // Fixed height prevents layout shift as core update states transition.
                            .child(
                                div()
                                    .debug_selector(|| "mihomo-update-status".to_owned())
                                    .h(px(18.0))
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .text_size(px(12.0))
                                    .line_height(px(18.0))
                                    .text_color(status_color)
                                    .child(self.mihomo_core_update_status(language)),
                            ),
                    ),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .h(px(30.0))
                    .flex()
                    .items_center()
                    .when_some(action, ParentElement::child),
            )
    }

    fn mihomo_core_check_trigger(
        &self,
        language: Language,
        theme: Theme,
        is_checking: bool,
        is_busy: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let disabled = is_busy || self.configuration_transfer.active;
        div()
            .id("core-update-check")
            .debug_selector(|| "core-update-check".to_owned())
            .aria_label(language.localized(copy::core_update::CHECK_ACCESSIBILITY_LABEL))
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
            .when(!is_checking && disabled, |el| {
                el.opacity(0.45)
                    .child(Icon::new(IconName::RefreshCw).size(px(13.0)))
            })
            .when(!is_checking && !disabled, |el| {
                el.cursor_pointer()
                    .hover(move |style| style.bg(theme.surface_low).text_color(theme.text_primary))
                    .on_click(cx.listener(|this, _, _window, cx| {
                        this.check_for_mihomo_core_update(cx);
                    }))
                    .child(Icon::new(IconName::RefreshCw).size(px(13.0)))
            })
    }

    fn mihomo_core_version_heading(&self, language: Language) -> String {
        let version = match self.mihomo_core_update_state.current_version() {
            Some(version) => version,
            None if self.mihomo_core_update_state.is_installed() => {
                language.localized(copy::configuration::INSTALLED)
            }
            None => language.localized(copy::configuration::NOT_INSTALLED),
        };
        copy::app_update::current_version(language, version)
    }

    fn mihomo_core_status_color(&self, theme: Theme) -> gpui::Rgba {
        match &self.mihomo_core_update_state.phase {
            MihomoCoreUpdatePhase::Available(_)
            | MihomoCoreUpdatePhase::Downloading { .. }
            | MihomoCoreUpdatePhase::Installing { .. } => theme.action_primary,
            MihomoCoreUpdatePhase::ReadyToInstall { .. }
            | MihomoCoreUpdatePhase::Current { .. }
            | MihomoCoreUpdatePhase::Updated { .. } => theme.status_success,
            MihomoCoreUpdatePhase::Failed(_) => theme.status_error,
            MihomoCoreUpdatePhase::Idle | MihomoCoreUpdatePhase::Checking => theme.text_tertiary,
        }
    }

    fn mihomo_core_update_status(&self, language: Language) -> String {
        let proxy_active = self.proxy_mode != ProxyMode::Off;
        match &self.mihomo_core_update_state.phase {
            MihomoCoreUpdatePhase::Idle if self.mihomo_core_update_state.is_installed() => language
                .localized(copy::core_update::MANAGED_STABLE_CORE)
                .to_owned(),
            MihomoCoreUpdatePhase::Idle => language
                .localized(copy::core_update::NOT_INSTALLED_HINT)
                .to_owned(),
            MihomoCoreUpdatePhase::Checking => {
                language.localized(copy::core_update::CHECKING).to_owned()
            }
            MihomoCoreUpdatePhase::Current { .. } => {
                language.localized(copy::core_update::UP_TO_DATE).to_owned()
            }
            MihomoCoreUpdatePhase::Available(asset) => {
                copy::core_update::available_version(language, &asset.version)
            }
            MihomoCoreUpdatePhase::Downloading { version, progress } => {
                copy::core_update::downloading_status(language, version.as_deref(), *progress)
            }
            MihomoCoreUpdatePhase::ReadyToInstall { asset, .. } => {
                copy::core_update::ready_to_install(language, &asset.version, proxy_active)
            }
            MihomoCoreUpdatePhase::Installing { version } => {
                copy::core_update::installing_version(language, version)
            }
            MihomoCoreUpdatePhase::Updated { version } => {
                copy::core_update::updated_version(language, version)
            }
            MihomoCoreUpdatePhase::Failed(message) => message.clone(),
        }
    }

    fn mihomo_core_update_action(
        &self,
        language: Language,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let transfer_active = self.configuration_transfer.active;
        match &self.mihomo_core_update_state.phase {
            MihomoCoreUpdatePhase::Available(_) => Some(
                style_action_button(
                    Button::new("mihomo-core-update")
                        .debug_selector(|| "mihomo-core-update".to_owned())
                        .accessibility_label(language.localized(
                            copy::configuration::DOWNLOAD_OR_UPDATE_THE_MANIS_MANAGED_MIHOMO_CORE,
                        ))
                        .label(language.localized(copy::core_update::UPDATE_ACTION))
                        .icon(IconName::ArrowDown)
                        .disabled(transfer_active)
                        .tab_stop(!transfer_active),
                    ActionRole::Primary,
                    ControlSize::Compact,
                )
                .on_click(cx.listener(|this, _, _window, cx| {
                    this.update_mihomo_core(cx);
                }))
                .into_any_element(),
            ),
            MihomoCoreUpdatePhase::Downloading { progress, .. } => Some(
                style_action_button(
                    Button::new("mihomo-core-downloading")
                        .debug_selector(|| "mihomo-core-downloading".to_owned())
                        .icon(IconName::ArrowDown)
                        .label(copy::app_update::downloading_progress_pill(
                            language, *progress,
                        )),
                    ActionRole::Secondary,
                    ControlSize::Compact,
                )
                .disabled(true)
                .into_any_element(),
            ),
            MihomoCoreUpdatePhase::ReadyToInstall { .. } => {
                let enabled = !transfer_active && self.proxy_mode == ProxyMode::Off;
                Some(
                    style_action_button(
                        Button::new("mihomo-core-install")
                            .debug_selector(|| "mihomo-core-install".to_owned())
                            .accessibility_label(
                                language.localized(copy::core_update::INSTALL_ACTION),
                            )
                            .label(language.localized(copy::core_update::INSTALL_ACTION))
                            .icon(IconName::Check)
                            .disabled(!enabled)
                            .tab_stop(enabled),
                        ActionRole::Primary,
                        ControlSize::Compact,
                    )
                    .when(!enabled, gpui::Styled::cursor_default)
                    .on_click(cx.listener(|this, _, _window, cx| {
                        this.update_mihomo_core(cx);
                    }))
                    .into_any_element(),
                )
            }
            MihomoCoreUpdatePhase::Installing { .. } => Some(
                style_action_button(
                    Button::new("mihomo-core-installing")
                        .debug_selector(|| "mihomo-core-installing".to_owned())
                        .label(language.localized(copy::core_update::INSTALLING_BUTTON))
                        .loading(true),
                    ActionRole::Secondary,
                    ControlSize::Compact,
                )
                .disabled(true)
                .into_any_element(),
            ),
            MihomoCoreUpdatePhase::Idle
            | MihomoCoreUpdatePhase::Checking
            | MihomoCoreUpdatePhase::Current { .. }
            | MihomoCoreUpdatePhase::Updated { .. }
            | MihomoCoreUpdatePhase::Failed(_) => (!self.mihomo_core_update_state.is_installed())
                .then(|| {
                    let enabled = !transfer_active && !self.mihomo_core_update_state.is_checking();
                    style_action_button(
                        Button::new("mihomo-core-update")
                            .debug_selector(|| "mihomo-core-update".to_owned())
                            .accessibility_label(language.localized(
                                copy::configuration::DOWNLOAD_OR_UPDATE_THE_MANIS_MANAGED_MIHOMO_CORE,
                            ))
                            .label(language.localized(copy::core_update::DOWNLOAD_STABLE))
                            .icon(IconName::ArrowDown)
                            .disabled(!enabled)
                            .tab_stop(enabled),
                        ActionRole::Primary,
                        ControlSize::Compact,
                    )
                    .when(!enabled, gpui::Styled::cursor_default)
                    .on_click(cx.listener(|this, _, _window, cx| {
                        this.update_mihomo_core(cx);
                    }))
                    .into_any_element()
                }),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use gpui::{AppContext as _, TestAppContext};
    use manis_core::ProxyMode;

    use crate::{
        app::{ManisApp, MihomoCoreUpdatePhase, MihomoCoreUpdateState},
        app_update::DownloadProgress,
        core_update::{ArchiveKind, CoreUpdateCheck, CoreUpdateError, ReleaseAsset},
        localization::{Language, copy},
    };

    fn sample_asset(version: &str) -> ReleaseAsset {
        ReleaseAsset {
            version: version.to_owned(),
            name: format!("mihomo-darwin-arm64-{version}.gz"),
            download_url: "https://example.test/mihomo.gz".to_owned(),
            archive: ArchiveKind::Gzip,
            sha256: "a".repeat(64),
            size: Some(20 * 1024 * 1024),
        }
    }

    #[gpui::test]
    fn kernel_panel_formats_versions_progress_speed_and_transitions(cx: &mut TestAppContext) {
        let app = cx.new(|_cx| ManisApp::with_fixture_controller("http://127.0.0.1:9090"));

        app.update(cx, |app, _cx| {
            app.mihomo_core_update_state = MihomoCoreUpdateState {
                installed_version: Some("v1.19.22".to_owned()),
                phase: MihomoCoreUpdatePhase::Idle,
            };
            assert_eq!(
                app.mihomo_core_version_heading(Language::SimplifiedChinese),
                "当前版本 · v1.19.22"
            );

            // Discover a newer release
            let asset = sample_asset("v1.19.30");
            app.finish_mihomo_core_update_check(
                Ok(CoreUpdateCheck::Available(asset.clone())),
                MihomoCoreUpdatePhase::Idle,
            );
            assert_eq!(
                app.mihomo_core_update_status(Language::SimplifiedChinese),
                "发现新版本 v1.19.30，可直接更新"
            );
            assert_eq!(
                app.mihomo_core_update_state.available_version(),
                Some("v1.19.30")
            );

            // Temporary network error preserves previously discovered release
            app.finish_mihomo_core_update_check(
                Err(CoreUpdateError::Io),
                MihomoCoreUpdatePhase::Available(asset.clone()),
            );
            assert!(matches!(
                app.mihomo_core_update_state.phase,
                MihomoCoreUpdatePhase::Available(_)
            ));

            // Downloading state surfaces version, transferred size, percent, and speed
            let progress = DownloadProgress {
                downloaded_bytes: 10 * 1024 * 1024,
                total_bytes: Some(20 * 1024 * 1024),
                bytes_per_second: 2_500_000,
            };
            app.mihomo_core_update_state.phase = MihomoCoreUpdatePhase::Downloading {
                version: Some("v1.19.30".to_owned()),
                progress,
            };
            assert_eq!(
                app.mihomo_core_update_status(Language::SimplifiedChinese),
                "正在下载 v1.19.30（10.0 MB / 20.0 MB）"
            );
            let pill =
                copy::app_update::downloading_progress_pill(Language::SimplifiedChinese, progress);
            assert!(pill.contains("50%"), "expected percent in {pill}");
            assert!(pill.contains("MB/s"), "expected speed in {pill}");

            // Ready-to-install copy adapts to proxy mode
            app.mihomo_core_update_state.phase = MihomoCoreUpdatePhase::ReadyToInstall {
                asset,
                package: Arc::new(vec![1, 2, 3]),
            };
            app.proxy_mode = ProxyMode::Tun;
            assert_eq!(
                app.mihomo_core_update_status(Language::SimplifiedChinese),
                "新版本 v1.19.30 已下载，暂停代理后即可安装"
            );
            app.proxy_mode = ProxyMode::Off;
            assert_eq!(
                app.mihomo_core_update_status(Language::SimplifiedChinese),
                "新版本 v1.19.30 已下载，点击即可安装"
            );
        });
    }
}
