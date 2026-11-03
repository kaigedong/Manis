use super::super::super::driver::{close_window, refresh, save_screenshot_at};
use super::super::common::manis_root;

#[cfg(target_os = "macos")]
pub(crate) fn capture_live_when_configured(
    cx: &mut gpui::VisualTestAppContext,
) -> Result<(), Box<dyn std::error::Error>> {
    use gpui::{AnyWindowHandle, Modifiers, point, px, size};
    use manis_ui::ManisApp;
    use std::path::PathBuf;

    let Ok(endpoint) = std::env::var("MANIS_MIHOMO_LIVE_CONTROLLER") else {
        return Ok(());
    };
    let output = PathBuf::from(
        std::env::var_os("MANIS_MIHOMO_LIVE_SCREENSHOT")
            .ok_or("MANIS_MIHOMO_LIVE_SCREENSHOT is required when live capture is enabled")?,
    );
    validate_live_output(&output)?;

    let window = cx.open_offscreen_window(size(px(1420.0), px(900.0)), |window, cx| {
        manis_root(window, cx, |_| ManisApp::with_fixture_controller(endpoint))
    })?;
    let window: AnyWindowHandle = window.into();

    refresh(cx, window)?;
    cx.simulate_click(window, point(px(480.0), px(80.0)), Modifiers::none());
    refresh(cx, window)?;
    save_screenshot_at(cx, window, &output)?;
    close_window(cx, window)
}

#[cfg(target_os = "macos")]
pub(crate) fn validate_live_output(
    output: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if std::fs::symlink_metadata(output).is_ok() {
        return Err("live screenshot output must be a new file".into());
    }

    let temp_root = std::env::temp_dir().canonicalize()?;
    let parent = output
        .parent()
        .ok_or("live screenshot output must have a parent directory")?
        .canonicalize()?;
    if !parent.starts_with(temp_root) {
        return Err("live screenshots must stay inside the system temporary directory".into());
    }
    Ok(())
}
