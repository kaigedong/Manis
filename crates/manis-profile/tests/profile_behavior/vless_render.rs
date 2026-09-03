use super::*;

#[test]
fn vless_share_link_compiles_into_a_direct_proxy_and_policy_reference() {
    let vless = VlessProxy::parse_share_link(
        "vless://00000000-0000-4000-8000-000000000000@edge.example.invalid:443?encryption=none&security=reality&type=grpc&sni=cdn.example.invalid&fp=chrome&pbk=fixture-public-key&sid=0123456789abcdef&serviceName=manis#Tokyo%20Edge",
    )
    .expect("supported fixture VLESS link should parse");
    let proxy_name = vless.name().clone();
    let mut profile = Profile::qx_default(fixture_secret()).expect("default profile is valid");
    profile.proxies.push(OutboundProxy::Vless(vless));
    let PolicyGroupKind::Select { proxies, .. } = &mut profile.groups[0].kind else {
        panic!("the hidden global exit is a select group");
    };
    proxies.insert(0, PolicyRef::Proxy(proxy_name));

    let yaml = render_mihomo_yaml(&profile).expect("profile with VLESS renders");

    assert!(yaml.contains("proxies:\n  - name: \"Tokyo Edge\""));
    assert!(yaml.contains("type: \"vless\""));
    assert!(yaml.contains("server: \"edge.example.invalid\""));
    assert!(yaml.contains("port: 443"));
    assert!(yaml.contains("uuid: \"00000000-0000-4000-8000-000000000000\""));
    assert!(yaml.contains("network: \"grpc\""));
    assert!(yaml.contains("tls: true"));
    assert!(yaml.contains("servername: \"cdn.example.invalid\""));
    assert!(yaml.contains("client-fingerprint: \"chrome\""));
    assert!(yaml.contains("public-key: \"fixture-public-key\""));
    assert!(yaml.contains("short-id: \"0123456789abcdef\""));
    assert!(yaml.contains("grpc-service-name: \"manis\""));
    assert!(yaml.contains("- \"Tokyo Edge\""));
    assert!(
        yaml.find("proxies:").expect("direct proxies section")
            < yaml.find("proxy-providers:").expect("providers section")
    );
}

#[test]
fn vless_reality_tcp_accepts_an_empty_optional_header_type() {
    let vless = VlessProxy::parse_share_link(
        "vless://00000000-0000-4000-8000-000000000000@198.51.100.7:443?security=reality&encryption=none&pbk=fixture_reality-public-key&headerType=&fp=chrome&type=tcp&flow=xtls-rprx-vision&sni=cdn.example.invalid#Reality%20TCP",
    )
    .expect("an empty optional headerType should behave like an omitted field");
    let yaml = render_mihomo_yaml(
        &Profile::qx_sources(Vec::new(), vec![vless], 17_890)
            .expect("Reality TCP fixture should build a profile"),
    )
    .expect("Reality TCP fixture should render");

    assert!(yaml.contains("name: \"Reality TCP\""));
    assert!(yaml.contains("network: \"tcp\""));
    assert!(yaml.contains("flow: \"xtls-rprx-vision\""));
    assert!(yaml.contains("servername: \"cdn.example.invalid\""));
    assert!(yaml.contains("public-key: \"fixture_reality-public-key\""));
}

#[test]
fn vless_parser_is_fail_closed_and_redacts_credentials() {
    let input = "vless://00000000-0000-4000-8000-000000000000@edge.example.invalid:443?encryption=none&security=tls&type=ws&path=%2Fmanis&host=cdn.example.invalid#Saved";
    let vless = VlessProxy::parse_share_link(input).expect("supported fixture should parse");
    let debug = format!("{vless:?}");

    assert_eq!(debug, "VlessProxy(<redacted>)");
    assert!(!debug.contains("00000000"));
    assert!(!debug.contains("cdn.example.invalid"));
    assert!(VlessProxy::parse_share_link(
        "vless://00000000-0000-4000-8000-000000000000@edge.example.invalid:443?encryption=none&security=tls&type=ws&unsupported=secret-value#Saved",
    )
    .is_err());
    assert!(VlessProxy::parse_share_link(
        "vless://00000000-0000-4000-8000-000000000000@edge.example.invalid:443?encryption=none&type=ws&type=grpc#Saved",
    )
    .is_err());
    assert!(VlessProxy::parse_share_link(
        "vless://00000000-0000-4000-8000-000000000000@edge.example.invalid:443?encryption=auto#Bad",
    )
    .is_err());
    assert!(VlessProxy::parse_share_link(
        "vless://00000000-0000-4000-8000-000000000000@edge.example.invalid:443?security=none&sni=cdn.example.invalid#Bad",
    )
    .is_err());
    assert!(VlessProxy::parse_share_link(
        "vless://00000000-0000-4000-8000-000000000000@edge.example.invalid:443?security=reality&pbk=&type=tcp#Bad",
    )
    .is_err());
}
