mod app;
mod demo;
mod diagnostics;
mod mihomo;
mod subscription;
mod subscription_input;
mod system_proxy;
mod theme;

pub use app::RelayApp;

pub fn init(cx: &mut gpui::App) {
    subscription_input::init(cx);
}
