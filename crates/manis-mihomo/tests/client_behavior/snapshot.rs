use super::*;

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
        "http://username@127.0.0.1:9090",
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
fn connections_preserve_sniffed_and_remote_targets() -> Result<(), Box<dyn std::error::Error>> {
    let state: ConnectionsState = serde_json::from_str(
        r#"{"connections":[{"metadata":{"host":"","sniffHost":"www.example.com","destinationIP":"","remoteDestination":"203.0.113.8","destinationPort":"443"}}]}"#,
    )?;
    let metadata = &state.connections[0].metadata;

    assert_eq!(metadata.sniff_host.as_deref(), Some("www.example.com"));
    assert_eq!(metadata.remote_destination.as_deref(), Some("203.0.113.8"));
    assert_eq!(metadata.target_host(), Some("www.example.com"));

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
    assert_eq!(providers[0].proxies.len(), 3);
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
fn fetch_provider_proxy_delay_uses_provider_healthcheck_endpoint()
-> Result<(), Box<dyn std::error::Error>> {
    let transport = FakeTransport::default();
    let client = MihomoClient::new(ControllerConfig::default(), &transport);

    let delay = client.fetch_provider_proxy_delay(
        "Subscription 1",
        "HK 01",
        "http://cp.cloudflare.com/generate_204",
        1500,
    )?;

    assert_eq!(
        transport.requests.borrow().as_slice(),
        [RecordedRequest::get(
            "/providers/proxies/Subscription%201/HK%2001/healthcheck?url=http%3A%2F%2Fcp.cloudflare.com%2Fgenerate_204&timeout=1500"
        )]
    );
    assert_eq!(delay, 63);
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
    let group: manis_mihomo::MihomoPolicyGroup =
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
    assert_eq!(snapshot.providers[0].proxies.len(), 3);
    assert_eq!(snapshot.providers[0].proxies[0].name, "Japan 01");
    assert_eq!(snapshot.providers[0].proxies[0].proxy_type, "VLESS");
    assert_eq!(
        snapshot.providers[0].proxies[0].latest_latency_ms(),
        Some(51.0)
    );
    assert_eq!(snapshot.providers[0].proxies[1].alive, Some(false));
    assert_eq!(snapshot.providers[0].proxies[2].name, "剩余流量：96.83 GB");
    Ok(())
}

#[test]
fn subscription_information_entries_remain_visible() -> Result<(), Box<dyn std::error::Error>> {
    let transport = FakeTransport::default();
    let snapshot = MihomoClient::new(ControllerConfig::default(), &transport).fetch_snapshot()?;

    assert!(
        snapshot.providers[0]
            .proxies
            .iter()
            .any(|proxy| proxy.name == "剩余流量：96.83 GB")
    );
    assert!(
        snapshot
            .policy_groups()
            .iter()
            .flat_map(|group| &group.nodes)
            .any(|name| name == "剩余流量：96.83 GB")
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

    assert_eq!(groups.len(), 4);
    let proxy = groups
        .iter()
        .find(|group| group.name == "Proxy")
        .expect("Proxy group");
    assert_eq!(proxy.kind, GroupKind::Selector);
    assert_eq!(proxy.current.as_deref(), Some("Japan 01"));
    assert_eq!(proxy.nodes, ["Japan 01", "US 01", "剩余流量：96.83 GB"]);
    assert_eq!(proxy.latest_latency_ms, Some(38.0));
    assert_eq!(proxy.provider_name.as_deref(), Some("airport"));
    let auto = groups
        .iter()
        .find(|group| group.name == "Auto")
        .expect("Auto group");
    assert_eq!(auto.kind, GroupKind::UrlTest);
    assert_eq!(auto.latest_latency_ms, None);
    assert!(groups.iter().any(|group| group.name == "GLOBAL"));
    assert!(groups.iter().any(|group| group.name == "__MANIS_GLOBAL__"));

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
