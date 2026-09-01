use super::*;

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
fn reloading_config_uses_mihomos_forced_full_config_endpoint()
-> Result<(), Box<dyn std::error::Error>> {
    let transport = FakeTransport::default();
    let client = MihomoClient::new(ControllerConfig::default(), &transport);

    client.reload_config_payload("mode: rule\ntun:\n  enable: true\n")?;

    assert_eq!(
        transport.requests.borrow().as_slice(),
        [RecordedRequest::put(
            "/configs?force=true",
            serde_json::json!({
                "payload": "mode: rule\ntun:\n  enable: true\n"
            })
        )]
    );
    Ok(())
}

#[test]
fn disabling_tun_sends_a_minimal_patch_so_mihomo_can_release_routes()
-> Result<(), Box<dyn std::error::Error>> {
    let transport = FakeTransport::default();
    let client = MihomoClient::new(ControllerConfig::default(), &transport);

    client.set_tun_enabled(false)?;

    assert_eq!(
        transport.requests.borrow().as_slice(),
        [RecordedRequest::patch(
            "/configs",
            serde_json::json!({"tun": {"enable": false}})
        )]
    );

    Ok(())
}

#[test]
fn enabling_tun_preserves_mihomo_interface_auto_detection() -> Result<(), Box<dyn std::error::Error>>
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
                "auto-detect-interface": true,
                "dns-hijack": ["any:53", "tcp://any:53"]
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
fn enabling_tun_defaults_missing_or_null_configuration() -> Result<(), Box<dyn std::error::Error>> {
    for config in ["{}", r#"{"tun":null}"#, "[]"] {
        let transport = FakeTransport {
            initial_config: RefCell::new(Some(config)),
            ..FakeTransport::default()
        };
        let client = MihomoClient::new(ControllerConfig::default(), &transport);

        client.set_tun_enabled(true)?;

        assert_eq!(
            transport.requests.borrow()[1],
            RecordedRequest::patch(
                "/configs",
                serde_json::json!({"tun": {
                    "enable": true,
                    "dns-hijack": ["any:53", "tcp://any:53"]
                }})
            ),
            "configuration: {config}"
        );
    }
    Ok(())
}

#[test]
fn enabling_tun_rejects_non_object_fields_before_patching() {
    for config in [
        r#"{"tun":false}"#,
        r#"{"tun":1}"#,
        r#"{"tun":"invalid"}"#,
        r#"{"tun":[]}"#,
    ] {
        let transport = FakeTransport {
            initial_config: RefCell::new(Some(config)),
            ..FakeTransport::default()
        };
        let client = MihomoClient::new(ControllerConfig::default(), &transport);

        assert!(matches!(
            client.set_tun_enabled(true),
            Err(MihomoError::InvalidResponse(message))
                if message == "/configs tun field was not an object"
        ));
        assert_eq!(
            transport.requests.borrow().as_slice(),
            [RecordedRequest::get("/configs")],
            "configuration: {config}"
        );
    }
}

#[test]
fn set_tun_enabled_rejects_an_async_kernel_rollback() {
    let transport = TunRejectedTransport::default();
    let client = MihomoClient::new(ControllerConfig::default(), &transport);

    let error = client
        .set_tun_enabled(true)
        .expect_err("a rejected TUN startup must not be reported as enabled");

    assert!(matches!(&error, MihomoError::InvalidResponse(_)));
    assert!(error.to_string().contains("kernel remains available"));
    assert_eq!(
        transport.requests.borrow().as_slice(),
        [
            RecordedRequest::get("/configs"),
            RecordedRequest::patch(
                "/configs",
                serde_json::json!({"tun":{
                    "enable":true,
                    "dns-hijack":["any:53", "tcp://any:53"]
                }})
            ),
            RecordedRequest::get("/configs"),
            RecordedRequest::patch("/configs", serde_json::json!({"tun":{"enable":false}})),
        ]
    );
}

#[test]
fn maps_snapshot_to_owned_policy_catalog() -> Result<(), Box<dyn std::error::Error>> {
    let transport = FakeTransport::default();
    let snapshot = MihomoClient::new(ControllerConfig::default(), &transport).fetch_snapshot()?;

    let catalog = to_policy_catalog(&snapshot)?;
    let groups: Vec<_> = catalog.iter().collect();

    assert_eq!(groups.len(), 2);
    assert!(
        groups
            .iter()
            .all(|group| group.name != "GLOBAL" && group.name != "__MANIS_GLOBAL__")
    );
    assert_eq!(groups[0].name, "Proxy");
    assert_eq!(groups[0].kind, PolicyGroupKind::Selector);
    assert_eq!(groups[0].target.as_deref(), Some("Japan 01"));
    assert_eq!(groups[0].nodes[0].name, "Japan 01");
    assert_eq!(groups[0].nodes[0].kind, PolicyCandidateKind::Node);
    assert_eq!(groups[0].nodes[0].provider.as_deref(), Some("airport"));
    assert_eq!(groups[0].nodes[0].latency_ms, Some(51));
    assert_eq!(groups[0].nodes[1].latency_ms, None);
    assert_eq!(groups[0].rules_count(), 1);
    assert_eq!(groups[0].rules[0].hit_count, Some(12));
    assert_eq!(groups[1].kind, PolicyGroupKind::UrlTest);
    let routing_rules: Vec<_> = catalog.routing_rules().collect();
    assert_eq!(routing_rules.len(), 2);
    assert_eq!(routing_rules[0].target, "Proxy");
    assert_eq!(routing_rules[1].target, "DIRECT");
    assert!(routing_rules[1].disabled);
    Ok(())
}

#[test]
fn empty_policy_group_has_no_synthetic_target() -> Result<(), Box<dyn std::error::Error>> {
    let transport = FakeTransport::default();
    let mut snapshot =
        MihomoClient::new(ControllerConfig::default(), &transport).fetch_snapshot()?;
    let group = snapshot
        .proxies
        .iter_mut()
        .find(|proxy| proxy.name == "Proxy")
        .expect("fixture policy group");
    group.current = None;
    group.all.clear();

    let catalog = to_policy_catalog(&snapshot)?;
    let group = catalog
        .iter()
        .find(|group| group.name == "Proxy")
        .expect("converted policy group");

    assert_eq!(group.target, None);
    assert!(group.nodes.is_empty());
    Ok(())
}

#[test]
fn policy_catalog_recovers_node_metadata_from_its_provider()
-> Result<(), Box<dyn std::error::Error>> {
    let transport = FakeTransport::default();
    let mut snapshot =
        MihomoClient::new(ControllerConfig::default(), &transport).fetch_snapshot()?;
    snapshot.proxies.retain(|proxy| proxy.name != "US 01");

    let catalog = to_policy_catalog(&snapshot)?;
    let node = catalog
        .iter()
        .find(|group| group.name == "Proxy")
        .and_then(|group| group.nodes.iter().find(|node| node.name == "US 01"))
        .expect("provider-backed policy node");

    assert_eq!(node.detail, "Trojan");
    assert_eq!(node.provider.as_deref(), Some("airport"));
    assert_eq!(node.alive, Some(false));
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
    assert!(header_value(&request, "Host").is_some());
    assert_eq!(
        header_value(&request, "Authorization"),
        Some("Bearer controller-token")
    );
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
    assert_eq!(
        header_value(&request, "Authorization"),
        Some("Bearer controller-token")
    );
    assert_eq!(
        header_value(&request, "Content-Type"),
        Some("application/json")
    );
    assert_eq!(header_value(&request, "Content-Length"), Some("23"));
    assert!(request.ends_with(r#"{"tun":{"enable":true}}"#));
    assert_eq!(response, "");

    Ok(())
}
