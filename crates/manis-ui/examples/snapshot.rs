#[cfg(target_os = "macos")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let platform = gpui_platform::current_platform(false);
    let mut cx = gpui::VisualTestAppContext::with_asset_source(
        platform,
        std::sync::Arc::new(manis_ui::Assets),
    );
    cx.update(manis_ui::init);
    if std::env::args().any(|argument| argument == "--policy-settings") {
        capture_managed_policy_settings(&mut cx)?;
        return Ok(());
    }
    if std::env::args().any(|argument| argument == "--automatic-policy") {
        capture_automatic_policy(&mut cx)?;
        return Ok(());
    }
    if std::env::args().any(|argument| argument == "--routing-rules") {
        capture_routing_rules(&mut cx)?;
        return Ok(());
    }
    if std::env::args().any(|argument| argument == "--route-prediction") {
        capture_route_prediction(&mut cx)?;
        return Ok(());
    }
    if std::env::args().any(|argument| argument == "--medium-sheet") {
        capture_medium_sheet(&mut cx)?;
        return Ok(());
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
    capture_medium_sheet(&mut cx)?;
    capture_connected(&mut cx)?;
    capture_live_when_configured(&mut cx)?;
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
    let window_store = store.clone();
    let wide_endpoint = endpoint.clone();
    let window = cx.open_offscreen_window(size(px(1420.0), px(900.0)), |window, cx| {
        manis_root(window, cx, |_| {
            ManisApp::with_fixture_controller_and_subscription_store(wide_endpoint, window_store)
        })
    })?;
    let window: AnyWindowHandle = window.into();

    refresh(cx, window)?;
    cx.simulate_click(window, point(px(620.0), px(510.0)), Modifiers::none());
    for _ in 0..24 {
        std::thread::sleep(std::time::Duration::from_millis(25));
        refresh(cx, window)?;
    }
    save_screenshot(cx, window, "native-wide-saved-policy-collapsed.png")?;
    cx.simulate_click(window, point(px(700.0), px(172.0)), Modifiers::none());
    refresh(cx, window)?;
    save_screenshot(cx, window, "native-wide-saved-policy-expanded.png")?;
    cx.simulate_click(window, point(px(1_290.0), px(280.0)), Modifiers::none());
    refresh(cx, window)?;
    save_screenshot(cx, window, "native-wide-policy-settings.png")?;
    cx.simulate_click(window, point(px(1_040.0), px(190.0)), Modifiers::none());
    refresh(cx, window)?;
    save_screenshot(cx, window, "native-wide-policy-type-menu.png")?;
    cx.simulate_click(window, point(px(1_040.0), px(190.0)), Modifiers::none());
    refresh(cx, window)?;
    cx.simulate_click(window, point(px(1_040.0), px(350.0)), Modifiers::none());
    refresh(cx, window)?;
    save_screenshot(cx, window, "native-wide-policy-icon-menu.png")?;
    cx.simulate_click(window, point(px(390.0), px(740.0)), Modifiers::none());
    refresh(cx, window)?;
    cx.simulate_click(window, point(px(820.0), px(450.0)), Modifiers::none());
    refresh(cx, window)?;
    save_screenshot(cx, window, "native-wide-policy-node-scope-menu.png")?;
    cx.simulate_click(window, point(px(820.0), px(700.0)), Modifiers::none());
    refresh(cx, window)?;
    save_screenshot(cx, window, "native-wide-policy-node-menu.png")?;

    close_window(cx, window)?;

    let compact_store = store.clone();
    let compact_window = cx.open_offscreen_window(size(px(720.0), px(720.0)), |window, cx| {
        manis_root(window, cx, |_| {
            ManisApp::with_fixture_controller_and_subscription_store(endpoint, compact_store)
        })
    })?;
    let compact_window: AnyWindowHandle = compact_window.into();
    refresh(cx, compact_window)?;
    for _ in 0..24 {
        std::thread::sleep(std::time::Duration::from_millis(25));
        refresh(cx, compact_window)?;
    }
    cx.simulate_click(
        compact_window,
        point(px(590.0), px(88.0)),
        Modifiers::none(),
    );
    refresh(cx, compact_window)?;
    save_screenshot(cx, compact_window, "native-compact-policy-editor.png")?;
    cx.simulate_click(
        compact_window,
        point(px(400.0), px(190.0)),
        Modifiers::none(),
    );
    refresh(cx, compact_window)?;
    save_screenshot(cx, compact_window, "native-compact-policy-type-menu.png")?;
    close_window(cx, compact_window)?;

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
    cx.simulate_click(window, point(px(1_350.0), px(82.0)), Modifiers::none());
    for _ in 0..24 {
        std::thread::sleep(std::time::Duration::from_millis(25));
        refresh(cx, window)?;
    }
    save_screenshot(cx, window, "native-wide-policy-groups-collapsed.png")?;
    cx.simulate_click(window, point(px(380.0), px(190.0)), Modifiers::none());
    refresh(cx, window)?;
    save_screenshot(cx, window, "native-wide-policy-groups-expanded.png")?;
    cx.simulate_click(window, point(px(268.0), px(190.0)), Modifiers::none());
    for _ in 0..24 {
        std::thread::sleep(std::time::Duration::from_millis(25));
        refresh(cx, window)?;
    }
    save_screenshot(cx, window, "native-wide-policy-groups-tested.png")?;
    close_window(cx, window)?;
    server.stop()
}

#[cfg(target_os = "macos")]
#[allow(clippy::too_many_lines)]
fn capture_remote_subscription_preview(
    cx: &mut gpui::VisualTestAppContext,
) -> Result<(), Box<dyn std::error::Error>> {
    use gpui::{AnyWindowHandle, Modifiers, point, px, size};
    use manis_ui::ManisApp;
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;
    use std::path::Path;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    let listener = TcpListener::bind("127.0.0.1:0")?;
    listener.set_nonblocking(true)?;
    let subscription_url = format!(
        "http://{}/subscription?name=Fixture%20Transit",
        listener.local_addr()?
    );
    let stop = Arc::new(AtomicBool::new(false));
    let server_stop = stop.clone();
    let server = std::thread::spawn(move || -> std::io::Result<()> {
        let body = r#"proxies:
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
        while !server_stop.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let mut request_line = String::new();
                    BufReader::new(stream.try_clone()?).read_line(&mut request_line)?;
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/yaml\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
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
    });

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
    cx.simulate_click(window, point(px(110.0), px(284.0)), Modifiers::none());
    refresh(cx, window)?;
    scroll_window(cx, window, 1_300.0, 760.0, -600.0)?;
    cx.simulate_click(window, point(px(700.0), px(510.0)), Modifiers::none());
    refresh(cx, window)?;
    cx.simulate_input(window, &subscription_url);
    refresh(cx, window)?;
    cx.simulate_click(window, point(px(700.0), px(560.0)), Modifiers::none());
    for _ in 0..40 {
        std::thread::sleep(Duration::from_millis(25));
        refresh(cx, window)?;
    }
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
    stop.store(true, Ordering::Relaxed);
    server.join().map_err(|_| "fixture server panicked")??;
    if fixture_root.exists() {
        std::fs::remove_dir_all(fixture_root)?;
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn write_managed_policy_fixture(store: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::PermissionsExt;

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
#[allow(clippy::too_many_lines)]
fn capture_restored_subscription_views(
    cx: &mut gpui::VisualTestAppContext,
    store: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    use gpui::{AnyWindowHandle, Modifiers, point, px, size};
    use manis_ui::ManisApp;
    use std::time::Duration;

    for (width, height, navigation_x, configuration_file, nodes_file, collapsed_file, group_y) in [
        (
            1420.0,
            900.0,
            110.0,
            "configuration-wide-import-restored.png",
            "nodes-wide-imported.png",
            "nodes-wide-imported-collapsed.png",
            365.0,
        ),
        (
            720.0,
            720.0,
            30.0,
            "configuration-compact-import-restored.png",
            "nodes-compact-imported.png",
            "nodes-compact-imported-collapsed.png",
            310.0,
        ),
    ] {
        let window_store = store.to_owned();
        let window = cx.open_offscreen_window(size(px(width), px(height)), |window, cx| {
            manis_root(window, cx, |_| {
                ManisApp::with_fixture_controller_and_subscription_store(
                    "http://127.0.0.1:9090",
                    window_store,
                )
            })
        })?;
        let window: AnyWindowHandle = window.into();
        refresh(cx, window)?;
        cx.simulate_click(
            window,
            point(px(navigation_x), px(284.0)),
            Modifiers::none(),
        );
        for _ in 0..40 {
            std::thread::sleep(Duration::from_millis(25));
            refresh(cx, window)?;
        }
        scroll_window(cx, window, width - 100.0, height - 120.0, -360.0)?;
        save_screenshot(cx, window, configuration_file)?;
        cx.simulate_click(window, point(px(navigation_x), px(76.0)), Modifiers::none());
        refresh(cx, window)?;
        save_screenshot(cx, window, nodes_file)?;
        cx.simulate_click(
            window,
            point(
                px(if width >= 1_280.0 { 1_360.0 } else { 660.0 }),
                px(group_y),
            ),
            Modifiers::none(),
        );
        refresh(cx, window)?;
        save_screenshot(cx, window, collapsed_file)?;
        close_window(cx, window)?;
    }
    Ok(())
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
    let navigation_x = if width >= 1_280.0 { 110.0 } else { 30.0 };
    cx.simulate_click(
        window,
        point(px(navigation_x), px(284.0)),
        Modifiers::none(),
    );
    refresh(cx, window)?;
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
    let navigation_x = if width >= 1_280.0 { 110.0 } else { 30.0 };
    cx.simulate_click(
        window,
        point(px(navigation_x), px(284.0)),
        Modifiers::none(),
    );
    refresh(cx, window)?;
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
    use manis_ui::ManisApp;
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
    let manual_file = store.join("manual-routing-rules.state");
    std::fs::write(
        &manual_file,
        concat!(
            "manis.manual-routing-rules.v2\n",
            "legacy-direct-rules-migrated\t1\n",
            "rule\tDIRECT\thost-suffix\tgithub.com\tdst-port\t22\n",
            "rule\tProxy\thost-keyword\tgoogle",
        ),
    )?;
    std::fs::set_permissions(manual_file, std::fs::Permissions::from_mode(0o600))?;

    for (width, height, navigation_x, file_name) in [
        (1420.0, 900.0, 110.0, "routing-rules-wide.png"),
        (720.0, 720.0, 30.0, "routing-rules-compact.png"),
    ] {
        let window_store = store.clone();
        let window = cx.open_offscreen_window(size(px(width), px(height)), |window, cx| {
            manis_root(window, cx, |_| {
                ManisApp::with_fixture_controller_and_subscription_store(
                    "http://127.0.0.1:9090",
                    window_store,
                )
            })
        })?;
        let window: AnyWindowHandle = window.into();
        refresh(cx, window)?;
        if width >= 1_280.0 {
            cx.simulate_click(
                window,
                point(px(navigation_x), px(284.0)),
                Modifiers::none(),
            );
            refresh(cx, window)?;
            scroll_window(cx, window, width - 100.0, height - 120.0, -680.0)?;
            save_screenshot(cx, window, "configuration-wide-rule-source.png")?;
        }
        cx.simulate_click(
            window,
            point(px(navigation_x), px(158.0)),
            Modifiers::none(),
        );
        refresh(cx, window)?;
        save_screenshot(cx, window, file_name)?;
        if width >= 1_280.0 {
            capture_routing_rule_interactions(cx, window)?;
        } else {
            cx.simulate_click(window, point(px(654.0), px(157.0)), Modifiers::none());
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

    cx.simulate_click(window, point(px(700.0), px(260.0)), Modifiers::none());
    settle_ui_animation(cx, window)?;
    save_screenshot(cx, window, "routing-rules-wide-manual-accordion-open.png")?;
    cx.simulate_click(window, point(px(700.0), px(260.0)), Modifiers::none());
    settle_ui_animation(cx, window)?;
    cx.simulate_click(window, point(px(700.0), px(348.0)), Modifiers::none());
    settle_ui_animation(cx, window)?;
    save_screenshot(cx, window, "routing-rules-wide-remote-accordion-open.png")?;
    cx.simulate_click(window, point(px(700.0), px(348.0)), Modifiers::none());
    settle_ui_animation(cx, window)?;

    cx.simulate_click(window, point(px(1_286.0), px(190.0)), Modifiers::none());
    settle_ui_animation(cx, window)?;
    save_screenshot(cx, window, "routing-rules-wide-add-modal.png")?;
    cx.simulate_click(window, point(px(500.0), px(414.0)), Modifiers::none());
    refresh(cx, window)?;
    save_screenshot(cx, window, "routing-rules-wide-type-menu.png")?;
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
    for _ in 0..24 {
        std::thread::sleep(std::time::Duration::from_millis(25));
        refresh(cx, window)?;
    }
    cx.simulate_click(window, point(px(300.0), px(312.0)), Modifiers::none());
    refresh(cx, window)?;
    save_screenshot(cx, window, "native-compact-detail.png")?;

    cx.simulate_click(window, point(px(592.0), px(24.0)), Modifiers::none());
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

    for _ in 0..24 {
        std::thread::sleep(std::time::Duration::from_millis(25));
        refresh(cx, window)?;
    }
    cx.simulate_click(window, point(px(985.0), px(80.0)), Modifiers::none());
    for _ in 0..24 {
        std::thread::sleep(std::time::Duration::from_millis(25));
        refresh(cx, window)?;
    }
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
    cx.simulate_click(window, point(px(620.0), px(510.0)), Modifiers::none());
    for _ in 0..24 {
        std::thread::sleep(std::time::Duration::from_millis(25));
        refresh(cx, window)?;
    }
    refresh(cx, window)?;
    save_screenshot(cx, window, "native-wide-connected.png")?;

    cx.simulate_click(window, point(px(110.0), px(80.0)), Modifiers::none());
    refresh(cx, window)?;
    save_screenshot(cx, window, "nodes-wide-connected-global.png")?;

    cx.simulate_click(window, point(px(110.0), px(117.0)), Modifiers::none());
    refresh(cx, window)?;

    cx.simulate_click(window, point(px(270.0), px(236.0)), Modifiers::none());
    for _ in 0..24 {
        std::thread::sleep(std::time::Duration::from_millis(25));
        refresh(cx, window)?;
    }
    save_screenshot(cx, window, "native-wide-connected-benchmark.png")?;

    cx.advance_clock(std::time::Duration::from_millis(500));
    refresh(cx, window)?;

    cx.simulate_click(window, point(px(110.0), px(199.0)), Modifiers::none());
    refresh(cx, window)?;
    save_screenshot(cx, window, "activity-wide-connected.png")?;

    cx.simulate_click(window, point(px(110.0), px(240.0)), Modifiers::none());
    refresh(cx, window)?;
    save_screenshot(cx, window, "logs-wide-connected.png")?;

    cx.simulate_click(window, point(px(110.0), px(284.0)), Modifiers::none());
    refresh(cx, window)?;
    save_screenshot(cx, window, "configuration-wide-connected-sources.png")?;

    close_window(cx, window)?;
    server.stop()?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn capture_route_prediction(
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
    cx.simulate_click(window, point(px(1_350.0), px(82.0)), Modifiers::none());
    for _ in 0..24 {
        std::thread::sleep(std::time::Duration::from_millis(25));
        refresh(cx, window)?;
    }
    cx.simulate_click(window, point(px(110.0), px(155.0)), Modifiers::none());
    refresh(cx, window)?;
    cx.simulate_click(window, point(px(1_190.0), px(175.0)), Modifiers::none());
    cx.simulate_input(window, "openai.com");
    refresh(cx, window)?;
    cx.simulate_click(window, point(px(1_365.0), px(175.0)), Modifiers::none());
    refresh(cx, window)?;
    save_screenshot(cx, window, "native-wide-route-prediction.png")?;
    close_window(cx, window)?;
    server.stop()
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
            let mut request_line = String::new();
            BufReader::new(stream.try_clone()?).read_line(&mut request_line)?;
            let path = request_line.split_whitespace().nth(1).unwrap_or("/");
            if path.starts_with("/connections?interval=") {
                let body = fixture_response("/connections");
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nTransfer-Encoding: chunked\r\n\r\n{:X}\r\n{body}\n\r\n0\r\n\r\n",
                    body.len() + 1
                );
                stream.write_all(response.as_bytes())?;
                continue;
            }
            if path.starts_with("/logs?level=") {
                let body = concat!(
                    "{\"type\":\"info\",\"payload\":\"[TCP] Safari → openai.com matched DOMAIN-SUFFIX\"}\n",
                    "{\"type\":\"warning\",\"payload\":\"provider https://fixture.invalid/private-token retrying\"}\n"
                );
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nTransfer-Encoding: chunked\r\n\r\n{:X}\r\n{body}\r\n0\r\n\r\n",
                    body.len()
                );
                stream.write_all(response.as_bytes())?;
                continue;
            }
            let body = fixture_response(path);
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
    for _ in 0..12 {
        std::thread::sleep(std::time::Duration::from_millis(25));
        cx.advance_clock(std::time::Duration::from_millis(25));
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
