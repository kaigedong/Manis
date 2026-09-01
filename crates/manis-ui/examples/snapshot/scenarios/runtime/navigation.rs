use super::super::super::driver::{
    SnapshotWorkspace, close_window, open_workspace, refresh, save_screenshot,
};
use super::super::common::manis_root;
use super::appearance::assert_appearance_mode;

#[cfg(target_os = "macos")]
pub(crate) fn capture_navigation_icons(
    cx: &mut gpui::VisualTestAppContext,
) -> Result<(), Box<dyn std::error::Error>> {
    use gpui::{AnyWindowHandle, Modifiers, point, px, size};

    let store =
        std::env::temp_dir().join(format!("manis-navigation-snapshot-{}", std::process::id()));
    manis_profile::write_private_atomic(&store, "language.preference", b"zh-CN")?;
    for (width, height, label) in [
        (1420.0, 900.0, "wide"),
        (1060.0, 800.0, "medium"),
        (640.0, 560.0, "compact"),
    ] {
        cx.update(manis_ui::init);
        let window_store = store.clone();
        let window = cx.open_offscreen_window(size(px(width), px(height)), |window, cx| {
            manis_root(window, cx, |_| {
                manis_ui::ManisApp::with_fixture_controller_and_subscription_store(
                    "http://127.0.0.1:1",
                    window_store,
                )
            })
        })?;
        let window: AnyWindowHandle = window.into();
        refresh(cx, window)?;
        assert_appearance_mode(cx, window, false)?;
        save_screenshot(cx, window, &format!("navigation-{label}-light.png"))?;
        // Exercise every destination using the unchanged row hit targets.
        for workspace in [
            SnapshotWorkspace::RoutingRules,
            SnapshotWorkspace::Activity,
            SnapshotWorkspace::Logs,
            SnapshotWorkspace::Configuration,
        ] {
            open_workspace(cx, window, width, workspace)?;
        }
        let toggle_x = width - if width >= 1280.0 { 550.0 } else { 205.0 };
        cx.simulate_click(window, point(px(toggle_x), px(24.0)), Modifiers::none());
        refresh(cx, window)?;
        assert_appearance_mode(cx, window, true)?;
        save_screenshot(cx, window, &format!("navigation-{label}-dark.png"))?;
        close_window(cx, window)?;
    }
    std::fs::remove_dir_all(store)?;
    Ok(())
}
