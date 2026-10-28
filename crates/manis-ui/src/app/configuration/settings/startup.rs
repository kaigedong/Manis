use gpui::{
    Context, Div, InteractiveElement, KeyDownEvent, ParentElement, Role, Stateful,
    StatefulInteractiveElement, Styled, Toggled, div, prelude::*, px,
};

use super::{
    ManisApp, MihomoCoreUpdateState, Space, TextRole, Theme, copy, panel_surface, section_heading,
};
use crate::auto_connect;
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
        let login_item = self.launch_at_login;
        let login_available = autostart::supported() && self.subscription_store_dir.is_some();
        let auto_connect_enabled = self.auto_connect.enabled;
        let auto_connect_available = self.subscription_store_dir.is_some();
        panel_surface("configuration-startup", compact, theme)
            .child(section_heading(
                language.localized(copy::configuration::STARTUP),
                language.localized(copy::configuration::START_MANIS_WITH_YOUR_DESKTOP_SESSION),
                None,
                theme,
            ))
            .child(startup_row(
                language.localized(copy::configuration::LAUNCH_AT_LOGIN),
                language
                    .localized(copy::configuration::OPENING_MANIS_AGAIN_BRINGS_THIS_WINDOW_FORWARD),
                Self::startup_switch(
                    "launch-at-login-toggle",
                    login_item,
                    login_available,
                    language.localized(copy::configuration::LAUNCH_AT_LOGIN),
                    theme,
                    cx,
                    |this, cx| this.set_launch_at_login(!this.launch_at_login, cx),
                ),
                theme,
                true,
            ))
            .child(startup_row(
                language.localized(copy::configuration::AUTO_CONNECT_ON_STARTUP),
                language.localized(copy::configuration::START_THE_KERNEL_AS_SOON_AS_MANIS_OPENS),
                Self::startup_switch(
                    "auto-connect-toggle",
                    auto_connect_enabled,
                    auto_connect_available,
                    language.localized(copy::configuration::AUTO_CONNECT_ON_STARTUP),
                    theme,
                    cx,
                    |this, cx| this.set_auto_connect(!this.auto_connect.enabled, cx),
                ),
                theme,
                false,
            ))
    }

    fn startup_switch(
        id: &'static str,
        enabled: bool,
        available: bool,
        label: &'static str,
        theme: Theme,
        cx: &mut Context<Self>,
        toggle: fn(&mut Self, &mut Context<Self>),
    ) -> Stateful<Div> {
        div()
            .id(id)
            .role(Role::Switch)
            .aria_label(label)
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
                    toggle(this, cx);
                }
            }))
            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, _, cx| {
                if available
                    && !event.is_held
                    && matches!(event.keystroke.key.as_str(), "enter" | "space")
                {
                    toggle(this, cx);
                    cx.stop_propagation();
                }
            }))
    }

    pub(in crate::app) fn set_auto_connect(&mut self, enabled: bool, cx: &mut Context<Self>) {
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
        match auto_connect::save_preference_in(&store_dir, enabled) {
            Ok(_) => {
                self.auto_connect.enabled = enabled;
                if enabled {
                    // Turning the setting on should not require a restart to take effect.
                    self.maybe_auto_connect(cx);
                    if matches!(
                        self.mihomo_core_update_state,
                        MihomoCoreUpdateState::Missing
                    ) {
                        // A previous launch may have failed to install the core; retry it now.
                        self.update_mihomo_core(cx);
                    }
                }
                language
                    .localized(copy::configuration::STARTUP_SETTING_SAVED)
                    .clone_into(&mut self.status);
            }
            Err(error) => {
                record_event(
                    LogLevel::Warn,
                    "auto_connect.preference.save_failed",
                    error.to_string(),
                );
                language
                    .localized(copy::configuration::STARTUP_SETTING_COULD_NOT_BE_SAVED)
                    .clone_into(&mut self.status);
            }
        }
        cx.notify();
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

fn startup_row(
    label: &'static str,
    detail: &'static str,
    switch: Stateful<Div>,
    theme: Theme,
    first: bool,
) -> Div {
    div()
        .mt(if first {
            Space::Md.px()
        } else {
            Space::Sm.px()
        })
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
        .child(switch)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch_store() -> std::path::PathBuf {
        let path =
            std::env::temp_dir().join(format!("manis-startup-settings-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("scratch store");
        path
    }

    #[gpui::test]
    fn auto_connect_toggle_persists_and_applies(cx: &mut gpui::TestAppContext) {
        use gpui::AppContext as _;

        cx.update(crate::init);
        let store = scratch_store();
        let app = cx.new(|_| {
            ManisApp::with_fixture_controller_and_subscription_store(
                "http://127.0.0.1:9090",
                store.clone(),
            )
        });
        app.update(cx, |app, cx| {
            assert!(app.auto_connect.enabled, "enabled by default");
            app.set_auto_connect(false, cx);
            assert!(!app.auto_connect.enabled);
        });
        assert!(!crate::auto_connect::load_preference(Some(&store)));

        app.update(cx, |app, cx| {
            app.set_auto_connect(true, cx);
            assert!(app.auto_connect.enabled);
        });
        assert!(crate::auto_connect::load_preference(Some(&store)));
        let _ = std::fs::remove_dir_all(&store);
    }
}
