use gpui::{AnyElement, Context, Div, IntoElement, ParentElement, Stateful};
use gpui_component::IconName;

use super::{
    ManisApp, MihomoCoreUpdateState, SettingsGroupKey, Theme, copy, settings_group_card,
    settings_inset_divider, settings_row,
};
use crate::auto_connect;
use crate::autostart;
use crate::components::toggle_switch;
use crate::diagnostics::{LogLevel, record_event};

impl ManisApp {
    pub(in crate::app) fn startup_panel(
        &self,
        theme: Theme,
        _compact: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let language = self.language();
        let login_item = self.launch_at_login;
        let login_available = autostart::supported() && self.subscription_store_dir.is_some();
        let auto_connect_enabled = self.auto_connect.enabled;
        let auto_connect_available = self.subscription_store_dir.is_some();

        self.collapsible_settings_section(
            SettingsGroupKey::Startup,
            language.localized(copy::configuration::STARTUP),
            None,
            None,
            |cx| {
                settings_group_card("configuration-startup", theme)
                    .child(settings_row(
                        IconName::Play,
                        language.localized(copy::configuration::LAUNCH_AT_LOGIN),
                        None,
                        Self::startup_switch(
                            "launch-at-login-toggle",
                            login_item,
                            login_available,
                            language.localized(copy::configuration::LAUNCH_AT_LOGIN),
                            cx,
                            |this, cx| this.set_launch_at_login(!this.launch_at_login, cx),
                        ),
                        theme,
                    ))
                    .child(settings_inset_divider(theme, 56.0))
                    .child(settings_row(
                        gpui_component_assets::IconName::Network,
                        language.localized(copy::configuration::AUTO_CONNECT_ON_STARTUP),
                        None,
                        Self::startup_switch(
                            "auto-connect-toggle",
                            auto_connect_enabled,
                            auto_connect_available,
                            language.localized(copy::configuration::AUTO_CONNECT_ON_STARTUP),
                            cx,
                            |this, cx| this.set_auto_connect(!this.auto_connect.enabled, cx),
                        ),
                        theme,
                    ))
                    .into_any_element()
            },
            theme,
            cx,
        )
    }

    fn startup_switch(
        id: &'static str,
        enabled: bool,
        available: bool,
        label: &'static str,
        cx: &mut Context<Self>,
        toggle: fn(&mut Self, &mut Context<Self>),
    ) -> AnyElement {
        toggle_switch(
            id,
            enabled,
            !available,
            label,
            cx.listener(move |this, _, _, cx| {
                if available {
                    toggle(this, cx);
                }
            }),
        )
        .into_any_element()
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
