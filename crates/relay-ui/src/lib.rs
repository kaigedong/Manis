mod app;
mod demo;
mod diagnostics;
mod mihomo;
mod rule_source;
mod subscription;
mod subscription_input;
mod system_proxy;
mod theme;
mod tray;

pub use app::RelayApp;
pub use tray::{install as install_tray, open_window, show_or_open_window};

pub fn init(cx: &mut gpui::App) {
    subscription_input::init(cx);
}
