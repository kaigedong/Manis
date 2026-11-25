use super::super::{
    driver::{
        SnapshotWorkspace, close_window, open_workspace, refresh, save_screenshot, scroll_window,
        settle_ui_animation, toggle_theme,
    },
    fixtures::write_managed_policy_fixture,
};
use super::common::manis_root;

#[cfg(target_os = "macos")]
pub(crate) fn capture_app_updates(
    cx: &mut gpui::VisualTestAppContext,
) -> Result<(), Box<dyn std::error::Error>> {
    use gpui::{AnyWindowHandle, AppContext as _, px, size};
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
        toggle_theme(cx, window, width)?;
        save_screenshot(cx, window, &format!("app-updates-{label}-dark.png"))?;
        cx.update_window(window, |_, window, cx| {
            app.update(cx, |app, cx| app.show_about_fixture(window, cx));
        })?;
        settle_ui_animation(cx, window)?;
        save_screenshot(cx, window, &format!("about-{label}-dark.png"))?;
        cx.simulate_keystrokes(window, "escape");
        settle_ui_animation(cx, window)?;
        toggle_theme(cx, window, width)?;
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
pub(crate) fn capture_configuration_transfer(
    cx: &mut gpui::VisualTestAppContext,
) -> Result<(), Box<dyn std::error::Error>> {
    use gpui::{AnyWindowHandle, AppContext as _, px, size};
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
        toggle_theme(cx, window, width)?;
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
    toggle_theme(cx, window, width)?;
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
pub(crate) fn capture_configuration(
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

/// Section directory rows inside the settings card, measured on a 1420x900 window.
#[cfg(target_os = "macos")]
const SECTION_ROWS: [f32; 6] = [107.0, 145.0, 183.0, 221.0, 259.0, 297.0];
#[cfg(target_os = "macos")]
const SECTION_ROW_X: f32 = 340.0;
/// The section card's trailing primary action, in the same rows as its title.
#[cfg(target_os = "macos")]
const SECTION_ACTION_X: f32 = 1_338.0;
#[cfg(target_os = "macos")]
const SECTION_ACTION_Y: f32 = 113.0;

#[cfg(target_os = "macos")]
pub(crate) fn capture_configuration_sections(
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
    cx.simulate_click(
        window,
        point(px(SECTION_ROW_X), px(SECTION_ROWS[2])),
        Modifiers::none(),
    );
    refresh(cx, window)?;
    save_screenshot(cx, window, "configuration-section-proxy-sources.png")?;
    // The section card's own primary action opens the source editor.
    cx.simulate_click(
        window,
        point(px(SECTION_ACTION_X), px(SECTION_ACTION_Y)),
        Modifiers::none(),
    );
    settle_ui_animation(cx, window)?;
    if !cx.update_window(window, |_, window, cx| window.has_active_dialog(cx))? {
        return Err("proxy source fixture did not open the editor dialog".into());
    }
    save_screenshot(cx, window, "configuration-proxy-source-modal.png")?;
    cx.simulate_keystrokes(window, "escape");
    settle_ui_animation(cx, window)?;

    for (index, file_name) in [
        (0, "configuration-section-general.png"),
        (1, "configuration-section-runtime.png"),
    ] {
        cx.simulate_click(
            window,
            point(px(SECTION_ROW_X), px(SECTION_ROWS[index])),
            Modifiers::none(),
        );
        refresh(cx, window)?;
        save_screenshot(cx, window, file_name)?;
    }
    cx.simulate_click(
        window,
        point(px(SECTION_ROW_X), px(SECTION_ROWS[3])),
        Modifiers::none(),
    );
    refresh(cx, window)?;
    save_screenshot(cx, window, "configuration-section-rule-sources.png")?;
    cx.simulate_click(
        window,
        point(px(SECTION_ACTION_X), px(SECTION_ACTION_Y)),
        Modifiers::none(),
    );
    settle_ui_animation(cx, window)?;
    if !cx.update_window(window, |_, window, cx| window.has_active_dialog(cx))? {
        return Err("rule source fixture did not open the editor dialog".into());
    }
    save_screenshot(cx, window, "configuration-rule-source-modal.png")?;
    cx.simulate_keystrokes(window, "escape");
    settle_ui_animation(cx, window)?;

    cx.simulate_click(
        window,
        point(px(SECTION_ROW_X), px(SECTION_ROWS[4])),
        Modifiers::none(),
    );
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
pub(crate) fn capture_localization(
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
    // Wide windows lay the three language options out in one row; compact stacks them, so the
    // third option sits near the bottom of the first card.
    let language_option = if width >= 1_280.0 {
        point(px(935.0), px(157.0))
    } else {
        point(px(387.0), px(354.0))
    };
    cx.simulate_click(window, language_option, Modifiers::none());
    refresh(cx, window)?;
    // The store migrates the loose fixture file into config.toml, so the saved value is read
    // back from the configuration table rather than from the original path.
    let saved = std::fs::read_to_string(fixture_root.join("config.toml"))?;
    let persisted = saved.lines().any(|line| line.trim() == preference)
        || saved.contains(&format!("= \"{preference}\""));
    if !persisted {
        return Err(format!(
            "language selector did not persist {preference}; config.toml is:\n{saved}"
        )
        .into());
    }
    save_screenshot(cx, window, file_name)?;
    close_window(cx, window)?;
    std::fs::remove_dir_all(fixture_root)?;
    Ok(())
}
