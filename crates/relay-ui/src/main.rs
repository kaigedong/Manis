use gpui::App;
use gpui_platform::application;
use relay_ui::{init, install_tray, open_window, show_or_open_window};

fn main() {
    let application = application();
    application.on_reopen(show_or_open_window);
    application.run(|cx: &mut App| {
        init(cx);
        if let Err(message) = install_tray(cx) {
            eprintln!("relay_ui level=WARN event=tray.unavailable message={message}");
        }
        open_window(cx).expect("failed to open Relay GPUI window");
        cx.activate(true);
    });
}
