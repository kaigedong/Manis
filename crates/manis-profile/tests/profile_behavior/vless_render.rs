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
fn manual_reality_tcp_profile_renders_as_sing_box_json() {
    let vless = VlessProxy::parse_share_link(
        "vless://00000000-0000-4000-8000-000000000000@198.51.100.7:443?security=reality&encryption=none&pbk=fixture_reality-public-key&headerType=&fp=chrome&type=tcp&flow=xtls-rprx-vision&sni=cdn.example.invalid#Reality%20TCP",
    )
    .expect("Reality TCP fixture should parse");
    let profile = Profile::qx_sources(Vec::new(), vec![vless], 17_890)
        .expect("manual VLESS fixture should build a profile");
    let json = render_sing_box_json(
        &profile,
        &SingBoxOptions::new("127.0.0.1:19090", "fixture-controller-secret"),
    )
    .expect("supported profile should render");

    assert!(json.contains("\"type\": \"mixed\""));
    assert!(json.contains("\"listen\": \"127.0.0.1\""));
    assert!(json.contains("\"listen_port\": 17890"));
    assert!(json.contains("\"type\": \"vless\""));
    assert!(json.contains("\"tag\": \"Reality TCP\""));
    assert!(json.contains("\"server\": \"198.51.100.7\""));
    assert!(json.contains("\"server_port\": 443"));
    assert!(json.contains("\"flow\": \"xtls-rprx-vision\""));
    assert!(json.contains("\"server_name\": \"cdn.example.invalid\""));
    assert!(json.contains("\"public_key\": \"fixture_reality-public-key\""));
    assert!(json.contains("\"fingerprint\": \"chrome\""));
    assert!(json.contains("\"type\": \"selector\""));
    assert!(json.contains("\"tag\": \"GLOBAL\""));
    let document: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(
        document["route"]["rules"][1],
        serde_json::json!({
            "clash_mode": "Global", "action": "route", "outbound": "GLOBAL"
        })
    );
    assert!(!json.contains("\"rule_set\": \"geoip-cn\""));
    assert!(json.contains("\"external_controller\": \"127.0.0.1:19090\""));
    assert!(json.contains("\"secret\": \"fixture-controller-secret\""));
}

#[test]
fn terminal_match_maps_to_mihomo_match_and_sing_box_route_final() {
    let vless = VlessProxy::parse_share_link(
        "vless://00000000-0000-4000-8000-000000000000@198.51.100.7:443?security=reality&encryption=none&pbk=fixture_reality-public-key&fp=chrome&type=tcp&flow=xtls-rprx-vision&sni=cdn.example.invalid#Reality%20TCP",
    )
    .expect("Reality TCP fixture should parse");
    let mut profile = Profile::qx_sources(Vec::new(), vec![vless], 17_890)
        .expect("manual VLESS fixture should build a profile");
    profile.rules = vec![
        Rule::DomainSuffix {
            value: "example.com".to_owned(),
            policy: PolicyRef::Direct,
        },
        Rule::Match {
            policy: global_exit_policy(),
        },
    ];

    let yaml = render_mihomo_yaml(&profile).expect("Mihomo profile should render");
    let json = render_sing_box_json(
        &profile,
        &SingBoxOptions::new("127.0.0.1:19090", "fixture-controller-secret"),
    )
    .expect("sing-box profile should render");

    assert!(yaml.trim_end().ends_with("\"MATCH,__MANIS_GLOBAL__\""));
    assert!(json.contains("\"final\": \"__MANIS_GLOBAL__\""));
    let document: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(
        document["route"]["rules"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|rule| rule["domain_suffix"] == serde_json::json!(["example.com"]))
            .count(),
        1
    );
}

#[test]
fn sing_box_renderer_rejects_untranslated_subscription_providers() {
    let profile = Profile::qx_default(fixture_secret()).expect("default profile is valid");

    let error = render_sing_box_json(
        &profile,
        &SingBoxOptions::new("127.0.0.1:19090", "fixture-controller-secret"),
    )
    .expect_err("Mihomo proxy providers must not be silently translated");

    assert_eq!(
        error,
        ProfileError::UnsupportedKernelFeature("subscription providers")
    );
    assert!(!error.to_string().contains("fixture-secret"));
}

#[test]
fn sing_box_renderer_rejects_names_reserved_by_generated_outbounds() {
    let vless = VlessProxy::parse_share_link(
        "vless://00000000-0000-4000-8000-000000000000@198.51.100.7:443?security=reality&encryption=none&pbk=fixture_reality-public-key&fp=chrome&type=tcp&flow=xtls-rprx-vision&sni=cdn.example.invalid#direct",
    )
    .expect("reserved-name fixture should parse before kernel rendering");
    let profile = Profile::qx_sources(Vec::new(), vec![vless], 17_890)
        .expect("reserved-name fixture is valid in the kernel-neutral model");

    assert_eq!(
        render_sing_box_json(
            &profile,
            &SingBoxOptions::new("127.0.0.1:19090", "fixture-controller-secret"),
        ),
        Err(ProfileError::UnsupportedKernelFeature(
            "a reserved sing-box outbound tag",
        ))
    );
}

#[test]
fn sing_box_controller_must_be_loopback_and_authenticated() {
    let vless = VlessProxy::parse_share_link(
        "vless://00000000-0000-4000-8000-000000000000@edge.example.invalid:443?encryption=none&security=reality&type=tcp&sni=cdn.example.invalid&pbk=fixture-key#Saved",
    )
    .expect("fixture should parse");
    let profile = Profile::qx_sources(Vec::new(), vec![vless], 17_890)
        .expect("manual VLESS fixture should build a profile");

    assert!(
        render_sing_box_json(&profile, &SingBoxOptions::new("0.0.0.0:19090", "secret")).is_err()
    );
    assert!(render_sing_box_json(&profile, &SingBoxOptions::new("127.0.0.1:19090", "")).is_err());
}

#[test]
fn qx_manual_rule_shapes_render_exactly_for_mihomo() {
    let mut profile = Profile::qx_default(fixture_secret()).expect("default profile");
    profile.rules.splice(
        0..0,
        [
            Rule::DomainWildcard {
                value: "*.example.?om".to_owned(),
                policy: global_exit_policy(),
            },
            Rule::IpCidr {
                value: "192.0.2.0/24".to_owned(),
                policy: PolicyRef::Direct,
                no_resolve: true,
            },
            Rule::IpCidr {
                value: "2001:db8::/32".to_owned(),
                policy: PolicyRef::Direct,
                no_resolve: true,
            },
            Rule::IpAsn {
                asn: 13_335,
                policy: global_exit_policy(),
                no_resolve: true,
            },
        ],
    );

    let yaml = render_mihomo_yaml(&profile).expect("manual rules should render");
    assert!(yaml.contains("DOMAIN-WILDCARD,*.example.?om,__MANIS_GLOBAL__"));
    assert!(yaml.contains("IP-CIDR,192.0.2.0/24,DIRECT,no-resolve"));
    assert!(yaml.contains("IP-CIDR6,2001:db8::/32,DIRECT,no-resolve"));
    assert!(yaml.contains("IP-ASN,13335,__MANIS_GLOBAL__,no-resolve"));
}

#[test]
fn sing_box_translates_wildcards_and_cidr_but_rejects_ip_asn() {
    let vless = VlessProxy::parse_share_link(
        "vless://00000000-0000-4000-8000-000000000000@edge.example.invalid:443?encryption=none&security=reality&type=tcp&sni=cdn.example.invalid&pbk=fixture-key#Saved",
    )
    .expect("fixture should parse");
    let mut profile =
        Profile::qx_sources(Vec::new(), vec![vless], 17_890).expect("fixture profile");
    profile.rules.splice(
        0..0,
        [
            Rule::DomainWildcard {
                value: "*.example.?om".to_owned(),
                policy: global_exit_policy(),
            },
            Rule::IpCidr {
                value: "192.0.2.0/24".to_owned(),
                policy: PolicyRef::Direct,
                no_resolve: true,
            },
        ],
    );
    let options = SingBoxOptions::new("127.0.0.1:19090", "fixture-controller-secret");
    let json = render_sing_box_json(&profile, &options).expect("supported rules should translate");
    let document: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(
        document["route"]["rules"][2]["domain_regex"],
        serde_json::json!([r"^.*\.example\..om$"])
    );
    assert_eq!(
        document["route"]["rules"][3]["ip_cidr"],
        serde_json::json!(["192.0.2.0/24"])
    );

    profile.rules.insert(
        0,
        Rule::IpAsn {
            asn: 13_335,
            policy: PolicyRef::Direct,
            no_resolve: true,
        },
    );
    assert!(matches!(
        render_sing_box_json(&profile, &options),
        Err(ProfileError::UnsupportedKernelFeature(
            "IP-ASN routing rules"
        ))
    ));
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
        "vless://00000000-0000-4000-8000-000000000000@edge.example.invalid:443?encryption=none&security=reality&pbk=&type=tcp#Bad",
    )
    .is_err());
}
