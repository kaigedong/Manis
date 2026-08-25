#[cfg(target_os = "macos")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let platform = gpui_platform::current_platform(false);
    let mut cx = gpui::VisualTestAppContext::new(platform);
    capture(&mut cx, 1420.0, 900.0, "native-wide.png")?;
    capture(&mut cx, 1060.0, 800.0, "native-medium.png")?;
    capture(&mut cx, 720.0, 720.0, "native-compact.png")?;
    capture_configuration(&mut cx, 1420.0, 900.0, "configuration-wide.png")?;
    capture_configuration(&mut cx, 1060.0, 800.0, "configuration-medium.png")?;
    capture_configuration(&mut cx, 720.0, 720.0, "configuration-compact.png")?;
    capture_compact_flow(&mut cx)?;
    capture_connected(&mut cx)?;
    capture_live_when_configured(&mut cx)?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn capture_configuration(
    cx: &mut gpui::VisualTestAppContext,
    width: f32,
    height: f32,
    file_name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    use gpui::{AnyWindowHandle, AppContext, Modifiers, point, px, size};
    use relay_ui::RelayApp;

    let window = cx.open_offscreen_window(size(px(width), px(height)), |_, cx| {
        cx.new(|_| RelayApp::new())
    })?;
    let window: AnyWindowHandle = window.into();

    refresh(cx, window)?;
    let navigation_x = if width >= 1_280.0 { 110.0 } else { 30.0 };
    cx.simulate_click(
        window,
        point(px(navigation_x), px(120.0)),
        Modifiers::none(),
    );
    refresh(cx, window)?;
    save_screenshot(cx, window, file_name)?;

    if width >= 1_280.0 {
        cx.simulate_click(window, point(px(275.0), px(145.0)), Modifiers::none());
        refresh(cx, window)?;
        save_screenshot(cx, window, "configuration-wide-subscription-focused.png")?;
        cx.simulate_input(
            window,
            "vless://00000000-0000-4000-8000-000000000000@edge.example.invalid:443?security=tls&type=ws#Tokyo%20Edge",
        );
        refresh(cx, window)?;
        cx.simulate_click(window, point(px(370.0), px(370.0)), Modifiers::none());
        refresh(cx, window)?;
        save_screenshot(cx, window, "configuration-wide-subscription-preview.png")?;
    }

    if (width - 720.0).abs() < f32::EPSILON {
        cx.simulate_click(window, point(px(180.0), px(145.0)), Modifiers::none());
        refresh(cx, window)?;
        cx.simulate_input(
            window,
            "vless://00000000-0000-4000-8000-000000000000@edge.example.invalid:443?security=tls&type=ws#Tokyo%20Edge",
        );
        refresh(cx, window)?;
        cx.simulate_click(window, point(px(360.0), px(390.0)), Modifiers::none());
        refresh(cx, window)?;
        save_screenshot(cx, window, "configuration-compact-subscription-preview.png")?;

        cx.simulate_click(window, point(px(548.0), px(145.0)), Modifiers::none());
        refresh(cx, window)?;
        save_screenshot(cx, window, "configuration-compact-rules.png")?;
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn capture(
    cx: &mut gpui::VisualTestAppContext,
    width: f32,
    height: f32,
    file_name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    use gpui::{AnyWindowHandle, AppContext, px, size};
    use relay_ui::RelayApp;

    let window = cx.open_offscreen_window(size(px(width), px(height)), |_, cx| {
        cx.new(|_| RelayApp::new())
    })?;
    let window: AnyWindowHandle = window.into();

    refresh(cx, window)?;
    save_screenshot(cx, window, file_name)
}

#[cfg(target_os = "macos")]
fn capture_compact_flow(
    cx: &mut gpui::VisualTestAppContext,
) -> Result<(), Box<dyn std::error::Error>> {
    use gpui::{AnyWindowHandle, AppContext, Modifiers, point, px, size};
    use relay_ui::RelayApp;

    let window = cx.open_offscreen_window(size(px(720.0), px(720.0)), |_, cx| {
        cx.new(|_| RelayApp::new())
    })?;
    let window: AnyWindowHandle = window.into();

    refresh(cx, window)?;
    cx.simulate_click(window, point(px(300.0), px(312.0)), Modifiers::none());
    refresh(cx, window)?;
    save_screenshot(cx, window, "native-compact-detail.png")?;

    cx.simulate_click(window, point(px(592.0), px(24.0)), Modifiers::none());
    refresh(cx, window)?;
    save_screenshot(cx, window, "native-compact-dark-detail.png")?;

    cx.simulate_click(window, point(px(664.0), px(80.0)), Modifiers::none());
    refresh(cx, window)?;
    save_screenshot(cx, window, "native-compact-dark-inspector.png")
}

#[cfg(target_os = "macos")]
fn capture_connected(
    cx: &mut gpui::VisualTestAppContext,
) -> Result<(), Box<dyn std::error::Error>> {
    use gpui::{AnyWindowHandle, AppContext, Modifiers, point, px, size};
    use relay_ui::RelayApp;

    let (endpoint, server) = spawn_mihomo_fixture()?;
    let window = cx.open_offscreen_window(size(px(1420.0), px(900.0)), |_, cx| {
        cx.new(|_| RelayApp::with_controller(endpoint))
    })?;
    let window: AnyWindowHandle = window.into();

    refresh(cx, window)?;
    cx.simulate_click(window, point(px(480.0), px(80.0)), Modifiers::none());
    refresh(cx, window)?;
    save_screenshot(cx, window, "native-wide-connected.png")?;

    cx.simulate_click(window, point(px(110.0), px(120.0)), Modifiers::none());
    refresh(cx, window)?;
    save_screenshot(cx, window, "configuration-wide-connected-sources.png")?;

    server
        .join()
        .map_err(|_| "Mihomo fixture server thread panicked")??;
    Ok(())
}

#[cfg(target_os = "macos")]
fn capture_live_when_configured(
    cx: &mut gpui::VisualTestAppContext,
) -> Result<(), Box<dyn std::error::Error>> {
    use gpui::{AnyWindowHandle, AppContext, Modifiers, point, px, size};
    use relay_ui::RelayApp;
    use std::path::PathBuf;

    let Ok(endpoint) = std::env::var("RELAY_MIHOMO_LIVE_CONTROLLER") else {
        return Ok(());
    };
    let output = PathBuf::from(
        std::env::var_os("RELAY_MIHOMO_LIVE_SCREENSHOT")
            .ok_or("RELAY_MIHOMO_LIVE_SCREENSHOT is required when live capture is enabled")?,
    );
    validate_live_output(&output)?;

    let window = cx.open_offscreen_window(size(px(1420.0), px(900.0)), |_, cx| {
        cx.new(|_| RelayApp::with_controller(endpoint))
    })?;
    let window: AnyWindowHandle = window.into();

    refresh(cx, window)?;
    cx.simulate_click(window, point(px(480.0), px(80.0)), Modifiers::none());
    refresh(cx, window)?;
    save_screenshot_at(cx, window, &output)
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
fn spawn_mihomo_fixture() -> Result<FixtureServer, Box<dyn std::error::Error>> {
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0")?;
    let endpoint = format!("http://{}", listener.local_addr()?);
    let server = std::thread::spawn(move || -> std::io::Result<()> {
        for _ in 0..5 {
            let (mut stream, _) = listener.accept()?;
            let mut request_line = String::new();
            BufReader::new(stream.try_clone()?).read_line(&mut request_line)?;
            let path = request_line.split_whitespace().nth(1).unwrap_or("/");
            let body = fixture_response(path);
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes())?;
        }
        Ok(())
    });

    Ok((endpoint, server))
}

#[cfg(target_os = "macos")]
type FixtureServer = (String, std::thread::JoinHandle<Result<(), std::io::Error>>);

#[cfg(target_os = "macos")]
fn fixture_response(path: &str) -> &'static str {
    match path {
        "/version" => r#"{"meta":true,"version":"v1.19.12"}"#,
        "/proxies" => {
            r#"{"proxies":{"AI 自动选择":{"name":"AI 自动选择","type":"Selector","now":"新加坡 SG-02","all":["新加坡 SG-02","日本 JP-03"],"alive":true},"视频服务":{"name":"视频服务","type":"URLTest","now":"香港 HK-01","all":["香港 HK-01","美国 US-01"],"alive":true},"新加坡 SG-02":{"name":"新加坡 SG-02","type":"VLESS","alive":true,"provider-name":"Provider A","history":[{"delay":54}]},"日本 JP-03":{"name":"日本 JP-03","type":"Trojan","alive":true,"provider-name":"Provider B","history":[{"delay":67}]},"香港 HK-01":{"name":"香港 HK-01","type":"Hysteria2","alive":true,"provider-name":"Provider A","history":[{"delay":38}]},"美国 US-01":{"name":"美国 US-01","type":"VLESS","alive":true,"provider-name":"Provider A","history":[{"delay":142}]}}}"#
        }
        "/providers/proxies" => {
            r#"{"providers":{"Provider A":{"name":"Provider A","type":"Proxy","vehicleType":"HTTP","proxies":[{"name":"香港 HK-01","type":"Hysteria2","alive":true,"history":[{"delay":38}]},{"name":"新加坡 SG-02","type":"VLESS","alive":true,"history":[{"delay":54}]},{"name":"美国 US-01","type":"VLESS","alive":true,"history":[{"delay":142}]}]},"Provider B":{"name":"Provider B","type":"Proxy","vehicleType":"HTTP","proxies":[{"name":"日本 JP-03","type":"Trojan","alive":true,"history":[{"delay":67}]}]}}}"#
        }
        "/rules" => {
            r#"{"rules":[{"index":27,"type":"DOMAIN-SUFFIX","payload":"openai.com","proxy":"AI 自动选择","extra":{"hitCount":12}},{"index":28,"type":"DOMAIN-SUFFIX","payload":"google.com","proxy":"AI 自动选择","extra":{"hitCount":4}},{"index":18,"type":"DOMAIN-SUFFIX","payload":"youtube.com","proxy":"视频服务","extra":{"hitCount":32}}]}"#
        }
        "/connections" => {
            r#"{"downloadTotal":7340032,"uploadTotal":1572864,"connections":[{"id":"fixture","metadata":{"host":"openai.com","process":"Safari","destinationPort":443},"chains":["新加坡 SG-02","AI 自动选择"],"providerChains":[["Provider A","新加坡 SG-02"]],"rule":"DOMAIN-SUFFIX","rulePayload":"openai.com","upload":2048,"download":8192}]}"#
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
fn save_screenshot(
    cx: &mut gpui::VisualTestAppContext,
    window: gpui::AnyWindowHandle,
    file_name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    use std::path::PathBuf;

    let screenshot = cx.capture_screenshot(window)?;
    let output = PathBuf::from(".impeccable/review").join(file_name);
    std::fs::create_dir_all(".impeccable/review")?;
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
        let safe = temp.join(format!("relay-live-test-{}.png", std::process::id()));
        super::validate_live_output(&safe)?;

        let escaped = temp.join("..").join("relay-live-escaped.png");
        assert!(super::validate_live_output(&escaped).is_err());
        Ok(())
    }
}
