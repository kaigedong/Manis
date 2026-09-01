use super::super::super::{
    driver::{
        SnapshotWorkspace, close_window, open_workspace, refresh, save_screenshot,
        settle_ui_animation, settle_ui_for,
    },
    fixtures::spawn_mihomo_fixture,
};
use super::super::common::manis_root;
use super::appearance::assert_appearance_mode;

#[cfg(target_os = "macos")]
pub(crate) fn capture_log_colors(
    cx: &mut gpui::VisualTestAppContext,
) -> Result<(), Box<dyn std::error::Error>> {
    use gpui::{AnyWindowHandle, Modifiers, point, px, size};

    for (width, height, label) in [(1420.0, 900.0, "wide"), (640.0, 560.0, "compact")] {
        cx.update(manis_ui::init);
        let (endpoint, server) = spawn_mihomo_fixture()?;
        let window = cx.open_offscreen_window(size(px(width), px(height)), |window, cx| {
            manis_root(window, cx, |_| {
                manis_ui::ManisApp::with_fixture_controller(endpoint)
            })
        })?;
        let window: AnyWindowHandle = window.into();
        refresh(cx, window)?;
        open_workspace(cx, window, width, SnapshotWorkspace::Logs)?;
        settle_ui_animation(cx, window)?;
        let refresh_position = if width >= 900.0 {
            point(px(width - 45.0), px(76.0))
        } else {
            point(px(350.0), px(143.0))
        };
        cx.simulate_click(window, refresh_position, Modifiers::none());
        settle_ui_for(cx, window, std::time::Duration::from_millis(600))?;
        for (dark, mode) in [(false, "light"), (true, "dark")] {
            if dark {
                let toggle_x = width - if width >= 1280.0 { 550.0 } else { 205.0 };
                cx.simulate_click(window, point(px(toggle_x), px(24.0)), Modifiers::none());
                refresh(cx, window)?;
            }
            assert_appearance_mode(cx, window, dark)?;
            save_screenshot(cx, window, &format!("log-colors-{label}-{mode}.png"))?;
        }
        close_window(cx, window)?;
        server.stop()?;
    }
    Ok(())
}

#[cfg(target_os = "macos")]
pub(super) fn capture_logs_compact_connected_and_filtered(
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
    open_workspace(cx, window, 720.0, SnapshotWorkspace::Logs)?;
    settle_ui_animation(cx, window)?;
    save_screenshot(cx, window, "logs-compact-connected.png")?;
    cx.simulate_click(window, point(px(220.0), px(145.0)), Modifiers::none());
    refresh(cx, window)?;
    cx.simulate_input(window, "no-match-fixture-token");
    refresh(cx, window)?;
    settle_ui_animation(cx, window)?;
    save_screenshot(cx, window, "logs-compact-empty-filtered.png")?;

    close_window(cx, window)?;
    server.stop()
}
