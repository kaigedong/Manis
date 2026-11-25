use super::super::super::{
    driver::{
        SnapshotWorkspace, close_window, open_workspace, refresh, save_screenshot,
        settle_ui_animation, settle_ui_for, theme_toggle_point, toggle_theme,
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
    use gpui::{AnyWindowHandle, AppContext as _, Modifiers, px, size};
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
    // The connected shell is proven by the connected capture set; this fixture only needs a
    // rendered rail, so it stays disconnected instead of guessing at page-internal buttons.
    settle_ui_for(cx, window, std::time::Duration::from_millis(600))?;
    let toggle = theme_toggle_point(width, height);
    for (dark, mode) in [(false, "light"), (true, "dark")] {
        if dark {
            cx.simulate_click(window, toggle, Modifiers::none());
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
    cx.simulate_click(window, toggle, Modifiers::none());
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
    let expected = if dark {
        [0x14, 0x14, 0x17, 255]
    } else {
        [0xef, 0xec, 0xe6, 255]
    };

    let observed = screenshot.get_pixel(4, 4).0;
    if observed != expected {
        return Err(format!(
            "appearance fixture is not in the expected dark={dark} mode: rail corner is \
             rgb({:02x},{:02x},{:02x}), expected rgb({:02x},{:02x},{:02x})",
            observed[0], observed[1], observed[2], expected[0], expected[1], expected[2]
        )
        .into());
    }
    Ok(())
}

#[cfg(target_os = "macos")]
pub(super) fn capture_configuration_dark(
    cx: &mut gpui::VisualTestAppContext,
) -> Result<(), Box<dyn std::error::Error>> {
    use gpui::{AnyWindowHandle, px, size};
    use manis_ui::ManisApp;

    let window = cx.open_offscreen_window(size(px(1420.0), px(900.0)), |window, cx| {
        manis_root(window, cx, |_| {
            ManisApp::with_fixture_controller("http://127.0.0.1:9090")
        })
    })?;
    let window: AnyWindowHandle = window.into();

    refresh(cx, window)?;
    toggle_theme(cx, window, 1_420.0)?;
    refresh(cx, window)?;
    open_workspace(cx, window, 1_420.0, SnapshotWorkspace::Configuration)?;
    settle_ui_animation(cx, window)?;
    save_screenshot(cx, window, "configuration-wide-dark.png")?;

    close_window(cx, window)
}
