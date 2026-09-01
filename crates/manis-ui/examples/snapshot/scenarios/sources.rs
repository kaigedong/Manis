use super::super::{
    driver::{
        SnapshotWorkspace, close_window, open_workspace, refresh, save_screenshot, scroll_window,
        settle_ui_for,
    },
    fixtures::{
        SubscriptionFixtureServer, write_managed_policy_fixture, write_source_cards_fixture,
    },
};
use super::common::manis_root;

#[cfg(target_os = "macos")]
pub(crate) fn capture_source_cards(
    cx: &mut gpui::VisualTestAppContext,
) -> Result<(), Box<dyn std::error::Error>> {
    use gpui::{AnyWindowHandle, AppContext as _, Modifiers, point, px, size};
    let store = std::env::temp_dir().join(format!("manis-source-cards-{}", std::process::id()));
    write_source_cards_fixture(&store)?;
    for (logical_width, height, label) in [(1420_u16, 900.0, "wide"), (640, 560.0, "compact")] {
        let width = f32::from(logical_width);
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
        let app = app.ok_or("missing fixture app")?;
        let window: AnyWindowHandle = window.into();
        refresh(cx, window)?;
        open_workspace(cx, window, width, SnapshotWorkspace::Configuration)?;
        for (dark, mode) in [(false, "light"), (true, "dark")] {
            if dark {
                let toggle_x = width - if width >= 1280.0 { 550.0 } else { 205.0 };
                cx.simulate_click(window, point(px(toggle_x), px(24.0)), Modifiers::none());
                refresh(cx, window)?;
            }
            for (rules, section) in [(false, "proxy"), (true, "rules")] {
                cx.update_window(window, |_, _, cx| {
                    app.update(cx, |app, cx| {
                        app.show_configuration_sources_fixture(rules, cx);
                    });
                })?;
                cx.simulate_mouse_move(window, point(px(20.0), px(20.0)), None, Modifiers::none());
                refresh(cx, window)?;
                save_screenshot(
                    cx,
                    window,
                    &format!("source-cards-{label}-{mode}-{section}.png"),
                )?;
                let hover_y = 295.0;
                let before_hover = cx.capture_screenshot(window)?;
                cx.simulate_mouse_move(
                    window,
                    point(px(width - 180.0), px(hover_y)),
                    None,
                    Modifiers::none(),
                );
                refresh(cx, window)?;
                save_screenshot(
                    cx,
                    window,
                    &format!("source-cards-{label}-{mode}-{section}-hover.png"),
                )?;
                let after_hover = cx.capture_screenshot(window)?;
                let changed = before_hover
                    .pixels()
                    .zip(after_hover.pixels())
                    .filter(|(a, b)| a != b)
                    .count();
                assert!(
                    changed > 20_000,
                    "{label}/{mode}/{section}: hover must highlight the entire card"
                );
                verify_source_control_hover(
                    cx,
                    window,
                    logical_width,
                    false,
                    &format!("source-cards-{label}-{mode}-{section}"),
                )?;
                if !rules {
                    verify_source_control_hover(
                        cx,
                        window,
                        logical_width,
                        true,
                        &format!("source-cards-{label}-{mode}-single-node"),
                    )?;
                }
            }
        }
        close_window(cx, window)?;
    }
    std::fs::remove_dir_all(store)?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn verify_source_control_hover(
    cx: &mut gpui::VisualTestAppContext,
    window: gpui::AnyWindowHandle,
    width: u16,
    second_row: bool,
    name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    use gpui::{Modifiers, point, px};
    cx.simulate_mouse_move(window, point(px(20.0), px(20.0)), None, Modifiers::none());
    refresh(cx, window)?;
    let normal = cx.capture_screenshot(window)?;
    let wide = width > 1000;
    let offset: u16 = if second_row { 106 } else { 0 };
    let scale = normal.width() / u32::from(width);
    let sample_x = u32::from(width - 180) * scale;
    let sample_y = u32::from(295 + offset) * scale;
    let offset = f32::from(offset);
    let width = f32::from(width);
    let sample = point(px(width - 180.0), px(295.0 + offset));
    cx.simulate_mouse_move(window, sample, None, Modifiers::none());
    refresh(cx, window)?;
    assert_ne!(
        normal.get_pixel(sample_x, sample_y),
        cx.capture_screenshot(window)?.get_pixel(sample_x, sample_y),
        "{name}: row body must keep its edit hover"
    );
    let button_y = if wide { 335.0 } else { 343.0 } + offset;
    let mut controls = vec![
        (
            "remove",
            point(px(width - if wide { 90.0 } else { 65.0 }), px(button_y)),
        ),
        (
            "checkbox",
            point(
                px(if wide { 522.0 } else { 102.0 }),
                px(if wide { 312.0 } else { 320.0 } + offset),
            ),
        ),
    ];
    if !second_row {
        controls.push((
            "refresh",
            point(px(width - if wide { 165.0 } else { 140.0 }), px(button_y)),
        ));
    }
    for (control, position) in controls {
        cx.simulate_mouse_move(window, position, None, Modifiers::none());
        refresh(cx, window)?;
        let hovered = cx.capture_screenshot(window)?;
        assert_eq!(
            normal.get_pixel(sample_x, sample_y),
            hovered.get_pixel(sample_x, sample_y),
            "{name}/{control}: nested controls must not highlight the row"
        );
        if control == "remove" {
            save_screenshot(cx, window, &format!("{name}-{control}-hover.png"))?;
        }
    }
    Ok(())
}

#[cfg(target_os = "macos")]
pub(crate) fn capture_remote_subscription_preview(
    cx: &mut gpui::VisualTestAppContext,
) -> Result<(), Box<dyn std::error::Error>> {
    use gpui::{AnyWindowHandle, Modifiers, point, px, size};
    use manis_ui::ManisApp;
    use std::path::Path;
    use std::time::Duration;

    let server = SubscriptionFixtureServer::start()?;
    let subscription_url = server.url().to_owned();

    let fixture_root =
        std::env::temp_dir().join(format!("manis-ui-import-snapshot-{}", std::process::id()));
    if Path::new(&fixture_root).exists() {
        std::fs::remove_dir_all(&fixture_root)?;
    }
    let store = fixture_root.join("subscriptions");
    let initial_store = store.clone();

    let window = cx.open_offscreen_window(size(px(1420.0), px(900.0)), |window, cx| {
        manis_root(window, cx, |_| {
            ManisApp::with_fixture_controller_and_subscription_store(
                "http://127.0.0.1:9090",
                initial_store,
            )
        })
    })?;
    let window: AnyWindowHandle = window.into();
    refresh(cx, window)?;
    open_workspace(cx, window, 1_420.0, SnapshotWorkspace::Configuration)?;
    scroll_window(cx, window, 1_300.0, 760.0, -600.0)?;
    cx.simulate_click(window, point(px(700.0), px(510.0)), Modifiers::none());
    refresh(cx, window)?;
    cx.simulate_input(window, &subscription_url);
    refresh(cx, window)?;
    cx.simulate_click(window, point(px(700.0), px(560.0)), Modifiers::none());
    settle_ui_for(cx, window, Duration::from_secs(1))?;
    scroll_window(cx, window, 1_300.0, 760.0, -360.0)?;
    save_screenshot(
        cx,
        window,
        "configuration-wide-remote-subscription-nodes.png",
    )?;

    scroll_window(cx, window, 1_300.0, 300.0, 360.0)?;
    cx.simulate_click(window, point(px(700.0), px(510.0)), Modifiers::none());
    refresh(cx, window)?;
    cx.simulate_input(
        window,
        "vless://00000000-0000-4000-8000-000000000000@edge.example.invalid:443?security=tls&type=ws#Saved%20Edge",
    );
    refresh(cx, window)?;
    cx.simulate_click(window, point(px(700.0), px(560.0)), Modifiers::none());
    refresh(cx, window)?;

    close_window(cx, window)?;
    write_managed_policy_fixture(&store)?;
    capture_restored_subscription_views(cx, &store)?;
    server.stop()?;
    if fixture_root.exists() {
        std::fs::remove_dir_all(fixture_root)?;
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn capture_restored_subscription_views(
    cx: &mut gpui::VisualTestAppContext,
    store: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    for view in [
        RestoredSubscriptionView {
            width: 1420.0,
            height: 900.0,
            configuration_file: "configuration-wide-import-restored.png",
            nodes_file: "nodes-wide-imported.png",
            collapsed_file: "nodes-wide-imported-collapsed.png",
            group_y: 365.0,
        },
        RestoredSubscriptionView {
            width: 720.0,
            height: 720.0,
            configuration_file: "configuration-compact-import-restored.png",
            nodes_file: "nodes-compact-imported.png",
            collapsed_file: "nodes-compact-imported-collapsed.png",
            group_y: 310.0,
        },
    ] {
        capture_restored_subscription_view(cx, store, view)?;
    }
    Ok(())
}

#[cfg(target_os = "macos")]
#[derive(Clone, Copy)]
struct RestoredSubscriptionView {
    width: f32,
    height: f32,
    configuration_file: &'static str,
    nodes_file: &'static str,
    collapsed_file: &'static str,
    group_y: f32,
}

#[cfg(target_os = "macos")]
fn capture_restored_subscription_view(
    cx: &mut gpui::VisualTestAppContext,
    store: &std::path::Path,
    view: RestoredSubscriptionView,
) -> Result<(), Box<dyn std::error::Error>> {
    use gpui::{AnyWindowHandle, Modifiers, point, px, size};
    use manis_ui::ManisApp;
    use std::time::Duration;

    let window_store = store.to_owned();
    let window =
        cx.open_offscreen_window(size(px(view.width), px(view.height)), |window, cx| {
            manis_root(window, cx, |_| {
                ManisApp::with_fixture_controller_and_subscription_store(
                    "http://127.0.0.1:9090",
                    window_store,
                )
            })
        })?;
    let window: AnyWindowHandle = window.into();
    refresh(cx, window)?;
    open_workspace(cx, window, view.width, SnapshotWorkspace::Configuration)?;
    settle_ui_for(cx, window, Duration::from_secs(1))?;
    scroll_window(cx, window, view.width - 100.0, view.height - 120.0, -360.0)?;
    save_screenshot(cx, window, view.configuration_file)?;
    open_workspace(cx, window, view.width, SnapshotWorkspace::Nodes)?;
    save_screenshot(cx, window, view.nodes_file)?;
    cx.simulate_click(
        window,
        point(
            px(if view.width >= 1_280.0 {
                1_360.0
            } else {
                660.0
            }),
            px(view.group_y),
        ),
        Modifiers::none(),
    );
    refresh(cx, window)?;
    save_screenshot(cx, window, view.collapsed_file)?;
    close_window(cx, window)
}
