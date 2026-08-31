use gpui::{Context, Div, ParentElement, Styled, Window, div, prelude::*, px};
use gpui_component::{IconName, WindowExt as _, button::Button};

use super::ManisApp;
use crate::{
    app_update,
    components::{
        ActionRole, action_button, dialog_footer_surface, dialog_header_surface,
        style_action_button, surface_dialog,
    },
    localization::{Language, copy},
    theme::{ControlSize, Space, TextRole, Theme},
};

impl ManisApp {
    pub(super) fn version_information(language: Language, theme: Theme) -> Div {
        div()
            .flex()
            .flex_col()
            .gap(Space::Xs.px())
            .text_size(TextRole::Metadata.size())
            .line_height(TextRole::Metadata.line_height())
            .text_color(theme.text_secondary)
            .child(
                div()
                    .debug_selector(|| "manis-current-version".to_owned())
                    .child(copy::app_update::current_version(
                        language,
                        app_update::current_version(),
                    )),
            )
            .child(app_update::REPOSITORY_URL)
    }

    pub(crate) fn open_about_dialog(&self, window: &mut Window, cx: &mut Context<Self>) {
        // Do not replace an editor containing unsaved changes or stack About dialogs.
        if window.has_active_dialog(cx) {
            return;
        }
        let language = self.language();
        let theme = self.theme();
        window.open_dialog(cx, move |dialog, window, _| {
            surface_dialog(dialog, theme)
                .width(px(
                    (window.viewport_size().width.as_f32() - 32.0).clamp(300.0, 420.0)
                ))
                .margin_top(px(
                    ((window.viewport_size().height.as_f32() - 240.0) / 2.0).max(16.0)
                ))
                .overlay(true)
                .overlay_closable(true)
                .keyboard(true)
                .close_button(false)
                .title(
                    dialog_header_surface(theme)
                        .text_size(TextRole::SectionTitle.size())
                        .line_height(TextRole::SectionTitle.line_height())
                        .font_weight(TextRole::SectionTitle.weight())
                        .child(language.localized(copy::tray::ABOUT_MANIS)),
                )
                .child(
                    div()
                        .debug_selector(|| "manis-about-content".to_owned())
                        .p(Space::Lg.px())
                        .child(Self::version_information(language, theme)),
                )
                .footer(
                    dialog_footer_surface(theme)
                        .child(
                            style_action_button(
                                Button::new("about-github")
                                    .debug_selector(|| "about-github".to_owned())
                                    .label(language.localized(copy::app_update::OPEN_GITHUB))
                                    .icon(IconName::ExternalLink),
                                ActionRole::Secondary,
                                ControlSize::Compact,
                            )
                            .on_click(|_, _, cx| cx.open_url(app_update::REPOSITORY_URL)),
                        )
                        .child(
                            action_button(
                                "about-close",
                                language.localized(copy::app_update::CLOSE),
                                ActionRole::Secondary,
                                ControlSize::Compact,
                            )
                            .on_click(|_, window, cx| window.close_dialog(cx)),
                        ),
                )
        });
    }

    #[cfg(feature = "snapshot-fixtures")]
    #[doc(hidden)]
    pub fn show_about_fixture(&self, window: &mut Window, cx: &mut Context<Self>) {
        if self.runtime.is_fixture() {
            self.open_about_dialog(window, cx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[gpui::test]
    fn about_shows_version_opens_repository_and_closes_without_stacking(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(crate::init);
        // Dialog animations use wall-clock time, not the test executor's clock.
        cx.update(|cx| cx.set_reduce_motion(true));
        let mut app = None;
        let (_, cx) = cx.add_window_view(|window, cx| {
            let entity = cx.new(|_| ManisApp::with_fixture_controller("http://127.0.0.1:9090"));
            app = Some(entity.clone());
            crate::root(entity, window, cx)
        });
        let app = app.expect("fixture app");
        cx.update(|window, cx| {
            app.update(cx, |app, cx| {
                app.open_about_dialog(window, cx);
                app.open_about_dialog(window, cx);
            });
            window.draw(cx).clear(cx);
        });
        assert!(cx.debug_bounds("manis-current-version").is_some());
        let github = cx.debug_bounds("about-github").expect("repository action");
        cx.simulate_click(github.center(), gpui::Modifiers::none());
        assert_eq!(cx.opened_url().as_deref(), Some(app_update::REPOSITORY_URL));
        cx.simulate_keystrokes("escape");
        cx.update(|window, cx| assert!(!window.has_active_dialog(cx), "one Escape closes About"));
    }

    #[gpui::test]
    fn about_preserves_an_existing_dialog(cx: &mut gpui::TestAppContext) {
        cx.update(crate::init);
        let mut app = None;
        let (_, cx) = cx.add_window_view(|window, cx| {
            let entity = cx.new(|_| ManisApp::with_fixture_controller("http://127.0.0.1:9090"));
            app = Some(entity.clone());
            crate::root(entity, window, cx)
        });
        cx.update(|window, cx| {
            window.open_dialog(cx, |dialog, _, _| {
                dialog.child(
                    div()
                        .debug_selector(|| "unsaved-editor".to_owned())
                        .child("Unsaved changes"),
                )
            });
            app.as_ref()
                .expect("fixture app")
                .update(cx, |app, cx| app.open_about_dialog(window, cx));
            window.draw(cx).clear(cx);
        });
        assert!(cx.debug_bounds("unsaved-editor").is_some());
        assert!(cx.debug_bounds("manis-about-content").is_none());
    }
}
