use super::scenarios::{
    capture, capture_app_updates, capture_appearance, capture_automatic_policy, capture_buttons,
    capture_compact_flow, capture_configuration, capture_configuration_sections,
    capture_configuration_transfer, capture_connected, capture_data_page_coverage,
    capture_live_when_configured, capture_localization, capture_log_colors,
    capture_managed_policy_settings, capture_medium_sheet, capture_merged_nodes,
    capture_navigation_icons, capture_nodes_toolbar, capture_proxy_candidate,
    capture_remote_subscription_preview, capture_routing_rules, capture_source_cards,
    capture_stream_status,
};

#[cfg(target_os = "macos")]
pub(super) fn run() -> Result<(), Box<dyn std::error::Error>> {
    let platform = gpui_platform::current_platform(false);
    let mut cx = gpui::VisualTestAppContext::with_asset_source(
        platform,
        std::sync::Arc::new(manis_ui::Assets),
    );
    cx.update(manis_ui::init);
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
    if std::env::args().any(|argument| argument == "--routing-rules") {
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
    capture(&mut cx, 1420.0, 900.0, "native-wide.png")?;
    capture_automatic_policy(&mut cx)?;
    capture_managed_policy_settings(&mut cx)?;
    capture(&mut cx, 1060.0, 800.0, "native-medium.png")?;
    capture(&mut cx, 720.0, 720.0, "native-compact.png")?;
    capture_configuration(&mut cx, 1420.0, 900.0, "configuration-wide.png")?;
    capture_configuration(&mut cx, 1060.0, 800.0, "configuration-medium.png")?;
    capture_configuration(&mut cx, 720.0, 720.0, "configuration-compact.png")?;
    capture_localization(
        &mut cx,
        1420.0,
        900.0,
        "en",
        "localization-english-wide.png",
    )?;
    capture_localization(
        &mut cx,
        720.0,
        720.0,
        "zh-CN",
        "localization-chinese-compact.png",
    )?;
    capture_routing_rules(&mut cx)?;
    capture_remote_subscription_preview(&mut cx)?;
    capture_compact_flow(&mut cx)?;
    cx.update(manis_ui::init);
    capture_medium_sheet(&mut cx)?;
    capture_connected(&mut cx)?;
    capture_data_page_coverage(&mut cx)?;
    capture_live_when_configured(&mut cx)?;
    Ok(())
}

#[cfg(target_os = "macos")]
#[derive(Clone, Copy)]
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
    use gpui::{Modifiers, point, px};

    let x = if width >= 1_280.0 { 110.0 } else { 30.0 };
    let y = match workspace {
        SnapshotWorkspace::Nodes => 76.0,
        SnapshotWorkspace::RoutingRules => 117.0,
        SnapshotWorkspace::Activity => 158.0,
        SnapshotWorkspace::Logs => 199.0,
        SnapshotWorkspace::Configuration => 240.0,
    };
    cx.simulate_click(window, point(px(x), px(y)), Modifiers::none());
    refresh(cx, window)
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
    use gpui::{Modifiers, ScrollDelta, ScrollWheelEvent, point, px};

    cx.simulate_event(
        window,
        ScrollWheelEvent {
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
