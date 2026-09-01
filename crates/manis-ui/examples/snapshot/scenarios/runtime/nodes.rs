use super::super::super::{
    driver::{
        close_window, refresh, save_screenshot, scroll_window, settle_ui_animation, settle_ui_for,
    },
    fixtures::spawn_mihomo_fixture,
};
use super::super::common::{manis_root, verify_secondary_click};
use super::appearance::assert_appearance_mode;

#[cfg(target_os = "macos")]
pub(crate) fn capture_nodes_toolbar(
    cx: &mut gpui::VisualTestAppContext,
) -> Result<(), Box<dyn std::error::Error>> {
    use gpui::{AnyWindowHandle, Modifiers, point, px, size};

    for (width, height, label) in [
        (1420.0, 900.0, "wide"),
        (1060.0, 800.0, "medium"),
        (640.0, 560.0, "minimum"),
    ] {
        cx.update(manis_ui::init);
        let (endpoint, server) = spawn_mihomo_fixture()?;
        let window = cx.open_offscreen_window(size(px(width), px(height)), |window, cx| {
            manis_root(window, cx, |_| {
                manis_ui::ManisApp::with_fixture_controller(endpoint)
            })
        })?;
        let window: AnyWindowHandle = window.into();
        refresh(cx, window)?;
        if label == "minimum" {
            save_screenshot(cx, window, "nodes-toolbar-minimum-empty.png")?;
        }
        cx.simulate_click(window, point(px(width - 80.0), px(90.0)), Modifiers::none());
        settle_ui_for(cx, window, std::time::Duration::from_millis(600))?;
        for (dark, mode) in [(false, "light"), (true, "dark")] {
            if dark {
                let toggle_x = width - if width >= 1280.0 { 550.0 } else { 205.0 };
                cx.simulate_click(window, point(px(toggle_x), px(24.0)), Modifiers::none());
                refresh(cx, window)?;
            }
            assert_appearance_mode(cx, window, dark)?;
            save_screenshot(cx, window, &format!("nodes-toolbar-{label}-{mode}.png"))?;
        }
        close_window(cx, window)?;
        server.stop()?;
    }
    Ok(())
}

#[cfg(target_os = "macos")]
pub(crate) fn capture_merged_nodes(
    cx: &mut gpui::VisualTestAppContext,
) -> Result<(), Box<dyn std::error::Error>> {
    use gpui::{AnyWindowHandle, AppContext as _, Modifiers, point, px, size};
    let store = std::env::temp_dir().join(format!("manis-merged-nodes-{}", std::process::id()));
    manis_profile::write_private_atomic(&store, "language.preference", b"zh-CN")?;
    for (width, height, label) in [(1420.0, 900.0, "wide"), (640.0, 560.0, "compact")] {
        cx.update(manis_ui::init);
        let mut app = None;
        let window_store = store.clone();
        let window = cx.open_offscreen_window(size(px(width), px(height)), |window, cx| {
            let entity = cx.new(|_| {
                manis_ui::ManisApp::with_fixture_controller_and_subscription_store(
                    "http://127.0.0.1:1",
                    window_store,
                )
            });
            app = Some(entity.clone());
            cx.new(|cx| manis_ui::root(entity, window, cx))
        })?;
        let app = app.ok_or("missing merged nodes app")?;
        let window: AnyWindowHandle = window.into();
        refresh(cx, window)?;
        save_screenshot(cx, window, &format!("merged-nodes-{label}-empty.png"))?;
        for (dark, mode) in [(false, "light"), (true, "dark")] {
            if dark {
                let toggle_x = width - if width >= 1280.0 { 550.0 } else { 205.0 };
                cx.simulate_click(window, point(px(toggle_x), px(24.0)), Modifiers::none());
            }
            cx.update_window(window, |_, _, cx| {
                app.update(cx, |app, cx| app.show_merged_nodes_fixture(false, cx));
            })?;
            refresh(cx, window)?;
            save_screenshot(cx, window, &format!("merged-nodes-{label}-{mode}.png"))?;
            verify_secondary_click(
                cx,
                window,
                point(px(width - 100.0), px(188.0)),
                &format!("merged-nodes-{label}-{mode}-secondary-click.png"),
            )?;
            cx.update_window(window, |_, _, cx| {
                app.update(cx, |app, cx| app.show_merged_nodes_fixture(true, cx));
            })?;
            refresh(cx, window)?;
            save_screenshot(
                cx,
                window,
                &format!("merged-nodes-{label}-{mode}-expanded.png"),
            )?;
            let top = cx.capture_screenshot(window)?;
            scroll_window(cx, window, width - 50.0, height - 100.0, -10_000.0)?;
            save_screenshot(
                cx,
                window,
                &format!("merged-nodes-{label}-{mode}-bottom.png"),
            )?;
            assert!(
                top != cx.capture_screenshot(window)?,
                "long groups must scroll"
            );
            scroll_window(cx, window, width - 50.0, height - 100.0, 10_000.0)?;
        }
        close_window(cx, window)?;
    }
    std::fs::remove_dir_all(store)?;
    Ok(())
}

#[cfg(target_os = "macos")]
pub(crate) fn capture_compact_flow(
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
    cx.simulate_click(window, point(px(185.0), px(412.0)), Modifiers::none());
    settle_ui_for(cx, window, std::time::Duration::from_millis(600))?;
    cx.simulate_click(window, point(px(300.0), px(312.0)), Modifiers::none());
    refresh(cx, window)?;
    save_screenshot(cx, window, "native-compact-detail.png")?;

    cx.simulate_click(window, point(px(475.0), px(24.0)), Modifiers::none());
    refresh(cx, window)?;
    save_screenshot(cx, window, "native-compact-dark-detail.png")?;

    cx.simulate_click(window, point(px(664.0), px(80.0)), Modifiers::none());
    refresh(cx, window)?;
    save_screenshot(cx, window, "native-compact-dark-inspector.png")?;
    close_window(cx, window)?;
    server.stop()
}

#[cfg(target_os = "macos")]
pub(crate) fn capture_medium_sheet(
    cx: &mut gpui::VisualTestAppContext,
) -> Result<(), Box<dyn std::error::Error>> {
    use gpui::{AnyWindowHandle, Modifiers, point, px, size};
    use manis_ui::ManisApp;

    let (endpoint, server) = spawn_mihomo_fixture()?;
    let window = cx.open_offscreen_window(size(px(1060.0), px(800.0)), |window, cx| {
        manis_root(window, cx, |_| ManisApp::with_fixture_controller(endpoint))
    })?;
    let window: AnyWindowHandle = window.into();

    settle_ui_for(cx, window, std::time::Duration::from_millis(600))?;
    cx.simulate_click(window, point(px(985.0), px(80.0)), Modifiers::none());
    settle_ui_for(cx, window, std::time::Duration::from_millis(600))?;
    cx.simulate_click(window, point(px(250.0), px(184.0)), Modifiers::none());
    settle_ui_animation(cx, window)?;
    cx.simulate_click(window, point(px(985.0), px(80.0)), Modifiers::none());
    settle_ui_animation(cx, window)?;
    cx.simulate_input(window, "openai.com");
    refresh(cx, window)?;
    cx.simulate_click(window, point(px(1_015.0), px(205.0)), Modifiers::none());
    settle_ui_animation(cx, window)?;
    save_screenshot(cx, window, "native-medium-sheet.png")?;
    cx.simulate_click(window, point(px(1_040.0), px(68.0)), Modifiers::none());
    settle_ui_animation(cx, window)?;
    save_screenshot(cx, window, "native-medium-sheet-closed.png")?;
    close_window(cx, window)?;
    server.stop()
}
