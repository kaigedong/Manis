use super::*;

#[test]
fn std_http_client_sends_routing_mode_patch() -> Result<(), Box<dyn std::error::Error>> {
    let (address, handle) =
        spawn_one_response_server("HTTP/1.1 204 No Content\r\nContent-Length: 0\r\n\r\n")?;

    let config = ControllerConfig::new(format!("http://{address}"))?
        .with_secret("controller-token")
        .with_timeouts(Duration::from_secs(1), Duration::from_secs(1));
    MihomoClient::new(config, StdHttpTransport::default()).set_routing_mode(RoutingMode::Direct)?;
    let request = handle.join().map_err(|_| "server thread panicked")?;

    assert!(request.starts_with("PATCH /configs HTTP/1.1\r\n"));
    assert_eq!(
        header_value(&request, "Authorization"),
        Some("Bearer controller-token")
    );
    assert_eq!(
        header_value(&request, "Content-Type"),
        Some("application/json")
    );
    assert!(request.ends_with(r#"{"mode":"direct"}"#));

    Ok(())
}

#[test]
fn std_http_transport_sends_json_put_with_bearer_auth() -> Result<(), Box<dyn std::error::Error>> {
    let (address, handle) =
        spawn_one_response_server("HTTP/1.1 204 No Content\r\nContent-Length: 0\r\n\r\n")?;

    let config = ControllerConfig::new(format!("http://{address}"))?
        .with_secret("controller-token")
        .with_timeouts(Duration::from_secs(1), Duration::from_secs(1));
    let body = serde_json::json!({"name":"US 01"});
    let response = StdHttpTransport::default().put_json(&config, "/proxies/Proxy", &body)?;
    let request = handle.join().map_err(|_| "server thread panicked")?;

    assert!(request.starts_with("PUT /proxies/Proxy HTTP/1.1\r\n"));
    assert_eq!(
        header_value(&request, "Authorization"),
        Some("Bearer controller-token")
    );
    assert_eq!(
        header_value(&request, "Content-Type"),
        Some("application/json")
    );
    assert_eq!(header_value(&request, "Content-Length"), Some("16"));
    assert!(request.ends_with(r#"{"name":"US 01"}"#));
    assert_eq!(response, "");

    Ok(())
}

#[test]
fn std_http_transport_rejects_non_absolute_patch_paths() {
    let result = StdHttpTransport::default().patch_json(
        &ControllerConfig::default(),
        "configs",
        &serde_json::json!({"tun":{"enable":true}}),
    );

    assert!(matches!(result, Err(MihomoError::InvalidRequestPath(_))));
}

#[test]
fn std_http_transport_rejects_non_absolute_put_paths() {
    let result = StdHttpTransport::default().put_json(
        &ControllerConfig::default(),
        "proxies/Proxy",
        &serde_json::json!({"name":"US 01"}),
    );

    assert!(matches!(result, Err(MihomoError::InvalidRequestPath(_))));
}

#[test]
fn std_http_transport_rejects_http_error_status() -> Result<(), Box<dyn std::error::Error>> {
    let (address, handle) = spawn_one_response_server(
        "HTTP/1.1 401 Unauthorized\r\nContent-Length: 12\r\n\r\nsecret denied",
    )?;
    let config = ControllerConfig::new(format!("http://{address}"))?;

    let err = StdHttpTransport::default()
        .get(&config, "/version")
        .expect_err("401 should fail");
    let request = handle.join().map_err(|_| "server thread panicked")?;

    assert!(request.starts_with("GET /version HTTP/1.1\r\n"));
    assert!(matches!(
        err,
        MihomoError::HttpStatus {
            status_code: 401,
            ..
        }
    ));
    assert!(!format!("{err}").contains("controller-token"));
    assert!(!format!("{err}").contains("secret denie"));

    Ok(())
}

#[test]
fn std_http_transport_waits_for_delayed_response_headers_with_read_timeout()
-> Result<(), Box<dyn std::error::Error>> {
    let delay = Duration::from_millis(220);
    let (address, handle) = spawn_delayed_response_server(
        delay,
        "HTTP/1.1 200 OK\r\nContent-Length: 13\r\n\r\n{\"meta\":true}",
    )?;
    let config = ControllerConfig::new(format!("http://{address}"))?
        .with_timeouts(Duration::from_millis(60), Duration::from_millis(800));

    let started = Instant::now();
    let body = StdHttpTransport::default().get(&config, "/version")?;
    let request = handle.join().map_err(|_| "server thread panicked")?;

    assert_eq!(body, r#"{"meta":true}"#);
    assert!(request.starts_with("GET /version HTTP/1.1\r\n"));
    assert!(started.elapsed() >= delay);
    assert!(started.elapsed() < Duration::from_secs(1));
    Ok(())
}

#[test]
fn std_http_transport_accepts_fragmented_response_headers_within_read_timeout()
-> Result<(), Box<dyn std::error::Error>> {
    let (address, handle) = spawn_fragmented_response_header_server()?;
    let config = ControllerConfig::new(format!("http://{address}"))?
        .with_timeouts(Duration::from_millis(40), Duration::from_secs(2));

    let response = StdHttpTransport::default().get(&config, "/version");
    let served = handle.join().map_err(|_| "server thread panicked")?;
    let body = response?;
    let request = served?;

    assert_eq!(body, r#"{"meta":true}"#);
    assert!(request.starts_with("GET /version HTTP/1.1\r\n"));
    Ok(())
}

#[test]
fn std_http_transport_uses_body_timeout_after_complete_response_headers()
-> Result<(), Box<dyn std::error::Error>> {
    let delay = Duration::from_millis(220);
    let (address, handle) = spawn_delayed_body_server(delay)?;
    let config = ControllerConfig::new(format!("http://{address}"))?
        .with_timeouts(Duration::from_millis(60), Duration::from_millis(800));

    let started = Instant::now();
    let body = StdHttpTransport::default().get(&config, "/version")?;
    let request = handle.join().map_err(|_| "server thread panicked")?;

    assert_eq!(body, r#"{"meta":true}"#);
    assert!(request.starts_with("GET /version HTTP/1.1\r\n"));
    assert!(started.elapsed() >= delay);
    assert!(started.elapsed() < Duration::from_secs(1));
    Ok(())
}

#[test]
fn std_http_transport_bounds_incomplete_response_headers() -> Result<(), Box<dyn std::error::Error>>
{
    let (address, handle) = spawn_trickling_incomplete_header_server()?;
    let config = ControllerConfig::new(format!("http://{address}"))?
        .with_timeouts(Duration::from_millis(40), Duration::from_millis(180));

    let started = Instant::now();
    let error = StdHttpTransport::default()
        .get(&config, "/version")
        .expect_err("incomplete headers should time out");
    let elapsed = started.elapsed();
    let request = handle.join().map_err(|_| "server thread panicked")?;

    assert!(request.starts_with("GET /version HTTP/1.1\r\n"));
    assert!(
        matches!(error, MihomoError::Io(ref source) if source.kind() == std::io::ErrorKind::TimedOut)
    );
    assert!(elapsed < Duration::from_millis(700));
    Ok(())
}

#[cfg(unix)]
#[test]
fn unix_socket_transport_sends_json_patch_without_auth() -> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::net::UnixListener;
    use std::time::{SystemTime, UNIX_EPOCH};

    let unique = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let socket_path = std::env::temp_dir().join(format!(
        "manis-mihomo-patch-{}-{unique}.sock",
        std::process::id()
    ));
    let listener = UnixListener::bind(&socket_path)?;
    let server = std::thread::spawn(move || -> std::io::Result<String> {
        let (mut stream, _) = listener.accept()?;
        let request = read_request(&mut stream)?;
        stream.write_all(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\n\r\n")?;
        Ok(request)
    });

    let config = ControllerConfig::default().with_secret("uds-token");
    UnixSocketTransport::new(&socket_path).patch_json(
        &config,
        "/configs",
        &serde_json::json!({"tun":{"enable":false}}),
    )?;
    let request = server.join().map_err(|_| "server thread panicked")??;
    std::fs::remove_file(&socket_path)?;

    assert!(request.starts_with("PATCH /configs HTTP/1.1\r\n"));
    assert_eq!(
        header_value(&request, "Content-Type"),
        Some("application/json")
    );
    assert!(header_value(&request, "Authorization").is_none());
    assert!(request.ends_with(r#"{"tun":{"enable":false}}"#));
    Ok(())
}

#[cfg(unix)]
#[test]
fn unix_socket_client_sends_routing_mode_patch() -> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::net::UnixListener;
    use std::time::{SystemTime, UNIX_EPOCH};

    let unique = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let socket_path = std::env::temp_dir().join(format!(
        "manis-mihomo-mode-{}-{unique}.sock",
        std::process::id()
    ));
    let listener = UnixListener::bind(&socket_path)?;
    let server = std::thread::spawn(move || -> std::io::Result<String> {
        let (mut stream, _) = listener.accept()?;
        let request = read_request(&mut stream)?;
        stream.write_all(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\n\r\n")?;
        Ok(request)
    });

    let config = ControllerConfig::default().with_secret("uds-token");
    MihomoClient::new(config, UnixSocketTransport::new(&socket_path))
        .set_routing_mode(RoutingMode::Rule)?;
    let request = server.join().map_err(|_| "server thread panicked")??;
    std::fs::remove_file(&socket_path)?;

    assert!(request.starts_with("PATCH /configs HTTP/1.1\r\n"));
    assert_eq!(
        header_value(&request, "Content-Type"),
        Some("application/json")
    );
    assert!(header_value(&request, "Authorization").is_none());
    assert!(request.ends_with(r#"{"mode":"rule"}"#));
    Ok(())
}

#[cfg(unix)]
#[test]
fn unix_socket_transport_sends_json_put_without_auth() -> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::net::UnixListener;
    use std::time::{SystemTime, UNIX_EPOCH};

    let unique = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let socket_path = std::env::temp_dir().join(format!(
        "manis-mihomo-put-{}-{unique}.sock",
        std::process::id()
    ));
    let listener = UnixListener::bind(&socket_path)?;
    let server = std::thread::spawn(move || -> std::io::Result<String> {
        let (mut stream, _) = listener.accept()?;
        let request = read_request(&mut stream)?;
        stream.write_all(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\n\r\n")?;
        Ok(request)
    });

    let config = ControllerConfig::default().with_secret("uds-token");
    UnixSocketTransport::new(&socket_path).put_json(
        &config,
        "/proxies/Proxy",
        &serde_json::json!({"name":"US 01"}),
    )?;
    let request = server.join().map_err(|_| "server thread panicked")??;
    std::fs::remove_file(&socket_path)?;

    assert!(request.starts_with("PUT /proxies/Proxy HTTP/1.1\r\n"));
    assert_eq!(
        header_value(&request, "Content-Type"),
        Some("application/json")
    );
    assert!(header_value(&request, "Authorization").is_none());
    assert!(request.ends_with(r#"{"name":"US 01"}"#));
    Ok(())
}

#[test]
fn std_http_transport_caps_response_body() -> Result<(), Box<dyn std::error::Error>> {
    let body = "x".repeat(32);
    let response = format!("HTTP/1.1 200 OK\r\nContent-Length: 32\r\n\r\n{body}");
    let (address, handle) = spawn_one_response_server(&response)?;
    let config = ControllerConfig::new(format!("http://{address}"))?;

    let err = StdHttpTransport::with_body_limit(16)
        .get(&config, "/version")
        .expect_err("oversized body should fail");
    let _request = handle.join().map_err(|_| "server thread panicked")?;

    assert!(matches!(err, MihomoError::BodyTooLarge { limit: 16 }));

    Ok(())
}

#[cfg(unix)]
#[test]
fn unix_socket_transport_sends_readonly_http_request() -> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::net::UnixListener;
    use std::time::{SystemTime, UNIX_EPOCH};

    let unique = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let socket_path =
        std::env::temp_dir().join(format!("manis-mihomo-{}-{unique}.sock", std::process::id()));
    let listener = UnixListener::bind(&socket_path)?;
    let server = std::thread::spawn(move || -> std::io::Result<String> {
        let (mut stream, _) = listener.accept()?;
        let request = read_request(&mut stream)?;
        stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 13\r\n\r\n{\"meta\":true}")?;
        Ok(request)
    });

    let config = ControllerConfig::default().with_secret("uds-token");
    let body = UnixSocketTransport::new(&socket_path).get(&config, "/version")?;
    let request = server.join().map_err(|_| "server thread panicked")??;
    std::fs::remove_file(&socket_path)?;

    assert_eq!(body, r#"{"meta":true}"#);
    assert!(request.starts_with("GET /version HTTP/1.1\r\n"));
    assert!(header_value(&request, "Authorization").is_none());
    Ok(())
}

#[cfg(unix)]
#[test]
fn unix_socket_transport_waits_for_delayed_get_headers_with_read_timeout()
-> Result<(), Box<dyn std::error::Error>> {
    let delay = Duration::from_millis(220);
    let (socket_path, server) = spawn_delayed_unix_response_server(
        "delayed-get",
        delay,
        "HTTP/1.1 200 OK\r\nContent-Length: 13\r\n\r\n{\"meta\":true}",
    )?;
    let config = ControllerConfig::default()
        .with_secret("uds-token")
        .with_timeouts(Duration::from_millis(60), Duration::from_millis(800));

    let started = Instant::now();
    let body = UnixSocketTransport::new(&socket_path).get(&config, "/version")?;
    let request = server.join().map_err(|_| "server thread panicked")??;
    std::fs::remove_file(&socket_path)?;

    assert_eq!(body, r#"{"meta":true}"#);
    assert!(request.starts_with("GET /version HTTP/1.1\r\n"));
    assert!(started.elapsed() >= delay);
    assert!(started.elapsed() < Duration::from_secs(1));
    Ok(())
}

#[cfg(unix)]
#[test]
fn unix_socket_transport_waits_for_delayed_write_response_headers_with_read_timeout()
-> Result<(), Box<dyn std::error::Error>> {
    for (label, method) in [("p", "PATCH"), ("u", "PUT")] {
        let delay = Duration::from_millis(220);
        let (socket_path, server) = spawn_delayed_unix_response_server(
            label,
            delay,
            "HTTP/1.1 204 No Content\r\nContent-Length: 0\r\n\r\n",
        )?;
        let config = ControllerConfig::default()
            .with_secret("uds-token")
            .with_timeouts(Duration::from_millis(60), Duration::from_millis(800));
        let transport = UnixSocketTransport::new(&socket_path);

        let started = Instant::now();
        match method {
            "PATCH" => transport.patch_json(
                &config,
                "/configs",
                &serde_json::json!({"tun":{"enable":false}}),
            )?,
            "PUT" => transport.put_json(
                &config,
                "/proxies/Proxy",
                &serde_json::json!({"name":"US 01"}),
            )?,
            _ => unreachable!(),
        };
        let request = server.join().map_err(|_| "server thread panicked")??;
        std::fs::remove_file(&socket_path)?;

        assert!(request.starts_with(&format!("{method} ")));
        assert_eq!(
            header_value(&request, "Content-Type"),
            Some("application/json")
        );
        assert!(started.elapsed() >= delay);
        assert!(started.elapsed() < Duration::from_secs(1));
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn unix_socket_transport_rejects_non_socket_paths() -> Result<(), Box<dyn std::error::Error>> {
    use std::time::{SystemTime, UNIX_EPOCH};

    let unique = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let path = std::env::temp_dir().join(format!(
        "manis-mihomo-regular-{}-{unique}",
        std::process::id()
    ));
    std::fs::write(&path, b"not a socket")?;

    let error = UnixSocketTransport::new(&path)
        .get(&ControllerConfig::default(), "/version")
        .expect_err("regular files must be rejected before connecting");
    std::fs::remove_file(path)?;

    assert!(matches!(error, MihomoError::InvalidConfig(_)));
    Ok(())
}
