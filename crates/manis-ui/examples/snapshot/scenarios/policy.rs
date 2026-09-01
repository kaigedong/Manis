use super::super::{
    driver::{
        SnapshotWorkspace, close_window, open_policy_section, open_workspace, refresh,
        save_screenshot, scroll_window, settle_ui_animation, settle_ui_for,
    },
    fixtures::spawn_mihomo_fixture,
};
use super::common::{manis_root, snapshot_hex, verify_secondary_click};

#[cfg(target_os = "macos")]
pub(crate) fn capture_proxy_candidate(
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
pub(crate) fn capture_managed_policy_settings(
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
pub(crate) fn capture_automatic_policy(
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
pub(crate) fn capture_routing_rules(
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
