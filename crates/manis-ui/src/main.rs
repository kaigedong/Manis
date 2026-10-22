use gpui::App;
use gpui_platform::application;
use manis_ui::{
    Assets, claim_single_instance, init, install_tray, open_window, show_or_open_window,
};

fn main() {
    if std::env::args_os().nth(1).as_deref() == Some(std::ffi::OsStr::new("--version")) {
        println!("Manis {}", manis_ui::version());
        return;
    }
    let instance = claim_single_instance();
    if !instance.is_primary() {
        return;
    }
    let application = application().with_assets(Assets);
    application.on_reopen(show_or_open_window);
    application.run(move |cx: &mut App| {
        init(cx);
        if let Err(message) = install_tray(cx) {
            eprintln!("manis_ui level=WARN event=tray.unavailable message={message}");
        }
        if let Err(message) = open_window(cx) {
            eprintln!("manis_ui level=ERROR event=window.open_failed message={message}");
            cx.quit();
        } else {
            instance.install(cx);
            cx.activate(true);
        }
    });
}
