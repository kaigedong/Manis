use gpui::{
    Context, Div, InteractiveElement, IntoElement, KeyDownEvent, ParentElement, Role, Stateful,
    StatefulInteractiveElement, Styled, Toggled, div, prelude::*, px,
};

use super::{
    ManisApp, Space, StatusTone, TextRole, Theme, copy, panel_surface, section_heading,
    status_badge,
};
use crate::autostart;
use crate::diagnostics::{LogLevel, record_event};

impl ManisApp {
    pub(in crate::app) fn startup_panel(
        &self,
        theme: Theme,
        compact: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let language = self.language();
        let enabled = self.launch_at_login;
        let available = autostart::supported() && self.subscription_store_dir.is_some();
        panel_surface("configuration-startup", compact, theme)
            .child(section_heading(
                language.localized(copy::configuration::STARTUP),
                language.localized(copy::configuration::START_MANIS_WITH_YOUR_DESKTOP_SESSION),
                Some(
                    status_badge(
                        language.localized(if !available {
                            copy::configuration::UNAVAILABLE
                        } else if enabled {
                            copy::configuration::ENABLED
                        } else {
                            copy::configuration::DISABLED
                        }),
                        if available && enabled {
                            StatusTone::Success
                        } else {
                            StatusTone::Neutral
                        },
                        theme,
                    )
                    .into_any_element(),
                ),
                theme,
            ))
            .child(
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
                            .flex_1()
                            .min_w(px(0.0))
                            .child(
                                div()
                                    .text_size(TextRole::Label.size())
                                    .line_height(TextRole::Label.line_height())
                                    .font_weight(TextRole::Label.weight())
                                    .text_color(theme.text_primary)
                                    .child(language.localized(copy::configuration::LAUNCH_AT_LOGIN)),
                            )
                            .child(
                                div()
                                    .mt(Space::Xs.px())
                                    .text_size(TextRole::Metadata.size())
                                    .line_height(TextRole::Metadata.line_height())
                                    .text_color(theme.text_secondary)
                                    .child(language.localized(
                                        copy::configuration::OPENING_MANIS_AGAIN_BRINGS_THIS_WINDOW_FORWARD,
                                    )),
                            ),
                    )
                    .child(Self::startup_switch(
                        "launch-at-login-toggle",
                        enabled,
                        available,
                        language,
                        theme,
                        cx,
                    )),
            )
    }

    fn startup_switch(
        id: &'static str,
        enabled: bool,
        available: bool,
        language: crate::localization::Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        div()
            .id(id)
            .role(Role::Switch)
            .aria_label(language.localized(copy::configuration::LAUNCH_AT_LOGIN))
            .aria_toggled(if enabled {
                Toggled::True
            } else {
                Toggled::False
            })
            .tab_stop(available)
            .focusable()
            .map(crate::components::primary_button_interaction)
            .cursor_pointer()
            .w(px(44.0))
            .h(px(24.0))
            .flex_shrink_0()
            .p(px(2.0))
            .rounded_full()
            .border_1()
            .border_color(if enabled {
                theme.action_primary
            } else {
                theme.outline_strong
            })
            .bg(if enabled {
                theme.action_primary
            } else {
                theme.surface_low
            })
            .focus_visible(move |switch| switch.border_color(theme.focus_ring))
            .when(!available, |switch| switch.cursor_default().opacity(0.5))
            .child(
                div()
                    .flex()
                    .w_full()
                    .h_full()
                    .when(enabled, Styled::justify_end)
                    .child(div().size(px(18.0)).rounded_full().bg(if enabled {
                        theme.surface_base
                    } else {
                        theme.outline_strong
                    })),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                if available {
                    this.set_launch_at_login(!this.launch_at_login, cx);
                }
            }))
            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, _, cx| {
                if available
                    && !event.is_held
                    && matches!(event.keystroke.key.as_str(), "enter" | "space")
                {
                    this.set_launch_at_login(!this.launch_at_login, cx);
                    cx.stop_propagation();
                }
            }))
    }

    pub(in crate::app) fn set_launch_at_login(&mut self, enabled: bool, cx: &mut Context<Self>) {
        if self.configuration_transfer.active {
            return;
        }
        let language = self.language();
        let Some(store_dir) = self.subscription_store_dir.clone() else {
            language
                .localized(copy::configuration::STARTUP_SETTING_DATA_DIRECTORY_UNAVAILABLE)
                .clone_into(&mut self.status);
            cx.notify();
            return;
        };
        // Persist first: the preference is the source of truth and is reapplied
        // to the platform login item on the next start.
        match autostart::save_preference_in(&store_dir, enabled) {
            Ok(_) => match autostart::set_enabled(enabled) {
                Ok(()) => {
                    self.launch_at_login = enabled;
                    language
                        .localized(copy::configuration::STARTUP_SETTING_SAVED)
                        .clone_into(&mut self.status);
                }
                Err(error) => {
                    record_event(LogLevel::Warn, "autostart.update_failed", error.to_string());
                    if let Err(rollback) =
                        autostart::save_preference_in(&store_dir, self.launch_at_login)
                    {
                        record_event(
                            LogLevel::Warn,
                            "autostart.preference.rollback_failed",
                            rollback.to_string(),
                        );
                    }
                    language
                        .localized(copy::configuration::STARTUP_SETTING_FAILED)
                        .clone_into(&mut self.status);
                }
            },
            Err(error) => {
                record_event(
                    LogLevel::Warn,
                    "autostart.preference.save_failed",
                    error.to_string(),
                );
                language
                    .localized(copy::configuration::STARTUP_SETTING_COULD_NOT_BE_SAVED)
                    .clone_into(&mut self.status);
            }
        }
        cx.notify();
    }

    /// Reapplies the saved startup choice after the executable may have moved.
    #[cfg(not(test))]
    pub(in crate::app) fn sync_launch_at_login(&self) {
        if !autostart::supported() || self.subscription_store_dir.is_none() {
            return;
        }
        if let Err(error) = autostart::set_enabled(self.launch_at_login) {
            record_event(LogLevel::Warn, "autostart.sync_failed", error.to_string());
        }
    }
}
