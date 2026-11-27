use std::process::ExitCode;

use gpui::App;
use gpui_platform::application;
use manis_ui::{
    Assets, claim_single_instance, init, install_control_api, install_tray, open_window,
    show_or_open_window, try_run_cli,
};

fn main() -> ExitCode {
    if let Some(code) = try_run_cli() {
        return code;
    }
    let instance = claim_single_instance();
    if !instance.is_primary() {
        return ExitCode::SUCCESS;
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
            install_control_api(cx);
            cx.activate(true);
        }
    });
    ExitCode::SUCCESS
}
