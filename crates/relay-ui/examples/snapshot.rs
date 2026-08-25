#[cfg(target_os = "macos")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let platform = gpui_platform::current_platform(false);
    let mut cx = gpui::VisualTestAppContext::new(platform);
    capture(&mut cx, 1420.0, 900.0, "native-wide.png")?;
    capture(&mut cx, 1060.0, 800.0, "native-medium.png")?;
    capture(&mut cx, 720.0, 720.0, "native-compact.png")?;
    capture_compact_flow(&mut cx)?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn capture(
    cx: &mut gpui::VisualTestAppContext,
    width: f32,
    height: f32,
    file_name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    use gpui::{AnyWindowHandle, AppContext, px, size};
    use relay_ui::RelayApp;

    let window = cx.open_offscreen_window(size(px(width), px(height)), |_, cx| {
        cx.new(|_| RelayApp::new())
    })?;
    let window: AnyWindowHandle = window.into();

    refresh(cx, window)?;
    save_screenshot(cx, window, file_name)
}

#[cfg(target_os = "macos")]
fn capture_compact_flow(
    cx: &mut gpui::VisualTestAppContext,
) -> Result<(), Box<dyn std::error::Error>> {
    use gpui::{AnyWindowHandle, AppContext, Modifiers, point, px, size};
    use relay_ui::RelayApp;

    let window = cx.open_offscreen_window(size(px(720.0), px(720.0)), |_, cx| {
        cx.new(|_| RelayApp::new())
    })?;
    let window: AnyWindowHandle = window.into();

    refresh(cx, window)?;
    cx.simulate_click(window, point(px(300.0), px(312.0)), Modifiers::none());
    refresh(cx, window)?;
    save_screenshot(cx, window, "native-compact-detail.png")?;

    cx.simulate_click(window, point(px(592.0), px(24.0)), Modifiers::none());
    refresh(cx, window)?;
    save_screenshot(cx, window, "native-compact-dark-detail.png")?;

    cx.simulate_click(window, point(px(664.0), px(80.0)), Modifiers::none());
    refresh(cx, window)?;
    save_screenshot(cx, window, "native-compact-dark-inspector.png")
}

#[cfg(target_os = "macos")]
fn refresh(
    cx: &mut gpui::VisualTestAppContext,
    window: gpui::AnyWindowHandle,
) -> Result<(), Box<dyn std::error::Error>> {
    cx.run_until_parked();
    cx.update_window(window, |_, window, _| window.refresh())?;
    cx.run_until_parked();
    Ok(())
}

#[cfg(target_os = "macos")]
fn save_screenshot(
    cx: &mut gpui::VisualTestAppContext,
    window: gpui::AnyWindowHandle,
    file_name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    use std::path::PathBuf;

    let screenshot = cx.capture_screenshot(window)?;
    let output = PathBuf::from(".impeccable/review").join(file_name);
    std::fs::create_dir_all(".impeccable/review")?;
    screenshot.save(&output)?;
    println!("saved {}", output.display());
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("native snapshot capture is currently available on macOS only");
}
