use std::cell::RefCell;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::Duration;

use relay_mihomo::{
    ControllerConfig, GroupKind, MihomoClient, MihomoError, ReadonlyTransport, StdHttpTransport,
    to_policy_catalog,
};

#[derive(Default)]
struct FakeTransport {
    requests: RefCell<Vec<String>>,
}

impl ReadonlyTransport for FakeTransport {
    fn get(&self, _config: &ControllerConfig, path: &str) -> Result<String, MihomoError> {
        self.requests.borrow_mut().push(path.to_owned());
        match path {
            "/version" => Ok(r#"{"meta":true,"version":"v1.19.0","ignored":1}"#.to_owned()),
            "/proxies" => Ok(proxy_fixture()),
            "/rules" => Ok(rule_fixture()),
            "/connections" => Ok(connection_fixture()),
            _ => Err(MihomoError::InvalidResponse(format!(
                "unexpected path {path}"
            ))),
        }
    }
}

#[test]
fn config_defaults_and_redacts_secret() {
    let config = ControllerConfig::default().with_secret("top-secret");

    assert_eq!(config.base_url(), "http://127.0.0.1:9090");
    assert_eq!(config.host(), "127.0.0.1");
    assert_eq!(config.port(), 9090);

    let debug = format!("{config:?}");
    assert!(debug.contains("<redacted>"));
    assert!(!debug.contains("top-secret"));
}

#[test]
fn config_rejects_non_plain_controller_addresses() {
    for address in [
        "https://127.0.0.1:9443",
        "http://user:pass@127.0.0.1:9090",
        "http://127.0.0.1:9090/api",
        "http://127.0.0.1:9090?x=1",
        "http://127.0.0.1:9090#fragment",
        "http://127.0.0.1",
        "http://192.168.1.20:9090",
        "http://mihomo.example.com:9090",
    ] {
        let result = ControllerConfig::new(address);
        assert!(result.is_err(), "{address} should be rejected");
    }
}

#[test]
fn config_accepts_only_plaintext_loopback_hosts() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(
        ControllerConfig::new("http://localhost:9090")?.host(),
        "localhost"
    );
    assert_eq!(
        ControllerConfig::new("http://127.42.0.1:9090")?.host(),
        "127.42.0.1"
    );
    assert_eq!(ControllerConfig::new("http://[::1]:9090")?.host(), "::1");
    Ok(())
}

#[test]
fn fetch_snapshot_requests_exact_readonly_endpoints() -> Result<(), Box<dyn std::error::Error>> {
    let transport = FakeTransport::default();
    let client = MihomoClient::new(ControllerConfig::default(), &transport);

    let snapshot = client.fetch_snapshot()?;

    assert_eq!(
        transport.requests.borrow().as_slice(),
        ["/version", "/proxies", "/rules", "/connections"]
    );
    assert_eq!(snapshot.version.version.as_deref(), Some("v1.19.0"));
    assert!(snapshot.version.meta);

    Ok(())
}

#[test]
fn parses_flexible_proxy_json_and_extracts_policy_groups() -> Result<(), Box<dyn std::error::Error>>
{
    let transport = FakeTransport::default();
    let snapshot = MihomoClient::new(ControllerConfig::default(), &transport).fetch_snapshot()?;

    let groups = snapshot.policy_groups();

    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0].name, "Proxy");
    assert_eq!(groups[0].kind, GroupKind::Selector);
    assert_eq!(groups[0].current.as_deref(), Some("Japan 01"));
    assert_eq!(groups[0].nodes, ["Japan 01", "US 01"]);
    assert_eq!(groups[0].latest_latency_ms, Some(38.0));
    assert_eq!(groups[0].provider_name.as_deref(), Some("airport"));
    assert_eq!(groups[1].kind, GroupKind::UrlTest);
    assert_eq!(groups[1].latest_latency_ms, None);

    Ok(())
}

#[test]
fn keeps_rules_order_and_extra_hit_fields() -> Result<(), Box<dyn std::error::Error>> {
    let transport = FakeTransport::default();
    let snapshot = MihomoClient::new(ControllerConfig::default(), &transport).fetch_snapshot()?;

    assert_eq!(snapshot.rules[0].index, 0);
    assert_eq!(snapshot.rules[0].kind, "DOMAIN-SUFFIX");
    assert_eq!(snapshot.rules[0].proxy, "Proxy");
    assert_eq!(snapshot.rules[0].extra.hit, Some(12));
    assert_eq!(snapshot.rules[0].extra.miss, Some(3));
    assert_eq!(snapshot.rules[1].index, 1);
    assert_eq!(snapshot.rules[1].extra.disabled, Some(true));
    assert_eq!(
        snapshot.connections.connections[0]
            .metadata
            .destination_port
            .as_deref(),
        Some("443")
    );

    Ok(())
}

#[test]
fn exposes_observed_route_evidence_from_connections() -> Result<(), Box<dyn std::error::Error>> {
    let transport = FakeTransport::default();
    let snapshot = MihomoClient::new(ControllerConfig::default(), &transport).fetch_snapshot()?;

    assert_eq!(snapshot.connections.download_total, 2048);
    assert_eq!(snapshot.connections.upload_total, 1024);

    let evidence = snapshot.observed_routes();

    assert_eq!(evidence.len(), 1);
    assert_eq!(evidence[0].host.as_deref(), Some("example.com"));
    assert_eq!(evidence[0].process.as_deref(), Some("curl"));
    assert_eq!(evidence[0].rule.as_deref(), Some("DOMAIN-SUFFIX"));
    assert_eq!(evidence[0].rule_payload.as_deref(), Some("example.com"));
    assert_eq!(evidence[0].chains, ["Japan 01", "Proxy"]);
    assert_eq!(
        evidence[0].provider_chains,
        [vec!["airport".to_owned(), "Japan 01".to_owned()]]
    );

    Ok(())
}

#[test]
fn maps_snapshot_to_owned_policy_catalog() -> Result<(), Box<dyn std::error::Error>> {
    let transport = FakeTransport::default();
    let snapshot = MihomoClient::new(ControllerConfig::default(), &transport).fetch_snapshot()?;

    let catalog = to_policy_catalog(&snapshot)?;
    let groups: Vec<_> = catalog.iter().collect();

    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0].name, "Proxy");
    assert_eq!(groups[0].kind, "手动选择");
    assert_eq!(groups[0].target, "Japan 01");
    assert_eq!(groups[0].nodes[0].name, "Japan 01");
    assert_eq!(groups[0].nodes[0].provider.as_deref(), Some("airport"));
    assert_eq!(groups[0].nodes[0].latency_ms, Some(51));
    assert_eq!(groups[0].rules_count(), 1);
    assert_eq!(groups[0].rules[0].hit_count, Some(12));
    assert_eq!(groups[1].kind, "自动测速");
    Ok(())
}

#[test]
fn std_http_transport_sends_bearer_auth_and_accepts_chunked()
-> Result<(), Box<dyn std::error::Error>> {
    let (address, handle) = spawn_one_response_server(
        "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nB\r\n{\"ok\":true}\r\n0\r\n\r\n",
    )?;

    let config = ControllerConfig::new(format!("http://{address}"))?
        .with_secret("controller-token")
        .with_timeouts(Duration::from_secs(1), Duration::from_secs(1));
    let body = StdHttpTransport::default().get(&config, "/version")?;
    let request = handle.join().map_err(|_| "server thread panicked")?;

    assert!(request.starts_with("GET /version HTTP/1.1\r\n"));
    assert!(request.contains("Host: "));
    assert!(request.contains("Authorization: Bearer controller-token\r\n"));
    assert_eq!(body, r#"{"ok":true}"#);

    Ok(())
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

fn proxy_fixture() -> String {
    r#"
    {
      "proxies": {
        "Proxy": {
          "name": "Proxy",
          "type": "Selector",
          "now": "Japan 01",
          "all": ["Japan 01", "US 01"],
          "history": [{"delay": 44}, {"delay": null}, {"delay": 38}],
          "providerName": "airport",
          "alive": true,
          "fixed": false,
          "hidden": false
        },
        "Auto": {
          "name": "Auto",
          "type": "url-test",
          "now": "US 01",
          "all": ["Japan 01", "US 01"],
          "history": null
        },
        "Japan 01": {
          "name": "Japan 01",
          "type": "ss",
          "history": [{"delay": 51}],
          "provider-name": "airport"
        }
      }
    }
    "#
    .to_owned()
}

fn rule_fixture() -> String {
    r#"
    {
      "rules": [
        {
          "index": 0,
          "type": "DOMAIN-SUFFIX",
          "payload": "example.com",
          "proxy": "Proxy",
          "size": -1,
          "extra": {"hitCount": 12, "missCount": 3, "disabled": false, "ignored": true}
        },
        {
          "index": 1,
          "type": "MATCH",
          "payload": "",
          "proxy": "DIRECT",
          "size": -1,
          "extra": {"disabled": true}
        }
      ]
    }
    "#
    .to_owned()
}

fn connection_fixture() -> String {
    r#"
    {
      "downloadTotal": 2048,
      "uploadTotal": 1024,
      "connections": [
        {
          "id": "abc",
          "metadata": {
            "host": "example.com",
            "destinationIP": "93.184.216.34",
            "process": "curl",
            "destinationPort": 443,
            "network": "tcp",
            "type": "HTTP"
          },
          "chains": ["Japan 01", "Proxy"],
          "providerChains": [["airport", "Japan 01"]],
          "rule": "DOMAIN-SUFFIX",
          "rulePayload": "example.com",
          "upload": 100,
          "download": 200,
          "start": "2026-08-25T00:00:00Z"
        }
      ]
    }
    "#
    .to_owned()
}

fn spawn_one_response_server(
    response: &str,
) -> Result<(String, std::thread::JoinHandle<String>), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?.to_string();
    let response = response.to_owned();
    let handle = std::thread::spawn(move || {
        let Ok((mut stream, _peer)) = listener.accept() else {
            return String::new();
        };
        let mut request = Vec::new();
        let mut buffer = [0_u8; 1024];
        while let Ok(read) = stream.read(&mut buffer) {
            if read == 0 {
                break;
            }
            request.extend_from_slice(&buffer[..read]);
            if request.windows(4).any(|window| window == b"\r\n\r\n") {
                break;
            }
        }
        let _ = stream.write_all(response.as_bytes());
        String::from_utf8_lossy(&request).into_owned()
    });

    Ok((address, handle))
}
