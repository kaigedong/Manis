//! Win32 shell for the shared NSH bridge.

pub(crate) fn main() {
    use nsh_platform_windows::{AppOptions, AppSource, Bundle, run_app};

    const BUNDLE: &str = include_str!("../../../android/app/src/main/assets/nsh/bundle.js");
    const CAPABILITIES: &str =
        include_str!("../../../android/app/src/main/assets/nsh/capabilities.json");

    /// The host functions are never granted without a declaration
    /// (`Capabilities::all` leaves `host` empty), so the desktop shell must
    /// pass the manifest the bundle was built with.
    fn bundle() -> Bundle {
        Bundle::source(BUNDLE.to_owned())
            .with_capabilities_json(Some(CAPABILITIES))
            .expect("the bundled capabilities.json parses")
    }

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
        size: (1180, 760),
        host,
        ..AppOptions::default()
    };
    let _ = run_app(AppSource::Bundle(bundle()), options);
}
