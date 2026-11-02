//! `AppKit` shell for the shared NSH bridge.

pub(crate) fn main() {
    use nsh_platform_macos::{AppOptions, AppSource, Bundle, run_app};

    const BUNDLE: &str = include_str!("../../../android/app/src/main/assets/nsh/bundle.js");

    if std::env::args_os().any(|argument| argument == "--version") {
        println!("Manis {}", manis_service::version());
        return;
    }
    let _engine = manis_nsh_host::ensure_engine();
    std::thread::sleep(std::time::Duration::from_millis(1500));
    let host = nsh_engine::HostChannel::default();
    let params = serde_json::json!({
        "data": manis_nsh_host::root_data(),
        "embedded": true,
        "topInset": 0,
        "bottomInset": 0,
        "tab": std::env::var("MANIS_NSH_TAB").unwrap_or_else(|_| "connection".into()),
    });
    if let Some(params) = params.as_object() {
        host.set_params(params.clone());
    }
    manis_nsh_host::install_handlers(&host);
    let options = AppOptions {
        app_id: "dev.manis.desktop.nsh".into(),
        title: "Manis".into(),
        size: (430, 900),
        keep_alive_on_close: true,
        host,
        ..AppOptions::default()
    };
    let _ = run_app(
        AppSource::Bundle(Bundle::source(BUNDLE.to_owned())),
        options,
    );
}
