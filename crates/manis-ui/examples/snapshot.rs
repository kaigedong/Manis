#[cfg(target_os = "macos")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
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
fn verify_secondary_click(
    cx: &mut gpui::VisualTestAppContext,
    window: gpui::AnyWindowHandle,
    position: gpui::Point<gpui::Pixels>,
    screenshot: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    use gpui::{Modifiers, MouseButton};
    cx.simulate_mouse_move(window, position, None, Modifiers::none());
    refresh(cx, window)?;
    let hover = cx.capture_screenshot(window)?;
    for button in [MouseButton::Right, MouseButton::Middle] {
        cx.simulate_mouse_down(window, position, button, Modifiers::none());
        refresh(cx, window)?;
        assert_eq!(
            hover,
            cx.capture_screenshot(window)?,
            "{screenshot}: secondary press changed the page"
        );
        cx.simulate_mouse_up(window, position, button, Modifiers::none());
        refresh(cx, window)?;
        assert_eq!(
            hover,
            cx.capture_screenshot(window)?,
            "{screenshot}: secondary click changed the page"
        );
    }
    save_screenshot(cx, window, screenshot)
}

#[cfg(target_os = "macos")]
fn capture_buttons(cx: &mut gpui::VisualTestAppContext) -> Result<(), Box<dyn std::error::Error>> {
    use gpui::{AppContext as _, Modifiers, MouseButton, point, px, size};
    for (dark, mode) in [(false, "light"), (true, "dark")] {
        let window = cx
            .open_offscreen_window(size(px(640.0), px(400.0)), |window, cx| {
                let view = cx.new(|cx| manis_ui::button_gallery_fixture(dark, cx));
                cx.new(|cx| gpui_component::Root::new(view, window, cx))
            })?
            .into();
        refresh(cx, window)?;
        save_screenshot(cx, window, &format!("buttons-{mode}-normal.png"))?;
        let normal = cx.capture_screenshot(window)?;
        let scale = normal.width() / 640;
        let position = point(px(60.0), px(70.0));
        cx.simulate_mouse_move(window, position, None, Modifiers::none());
        refresh(cx, window)?;
        let hover = cx.capture_screenshot(window)?;
        assert_ne!(
            normal.get_pixel(30 * scale, 70 * scale),
            hover.get_pixel(30 * scale, 70 * scale),
            "{mode}: hover must change the button fill"
        );
        save_screenshot(cx, window, &format!("buttons-{mode}-hover.png"))?;
        for button in [MouseButton::Right, MouseButton::Middle] {
            cx.simulate_mouse_down(window, position, button, Modifiers::none());
            refresh(cx, window)?;
            let secondary = cx.capture_screenshot(window)?;
            // Ignore the loading spinner elsewhere in the gallery.
            for y in 48 * scale..96 * scale {
                for x in 20 * scale..140 * scale {
                    assert_eq!(
                        hover.get_pixel(x, y),
                        secondary.get_pixel(x, y),
                        "{mode}: {button:?} must not paint a focus ring or pressed state"
                    );
                }
            }
            cx.simulate_mouse_up(window, position, button, Modifiers::none());
        }
        save_screenshot(cx, window, &format!("buttons-{mode}-secondary-click.png"))?;
        cx.simulate_mouse_down(window, position, MouseButton::Left, Modifiers::none());
        refresh(cx, window)?;
        let pressed = cx.capture_screenshot(window)?;
        assert_ne!(
            hover.get_pixel(30 * scale, 70 * scale),
            pressed.get_pixel(30 * scale, 70 * scale),
            "{mode}: pressed must differ from hover"
        );
        save_screenshot(cx, window, &format!("buttons-{mode}-pressed.png"))?;
        cx.simulate_mouse_up(window, position, MouseButton::Left, Modifiers::none());
        cx.simulate_mouse_move(window, point(px(200.0), px(240.0)), None, Modifiers::none());
        refresh(cx, window)?;
        let disabled = cx.capture_screenshot(window)?;
        assert_eq!(
            normal.get_pixel(192 * scale, 240 * scale),
            disabled.get_pixel(192 * scale, 240 * scale),
            "{mode}: disabled buttons must not react to hover"
        );
        cx.simulate_mouse_move(window, point(px(600.0), px(380.0)), None, Modifiers::none());
        cx.simulate_keystrokes(window, "tab");
        cx.update_window(window, |_, window, cx| {
            if window.focused(cx).is_none() {
                window.focus_next(cx);
            }
            assert!(
                window.focused(cx).is_some(),
                "keyboard focus must be reachable"
            );
        })?;
        refresh(cx, window)?;
        save_screenshot(cx, window, &format!("buttons-{mode}-focus.png"))?;
        close_window(cx, window)?;
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn capture_source_cards(
    cx: &mut gpui::VisualTestAppContext,
) -> Result<(), Box<dyn std::error::Error>> {
    use gpui::{AnyWindowHandle, AppContext as _, Modifiers, point, px, size};
    let store = std::env::temp_dir().join(format!("manis-source-cards-{}", std::process::id()));
    write_source_cards_fixture(&store)?;
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
                let hover_y = match (rules, label) {
                    (true, "wide") => 400.0,
                    (false, "wide") => 295.0,
                    (true, _) => 325.0,
                    (false, _) => 330.0,
                };
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
            }
        }
        close_window(cx, window)?;
    }
    std::fs::remove_dir_all(store)?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn write_source_cards_fixture(store: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    use manis_profile::write_private_atomic;
    write_private_atomic(store, "language.preference", b"zh-CN")?;
    let subscription = [
        "manis-subscription-source-v3".to_owned(),
        "id\tsource-deadbeef".to_owned(),
        format!("name\t{}", snapshot_hex("示例订阅")),
        format!(
            "url\t{}",
            snapshot_hex("https://subscriptions.example.invalid/nodes")
        ),
        "enabled\tfalse".to_owned(),
    ]
    .join("\n");
    write_private_atomic(store, "source-deadbeef.url", subscription.as_bytes())?;
    let node = [
        "manis-single-node-source-v1".to_owned(),
        "id\tsaved-deadbeef".to_owned(),
        format!("name\t{}", snapshot_hex("家庭节点")),
        format!(
            "url\t{}",
            snapshot_hex("trojan://fixture-password@example.invalid:443?security=tls#Home")
        ),
        "enabled\ttrue".to_owned(),
    ]
    .join("\n");
    write_private_atomic(store, "saved-deadbeef.vless", node.as_bytes())?;
    let rules = [
        "manis-qx-rule-source-v1".to_owned(),
        "id\tqx-rule-deadbeef".to_owned(),
        format!(
            "url\t{}",
            snapshot_hex("https://rules.example.invalid/media.list")
        ),
        format!("target\t{}", snapshot_hex("Proxy")),
        format!(
            "content\t{}",
            snapshot_hex("DOMAIN-SUFFIX,example.com,PROXY\n")
        ),
    ]
    .join("\n");
    write_private_atomic(store, "qx-rule-deadbeef.qxrules", rules.as_bytes())?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn capture_proxy_candidate(
    cx: &mut gpui::VisualTestAppContext,
) -> Result<(), Box<dyn std::error::Error>> {
    use gpui::{AnyWindowHandle, Modifiers, point, px, size};
    let store = std::env::temp_dir().join(format!("manis-proxy-snapshot-{}", std::process::id()));
    manis_profile::write_private_atomic(&store, "language.preference", b"zh-CN")?;
    let policy = format!(
        "manis-policy-group-v1\nid\tpolicy-deadbeef\nname\t{}\nicon\tnone\nstrategy\tmanual\ninterval\t600\nmatcher\texplicit\nfilter\t\nmember\t{}\t{}",
        snapshot_hex("跟随首页"),
        snapshot_hex("builtin"),
        snapshot_hex("PROXY"),
    );
    manis_profile::write_private_atomic(&store, "policy-deadbeef.policy", policy.as_bytes())?;
    for (width, height, label) in [(1420.0, 900.0, "wide"), (640.0, 560.0, "compact")] {
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
        open_policy_section(cx, window, width)?;
        cx.simulate_click(window, point(px(320.0), px(172.0)), Modifiers::none());
        refresh(cx, window)?;
        save_screenshot(cx, window, &format!("proxy-candidate-{label}-row.png"))?;
        for (dark, mode) in [(false, "light"), (true, "dark")] {
            if dark {
                let toggle_x = width - if width >= 1280.0 { 550.0 } else { 205.0 };
                cx.simulate_click(window, point(px(toggle_x), px(24.0)), Modifiers::none());
                refresh(cx, window)?;
            }
            cx.simulate_click(
                window,
                point(px(width - 132.0), px(172.0)),
                Modifiers::none(),
            );
            refresh(cx, window)?;
            if label == "compact" {
                scroll_window(cx, window, width - 90.0, height - 180.0, -420.0)?;
            }
            save_screenshot(
                cx,
                window,
                &format!("proxy-candidate-{label}-{mode}-editor.png"),
            )?;
            cx.simulate_click(
                window,
                point(
                    px(width / 2.0 + 100.0),
                    px(if label == "wide" { 507.0 } else { 355.0 }),
                ),
                Modifiers::none(),
            );
            refresh(cx, window)?;
            save_screenshot(
                cx,
                window,
                &format!("proxy-candidate-{label}-{mode}-picker.png"),
            )?;
            cx.simulate_keystrokes(window, "escape");
            refresh(cx, window)?;
            cx.simulate_keystrokes(window, "escape");
            refresh(cx, window)?;
        }
        close_window(cx, window)?;
    }
    std::fs::remove_dir_all(store)?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn capture_navigation_icons(
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

#[cfg(target_os = "macos")]
fn capture_app_updates(
    cx: &mut gpui::VisualTestAppContext,
) -> Result<(), Box<dyn std::error::Error>> {
    use gpui::{AnyWindowHandle, AppContext as _, Modifiers, point, px, size};
    let root = std::env::temp_dir().join(format!("manis-update-snapshot-{}", std::process::id()));
    let store = root.join("subscriptions");
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
        let app = app.ok_or("missing update fixture")?;
        let window: AnyWindowHandle = window.into();
        refresh(cx, window)?;
        open_workspace(cx, window, width, SnapshotWorkspace::Configuration)?;
        for (failed, suffix) in [(false, "available"), (true, "failed")] {
            cx.update_window(window, |_, _, cx| {
                app.update(cx, |app, cx| app.show_app_update_fixture(failed, cx));
            })?;
            refresh(cx, window)?;
            save_screenshot(cx, window, &format!("app-updates-{label}-{suffix}.png"))?;
        }
        let toggle_x = width - if width >= 1280.0 { 550.0 } else { 205.0 };
        cx.simulate_click(window, point(px(toggle_x), px(24.0)), Modifiers::none());
        refresh(cx, window)?;
        save_screenshot(cx, window, &format!("app-updates-{label}-dark.png"))?;
        cx.update_window(window, |_, window, cx| {
            app.update(cx, |app, cx| app.show_about_fixture(window, cx));
        })?;
        settle_ui_animation(cx, window)?;
        save_screenshot(cx, window, &format!("about-{label}-dark.png"))?;
        cx.simulate_keystrokes(window, "escape");
        settle_ui_animation(cx, window)?;
        cx.simulate_click(window, point(px(toggle_x), px(24.0)), Modifiers::none());
        refresh(cx, window)?;
        cx.update_window(window, |_, window, cx| {
            app.update(cx, |app, cx| app.show_about_fixture(window, cx));
        })?;
        settle_ui_animation(cx, window)?;
        save_screenshot(cx, window, &format!("about-{label}.png"))?;
        close_window(cx, window)?;
    }
    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn capture_configuration_transfer(
    cx: &mut gpui::VisualTestAppContext,
) -> Result<(), Box<dyn std::error::Error>> {
    use gpui::{AnyWindowHandle, AppContext as _, Modifiers, point, px, size};
    let root = std::env::temp_dir().join(format!("manis-transfer-snapshot-{}", std::process::id()));
    let store = root.join("subscriptions");
    write_managed_policy_fixture(&store)?;
    manis_profile::write_private_atomic(&store, "language.preference", b"zh-CN")?;
    let backup = serde_json::json!({
        "schema": "manis.configuration-backup", "version": 1, "created_unix_secs": 0,
        "files": [{"name": "policy-deadbeef.policy", "contents": std::fs::read_to_string(store.join("policy-deadbeef.policy"))?}]
    }).to_string();
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
        let app = app.ok_or("missing fixture app")?;
        let window: AnyWindowHandle = window.into();
        refresh(cx, window)?;
        open_workspace(cx, window, width, SnapshotWorkspace::Configuration)?;
        if label == "compact" {
            scroll_window(cx, window, width - 60.0, height - 100.0, -210.0)?;
        }
        save_screenshot(cx, window, &format!("configuration-transfer-{label}.png"))?;
        cx.update_window(window, |_, window, cx| {
            app.update(cx, |app, cx| {
                app.show_configuration_backup_fixture(&backup, window, cx);
            });
        })?;
        settle_ui_animation(cx, window)?;
        save_screenshot(
            cx,
            window,
            &format!("configuration-transfer-{label}-preview.png"),
        )?;
        cx.simulate_keystrokes(window, "escape");
        settle_ui_animation(cx, window)?;
        let toggle_x = width - if width >= 1280.0 { 550.0 } else { 205.0 };
        cx.simulate_click(window, point(px(toggle_x), px(24.0)), Modifiers::none());
        refresh(cx, window)?;
        cx.update_window(window, |_, window, cx| {
            app.update(cx, |app, cx| {
                app.show_configuration_backup_fixture(&backup, window, cx);
            });
        })?;
        settle_ui_animation(cx, window)?;
        save_screenshot(
            cx,
            window,
            &format!("configuration-transfer-{label}-preview-dark.png"),
        )?;
        capture_configuration_editor(cx, window, &app, label)?;
        close_window(cx, window)?;
    }
    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn capture_configuration_editor(
    cx: &mut gpui::VisualTestAppContext,
    window: gpui::AnyWindowHandle,
    app: &gpui::Entity<manis_ui::ManisApp>,
    label: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    cx.simulate_keystrokes(window, "escape");
    settle_ui_animation(cx, window)?;
    cx.update_window(window, |_, window, cx| {
        app.update(cx, |app, cx| {
            app.show_configuration_editor_fixture(window, cx);
        });
    })?;
    settle_ui_animation(cx, window)?;
    save_screenshot(
        cx,
        window,
        &format!("configuration-editor-{label}-dark.png"),
    )?;
    cx.simulate_keystrokes(window, "escape");
    settle_ui_animation(cx, window)?;
    let width = if label == "wide" { 1420.0 } else { 640.0 };
    let toggle_x = width - if width >= 1280.0 { 550.0 } else { 205.0 };
    cx.simulate_click(
        window,
        gpui::point(gpui::px(toggle_x), gpui::px(24.0)),
        gpui::Modifiers::none(),
    );
    refresh(cx, window)?;
    cx.update_window(window, |_, window, cx| {
        app.update(cx, |app, cx| {
            app.show_configuration_editor_fixture(window, cx);
        });
    })?;
    settle_ui_animation(cx, window)?;
    save_screenshot(cx, window, &format!("configuration-editor-{label}.png"))?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn capture_stream_status(
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
fn capture_nodes_toolbar(
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
fn capture_merged_nodes(
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
fn capture_log_colors(
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
fn capture_appearance(
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
fn assert_appearance_mode(
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
fn manis_root(
    window: &mut gpui::Window,
    cx: &mut gpui::App,
    build: impl FnOnce(&mut gpui::Context<manis_ui::ManisApp>) -> manis_ui::ManisApp + 'static,
) -> gpui::Entity<gpui_component::Root> {
    use gpui::AppContext as _;

    let app = cx.new(build);
    cx.new(|cx| manis_ui::root(app, window, cx))
}

#[cfg(target_os = "macos")]
fn capture_managed_policy_settings(
    cx: &mut gpui::VisualTestAppContext,
) -> Result<(), Box<dyn std::error::Error>> {
    use gpui::{AnyWindowHandle, Modifiers, point, px, size};
    use manis_ui::ManisApp;
    use std::os::unix::fs::PermissionsExt;

    let root = std::env::temp_dir().join(format!("manis-policy-settings-{}", std::process::id()));
    if root.exists() {
        std::fs::remove_dir_all(&root)?;
    }
    let store = root.join("subscriptions");
    std::fs::create_dir_all(&store)?;
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700))?;
    std::fs::set_permissions(&store, std::fs::Permissions::from_mode(0o700))?;
    let group_path = store.join("policy-deadbeef.policy");
    std::fs::write(
        &group_path,
        format!(
            "manis-policy-group-v1\nid\tpolicy-deadbeef\nname\t{}\nicon\tnone\nstrategy\tlatency\ninterval\t600\nmatcher\tall\nfilter\t\n",
            snapshot_hex("AI 自动选择")
        ),
    )?;
    std::fs::set_permissions(&group_path, std::fs::Permissions::from_mode(0o600))?;

    let (endpoint, server) = spawn_mihomo_fixture()?;
    for (width, height, label) in [(1420.0, 900.0, "wide"), (640.0, 560.0, "compact")] {
        cx.update(manis_ui::init);
        let window_store = store.clone();
        let fixture_endpoint = endpoint.clone();
        let window = cx.open_offscreen_window(size(px(width), px(height)), |window, cx| {
            manis_root(window, cx, |_| {
                ManisApp::with_fixture_controller_and_subscription_store(
                    fixture_endpoint,
                    window_store,
                )
            })
        })?;
        let window: AnyWindowHandle = window.into();
        refresh(cx, window)?;
        save_screenshot(cx, window, &format!("policy-home-{label}.png"))?;
        cx.simulate_click(window, point(px(width - 80.0), px(90.0)), Modifiers::none());
        settle_ui_for(cx, window, std::time::Duration::from_millis(600))?;
        open_policy_section(cx, window, width)?;
        save_screenshot(cx, window, &format!("policy-flat-{label}-collapsed.png"))?;
        cx.simulate_click(window, point(px(320.0), px(172.0)), Modifiers::none());
        refresh(cx, window)?;
        save_screenshot(cx, window, &format!("policy-flat-{label}-expanded.png"))?;
        cx.simulate_click(
            window,
            point(px(width - 132.0), px(172.0)),
            Modifiers::none(),
        );
        refresh(cx, window)?;
        save_screenshot(cx, window, &format!("policy-settings-{label}-dialog.png"))?;
        if label == "compact" {
            scroll_window(cx, window, width - 90.0, height - 180.0, -480.0)?;
            save_screenshot(cx, window, "policy-tolerance-compact.png")?;
        } else {
            cx.simulate_click(window, point(px(900.0), px(575.0)), Modifiers::none());
            refresh(cx, window)?;
            save_screenshot(cx, window, "policy-tolerance-menu.png")?;
            cx.simulate_keystrokes(window, "escape");
            refresh(cx, window)?;
        }
        cx.simulate_keystrokes(window, "escape");
        refresh(cx, window)?;
        let toggle_x = width - if width >= 1280.0 { 550.0 } else { 205.0 };
        cx.simulate_click(window, point(px(toggle_x), px(24.0)), Modifiers::none());
        refresh(cx, window)?;
        save_screenshot(cx, window, &format!("policy-flat-{label}-dark.png"))?;
        close_window(cx, window)?;
    }

    server.stop()?;
    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn capture_automatic_policy(
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
    open_policy_section(cx, window, 1_420.0)?;
    save_screenshot(cx, window, "native-wide-policy-groups-collapsed.png")?;
    cx.simulate_click(window, point(px(380.0), px(172.0)), Modifiers::none());
    refresh(cx, window)?;
    save_screenshot(cx, window, "native-wide-policy-groups-expanded.png")?;
    cx.simulate_click(window, point(px(268.0), px(172.0)), Modifiers::none());
    settle_ui_for(cx, window, std::time::Duration::from_millis(600))?;
    save_screenshot(cx, window, "native-wide-policy-groups-tested.png")?;
    close_window(cx, window)?;
    server.stop()
}

#[cfg(target_os = "macos")]
fn capture_remote_subscription_preview(
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
struct SubscriptionFixtureServer {
    url: String,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    thread: std::thread::JoinHandle<std::io::Result<()>>,
}

#[cfg(target_os = "macos")]
impl SubscriptionFixtureServer {
    fn start() -> std::io::Result<Self> {
        use std::net::TcpListener;
        use std::sync::Arc;
        use std::sync::atomic::AtomicBool;

        let listener = TcpListener::bind("127.0.0.1:0")?;
        listener.set_nonblocking(true)?;
        let url = format!(
            "http://{}/subscription?name=Fixture%20Transit",
            listener.local_addr()?
        );
        let stop = Arc::new(AtomicBool::new(false));
        let server_stop = stop.clone();
        let thread =
            std::thread::spawn(move || serve_subscription_fixture(&listener, &server_stop));
        Ok(Self { url, stop, thread })
    }

    fn url(&self) -> &str {
        &self.url
    }

    fn stop(self) -> Result<(), Box<dyn std::error::Error>> {
        use std::sync::atomic::Ordering;

        self.stop.store(true, Ordering::Relaxed);
        self.thread
            .join()
            .map_err(|_| "fixture server panicked")??;
        Ok(())
    }
}

#[cfg(target_os = "macos")]
fn serve_subscription_fixture(
    listener: &std::net::TcpListener,
    stop: &std::sync::atomic::AtomicBool,
) -> std::io::Result<()> {
    use std::io::{BufRead, BufReader, Write};
    use std::sync::atomic::Ordering;
    use std::time::Duration;

    const BODY: &str = r#"proxies:
  - name: "Tokyo Edge"
    type: ss
    server: 127.0.0.1
    port: 443
    cipher: aes-128-gcm
    password: fixture-alpha
  - name: "Singapore Core"
    type: ss
    server: 127.0.0.1
    port: 8443
    cipher: aes-128-gcm
    password: fixture-beta
"#;
    while !stop.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((mut stream, _)) => {
                let mut request_line = String::new();
                BufReader::new(stream.try_clone()?).read_line(&mut request_line)?;
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/yaml\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{BODY}",
                    BODY.len()
                );
                stream.write_all(response.as_bytes())?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn write_managed_policy_fixture(store: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::PermissionsExt;

    std::fs::create_dir_all(store)?;
    std::fs::set_permissions(store, std::fs::Permissions::from_mode(0o700))?;
    let path = store.join("policy-deadbeef.policy");
    std::fs::write(
        &path,
        concat!(
            "manis-policy-group-v1\n",
            "id\tpolicy-deadbeef\n",
            "name\t46697874757265204175746f\n",
            "icon\tbolt\n",
            "strategy\tlatency\n",
            "matcher\tall\n",
            "filter\t"
        ),
    )?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
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

#[cfg(target_os = "macos")]
fn capture_configuration(
    cx: &mut gpui::VisualTestAppContext,
    width: f32,
    height: f32,
    file_name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    use gpui::{AnyWindowHandle, Modifiers, point, px, size};
    use manis_ui::ManisApp;

    let window = cx.open_offscreen_window(size(px(width), px(height)), |window, cx| {
        manis_root(window, cx, |_| {
            ManisApp::with_fixture_controller("http://127.0.0.1:9090")
        })
    })?;
    let window: AnyWindowHandle = window.into();

    refresh(cx, window)?;
    open_workspace(cx, window, width, SnapshotWorkspace::Configuration)?;
    save_screenshot(cx, window, file_name)?;

    if width >= 1_280.0 {
        scroll_window(cx, window, 1_300.0, 760.0, -600.0)?;
        cx.simulate_click(window, point(px(700.0), px(510.0)), Modifiers::none());
        refresh(cx, window)?;
        save_screenshot(cx, window, "configuration-wide-subscription-focused.png")?;
        cx.simulate_input(
            window,
            "vless://00000000-0000-4000-8000-000000000000@edge.example.invalid:443?security=tls&type=ws#Tokyo%20Edge",
        );
        refresh(cx, window)?;
        cx.simulate_click(window, point(px(700.0), px(560.0)), Modifiers::none());
        refresh(cx, window)?;
        save_screenshot(cx, window, "configuration-wide-subscription-preview.png")?;
    }

    if (width - 720.0).abs() < f32::EPSILON {
        scroll_window(cx, window, 650.0, 620.0, -1_200.0)?;
        cx.simulate_click(window, point(px(420.0), px(420.0)), Modifiers::none());
        refresh(cx, window)?;
        cx.simulate_input(
            window,
            "vless://00000000-0000-4000-8000-000000000000@edge.example.invalid:443?security=tls&type=ws#Tokyo%20Edge",
        );
        refresh(cx, window)?;
        cx.simulate_click(window, point(px(500.0), px(485.0)), Modifiers::none());
        refresh(cx, window)?;
        save_screenshot(cx, window, "configuration-compact-subscription-preview.png")?;
    }
    close_window(cx, window)
}

#[cfg(target_os = "macos")]
fn capture_configuration_sections(
    cx: &mut gpui::VisualTestAppContext,
) -> Result<(), Box<dyn std::error::Error>> {
    use gpui::{AnyWindowHandle, Modifiers, point, px, size};
    use gpui_component::WindowExt as _;
    use manis_ui::ManisApp;

    let width = 1_420.0;
    let window = cx.open_offscreen_window(size(px(width), px(900.0)), |window, cx| {
        manis_root(window, cx, |_| {
            ManisApp::with_fixture_controller("http://127.0.0.1:9090")
        })
    })?;
    let window: AnyWindowHandle = window.into();
    refresh(cx, window)?;
    open_workspace(cx, window, width, SnapshotWorkspace::Configuration)?;

    save_screenshot(cx, window, "configuration-section-general.png")?;
    cx.simulate_click(window, point(px(340.0), px(295.0)), Modifiers::none());
    refresh(cx, window)?;
    save_screenshot(cx, window, "configuration-section-proxy-sources.png")?;
    cx.simulate_click(window, point(px(1_330.0), px(180.0)), Modifiers::none());
    settle_ui_animation(cx, window)?;
    save_screenshot(cx, window, "configuration-proxy-source-modal.png")?;
    cx.simulate_keystrokes(window, "escape");
    settle_ui_animation(cx, window)?;

    for (y, file_name) in [
        (177.0, "configuration-section-general.png"),
        (235.0, "configuration-section-runtime.png"),
    ] {
        cx.simulate_click(window, point(px(340.0), px(y)), Modifiers::none());
        refresh(cx, window)?;
        save_screenshot(cx, window, file_name)?;
    }
    cx.simulate_click(window, point(px(340.0), px(350.0)), Modifiers::none());
    refresh(cx, window)?;
    save_screenshot(cx, window, "configuration-section-rule-sources.png")?;
    cx.simulate_click(window, point(px(1_330.0), px(220.0)), Modifiers::none());
    settle_ui_animation(cx, window)?;
    if !cx.update_window(window, |_, window, cx| window.has_active_dialog(cx))? {
        return Err("rule source fixture did not open the editor dialog".into());
    }
    save_screenshot(cx, window, "configuration-rule-source-modal.png")?;
    cx.simulate_keystrokes(window, "escape");
    settle_ui_animation(cx, window)?;

    cx.simulate_click(window, point(px(340.0), px(410.0)), Modifiers::none());
    refresh(cx, window)?;
    save_screenshot(cx, window, "configuration-section-advanced.png")?;
    close_window(cx, window)?;

    for (width, height, label) in [(1060.0, 800.0, "medium"), (640.0, 560.0, "compact")] {
        let window = cx.open_offscreen_window(size(px(width), px(height)), |window, cx| {
            manis_root(window, cx, |_| {
                ManisApp::with_fixture_controller("http://127.0.0.1:9090")
            })
        })?;
        let window: AnyWindowHandle = window.into();
        refresh(cx, window)?;
        open_workspace(cx, window, width, SnapshotWorkspace::Configuration)?;
        save_screenshot(cx, window, &format!("configuration-{label}-general.png"))?;
        scroll_window(cx, window, width - 60.0, height - 100.0, -440.0)?;
        settle_ui_animation(cx, window)?;
        save_screenshot(cx, window, &format!("configuration-{label}-scrolled.png"))?;
        scroll_window(cx, window, width - 60.0, height - 100.0, -10_000.0)?;
        settle_ui_animation(cx, window)?;
        save_screenshot(cx, window, &format!("configuration-{label}-bottom.png"))?;
        close_window(cx, window)?;
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn capture_localization(
    cx: &mut gpui::VisualTestAppContext,
    width: f32,
    height: f32,
    preference: &str,
    file_name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    use gpui::{AnyWindowHandle, Modifiers, point, px, size};
    use manis_ui::ManisApp;
    use std::os::unix::fs::PermissionsExt as _;

    let fixture_root = std::env::temp_dir().join(format!(
        "manis-ui-language-snapshot-{}-{preference}",
        std::process::id()
    ));
    if fixture_root.exists() {
        std::fs::remove_dir_all(&fixture_root)?;
    }
    std::fs::create_dir_all(&fixture_root)?;
    let preference_file = fixture_root.join("language.preference");
    let initial_preference = if preference == "en" { "system" } else { "en" };
    std::fs::write(&preference_file, format!("{initial_preference}\n"))?;
    std::fs::set_permissions(&preference_file, std::fs::Permissions::from_mode(0o600))?;

    let store = fixture_root.clone();
    let window = cx.open_offscreen_window(size(px(width), px(height)), |window, cx| {
        manis_root(window, cx, |_| {
            ManisApp::with_fixture_controller_and_subscription_store("http://127.0.0.1:9090", store)
        })
    })?;
    let window: AnyWindowHandle = window.into();
    refresh(cx, window)?;
    open_workspace(cx, window, width, SnapshotWorkspace::Configuration)?;
    let language_option = if width >= 1_280.0 {
        point(px(820.0), px(270.0))
    } else {
        point(px(400.0), px(380.0))
    };
    cx.simulate_click(window, language_option, Modifiers::none());
    refresh(cx, window)?;
    let saved_preference = std::fs::read_to_string(&preference_file)?;
    if saved_preference.trim() != preference {
        return Err("language selector did not persist the requested preference".into());
    }
    save_screenshot(cx, window, file_name)?;
    close_window(cx, window)?;
    std::fs::remove_dir_all(fixture_root)?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn capture_routing_rules(
    cx: &mut gpui::VisualTestAppContext,
) -> Result<(), Box<dyn std::error::Error>> {
    use gpui::{AnyWindowHandle, Modifiers, point, px, size};
    use std::os::unix::fs::PermissionsExt;

    let root = std::env::temp_dir().join(format!(
        "manis-routing-rules-snapshot-{}",
        std::process::id()
    ));
    if root.exists() {
        std::fs::remove_dir_all(&root)?;
    }
    let store = root.join("subscriptions");
    std::fs::create_dir_all(&store)?;
    std::fs::set_permissions(&store, std::fs::Permissions::from_mode(0o700))?;
    manis_profile::write_private_atomic(&store, "language.preference", b"zh-CN")?;
    let content = concat!(
        "DOMAIN-SUFFIX,openai.com,PROXY\n",
        "DOMAIN-SUFFIX,google.com,PROXY\n",
        "DOMAIN-KEYWORD,youtube,PROXY\n",
        "DOMAIN-KEYWORD,netflix,PROXY\n",
        "DOMAIN,api.anthropic.com,PROXY\n",
        "DOMAIN-SUFFIX,github.com,PROXY\n",
        "DOMAIN-KEYWORD,telegram,PROXY\n",
        "DOMAIN-SUFFIX,wikipedia.org,PROXY\n",
    );
    let source_file = store.join("qx-rule-deadbeef.qxrules");
    std::fs::write(
        &source_file,
        [
            "manis-qx-rule-source-v1".to_owned(),
            "id\tqx-rule-deadbeef".to_owned(),
            format!(
                "url\t{}",
                snapshot_hex("https://rules.example.invalid/media.list")
            ),
            format!("target\t{}", snapshot_hex("Proxy")),
            format!("content\t{}", snapshot_hex(content)),
        ]
        .join("\n"),
    )?;
    std::fs::set_permissions(source_file, std::fs::Permissions::from_mode(0o600))?;
    manis_profile::write_private_atomic(
        &store,
        "manual-routing-rules.state",
        concat!(
            "manis.manual-routing-rules.v3\n",
            "legacy-direct-rules-migrated\t1\n",
            "rule\t1\tDIRECT\thost-suffix\tgithub.com\tdst-port\t22\n",
            "rule\t0\tProxy\thost-keyword\tgoogle",
        )
        .as_bytes(),
    )?;

    for (width, height, file_name) in [
        (1420.0, 900.0, "routing-rules-wide.png"),
        (720.0, 720.0, "routing-rules-compact.png"),
    ] {
        manis_profile::write_private_atomic(
            &store,
            "workspace.state",
            b"routing-manual-rules\nrouting-rule-source:qx-rule-deadbeef",
        )?;
        let window_store = store.clone();
        let window = cx.open_offscreen_window(size(px(width), px(height)), |window, cx| {
            manis_root(window, cx, |_| {
                manis_ui::ManisApp::with_fixture_controller_and_subscription_store(
                    "http://127.0.0.1:9090",
                    window_store,
                )
            })
        })?;
        let window: AnyWindowHandle = window.into();
        refresh(cx, window)?;
        if width >= 1_280.0 {
            open_workspace(cx, window, width, SnapshotWorkspace::Configuration)?;
            scroll_window(cx, window, width - 100.0, height - 120.0, -680.0)?;
            save_screenshot(cx, window, "configuration-wide-rule-source.png")?;
        }
        open_workspace(cx, window, width, SnapshotWorkspace::RoutingRules)?;
        save_screenshot(cx, window, file_name)?;
        verify_secondary_click(
            cx,
            window,
            point(px(width - 80.0), px(190.0)),
            &file_name.replace(".png", "-secondary-click.png"),
        )?;
        let toggle_x = width - if width >= 1280.0 { 550.0 } else { 205.0 };
        cx.simulate_click(window, point(px(toggle_x), px(24.0)), Modifiers::none());
        refresh(cx, window)?;
        save_screenshot(cx, window, &file_name.replace(".png", "-dark.png"))?;
        cx.simulate_click(window, point(px(toggle_x), px(24.0)), Modifiers::none());
        refresh(cx, window)?;
        if width >= 1_280.0 {
            capture_routing_rule_interactions(cx, window)?;
        } else {
            cx.simulate_click(window, point(px(654.0), px(80.0)), Modifiers::none());
            settle_ui_animation(cx, window)?;
            save_screenshot(cx, window, "routing-rules-compact-add-modal.png")?;
        }
        close_window(cx, window)?;
    }

    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn capture_routing_rule_interactions(
    cx: &mut gpui::VisualTestAppContext,
    window: gpui::AnyWindowHandle,
) -> Result<(), Box<dyn std::error::Error>> {
    use gpui::{Modifiers, point, px};

    cx.simulate_click(window, point(px(700.0), px(180.0)), Modifiers::none());
    settle_ui_animation(cx, window)?;
    save_screenshot(cx, window, "routing-rules-wide-manual-accordion-open.png")?;
    cx.simulate_click(window, point(px(700.0), px(250.0)), Modifiers::none());
    settle_ui_animation(cx, window)?;
    save_screenshot(cx, window, "routing-rules-wide-manual-edit-modal.png")?;
    cx.simulate_keystrokes(window, "escape");
    settle_ui_animation(cx, window)?;
    cx.simulate_click(window, point(px(700.0), px(180.0)), Modifiers::none());
    settle_ui_animation(cx, window)?;
    cx.simulate_click(window, point(px(700.0), px(250.0)), Modifiers::none());
    settle_ui_animation(cx, window)?;
    save_screenshot(cx, window, "routing-rules-wide-remote-accordion-open.png")?;
    cx.simulate_click(window, point(px(700.0), px(250.0)), Modifiers::none());
    settle_ui_animation(cx, window)?;

    cx.simulate_click(window, point(px(1_366.0), px(80.0)), Modifiers::none());
    settle_ui_animation(cx, window)?;
    save_screenshot(cx, window, "routing-rules-wide-add-modal.png")?;
    cx.simulate_click(window, point(px(500.0), px(370.0)), Modifiers::none());
    refresh(cx, window)?;
    save_screenshot(cx, window, "routing-rules-wide-type-menu.png")?;
    scroll_window(cx, window, 500.0, 560.0, -220.0)?;
    save_screenshot(cx, window, "routing-rules-wide-final-type-option.png")?;
    cx.simulate_click(window, point(px(500.0), px(370.0)), Modifiers::none());
    refresh(cx, window)?;
    cx.simulate_click(window, point(px(700.0), px(608.0)), Modifiers::none());
    refresh(cx, window)?;
    save_screenshot(cx, window, "routing-rules-wide-target-menu.png")?;
    cx.simulate_click(window, point(px(700.0), px(608.0)), Modifiers::none());
    refresh(cx, window)?;
    cx.simulate_click(window, point(px(410.0), px(260.0)), Modifiers::none());
    refresh(cx, window)?;
    cx.simulate_click(window, point(px(700.0), px(548.0)), Modifiers::none());
    settle_ui_animation(cx, window)?;
    save_screenshot(cx, window, "routing-rules-wide-manual-expanded.png")?;
    cx.simulate_click(window, point(px(700.0), px(548.0)), Modifiers::none());
    settle_ui_animation(cx, window)?;
    cx.simulate_click(window, point(px(700.0), px(632.0)), Modifiers::none());
    settle_ui_animation(cx, window)?;
    save_screenshot(cx, window, "routing-rules-wide-expanded.png")?;
    cx.simulate_click(window, point(px(500.0), px(307.0)), Modifiers::none());
    refresh(cx, window)?;
    save_screenshot(cx, window, "routing-rules-wide-compound.png")?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn snapshot_hex(value: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(value.len() * 2);
    for byte in value.bytes() {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

#[cfg(target_os = "macos")]
fn capture(
    cx: &mut gpui::VisualTestAppContext,
    width: f32,
    height: f32,
    file_name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    use gpui::{AnyWindowHandle, px, size};
    use manis_ui::ManisApp;

    let window = cx.open_offscreen_window(size(px(width), px(height)), |window, cx| {
        manis_root(window, cx, |_| {
            ManisApp::with_fixture_controller("http://127.0.0.1:9090")
        })
    })?;
    let window: AnyWindowHandle = window.into();

    refresh(cx, window)?;
    save_screenshot(cx, window, file_name)?;
    close_window(cx, window)
}

#[cfg(target_os = "macos")]
fn capture_compact_flow(
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
fn capture_medium_sheet(
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

#[cfg(target_os = "macos")]
fn capture_connected(
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
fn capture_data_page_coverage(
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

#[cfg(target_os = "macos")]
fn capture_logs_compact_connected_and_filtered(
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

#[cfg(target_os = "macos")]
fn capture_configuration_dark(
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

#[cfg(target_os = "macos")]
fn capture_live_when_configured(
    cx: &mut gpui::VisualTestAppContext,
) -> Result<(), Box<dyn std::error::Error>> {
    use gpui::{AnyWindowHandle, Modifiers, point, px, size};
    use manis_ui::ManisApp;
    use std::path::PathBuf;

    let Ok(endpoint) = std::env::var("MANIS_MIHOMO_LIVE_CONTROLLER") else {
        return Ok(());
    };
    let output = PathBuf::from(
        std::env::var_os("MANIS_MIHOMO_LIVE_SCREENSHOT")
            .ok_or("MANIS_MIHOMO_LIVE_SCREENSHOT is required when live capture is enabled")?,
    );
    validate_live_output(&output)?;

    let window = cx.open_offscreen_window(size(px(1420.0), px(900.0)), |window, cx| {
        manis_root(window, cx, |_| ManisApp::with_fixture_controller(endpoint))
    })?;
    let window: AnyWindowHandle = window.into();

    refresh(cx, window)?;
    cx.simulate_click(window, point(px(480.0), px(80.0)), Modifiers::none());
    refresh(cx, window)?;
    save_screenshot_at(cx, window, &output)?;
    close_window(cx, window)
}

#[cfg(target_os = "macos")]
fn validate_live_output(output: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    if std::fs::symlink_metadata(output).is_ok() {
        return Err("live screenshot output must be a new file".into());
    }

    let temp_root = std::env::temp_dir().canonicalize()?;
    let parent = output
        .parent()
        .ok_or("live screenshot output must have a parent directory")?
        .canonicalize()?;
    if !parent.starts_with(temp_root) {
        return Err("live screenshots must stay inside the system temporary directory".into());
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn spawn_mihomo_fixture() -> Result<(String, FixtureServer), Box<dyn std::error::Error>> {
    spawn_mihomo_fixture_with_stream_failure(false)
}

#[cfg(target_os = "macos")]
fn spawn_mihomo_fixture_with_stream_failure(
    fail_streams: bool,
) -> Result<(String, FixtureServer), Box<dyn std::error::Error>> {
    spawn_mihomo_fixture_with_response(fail_streams, |path| fixture_response(path).to_owned())
}

#[cfg(target_os = "macos")]
fn spawn_mihomo_fixture_with_response(
    fail_streams: bool,
    response_body: fn(&str) -> String,
) -> Result<(String, FixtureServer), Box<dyn std::error::Error>> {
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    let listener = TcpListener::bind("127.0.0.1:0")?;
    listener.set_nonblocking(true)?;
    let endpoint = format!("http://{}", listener.local_addr()?);
    let stop = Arc::new(AtomicBool::new(false));
    let server_stop = stop.clone();
    let server = std::thread::spawn(move || -> std::io::Result<()> {
        while !server_stop.load(Ordering::Relaxed) {
            let (mut stream, _) = match listener.accept() {
                Ok(connection) => connection,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(10));
                    continue;
                }
                Err(error) => return Err(error),
            };
            // macOS can inherit the listener's nonblocking flag on accepted sockets.
            // Read complete fixture requests instead of racing the first request byte.
            stream.set_nonblocking(false)?;
            stream.set_read_timeout(Some(Duration::from_secs(2)))?;
            let mut request_line = String::new();
            BufReader::new(stream.try_clone()?).read_line(&mut request_line)?;
            let path = request_line.split_whitespace().nth(1).unwrap_or("/");
            if fail_streams
                && (path.starts_with("/connections?interval=") || path.starts_with("/logs?level="))
            {
                stream.write_all(b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")?;
                continue;
            }
            if path.starts_with("/connections?interval=") {
                let body = response_body("/connections");
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nTransfer-Encoding: chunked\r\n\r\n{:X}\r\n{body}\n\r\n0\r\n\r\n",
                    body.len() + 1
                );
                stream.write_all(response.as_bytes())?;
                continue;
            }
            if path.starts_with("/logs?level=") {
                let body = concat!(
                    "{\"type\":\"trace\",\"payload\":\"[DNS] cache lookup complete\"}\n",
                    "{\"type\":\"debug\",\"payload\":\"[Router] policy group resolved\"}\n",
                    "{\"type\":\"info\",\"payload\":\"[TCP] Safari → openai.com matched DOMAIN-SUFFIX\"}\n",
                    "{\"type\":\"warning\",\"payload\":\"provider https://fixture.invalid/private-token retrying\"}\n",
                    "{\"type\":\"error\",\"payload\":\"[TCP] connection timed out\"}\n"
                );
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nTransfer-Encoding: chunked\r\n\r\n{:X}\r\n{body}\r\n0\r\n\r\n",
                    body.len()
                );
                stream.write_all(response.as_bytes())?;
                continue;
            }
            let body = response_body(path);
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes())?;
        }
        Ok(())
    });

    Ok((endpoint, FixtureServer { stop, server }))
}

#[cfg(target_os = "macos")]
fn spawn_empty_mihomo_fixture() -> Result<(String, FixtureServer), Box<dyn std::error::Error>> {
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    let listener = TcpListener::bind("127.0.0.1:0")?;
    listener.set_nonblocking(true)?;
    let endpoint = format!("http://{}", listener.local_addr()?);
    let stop = Arc::new(AtomicBool::new(false));
    let server_stop = stop.clone();
    let server = std::thread::spawn(move || -> std::io::Result<()> {
        while !server_stop.load(Ordering::Relaxed) {
            let (mut stream, _) = match listener.accept() {
                Ok(connection) => connection,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(10));
                    continue;
                }
                Err(error) => return Err(error),
            };
            stream.set_nonblocking(false)?;
            stream.set_read_timeout(Some(Duration::from_secs(2)))?;
            let mut request_line = String::new();
            BufReader::new(stream.try_clone()?).read_line(&mut request_line)?;
            let path = request_line.split_whitespace().nth(1).unwrap_or("/");
            let body = if path.starts_with("/connections") {
                r#"{"downloadTotal":0,"uploadTotal":0,"connections":[]}"#
            } else if path.starts_with("/logs?level=") {
                ""
            } else {
                fixture_response(path)
            };
            let response = if path.starts_with("/logs?level=") {
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n\r\n".to_owned()
            } else {
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
            };
            stream.write_all(response.as_bytes())?;
        }
        Ok(())
    });

    Ok((endpoint, FixtureServer { stop, server }))
}

#[cfg(target_os = "macos")]
struct FixtureServer {
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    server: std::thread::JoinHandle<Result<(), std::io::Error>>,
}

#[cfg(target_os = "macos")]
impl FixtureServer {
    fn stop(self) -> Result<(), Box<dyn std::error::Error>> {
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
        self.server
            .join()
            .map_err(|_| "Mihomo fixture server thread panicked")??;
        Ok(())
    }
}

#[cfg(target_os = "macos")]
fn fixture_response(path: &str) -> &'static str {
    if path.starts_with("/group/AI%20%E8%87%AA%E5%8A%A8%E9%80%89%E6%8B%A9/delay?") {
        return r#"{"新加坡 SG-02":31,"日本 JP-03":88}"#;
    }
    match path {
        "/version" => r#"{"meta":true,"version":"v1.19.12"}"#,
        "/proxies" => {
            r#"{"proxies":{"GLOBAL":{"name":"GLOBAL","type":"Selector","now":"新加坡 SG-02","all":["香港 HK-01","新加坡 SG-02","日本 JP-03","美国 US-01","DIRECT"],"alive":true},"AI 自动选择":{"name":"AI 自动选择","type":"URLTest","now":"新加坡 SG-02","all":["新加坡 SG-02","日本 JP-03"],"alive":true},"视频服务":{"name":"视频服务","type":"URLTest","now":"香港 HK-01","all":["香港 HK-01","美国 US-01"],"alive":true},"新加坡 SG-02":{"name":"新加坡 SG-02","type":"VLESS","alive":true,"provider-name":"Provider A","history":[{"delay":54}]},"日本 JP-03":{"name":"日本 JP-03","type":"Trojan","alive":true,"provider-name":"Provider B","history":[{"delay":67}]},"香港 HK-01":{"name":"香港 HK-01","type":"Hysteria2","alive":true,"provider-name":"Provider A","history":[{"delay":38}]},"美国 US-01":{"name":"美国 US-01","type":"VLESS","alive":true,"provider-name":"Provider A","history":[{"delay":142}]}}}"#
        }
        "/proxies/AI%20%E8%87%AA%E5%8A%A8%E9%80%89%E6%8B%A9" => {
            r#"{"name":"AI 自动选择","type":"URLTest","now":"新加坡 SG-02","all":["新加坡 SG-02","日本 JP-03"]}"#
        }
        "/providers/proxies" => {
            r#"{"providers":{"Provider A":{"name":"Provider A","type":"Proxy","vehicleType":"HTTP","proxies":[{"name":"香港 HK-01","type":"Hysteria2","alive":true,"history":[{"delay":38}]},{"name":"新加坡 SG-02","type":"VLESS","alive":true,"history":[{"delay":54}]},{"name":"美国 US-01","type":"VLESS","alive":true,"history":[{"delay":142}]},{"name":"剩余流量：96.83 GB","type":"Trojan","alive":false,"history":[]}]},"Provider B":{"name":"Provider B","type":"Proxy","vehicleType":"HTTP","proxies":[{"name":"日本 JP-03","type":"Trojan","alive":true,"history":[{"delay":67}]}]}}}"#
        }
        "/rules" => {
            r#"{"rules":[{"index":27,"type":"DOMAIN-SUFFIX","payload":"openai.com","proxy":"AI 自动选择","extra":{"hitCount":12}},{"index":28,"type":"DOMAIN-SUFFIX","payload":"google.com","proxy":"AI 自动选择","extra":{"hitCount":4}},{"index":18,"type":"DOMAIN-SUFFIX","payload":"youtube.com","proxy":"视频服务","extra":{"hitCount":32}}]}"#
        }
        "/connections" => {
            r#"{"downloadTotal":7340032,"uploadTotal":1572864,"connections":[{"id":"fixture","metadata":{"host":"","sniffHost":"openai.com","destinationIP":"104.18.33.45","remoteDestination":"104.18.32.45","process":"Safari","destinationPort":443},"chains":["新加坡 SG-02","AI 自动选择"],"providerChains":[["Provider A","新加坡 SG-02"]],"rule":"DOMAIN-SUFFIX","rulePayload":"openai.com","upload":2048,"download":8192}]}"#
        }
        "/configs" => {
            r#"{"mixed-port":7890,"port":0,"socks-port":0,"mode":"rule","tun":{"enable":false}}"#
        }
        _ => r"{}",
    }
}

#[cfg(target_os = "macos")]
#[derive(Clone, Copy)]
enum SnapshotWorkspace {
    Nodes,
    RoutingRules,
    Activity,
    Logs,
    Configuration,
}

#[cfg(target_os = "macos")]
fn open_workspace(
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
fn open_policy_section(
    cx: &mut gpui::VisualTestAppContext,
    window: gpui::AnyWindowHandle,
    width: f32,
) -> Result<(), Box<dyn std::error::Error>> {
    open_workspace(cx, window, width, SnapshotWorkspace::Nodes)?;
    scroll_window(cx, window, width - 50.0, 300.0, -220.0)
}

#[cfg(target_os = "macos")]
fn refresh(
    cx: &mut gpui::VisualTestAppContext,
    window: gpui::AnyWindowHandle,
) -> Result<(), Box<dyn std::error::Error>> {
    cx.run_until_parked();
    cx.update_window(window, |_, window, _| window.refresh())?;
    cx.run_until_parked();
    Ok(())
}

#[cfg(target_os = "macos")]
fn settle_ui_animation(
    cx: &mut gpui::VisualTestAppContext,
    window: gpui::AnyWindowHandle,
) -> Result<(), Box<dyn std::error::Error>> {
    settle_ui_for(cx, window, std::time::Duration::from_millis(300))
}

#[cfg(target_os = "macos")]
fn settle_ui_for(
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
fn scroll_window(
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
fn close_window(
    cx: &mut gpui::VisualTestAppContext,
    window: gpui::AnyWindowHandle,
) -> Result<(), Box<dyn std::error::Error>> {
    cx.update_window(window, |_, window, _| window.remove_window())?;
    cx.run_until_parked();
    Ok(())
}

#[cfg(target_os = "macos")]
fn save_screenshot(
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
fn save_screenshot_at(
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

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("native snapshot capture is currently available on macOS only");
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    #[test]
    fn live_output_path_must_resolve_inside_system_temp() -> Result<(), Box<dyn std::error::Error>>
    {
        let temp = std::env::temp_dir();
        let safe = temp.join(format!("manis-live-test-{}.png", std::process::id()));
        super::validate_live_output(&safe)?;

        let escaped = temp.join("..").join("manis-live-escaped.png");
        assert!(super::validate_live_output(&escaped).is_err());
        Ok(())
    }
}
