use super::super::super::{
    driver::{
        SnapshotWorkspace, close_window, open_workspace, refresh, save_screenshot,
        settle_ui_animation, settle_ui_for,
    },
    fixtures::spawn_mihomo_fixture,
};
use super::super::common::manis_root;

#[cfg(target_os = "macos")]
pub(crate) fn capture_appearance(
    cx: &mut gpui::VisualTestAppContext,
) -> Result<(), Box<dyn std::error::Error>> {
    for (width, height, label) in [
        (1420.0, 900.0, "wide"),
        (1060.0, 800.0, "medium"),
        (720.0, 720.0, "compact"),
        (640.0, 560.0, "minimum"),
    ] {
        capture_appearance_at_size(cx, width, height, label)?;
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn capture_appearance_at_size(
    cx: &mut gpui::VisualTestAppContext,
    width: f32,
    height: f32,
    label: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    use gpui::{AnyWindowHandle, AppContext as _, Modifiers, point, px, size};
    use gpui_component::WindowExt as _;

    let (endpoint, server) = spawn_mihomo_fixture()?;
    let mut app = None;
    let window = cx.open_offscreen_window(size(px(width), px(height)), |window, cx| {
        let entity = cx.new(|_| manis_ui::ManisApp::with_fixture_controller(endpoint));
        app = Some(entity.clone());
        cx.new(|cx| manis_ui::root(entity, window, cx))
    })?;
    let app = app.ok_or("missing appearance app")?;
    let window: AnyWindowHandle = window.into();
    refresh(cx, window)?;
    cx.simulate_click(window, point(px(width - 80.0), px(90.0)), Modifiers::none());
    settle_ui_for(cx, window, std::time::Duration::from_millis(600))?;
    let toggle_x = width - if width >= 1280.0 { 550.0 } else { 205.0 };
    for (dark, mode) in [(false, "light"), (true, "dark")] {
        if dark {
            cx.simulate_click(window, point(px(toggle_x), px(24.0)), Modifiers::none());
            refresh(cx, window)?;
        }
        for (workspace, name) in [
            (SnapshotWorkspace::Nodes, "nodes"),
            (SnapshotWorkspace::RoutingRules, "rules"),
            (SnapshotWorkspace::Activity, "activity"),
            (SnapshotWorkspace::Logs, "logs"),
            (SnapshotWorkspace::Configuration, "configuration"),
        ] {
            open_workspace(cx, window, width, workspace)?;
            assert_appearance_mode(cx, window, dark)?;
            save_screenshot(cx, window, &format!("appearance-{label}-{mode}-{name}.png"))?;
        }
        if width >= 1280.0 || width <= 720.0 {
            cx.update_window(window, |_, window, cx| {
                app.update(cx, |app, cx| {
                    app.show_proxy_source_dialog_fixture(false, window, cx);
                });
            })?;
            settle_ui_animation(cx, window)?;
            if !cx.update_window(window, |_, window, cx| window.has_active_dialog(cx))? {
                return Err("appearance fixture did not open the source dialog".into());
            }
            save_screenshot(cx, window, &format!("appearance-{label}-{mode}-dialog.png"))?;
            if width >= 1280.0 {
                // Open the interval menu inside the modal to verify nested popup materials.
                cx.update_window(window, |_, window, cx| {
                    app.update(cx, |app, cx| {
                        app.show_proxy_source_dialog_fixture(true, window, cx);
                    });
                })?;
                settle_ui_animation(cx, window)?;
                save_screenshot(
                    cx,
                    window,
                    &format!("appearance-{label}-{mode}-popover.png"),
                )?;
                cx.simulate_keystrokes(window, "escape");
                refresh(cx, window)?;
            }
            cx.update_window(window, |_, window, cx| window.close_dialog(cx))?;
            settle_ui_animation(cx, window)?;
        }
    }
    // Exercise the reverse transition too, including component token synchronization.
    cx.simulate_click(window, point(px(toggle_x), px(24.0)), Modifiers::none());
    refresh(cx, window)?;
    assert_appearance_mode(cx, window, false)?;
    close_window(cx, window)?;
    server.stop()
}

#[cfg(target_os = "macos")]
pub(super) fn assert_appearance_mode(
    cx: &mut gpui::VisualTestAppContext,
    window: gpui::AnyWindowHandle,
    dark: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let screenshot = cx.capture_screenshot(window)?;
    // This unobstructed chrome pixel also catches stale coordinate-based theme clicks.
    let expected = if dark { 0x18 } else { 0xf9 };
    if screenshot.get_pixel(4, 4).0 != [expected, expected, expected, 255] {
        return Err(format!("appearance fixture is not in the expected dark={dark} mode").into());
    }
    Ok(())
}

#[cfg(target_os = "macos")]
pub(super) fn capture_configuration_dark(
    cx: &mut gpui::VisualTestAppContext,
) -> Result<(), Box<dyn std::error::Error>> {
    use gpui::{AnyWindowHandle, Modifiers, point, px, size};
    use manis_ui::ManisApp;

    let window = cx.open_offscreen_window(size(px(1420.0), px(900.0)), |window, cx| {
        manis_root(window, cx, |_| {
            ManisApp::with_fixture_controller("http://127.0.0.1:9090")
        })
    })?;
    let window: AnyWindowHandle = window.into();

    refresh(cx, window)?;
    cx.simulate_click(window, point(px(850.0), px(24.0)), Modifiers::none());
    refresh(cx, window)?;
    open_workspace(cx, window, 1_420.0, SnapshotWorkspace::Configuration)?;
    settle_ui_animation(cx, window)?;
    save_screenshot(cx, window, "configuration-wide-dark.png")?;

    close_window(cx, window)
}
