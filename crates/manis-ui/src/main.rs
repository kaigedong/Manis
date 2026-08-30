use gpui::App;
use gpui_platform::application;
use manis_ui::{Assets, init, install_tray, open_window, show_or_open_window};

fn main() {
    #[cfg(target_os = "linux")]
    if let Some(result) = manis_ui::run_linux_tun_dns_helper_from_args() {
        if let Err(error) = result {
            eprintln!("{error}");
            std::process::exit(1);
        }
        return;
    }
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
        open_window(cx).expect("failed to open Manis GPUI window");
        cx.activate(true);
    });
}
