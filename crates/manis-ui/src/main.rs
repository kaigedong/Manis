use gpui::App;
use gpui_platform::application;
use manis_ui::{Assets, init, install_tray, open_window, show_or_open_window};

fn main() {
    let application = application().with_assets(Assets);
    application.on_reopen(show_or_open_window);
    application.run(|cx: &mut App| {
        init(cx);
        if let Err(message) = install_tray(cx) {
            eprintln!("manis_ui level=WARN event=tray.unavailable message={message}");
        }
        open_window(cx).expect("failed to open Manis GPUI window");
        cx.activate(true);
    });
}
