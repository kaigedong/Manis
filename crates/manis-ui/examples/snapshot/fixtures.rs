use super::scenarios::snapshot_hex;

#[cfg(target_os = "macos")]
pub(super) fn write_source_cards_fixture(
    store: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
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
pub(super) struct SubscriptionFixtureServer {
    url: String,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    thread: std::thread::JoinHandle<std::io::Result<()>>,
}

#[cfg(target_os = "macos")]
impl SubscriptionFixtureServer {
    pub(super) fn start() -> std::io::Result<Self> {
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

    pub(super) fn url(&self) -> &str {
        &self.url
    }

    pub(super) fn stop(self) -> Result<(), Box<dyn std::error::Error>> {
        use std::sync::atomic::Ordering;

        self.stop.store(true, Ordering::Relaxed);
        self.thread
            .join()
            .map_err(|_| "fixture server panicked")??;
        Ok(())
    }
}

#[cfg(target_os = "macos")]
pub(super) fn serve_subscription_fixture(
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
pub(super) fn write_managed_policy_fixture(
    store: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
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
pub(super) fn spawn_mihomo_fixture() -> Result<(String, FixtureServer), Box<dyn std::error::Error>>
{
    spawn_mihomo_fixture_with_stream_failure(false)
}

#[cfg(target_os = "macos")]
pub(super) fn spawn_mihomo_fixture_with_stream_failure(
    fail_streams: bool,
) -> Result<(String, FixtureServer), Box<dyn std::error::Error>> {
    spawn_mihomo_fixture_with_response(fail_streams, |path| fixture_response(path).to_owned())
}

#[cfg(target_os = "macos")]
pub(super) fn spawn_mihomo_fixture_with_response(
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
pub(super) fn spawn_empty_mihomo_fixture()
-> Result<(String, FixtureServer), Box<dyn std::error::Error>> {
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
pub(super) struct FixtureServer {
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    server: std::thread::JoinHandle<Result<(), std::io::Error>>,
}

#[cfg(target_os = "macos")]
impl FixtureServer {
    pub(super) fn stop(self) -> Result<(), Box<dyn std::error::Error>> {
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
        self.server
            .join()
            .map_err(|_| "Mihomo fixture server thread panicked")??;
        Ok(())
    }
}

#[cfg(target_os = "macos")]
pub(super) fn fixture_response(path: &str) -> &'static str {
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
