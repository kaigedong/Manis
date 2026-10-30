mod app;
mod app_update;
mod assets;
mod auto_connect;
mod autostart;
mod brand;
mod components;
mod config_backup;
mod config_jsonc;
mod config_toml;
mod core_update;
mod diagnostics;
mod direct_rule;
mod kernel;
#[cfg(target_os = "linux")]
mod linux_privileged;
mod localization;
#[cfg(target_os = "macos")]
mod macos_privileged;
mod manual_rule;
mod mihomo;
mod rule_source;
mod single_instance;
mod subscription;
mod subscription_input;
mod system_proxy;
mod theme;
mod tray;

pub use app::ManisApp;
#[cfg(feature = "snapshot-fixtures")]
pub use app::windows::{
    AuxiliaryKind as UtilityWindowFixture, fixture_root as utility_window_fixture_root,
};
pub use assets::Assets;
#[cfg(feature = "snapshot-fixtures")]
pub use components::button_gallery_fixture;
pub use single_instance::SingleInstance;
pub use single_instance::claim as claim_single_instance;
pub use tray::{install as install_tray, open_window, show_or_open_window};

/// Returns the version embedded by the packaging workflow.
#[must_use]
pub fn version() -> &'static str {
    app_update::current_version()
}

struct ManisRootView {
    app: gpui::Entity<ManisApp>,
    focus: gpui::FocusHandle,
}

impl gpui::Render for ManisRootView {
    fn render(
        &mut self,
        _window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        use gpui::{InteractiveElement as _, ParentElement as _, Styled as _, div};

        div()
            .id("manis-main-window")
            .key_context("ManisWindow")
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|_, event: &gpui::KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape" && cx.stop_active_drag(window) {
                    cx.stop_propagation();
                }
            }))
            .on_action(cx.listener(|this, _: &app::windows::ShowSettings, _, cx| {
                this.app.update(cx, app::ManisApp::open_settings);
            }))
            .relative()
            .size_full()
            .child(self.app.clone())
    }
}

/// Builds the common application host used by both native windows and visual tests.
pub fn root(
    app: gpui::Entity<ManisApp>,
    window: &mut gpui::Window,
    cx: &mut gpui::Context<gpui_component::Root>,
) -> gpui_component::Root {
    use gpui::AppContext as _;

    app.update(cx, |app, cx| app.attach_window(window, cx));
    let view = cx.new(|cx| ManisRootView {
        app,
        focus: cx.focus_handle(),
    });
    let focus = view.read(cx).focus.clone();
    window.focus(&focus, cx);
    gpui_component::Root::new(view, window, cx)
}

pub fn init(cx: &mut gpui::App) {
    gpui_component::init(cx);
    app::windows::init(cx);
    theme::sync_component_theme(theme::Theme::light(), false, None, cx);
}
