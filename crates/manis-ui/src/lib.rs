mod app;
mod brand;
mod diagnostics;
mod direct_rule;
mod kernel;
mod localization;
#[cfg(target_os = "macos")]
mod macos_privileged;
mod mihomo;
mod rule_source;
mod subscription;
mod subscription_input;
mod system_proxy;
mod theme;
mod tray;

pub use app::ManisApp;
pub use tray::{install as install_tray, open_window, show_or_open_window};

pub fn init(cx: &mut gpui::App) {
    subscription_input::init(cx);
}
