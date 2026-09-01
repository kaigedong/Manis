use gpui::App;
use gpui_platform::application;
use manis_ui::{Assets, init, install_tray, open_window, show_or_open_window};

fn main() {
    if std::env::args_os().nth(1).as_deref() == Some(std::ffi::OsStr::new("--version")) {
        println!("Manis {}", manis_ui::version());
        return;
    }
    let application = application().with_assets(Assets);
    application.on_reopen(show_or_open_window);
    application.run(|cx: &mut App| {
        init(cx);
        if let Err(message) = install_tray(cx) {
            eprintln!("manis_ui level=WARN event=tray.unavailable message={message}");
        }
        if let Err(message) = open_window(cx) {
            eprintln!("manis_ui level=ERROR event=window.open_failed message={message}");
            cx.quit();
        } else {
            cx.activate(true);
        }
    });
}
