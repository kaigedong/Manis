#![allow(unsafe_code)]

#[cfg(target_os = "android")]
use android_activity::AndroidApp;
#[cfg(target_os = "android")]
use gpui::{App, Application};
#[cfg(target_os = "android")]
use gpui_mobile::android::jni;

/// NativeActivity entry point used by the Android APK.
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: AndroidApp) {
    android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Info)
            .with_tag("Manis"),
    );
    jni::install_panic_hook();

    if let Some(data_dir) = app.internal_data_path() {
        // Android has no desktop home directory. Set app-private XDG roots before
        // GPUI starts worker threads so Manis configuration, subscriptions and
        // the downloaded Mihomo core stay inside this application's sandbox.
        // SAFETY: this runs once at NativeActivity entry, before Manis starts threads.
        unsafe {
            std::env::set_var("HOME", &data_dir);
            std::env::set_var("XDG_DATA_HOME", data_dir.join(".local/share"));
            std::env::set_var("XDG_CONFIG_HOME", data_dir.join(".config"));
        }
    } else {
        log::error!("Android app-private data directory is unavailable");
        return;
    }

    let _platform = jni::init_platform(&app);
    let Some(platform) = jni::shared_platform() else {
        log::error!("GPUI Android platform initialization failed");
        return;
    };

    log::info!("Starting GPUI Application with Android platform");
    Application::with_platform(platform.into_rc()).run(|cx: &mut App| {
        log::info!("GPUI launch callback: initializing Manis");
        manis_ui::init(cx);
        log::info!("GPUI launch callback: opening main window");
        if let Err(error) = manis_ui::open_window(cx) {
            log::error!("Could not open the Manis window: {error}");
            cx.quit();
        } else {
            log::info!("GPUI launch callback: main window opened");
        }
    });
}
