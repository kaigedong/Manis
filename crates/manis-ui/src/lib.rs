mod app;
mod app_update;
mod assets;
mod brand;
mod components;
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
mod subscription;
mod subscription_input;
mod system_proxy;
mod theme;
mod tray;

pub use app::ManisApp;
pub use assets::Assets;
pub use tray::{install as install_tray, open_window, show_or_open_window};

/// Returns the version embedded by the packaging workflow.
#[must_use]
pub fn version() -> &'static str {
    app_update::current_version()
}

struct ManisRootView {
    app: gpui::Entity<ManisApp>,
}

impl gpui::Render for ManisRootView {
    fn render(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        use gpui::{ParentElement as _, Styled as _, div};

        div()
            .size_full()
            .child(self.app.clone())
            .children(gpui_component::Root::render_dialog_layer(window, cx))
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
    let view = cx.new(|_| ManisRootView { app });
    gpui_component::Root::new(view, window, cx)
}

pub fn init(cx: &mut gpui::App) {
    gpui_component::init(cx);
    theme::sync_component_theme(theme::Theme::light(), false, None, cx);
}
