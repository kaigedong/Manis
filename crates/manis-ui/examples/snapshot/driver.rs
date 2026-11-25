use super::scenarios::{
    capture, capture_app_updates, capture_appearance, capture_automatic_policy, capture_buttons,
    capture_compact_flow, capture_configuration, capture_configuration_sections,
    capture_configuration_transfer, capture_connected, capture_data_page_coverage,
    capture_live_when_configured, capture_localization, capture_log_colors,
    capture_managed_policy_settings, capture_medium_sheet, capture_merged_nodes,
    capture_navigation_icons, capture_nodes_toolbar, capture_proxy_candidate,
    capture_remote_subscription_preview, capture_routing_rules, capture_source_cards,
    capture_stream_status, capture_utility_windows,
};

#[cfg(target_os = "macos")]
pub(super) fn run() -> Result<(), Box<dyn std::error::Error>> {
    let platform = gpui_platform::current_platform(false);
    let mut cx = gpui::VisualTestAppContext::with_asset_source(
        platform,
        std::sync::Arc::new(manis_ui::Assets),
    );
    cx.update(manis_ui::init);
    if std::env::args().any(|argument| argument == "--utility-windows") {
        capture_utility_windows(&mut cx)?;
        if std::env::args().any(|argument| argument == "--routing-rules-review") {
            return capture_routing_rules(&mut cx);
        }
        return Ok(());
    }
    if std::env::args().any(|argument| argument == "--chrome") {
        capture(&mut cx, 900.0, 800.0, "chrome-900.png")?;
        capture(&mut cx, 1060.0, 800.0, "chrome-1060.png")?;
        capture(&mut cx, 1280.0, 800.0, "chrome-1280.png")?;
        capture(&mut cx, 1420.0, 900.0, "chrome-1420.png")?;
        capture(&mut cx, 1600.0, 900.0, "chrome-1600.png")?;
        return Ok(());
    }
    if std::env::args().any(|argument| argument == "--app-updates") {
        return capture_app_updates(&mut cx);
    }
    if std::env::args().any(|argument| argument == "--buttons") {
        return capture_buttons(&mut cx);
    }
    if std::env::args().any(|argument| argument == "--source-cards") {
        return capture_source_cards(&mut cx);
    }
    if std::env::args().any(|argument| argument == "--policy-scrolling") {
        return capture_merged_nodes(&mut cx);
    }
    if std::env::args().any(|argument| argument == "--proxy-candidate") {
        return capture_proxy_candidate(&mut cx);
    }
    if std::env::args().any(|argument| argument == "--navigation-icons") {
        return capture_navigation_icons(&mut cx);
    }
    if std::env::args().any(|argument| argument == "--configuration-transfer") {
        return capture_configuration_transfer(&mut cx);
    }
    if std::env::args().any(|argument| argument == "--stream-status") {
        return capture_stream_status(&mut cx);
    }
    if std::env::args().any(|argument| argument == "--nodes-toolbar") {
        return capture_nodes_toolbar(&mut cx);
    }
    if std::env::args().any(|argument| argument == "--merged-nodes") {
        return capture_merged_nodes(&mut cx);
    }
    if std::env::args().any(|argument| argument == "--log-colors") {
        return capture_log_colors(&mut cx);
    }
    if std::env::args().any(|argument| argument == "--appearance") {
        return capture_appearance(&mut cx);
    }
    if std::env::args().any(|argument| argument == "--policy-settings") {
        return capture_managed_policy_settings(&mut cx);
    }
    if std::env::args().any(|argument| argument == "--automatic-policy") {
        return capture_automatic_policy(&mut cx);
    }
    if std::env::args()
        .any(|argument| argument == "--routing-rules" || argument == "--routing-rules-review")
    {
        return capture_routing_rules(&mut cx);
    }
    if std::env::args().any(|argument| argument == "--medium-sheet") {
        return capture_medium_sheet(&mut cx);
    }
    if std::env::args().any(|argument| argument == "--data-pages") {
        return capture_data_page_coverage(&mut cx);
    }
    if std::env::args().any(|argument| argument == "--connected") {
        return capture_connected(&mut cx);
    }
    if std::env::args().any(|argument| argument == "--configuration-sections") {
        return capture_configuration_sections(&mut cx);
    }
    if std::env::args().any(|argument| argument == "--compact-flow") {
        return capture_compact_flow(&mut cx);
    }
    capture_default(&mut cx)
}

/// Starts the managed kernel from the nodes page's own connect action.
///
/// The command bar's 系统代理 segment cannot complete inside the visual-test sandbox, so the
/// captures connect the controller the same way a user does: the trailing action beside the
/// policy-group header, measured per breakpoint.
#[cfg(target_os = "macos")]
pub(super) fn connect_proxy(
    cx: &mut gpui::VisualTestAppContext,
    window: gpui::AnyWindowHandle,
    width: f32,
) -> Result<(), Box<dyn std::error::Error>> {
    use gpui::{Modifiers, point, px};

    let x = if wide_rail(width) {
        1_237.0
    } else if width >= 900.0 {
        877.0
    } else {
        469.0
    };
    cx.simulate_click(window, point(px(x), px(100.0)), Modifiers::none());
    refresh(cx, window)
}

#[cfg(target_os = "macos")]
fn capture_default(cx: &mut gpui::VisualTestAppContext) -> Result<(), Box<dyn std::error::Error>> {
    capture(cx, 1420.0, 900.0, "native-wide.png")?;
    capture_automatic_policy(cx)?;
    capture_managed_policy_settings(cx)?;
    capture(cx, 1060.0, 800.0, "native-medium.png")?;
    capture(cx, 720.0, 720.0, "native-compact.png")?;
    capture_configuration(cx, 1420.0, 900.0, "configuration-wide.png")?;
    capture_configuration(cx, 1060.0, 800.0, "configuration-medium.png")?;
    capture_configuration(cx, 720.0, 720.0, "configuration-compact.png")?;
    capture_localization(cx, 1420.0, 900.0, "en", "localization-english-wide.png")?;
    capture_localization(
        cx,
        720.0,
        720.0,
        "zh-CN",
        "localization-chinese-compact.png",
    )?;
    capture_routing_rules(cx)?;
    capture_remote_subscription_preview(cx)?;
    capture_compact_flow(cx)?;
    cx.update(manis_ui::init);
    capture_medium_sheet(cx)?;
    capture_connected(cx)?;
    capture_data_page_coverage(cx)?;
    capture_live_when_configured(cx)?;
    Ok(())
}

/// True when the shell renders the expanded 232px rail.
#[cfg(target_os = "macos")]
pub(super) const fn wide_rail(width: f32) -> bool {
    width >= 1_280.0
}

/// The rail's theme button, centred in the pinned bottom tool block of the Manis 2.0 shell.
///
/// Wide windows put the settings row and the theme button on one row; narrower rails stack
/// them, which is why the button keeps its y while the settings row moves up.
#[cfg(target_os = "macos")]
pub(super) fn theme_toggle_point(width: f32, height: f32) -> gpui::Point<gpui::Pixels> {
    let x = if wide_rail(width) {
        208.0
    } else if width >= 900.0 {
        44.0
    } else {
        32.0
    };
    gpui::point(gpui::px(x), gpui::px(height - 24.0))
}

/// The settings row sits beside the theme button when the rail is wide, above it otherwise.
#[cfg(target_os = "macos")]
pub(super) fn settings_row_point(width: f32, height: f32) -> gpui::Point<gpui::Pixels> {
    let x = if wide_rail(width) { 110.0 } else { 30.0 };
    let y = if wide_rail(width) {
        height - 25.0
    } else {
        height - 62.0
    };
    gpui::point(gpui::px(x), gpui::px(y))
}

/// Clicks the rail theme button for the window's own size and settles the repaint.
#[cfg(target_os = "macos")]
pub(super) fn toggle_theme(
    cx: &mut gpui::VisualTestAppContext,
    window: gpui::AnyWindowHandle,
    width: f32,
) -> Result<(), Box<dyn std::error::Error>> {
    use gpui::Modifiers;

    let height = cx.update_window(window, |_, window, _| {
        window.viewport_size().height.as_f32()
    })?;
    cx.simulate_click(window, theme_toggle_point(width, height), Modifiers::none());
    refresh(cx, window)
}

#[cfg(target_os = "macos")]
#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) enum SnapshotWorkspace {
    Nodes,
    RoutingRules,
    Activity,
    Logs,
    Configuration,
}

#[cfg(target_os = "macos")]
pub(super) fn open_workspace(
    cx: &mut gpui::VisualTestAppContext,
    window: gpui::AnyWindowHandle,
    width: f32,
    workspace: SnapshotWorkspace,
) -> Result<(), Box<dyn std::error::Error>> {
    use gpui::Modifiers;

    let height = cx.update_window(window, |_, window, _| {
        window.viewport_size().height.as_f32()
    })?;
    let point = if workspace == SnapshotWorkspace::Configuration {
        settings_row_point(width, height)
    } else {
        workspace_row_point(width, workspace)
    };
    cx.simulate_click(window, point, Modifiers::none());
    refresh(cx, window)
}

/// A workspace row's centre inside the rail.
///
/// A wide rail starts its list directly under the 56px brand row; narrower rails reserve the
/// macOS traffic-light inset above it, so the same row lands 38px lower.
#[cfg(target_os = "macos")]
pub(super) fn workspace_row_point(
    width: f32,
    workspace: SnapshotWorkspace,
) -> gpui::Point<gpui::Pixels> {
    let x = if wide_rail(width) { 110.0 } else { 30.0 };
    let (first_row, row_step) = if wide_rail(width) {
        (82.0, 39.0)
    } else {
        (120.0, 39.0)
    };
    let row = match workspace {
        SnapshotWorkspace::Nodes => 0,
        SnapshotWorkspace::RoutingRules => 1,
        SnapshotWorkspace::Activity => 2,
        SnapshotWorkspace::Logs => 3,
        SnapshotWorkspace::Configuration => 0,
    };
    gpui::point(gpui::px(x), gpui::px(first_row + row_step * row as f32))
}

#[cfg(target_os = "macos")]
pub(super) fn open_policy_section(
    cx: &mut gpui::VisualTestAppContext,
    window: gpui::AnyWindowHandle,
    width: f32,
) -> Result<(), Box<dyn std::error::Error>> {
    open_workspace(cx, window, width, SnapshotWorkspace::Nodes)?;
    scroll_window(cx, window, width - 50.0, 300.0, -220.0)
}

#[cfg(target_os = "macos")]
pub(super) fn refresh(
    cx: &mut gpui::VisualTestAppContext,
    window: gpui::AnyWindowHandle,
) -> Result<(), Box<dyn std::error::Error>> {
    cx.run_until_parked();
    cx.update_window(window, |_, window, _| window.refresh())?;
    cx.run_until_parked();
    Ok(())
}

#[cfg(target_os = "macos")]
pub(super) fn settle_ui_animation(
    cx: &mut gpui::VisualTestAppContext,
    window: gpui::AnyWindowHandle,
) -> Result<(), Box<dyn std::error::Error>> {
    settle_ui_for(cx, window, std::time::Duration::from_millis(300))
}

#[cfg(target_os = "macos")]
pub(super) fn settle_ui_for(
    cx: &mut gpui::VisualTestAppContext,
    window: gpui::AnyWindowHandle,
    duration: std::time::Duration,
) -> Result<(), Box<dyn std::error::Error>> {
    const FRAME: std::time::Duration = std::time::Duration::from_millis(25);
    let frames = duration.as_millis().div_ceil(FRAME.as_millis());
    for _ in 0..frames {
        cx.advance_clock(FRAME);
        refresh(cx, window)?;
    }
    Ok(())
}

#[cfg(target_os = "macos")]
pub(super) fn scroll_window(
    cx: &mut gpui::VisualTestAppContext,
    window: gpui::AnyWindowHandle,
    x: f32,
    y: f32,
    delta_y: f32,
) -> Result<(), Box<dyn std::error::Error>> {
    use gpui::{Modifiers, ScrollDelta, ScrollEvent, point, px};

    cx.simulate_event(
        window,
        ScrollEvent {
            position: point(px(x), px(y)),
            delta: ScrollDelta::Pixels(point(px(0.0), px(delta_y))),
            modifiers: Modifiers::none(),
            ..Default::default()
        },
    );
    refresh(cx, window)
}

#[cfg(target_os = "macos")]
pub(super) fn close_window(
    cx: &mut gpui::VisualTestAppContext,
    window: gpui::AnyWindowHandle,
) -> Result<(), Box<dyn std::error::Error>> {
    cx.update_window(window, |_, window, _| window.remove_window())?;
    cx.run_until_parked();
    Ok(())
}

#[cfg(target_os = "macos")]
pub(super) fn save_screenshot(
    cx: &mut gpui::VisualTestAppContext,
    window: gpui::AnyWindowHandle,
    file_name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    use std::path::PathBuf;

    let screenshot = cx.capture_screenshot(window)?;
    let output = PathBuf::from("target/manis-snapshots").join(file_name);
    if screenshot.pixels().any(|pixel| pixel.0[3] != 255) {
        return Err(format!("{file_name}: application content must be fully opaque").into());
    }
    std::fs::create_dir_all("target/manis-snapshots")?;
    screenshot.save(&output)?;
    println!("saved {}", output.display());
    Ok(())
}

#[cfg(target_os = "macos")]
pub(super) fn save_screenshot_at(
    cx: &mut gpui::VisualTestAppContext,
    window: gpui::AnyWindowHandle,
    output: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let screenshot = cx.capture_screenshot(window)?;
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    screenshot.save(&output)?;
    println!("saved {}", output.display());
    Ok(())
}
