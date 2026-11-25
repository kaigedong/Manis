use super::super::super::driver::{
    SnapshotWorkspace, close_window, open_workspace, refresh, save_screenshot, toggle_theme,
};
use super::super::common::manis_root;
use super::appearance::assert_appearance_mode;

#[cfg(target_os = "macos")]
pub(crate) fn capture_navigation_icons(
    cx: &mut gpui::VisualTestAppContext,
) -> Result<(), Box<dyn std::error::Error>> {
    use gpui::{AnyWindowHandle, px, size};

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
        toggle_theme(cx, window, width)?;
        assert_appearance_mode(cx, window, true)?;
        save_screenshot(cx, window, &format!("navigation-{label}-dark.png"))?;
        close_window(cx, window)?;
    }
    std::fs::remove_dir_all(store)?;
    Ok(())
}
