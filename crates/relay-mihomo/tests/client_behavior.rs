use std::cell::RefCell;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::Duration;

use relay_core::{PolicyCandidateKind, PolicyGroupKind, RoutingMode};
#[cfg(unix)]
use relay_mihomo::UnixSocketTransport;
use relay_mihomo::{
    ConnectionsState, ControllerConfig, ControllerTransport, GroupKind, MihomoClient, MihomoError,
    RuntimeConfig, RuntimeTunConfig, StdHttpTransport, to_policy_catalog,
};
use serde_json::Value;

#[derive(Default)]
struct FakeTransport {
    requests: RefCell<Vec<RecordedRequest>>,
}

#[derive(Debug, Clone, PartialEq)]
struct RecordedRequest {
    method: &'static str,
    path: String,
    body: Option<Value>,
}

impl RecordedRequest {
    fn get(path: &str) -> Self {
        Self {
            method: "GET",
            path: path.to_owned(),
            body: None,
        }
    }

    fn patch(path: &str, body: Value) -> Self {
        Self {
            method: "PATCH",
            path: path.to_owned(),
            body: Some(body),
        }
    }

    fn put(path: &str, body: Value) -> Self {
        Self {
            method: "PUT",
            path: path.to_owned(),
            body: Some(body),
        }
    }
}

impl ControllerTransport for FakeTransport {
    fn get(&self, _config: &ControllerConfig, path: &str) -> Result<String, MihomoError> {
        self.requests.borrow_mut().push(RecordedRequest::get(path));
        match path {
            "/version" => Ok(r#"{"meta":true,"version":"v1.19.0","ignored":1}"#.to_owned()),
            "/proxies" => Ok(proxy_fixture()),
            "/providers/proxies" => Ok(provider_fixture()),
            "/group/Auto%20%F0%9F%8C%90%2FHK/delay?url=https%3A%2F%2Fwww.gstatic.com%2Fgenerate_204%3Fx%3D1%26ok%3Dtrue&timeout=5000" => {
                Ok(r#"{"Japan 01":51,"US 01":238}"#.to_owned())
            }
            "/proxies/Japan%2001%20%5B%E5%80%8D%E7%8E%87%C3%971%5D/delay?url=http%3A%2F%2Fcp.cloudflare.com%2Fgenerate_204&timeout=1500" => {
                Ok(r#"{"delay":47}"#.to_owned())
            }
            "/proxies/Proxy%2F%F0%9F%8C%90%20Select" => Ok(
                r#"{"name":"Proxy/🌐 Select","type":"Selector","now":"Japan 01","all":["Japan 01","US 01"],"unexpected":true}"#
                    .to_owned(),
            ),
            "/rules" => Ok(rule_fixture()),
            "/connections" => Ok(connection_fixture()),
            "/configs" => Ok(config_fixture()),
            _ => Err(MihomoError::InvalidResponse(format!(
                "unexpected path {path}"
            ))),
        }
    }

    fn patch_json(
        &self,
        _config: &ControllerConfig,
        path: &str,
        body: &Value,
    ) -> Result<String, MihomoError> {
        self.requests
            .borrow_mut()
            .push(RecordedRequest::patch(path, body.clone()));
        match path {
            "/configs" => Ok(String::new()),
            _ => Err(MihomoError::InvalidResponse(format!(
                "unexpected patch path {path}"
            ))),
        }
    }

    fn put_json(
        &self,
        _config: &ControllerConfig,
        path: &str,
        body: &Value,
    ) -> Result<String, MihomoError> {
        self.requests
            .borrow_mut()
            .push(RecordedRequest::put(path, body.clone()));
        match path {
            "/proxies/Proxy%2F%F0%9F%8C%90%20Select" => Ok(String::new()),
            _ => Err(MihomoError::InvalidResponse(format!(
                "unexpected put path {path}"
            ))),
        }
    }
}

#[derive(Default)]
struct TunRejectedTransport {
    requests: RefCell<Vec<RecordedRequest>>,
}

impl ControllerTransport for TunRejectedTransport {
    fn get(&self, _config: &ControllerConfig, path: &str) -> Result<String, MihomoError> {
        self.requests.borrow_mut().push(RecordedRequest::get(path));
        Ok(r#"{"tun":{"enable":false}}"#.to_owned())
    }

    fn patch_json(
        &self,
        _config: &ControllerConfig,
        path: &str,
        body: &Value,
    ) -> Result<String, MihomoError> {
        self.requests
            .borrow_mut()
            .push(RecordedRequest::patch(path, body.clone()));
        Ok(String::new())
    }

    fn put_json(
        &self,
        _config: &ControllerConfig,
        path: &str,
        _body: &Value,
    ) -> Result<String, MihomoError> {
        Err(MihomoError::InvalidResponse(format!(
            "unexpected put path {path}"
        )))
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
        [
            RecordedRequest::get("/version"),
            RecordedRequest::get("/proxies"),
            RecordedRequest::get("/providers/proxies"),
            RecordedRequest::get("/rules"),
            RecordedRequest::get("/connections"),
            RecordedRequest::get("/configs")
        ]
    );
    assert_eq!(snapshot.version.version.as_deref(), Some("v1.19.0"));
    assert!(snapshot.version.meta);
    assert_eq!(snapshot.runtime.mixed_port, Some(7890));
    assert_eq!(snapshot.runtime.mode, RoutingMode::Rule);
    assert!(snapshot.runtime.tun.enable);

    Ok(())
}

#[test]
fn connections_accepts_null_as_an_empty_list() -> Result<(), Box<dyn std::error::Error>> {
    let state: ConnectionsState =
        serde_json::from_str(r#"{"downloadTotal":2048,"uploadTotal":1024,"connections":null}"#)?;

    assert_eq!(state.download_total, 2048);
    assert_eq!(state.upload_total, 1024);
    assert!(state.connections.is_empty());
    Ok(())
}

#[test]
fn provider_preview_requests_only_the_provider_endpoint() -> Result<(), Box<dyn std::error::Error>>
{
    let transport = FakeTransport::default();
    let client = MihomoClient::new(ControllerConfig::default(), &transport);

    let providers = client.fetch_proxy_providers()?;

    assert_eq!(
        transport.requests.borrow().as_slice(),
        [RecordedRequest::get("/providers/proxies")]
    );
    assert_eq!(providers.len(), 1);
    assert_eq!(providers[0].proxies.len(), 2);
    Ok(())
}

#[test]
fn fetch_group_delay_encodes_name_and_query_and_parses_node_latencies()
-> Result<(), Box<dyn std::error::Error>> {
    let transport = FakeTransport::default();
    let client = MihomoClient::new(ControllerConfig::default(), &transport);

    let delays = client.fetch_group_delay(
        "Auto 🌐/HK",
        "https://www.gstatic.com/generate_204?x=1&ok=true",
        5000,
    )?;

    assert_eq!(
        transport.requests.borrow().as_slice(),
        [RecordedRequest::get(
            "/group/Auto%20%F0%9F%8C%90%2FHK/delay?url=https%3A%2F%2Fwww.gstatic.com%2Fgenerate_204%3Fx%3D1%26ok%3Dtrue&timeout=5000"
        )]
    );
    assert_eq!(delays.get("Japan 01"), Some(&51));
    assert_eq!(delays.get("US 01"), Some(&238));
    Ok(())
}

#[test]
fn fetch_proxy_delay_encodes_name_and_query_and_parses_delay()
-> Result<(), Box<dyn std::error::Error>> {
    let transport = FakeTransport::default();
    let client = MihomoClient::new(ControllerConfig::default(), &transport);

    let delay = client.fetch_proxy_delay(
        "Japan 01 [倍率×1]",
        "http://cp.cloudflare.com/generate_204",
        1500,
    )?;

    assert_eq!(
        transport.requests.borrow().as_slice(),
        [RecordedRequest::get(
            "/proxies/Japan%2001%20%5B%E5%80%8D%E7%8E%87%C3%971%5D/delay?url=http%3A%2F%2Fcp.cloudflare.com%2Fgenerate_204&timeout=1500"
        )]
    );
    assert_eq!(delay, 47);
    Ok(())
}

#[test]
fn fetch_policy_group_details_encodes_name_and_tolerates_response_drift()
-> Result<(), Box<dyn std::error::Error>> {
    let transport = FakeTransport::default();
    let client = MihomoClient::new(ControllerConfig::default(), &transport);

    let group = client.fetch_policy_group("Proxy/🌐 Select")?;

    assert_eq!(
        transport.requests.borrow().as_slice(),
        [RecordedRequest::get(
            "/proxies/Proxy%2F%F0%9F%8C%90%20Select"
        )]
    );
    assert_eq!(group.name.as_deref(), Some("Proxy/🌐 Select"));
    assert_eq!(group.proxy_type.as_deref(), Some("Selector"));
    assert_eq!(group.current.as_deref(), Some("Japan 01"));
    assert_eq!(group.all, ["Japan 01", "US 01"]);
    Ok(())
}

#[test]
fn fetch_policy_group_details_accepts_missing_group_fields()
-> Result<(), Box<dyn std::error::Error>> {
    let group: relay_mihomo::MihomoPolicyGroup =
        serde_json::from_str(r#"{"name":"DIRECT","type":"Direct","ignored":true}"#)?;

    assert_eq!(group.name.as_deref(), Some("DIRECT"));
    assert_eq!(group.proxy_type.as_deref(), Some("Direct"));
    assert_eq!(group.current, None);
    assert!(group.all.is_empty());
    Ok(())
}

#[test]
fn select_policy_group_node_puts_exact_json_body_and_encoded_path()
-> Result<(), Box<dyn std::error::Error>> {
    let transport = FakeTransport::default();
    let client = MihomoClient::new(ControllerConfig::default(), &transport);

    client.select_policy_group_node("Proxy/🌐 Select", "US 01")?;

    assert_eq!(
        transport.requests.borrow().as_slice(),
        [RecordedRequest::put(
            "/proxies/Proxy%2F%F0%9F%8C%90%20Select",
            serde_json::json!({"name":"US 01"})
        )]
    );
    Ok(())
}

#[test]
fn parses_all_proxy_provider_nodes_for_source_browsing() -> Result<(), Box<dyn std::error::Error>> {
    let transport = FakeTransport::default();
    let snapshot = MihomoClient::new(ControllerConfig::default(), &transport).fetch_snapshot()?;

    assert_eq!(snapshot.providers.len(), 1);
    assert_eq!(snapshot.providers[0].name, "airport");
    assert_eq!(snapshot.providers[0].vehicle_type.as_deref(), Some("HTTP"));
    assert_eq!(snapshot.providers[0].proxies.len(), 2);
    assert_eq!(snapshot.providers[0].proxies[0].name, "Japan 01");
    assert_eq!(snapshot.providers[0].proxies[0].proxy_type, "VLESS");
    assert_eq!(
        snapshot.providers[0].proxies[0].latest_latency_ms(),
        Some(51.0)
    );
    assert_eq!(snapshot.providers[0].proxies[1].alive, Some(false));
    Ok(())
}

#[test]
fn subscription_metadata_entries_are_not_exposed_as_nodes() -> Result<(), Box<dyn std::error::Error>>
{
    let transport = FakeTransport::default();
    let snapshot = MihomoClient::new(ControllerConfig::default(), &transport).fetch_snapshot()?;

    assert!(
        snapshot.providers[0]
            .proxies
            .iter()
            .all(|proxy| !proxy.name.starts_with("剩余流量"))
    );
    assert!(
        snapshot
            .policy_groups()
            .iter()
            .flat_map(|group| &group.nodes)
            .all(|name| !name.starts_with("剩余流量"))
    );
    Ok(())
}

#[test]
fn readiness_fetches_only_the_version_endpoint() -> Result<(), Box<dyn std::error::Error>> {
    let transport = FakeTransport::default();
    let client = MihomoClient::new(ControllerConfig::default(), &transport);

    let version = client.fetch_version()?;

    assert_eq!(
        transport.requests.borrow().as_slice(),
        [RecordedRequest::get("/version")]
    );
    assert_eq!(version.version.as_deref(), Some("v1.19.0"));
    assert!(version.meta);
    Ok(())
}

#[test]
fn parses_flexible_proxy_json_and_extracts_policy_groups() -> Result<(), Box<dyn std::error::Error>>
{
    let transport = FakeTransport::default();
    let snapshot = MihomoClient::new(ControllerConfig::default(), &transport).fetch_snapshot()?;

    let groups = snapshot.policy_groups();

    assert_eq!(groups.len(), 3);
    assert_eq!(groups[0].name, "Proxy");
    assert_eq!(groups[0].kind, GroupKind::Selector);
    assert_eq!(groups[0].current.as_deref(), Some("Japan 01"));
    assert_eq!(groups[0].nodes, ["Japan 01", "US 01"]);
    assert_eq!(groups[0].latest_latency_ms, Some(38.0));
    assert_eq!(groups[0].provider_name.as_deref(), Some("airport"));
    assert_eq!(groups[1].kind, GroupKind::UrlTest);
    assert_eq!(groups[1].latest_latency_ms, None);
    assert_eq!(groups[2].name, "GLOBAL");

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
    assert_eq!(evidence[0].host.as_deref(), Some("93.184.216.34"));
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
fn fetch_runtime_config_parses_known_fields_and_tolerates_missing_values()
-> Result<(), Box<dyn std::error::Error>> {
    let runtime = MihomoClient::new(ControllerConfig::default(), FakeTransport::default())
        .fetch_runtime_config()?;

    assert_eq!(
        runtime,
        RuntimeConfig {
            port: Some(7891),
            socks_port: Some(7892),
            mixed_port: Some(7890),
            mode: RoutingMode::Rule,
            tun: RuntimeTunConfig { enable: true }
        }
    );

    let minimal: RuntimeConfig = serde_json::from_str(r#"{"mode":"rule"}"#)?;
    assert_eq!(minimal.mode, RoutingMode::Rule);
    assert_eq!(minimal.port, None);
    assert_eq!(minimal.socks_port, None);
    assert_eq!(minimal.mixed_port, None);
    assert!(!minimal.tun.enable);

    let global: RuntimeConfig = serde_json::from_str(r#"{"mode":"GLOBAL"}"#)?;
    assert_eq!(global.mode, RoutingMode::Global);

    for payload in [
        r"{}",
        r#"{"mode":null}"#,
        r#"{"mode":"unknown"}"#,
        r#"{"mode":42}"#,
    ] {
        let config: RuntimeConfig = serde_json::from_str(payload)?;
        assert_eq!(config.mode, RoutingMode::Rule, "{payload}");
    }

    Ok(())
}

#[test]
fn set_routing_mode_patches_exact_mode_body() -> Result<(), Box<dyn std::error::Error>> {
    let transport = FakeTransport::default();
    let client = MihomoClient::new(ControllerConfig::default(), &transport);

    client.set_routing_mode(RoutingMode::Global)?;

    assert_eq!(
        transport.requests.borrow().as_slice(),
        [RecordedRequest::patch(
            "/configs",
            serde_json::json!({"mode":"global"})
        )]
    );
    Ok(())
}

#[test]
fn set_tun_enabled_preserves_existing_tun_fields_in_patch_body()
-> Result<(), Box<dyn std::error::Error>> {
    let transport = FakeTransport::default();
    let client = MihomoClient::new(ControllerConfig::default(), &transport);

    client.set_tun_enabled(false)?;

    assert_eq!(
        transport.requests.borrow().as_slice(),
        [
            RecordedRequest::get("/configs"),
            RecordedRequest::patch(
                "/configs",
                serde_json::json!({
                    "tun": {
                        "enable": false,
                        "stack": "system",
                        "auto-route": true,
                        "dns-hijack": ["any:53"]
                    }
                })
            )
        ]
    );

    Ok(())
}

#[test]
fn set_tun_enabled_confirms_that_mihomo_keeps_it_active() -> Result<(), Box<dyn std::error::Error>>
{
    let transport = FakeTransport::default();
    let client = MihomoClient::new(ControllerConfig::default(), &transport);

    client.set_tun_enabled(true)?;

    let requests = transport.requests.borrow();
    assert_eq!(requests[0], RecordedRequest::get("/configs"));
    assert_eq!(requests[1].method, "PATCH");
    assert_eq!(
        requests[1].body,
        Some(serde_json::json!({
            "tun": {
                "enable": true,
                "stack": "system",
                "auto-route": true,
                "dns-hijack": ["any:53"]
            }
        }))
    );
    assert_eq!(
        requests[2..],
        [
            RecordedRequest::get("/configs"),
            RecordedRequest::get("/configs"),
            RecordedRequest::get("/configs"),
        ]
    );
    Ok(())
}

#[test]
fn set_tun_enabled_rejects_an_async_kernel_rollback() {
    let transport = TunRejectedTransport::default();
    let client = MihomoClient::new(ControllerConfig::default(), &transport);

    let error = client
        .set_tun_enabled(true)
        .expect_err("a rejected TUN startup must not be reported as enabled");

    assert!(matches!(error, MihomoError::InvalidResponse(_)));
    assert_eq!(
        transport.requests.borrow().as_slice(),
        [
            RecordedRequest::get("/configs"),
            RecordedRequest::patch("/configs", serde_json::json!({"tun":{"enable":true}})),
            RecordedRequest::get("/configs"),
        ]
    );
}

#[test]
fn maps_snapshot_to_owned_policy_catalog() -> Result<(), Box<dyn std::error::Error>> {
    let transport = FakeTransport::default();
    let snapshot = MihomoClient::new(ControllerConfig::default(), &transport).fetch_snapshot()?;

    let catalog = to_policy_catalog(&snapshot)?;
    let groups: Vec<_> = catalog.iter().collect();

    assert_eq!(groups.len(), 3);
    assert_eq!(groups[0].name, "Proxy");
    assert_eq!(groups[0].kind, PolicyGroupKind::Selector);
    assert_eq!(groups[0].target, "Japan 01");
    assert_eq!(groups[0].nodes[0].name, "Japan 01");
    assert_eq!(groups[0].nodes[0].kind, PolicyCandidateKind::Node);
    assert_eq!(groups[0].nodes[0].provider.as_deref(), Some("airport"));
    assert_eq!(groups[0].nodes[0].latency_ms, Some(51));
    assert_eq!(groups[0].nodes[1].latency_ms, None);
    assert_eq!(groups[0].rules_count(), 1);
    assert_eq!(groups[0].rules[0].hit_count, Some(12));
    assert_eq!(groups[1].kind, PolicyGroupKind::UrlTest);
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
fn std_http_transport_sends_json_patch_with_bearer_auth() -> Result<(), Box<dyn std::error::Error>>
{
    let (address, handle) =
        spawn_one_response_server("HTTP/1.1 204 No Content\r\nContent-Length: 0\r\n\r\n")?;

    let config = ControllerConfig::new(format!("http://{address}"))?
        .with_secret("controller-token")
        .with_timeouts(Duration::from_secs(1), Duration::from_secs(1));
    let body = serde_json::json!({"tun":{"enable":true}});
    let response = StdHttpTransport::default().patch_json(&config, "/configs", &body)?;
    let request = handle.join().map_err(|_| "server thread panicked")?;

    assert!(request.starts_with("PATCH /configs HTTP/1.1\r\n"));
    assert!(request.contains("Authorization: Bearer controller-token\r\n"));
    assert!(request.contains("Content-Type: application/json\r\n"));
    assert!(request.contains("Content-Length: 23\r\n"));
    assert!(request.ends_with(r#"{"tun":{"enable":true}}"#));
    assert_eq!(response, "");

    Ok(())
}

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
    assert!(request.contains("Authorization: Bearer controller-token\r\n"));
    assert!(request.contains("Content-Type: application/json\r\n"));
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
    assert!(request.contains("Authorization: Bearer controller-token\r\n"));
    assert!(request.contains("Content-Type: application/json\r\n"));
    assert!(request.contains("Content-Length: 16\r\n"));
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

#[cfg(unix)]
#[test]
fn unix_socket_transport_sends_json_patch_without_auth() -> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::net::UnixListener;
    use std::time::{SystemTime, UNIX_EPOCH};

    let unique = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let socket_path = std::env::temp_dir().join(format!(
        "relay-mihomo-patch-{}-{unique}.sock",
        std::process::id()
    ));
    let listener = UnixListener::bind(&socket_path)?;
    let server = std::thread::spawn(move || -> std::io::Result<String> {
        let (mut stream, _) = listener.accept()?;
        let mut request = [0_u8; 2048];
        let read = stream.read(&mut request)?;
        stream.write_all(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\n\r\n")?;
        Ok(String::from_utf8_lossy(&request[..read]).into_owned())
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
    assert!(request.contains("Content-Type: application/json\r\n"));
    assert!(!request.contains("Authorization:"));
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
        "relay-mihomo-mode-{}-{unique}.sock",
        std::process::id()
    ));
    let listener = UnixListener::bind(&socket_path)?;
    let server = std::thread::spawn(move || -> std::io::Result<String> {
        let (mut stream, _) = listener.accept()?;
        let mut request = [0_u8; 2048];
        let read = stream.read(&mut request)?;
        stream.write_all(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\n\r\n")?;
        Ok(String::from_utf8_lossy(&request[..read]).into_owned())
    });

    let config = ControllerConfig::default().with_secret("uds-token");
    MihomoClient::new(config, UnixSocketTransport::new(&socket_path))
        .set_routing_mode(RoutingMode::Rule)?;
    let request = server.join().map_err(|_| "server thread panicked")??;
    std::fs::remove_file(&socket_path)?;

    assert!(request.starts_with("PATCH /configs HTTP/1.1\r\n"));
    assert!(request.contains("Content-Type: application/json\r\n"));
    assert!(!request.contains("Authorization:"));
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
        "relay-mihomo-put-{}-{unique}.sock",
        std::process::id()
    ));
    let listener = UnixListener::bind(&socket_path)?;
    let server = std::thread::spawn(move || -> std::io::Result<String> {
        let (mut stream, _) = listener.accept()?;
        let mut request = [0_u8; 2048];
        let read = stream.read(&mut request)?;
        stream.write_all(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\n\r\n")?;
        Ok(String::from_utf8_lossy(&request[..read]).into_owned())
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
    assert!(request.contains("Content-Type: application/json\r\n"));
    assert!(!request.contains("Authorization:"));
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
        std::env::temp_dir().join(format!("relay-mihomo-{}-{unique}.sock", std::process::id()));
    let listener = UnixListener::bind(&socket_path)?;
    let server = std::thread::spawn(move || -> std::io::Result<String> {
        let (mut stream, _) = listener.accept()?;
        let mut request = [0_u8; 2048];
        let read = stream.read(&mut request)?;
        stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 13\r\n\r\n{\"meta\":true}")?;
        Ok(String::from_utf8_lossy(&request[..read]).into_owned())
    });

    let config = ControllerConfig::default().with_secret("uds-token");
    let body = UnixSocketTransport::new(&socket_path).get(&config, "/version")?;
    let request = server.join().map_err(|_| "server thread panicked")??;
    std::fs::remove_file(&socket_path)?;

    assert_eq!(body, r#"{"meta":true}"#);
    assert!(request.starts_with("GET /version HTTP/1.1\r\n"));
    assert!(!request.contains("Authorization:"));
    Ok(())
}

#[cfg(unix)]
#[test]
fn unix_socket_transport_rejects_non_socket_paths() -> Result<(), Box<dyn std::error::Error>> {
    use std::time::{SystemTime, UNIX_EPOCH};

    let unique = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let path = std::env::temp_dir().join(format!(
        "relay-mihomo-regular-{}-{unique}",
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

fn proxy_fixture() -> String {
    r#"
    {
      "proxies": {
        "Proxy": {
          "name": "Proxy",
          "type": "Selector",
          "now": "Japan 01",
          "all": ["Japan 01", "US 01", "剩余流量：96.83 GB"],
          "history": [{"delay": 44}, {"delay": null}, {"delay": 38}],
          "providerName": "airport",
          "alive": true,
          "fixed": "Japan 01",
          "hidden": false
        },
        "Auto": {
          "name": "Auto",
          "type": "url-test",
          "now": "US 01",
          "all": ["Japan 01", "US 01"],
          "history": null
        },
        "GLOBAL": {
          "name": "GLOBAL",
          "type": "Selector",
          "now": "DIRECT",
          "all": ["DIRECT", "Proxy"]
        },
        "Japan 01": {
          "name": "Japan 01",
          "type": "ss",
          "history": [{"delay": 51}],
          "provider-name": "airport"
        },
        "US 01": {
          "name": "US 01",
          "type": "ss",
          "history": [{"delay": 0}]
        }
      }
    }
    "#
    .to_owned()
}

fn provider_fixture() -> String {
    r#"
    {
      "providers": {
        "airport": {
          "name": "airport",
          "type": "Proxy",
          "vehicleType": "HTTP",
          "updatedAt": "2026-08-25T00:00:00Z",
          "proxies": [
            {
              "name": "Japan 01",
              "type": "VLESS",
              "alive": true,
              "history": [{"delay": 51}]
            },
            {
              "name": "US 01",
              "type": "Trojan",
              "alive": false,
              "history": []
            },
            {
              "name": "剩余流量：96.83 GB",
              "type": "Trojan",
              "alive": false,
              "history": [{"delay": 65535}]
            }
          ]
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
            "host": "",
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

fn config_fixture() -> String {
    r#"
    {
      "port": 7891,
      "socks-port": 7892,
      "mixed-port": 7890,
      "mode": "rule",
      "tun": {
        "enable": true,
        "stack": "system",
        "auto-route": true,
        "dns-hijack": ["any:53"]
      }
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
