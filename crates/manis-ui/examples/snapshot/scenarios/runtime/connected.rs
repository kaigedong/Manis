use super::super::super::{
    driver::{
        SnapshotWorkspace, close_window, open_policy_section, open_workspace, refresh,
        save_screenshot, settle_ui_animation, settle_ui_for,
    },
    fixtures::{
        spawn_empty_mihomo_fixture, spawn_mihomo_fixture, spawn_mihomo_fixture_with_stream_failure,
    },
};
use super::super::common::manis_root;
use super::{
    appearance::capture_configuration_dark, logs::capture_logs_compact_connected_and_filtered,
};

#[cfg(target_os = "macos")]
pub(crate) fn capture_stream_status(
    cx: &mut gpui::VisualTestAppContext,
) -> Result<(), Box<dyn std::error::Error>> {
    use gpui::{AnyWindowHandle, Modifiers, point, px, size};
    for (width, height, label) in [(1420.0, 900.0, "wide"), (640.0, 560.0, "compact")] {
        cx.update(manis_ui::init);
        let (endpoint, server) = spawn_mihomo_fixture_with_stream_failure(true)?;
        let window = cx.open_offscreen_window(size(px(width), px(height)), |window, cx| {
            manis_root(window, cx, |_| {
                manis_ui::ManisApp::with_fixture_controller(endpoint)
            })
        })?;
        let window: AnyWindowHandle = window.into();
        refresh(cx, window)?;
        cx.simulate_click(window, point(px(width - 80.0), px(90.0)), Modifiers::none());
        settle_ui_for(cx, window, std::time::Duration::from_millis(800))?;
        open_workspace(cx, window, width, SnapshotWorkspace::Activity)?;
        save_screenshot(cx, window, &format!("stream-status-{label}-activity.png"))?;
        open_workspace(cx, window, width, SnapshotWorkspace::Nodes)?;
        save_screenshot(cx, window, &format!("stream-status-{label}-nodes.png"))?;
        let toggle_x = width - if width >= 1280.0 { 550.0 } else { 205.0 };
        cx.simulate_click(window, point(px(toggle_x), px(24.0)), Modifiers::none());
        open_workspace(cx, window, width, SnapshotWorkspace::Logs)?;
        save_screenshot(cx, window, &format!("stream-status-{label}-logs-dark.png"))?;
        close_window(cx, window)?;
        server.stop()?;
    }
    Ok(())
}

#[cfg(target_os = "macos")]
pub(crate) fn capture_connected(
    cx: &mut gpui::VisualTestAppContext,
) -> Result<(), Box<dyn std::error::Error>> {
    use gpui::{AnyWindowHandle, Modifiers, point, px, size};
    use manis_ui::ManisApp;

    let (endpoint, server) = spawn_mihomo_fixture()?;
    let window = cx.open_offscreen_window(size(px(1420.0), px(900.0)), |window, cx| {
        manis_root(window, cx, |_| ManisApp::with_fixture_controller(endpoint))
    })?;
    let window: AnyWindowHandle = window.into();

    refresh(cx, window)?;
    cx.simulate_click(window, point(px(1_340.0), px(126.0)), Modifiers::none());
    settle_ui_for(cx, window, std::time::Duration::from_millis(600))?;
    refresh(cx, window)?;
    save_screenshot(cx, window, "native-wide-connected.png")?;

    cx.simulate_click(window, point(px(110.0), px(80.0)), Modifiers::none());
    refresh(cx, window)?;
    save_screenshot(cx, window, "nodes-wide-connected-global.png")?;

    open_policy_section(cx, window, 1_420.0)?;

    cx.simulate_click(window, point(px(270.0), px(236.0)), Modifiers::none());
    settle_ui_for(cx, window, std::time::Duration::from_millis(600))?;
    save_screenshot(cx, window, "native-wide-connected-benchmark.png")?;

    cx.advance_clock(std::time::Duration::from_millis(500));
    refresh(cx, window)?;

    open_workspace(cx, window, 1_420.0, SnapshotWorkspace::Activity)?;
    save_screenshot(cx, window, "activity-wide-connected.png")?;

    open_workspace(cx, window, 1_420.0, SnapshotWorkspace::Logs)?;
    save_screenshot(cx, window, "logs-wide-connected.png")?;

    open_workspace(cx, window, 1_420.0, SnapshotWorkspace::Configuration)?;
    save_screenshot(cx, window, "configuration-wide-connected-sources.png")?;

    close_window(cx, window)?;
    server.stop()?;
    Ok(())
}

#[cfg(target_os = "macos")]
pub(crate) fn capture_data_page_coverage(
    cx: &mut gpui::VisualTestAppContext,
) -> Result<(), Box<dyn std::error::Error>> {
    capture_activity_compact_connected(cx)?;
    capture_activity_compact_empty(cx)?;
    capture_logs_compact_connected_and_filtered(cx)?;
    capture_activity_wide_dark_connected(cx)?;
    capture_configuration_dark(cx)
}

#[cfg(target_os = "macos")]
fn capture_activity_wide_dark_connected(
    cx: &mut gpui::VisualTestAppContext,
) -> Result<(), Box<dyn std::error::Error>> {
    use gpui::{AnyWindowHandle, Modifiers, point, px, size};
    use manis_ui::ManisApp;

    let (endpoint, server) = spawn_mihomo_fixture()?;
    let window = cx.open_offscreen_window(size(px(1420.0), px(900.0)), |window, cx| {
        manis_root(window, cx, |_| ManisApp::with_fixture_controller(endpoint))
    })?;
    let window: AnyWindowHandle = window.into();

    refresh(cx, window)?;
    cx.simulate_click(window, point(px(1_340.0), px(126.0)), Modifiers::none());
    settle_ui_animation(cx, window)?;
    cx.simulate_click(window, point(px(850.0), px(24.0)), Modifiers::none());
    settle_ui_animation(cx, window)?;
    open_workspace(cx, window, 1_420.0, SnapshotWorkspace::Activity)?;
    settle_ui_animation(cx, window)?;
    save_screenshot(cx, window, "activity-wide-dark-connected.png")?;

    close_window(cx, window)?;
    server.stop()
}

#[cfg(target_os = "macos")]
fn capture_activity_compact_connected(
    cx: &mut gpui::VisualTestAppContext,
) -> Result<(), Box<dyn std::error::Error>> {
    use gpui::{AnyWindowHandle, Modifiers, point, px, size};
    use manis_ui::ManisApp;

    let (endpoint, server) = spawn_mihomo_fixture()?;
    let window = cx.open_offscreen_window(size(px(720.0), px(720.0)), |window, cx| {
        manis_root(window, cx, |_| ManisApp::with_fixture_controller(endpoint))
    })?;
    let window: AnyWindowHandle = window.into();

    refresh(cx, window)?;
    cx.simulate_click(window, point(px(520.0), px(76.0)), Modifiers::none());
    settle_ui_animation(cx, window)?;
    open_workspace(cx, window, 720.0, SnapshotWorkspace::Activity)?;
    settle_ui_animation(cx, window)?;
    save_screenshot(cx, window, "activity-compact-connected.png")?;

    close_window(cx, window)?;
    server.stop()
}

#[cfg(target_os = "macos")]
fn capture_activity_compact_empty(
    cx: &mut gpui::VisualTestAppContext,
) -> Result<(), Box<dyn std::error::Error>> {
    use gpui::{AnyWindowHandle, Modifiers, point, px, size};
    use manis_ui::ManisApp;

    let (endpoint, server) = spawn_empty_mihomo_fixture()?;
    let window = cx.open_offscreen_window(size(px(720.0), px(720.0)), |window, cx| {
        manis_root(window, cx, |_| ManisApp::with_fixture_controller(endpoint))
    })?;
    let window: AnyWindowHandle = window.into();

    refresh(cx, window)?;
    cx.simulate_click(window, point(px(520.0), px(76.0)), Modifiers::none());
    settle_ui_animation(cx, window)?;
    open_workspace(cx, window, 720.0, SnapshotWorkspace::Activity)?;
    settle_ui_animation(cx, window)?;
    save_screenshot(cx, window, "activity-compact-empty.png")?;

    close_window(cx, window)?;
    server.stop()
}
