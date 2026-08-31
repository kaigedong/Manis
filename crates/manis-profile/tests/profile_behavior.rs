use std::fs;
use std::path::Path;

use manis_profile::{
    HealthCheck, LogLevel, MANIS_GLOBAL_GROUP_NAME, Name, OutboundProxy, PolicyGroup,
    PolicyGroupKind, PolicyRef, Profile, ProfileError, ProfileMode, ProxyDnsServer, ProxyProvider,
    QxRuleDiagnosticKind, QxRuleKind, QxRuleList, Rule, SecretUrl, SingBoxOptions, UserPolicyGroup,
    UserPolicyGroupKind, VlessProxy, render_mihomo_yaml, render_mihomo_yaml_with_tun,
    render_sing_box_json, write_private_atomic,
};

fn fixture_secret() -> SecretUrl {
    SecretUrl::parse_https("https://subscription.example.invalid/client?token=fixture-secret")
        .expect("fixture url is valid")
}

fn global_exit_policy() -> PolicyRef {
    PolicyRef::Group(Name::parse(MANIS_GLOBAL_GROUP_NAME).expect("valid internal group"))
}

#[test]
fn secret_url_is_redacted_in_debug_and_errors() {
    let secret = fixture_secret();

    assert_eq!(format!("{secret:?}"), "SecretUrl(<redacted>)");
    assert!(!format!("{secret:?}").contains("fixture-secret"));

    let invalid = SecretUrl::parse_https("http://subscription.example.invalid/token");
    let message = invalid.expect_err("http must be rejected").to_string();
    assert!(!message.contains("subscription"));
    assert!(!message.contains("token"));
}

#[test]
fn secret_url_can_be_exposed_only_inside_a_caller_owned_closure() {
    let secret = fixture_secret();

    let host_is_expected = secret.expose_to(|value| {
        value.starts_with("https://subscription.example.invalid/")
            && value.ends_with("fixture-secret")
    });

    assert!(host_is_expected);
    assert_eq!(format!("{secret:?}"), "SecretUrl(<redacted>)");
}

#[test]
fn subscription_url_accepts_http_and_https_without_exposing_values() {
    let http = SecretUrl::parse_subscription(
        "http://subscription.example.invalid/client?token=fixture-secret",
    )
    .expect("plain HTTP providers are supported by Mihomo");
    let https = SecretUrl::parse_subscription(
        "https://subscription.example.invalid/client?token=fixture-secret",
    )
    .expect("HTTPS providers are supported by Mihomo");

    assert_eq!(format!("{http:?}"), "SecretUrl(<redacted>)");
    assert_eq!(format!("{https:?}"), "SecretUrl(<redacted>)");
    assert!(!http.is_https());
    assert!(https.is_https());
    assert!(SecretUrl::parse_subscription("vless://not-a-provider").is_err());
}

#[test]
fn subscription_name_uses_only_a_bounded_explicit_name_parameter() {
    let named = SecretUrl::parse_subscription(
        "https://subscription.example.invalid/client?token=fixture-secret&name=Example_Net",
    )
    .expect("fixture subscription is valid");
    let encoded = SecretUrl::parse_subscription(
        "https://subscription.example.invalid/client?name=%E6%9C%BA%E5%9C%BA+%41&token=hidden",
    )
    .expect("encoded fixture subscription is valid");
    let unnamed = SecretUrl::parse_subscription(
        "https://subscription.example.invalid/client?token=fixture-secret",
    )
    .expect("unnamed fixture subscription is valid");
    let unsafe_name = SecretUrl::parse_subscription(
        "https://subscription.example.invalid/client?name=bad%0Aname&token=fixture-secret",
    )
    .expect("URL itself remains valid");
    let bounded_name = SecretUrl::parse_subscription(&format!(
        "https://subscription.example.invalid/client?name={}",
        "a".repeat(96)
    ))
    .expect("bounded fixture subscription is valid");
    let oversized_name = SecretUrl::parse_subscription(&format!(
        "https://subscription.example.invalid/client?name={}",
        "a".repeat(97)
    ))
    .expect("oversized fixture subscription is still a valid URL");

    assert_eq!(named.subscription_name().as_deref(), Some("Example_Net"));
    assert_eq!(encoded.subscription_name().as_deref(), Some("机场 A"));
    assert_eq!(unnamed.subscription_name(), None);
    assert_eq!(unsafe_name.subscription_name(), None);
    assert_eq!(
        bounded_name.subscription_name().as_deref(),
        Some("a".repeat(96).as_str())
    );
    assert_eq!(oversized_name.subscription_name(), None);
    assert!(!format!("{named:?}").contains("fixture-secret"));
}

#[test]
fn subscription_preview_profile_avoids_geodata_and_health_check_downloads() {
    let subscription = SecretUrl::parse_subscription(
        "https://subscription.example.invalid/client?token=fixture-secret",
    )
    .expect("fixture subscription is valid");
    let profile = Profile::subscription_preview(subscription, 17_891)
        .expect("preview profile should be valid");
    let yaml = render_mihomo_yaml(&profile).expect("preview profile should render");

    assert!(yaml.contains("name: \"Preview\""));
    assert!(yaml.contains("- \"MATCH,Preview\""));
    assert!(yaml.contains("enable: false"));
    assert!(!yaml.contains("GEOIP"));
    assert!(!yaml.contains("url-test"));
    assert!(!yaml.contains("exclude-filter:"));
}

#[test]
fn qx_default_renders_ordered_minimal_mihomo_yaml() {
    let profile = Profile::qx_default(fixture_secret()).expect("default profile is valid");
    let yaml = render_mihomo_yaml(&profile).expect("default profile renders");

    assert_eq!(profile.mode, ProfileMode::Rule);
    assert!(yaml.contains("mode: \"rule\""));
    assert!(yaml.contains("find-process-mode: \"always\""));
    assert!(yaml.contains("unified-delay: true"));
    assert!(yaml.contains("allow-lan: false"));
    assert!(yaml.contains("bind-address: \"127.0.0.1\""));
    assert!(yaml.contains("mixed-port: 7890"));
    assert!(yaml.contains("log-level: \"info\""));
    assert!(yaml.contains("store-selected: true"));
    assert!(yaml.contains("ipv6: false"));
    let strict_route = cfg!(target_os = "linux");
    #[cfg(target_os = "linux")]
    let device = "  device: \"Meta\"\n";
    #[cfg(not(target_os = "linux"))]
    let device = "";
    assert!(yaml.contains(&format!(
        "tun:\n  enable: false\n  stack: \"gvisor\"\n  auto-route: true\n{device}  strict-route: {strict_route}\n  auto-detect-interface: true\n  dns-hijack:\n    - \"any:53\"\n    - \"tcp://any:53\""
    )));
    assert!(yaml.contains("store-fake-ip: true"));
    assert!(yaml.contains("dns:\n  enable: true\n  ipv6: false\n  enhanced-mode: \"fake-ip\""));
    assert!(yaml.contains("fake-ip-range: \"198.18.0.1/16\""));
    assert!(!yaml.contains("fake-ip-range6:"));
    assert!(yaml.contains(
        "proxy-server-nameserver:\n    - \"https://223.5.5.5/dns-query\"\n    - \"https://1.12.12.12/dns-query\""
    ));
    assert!(yaml.contains("proxy-providers:"));
    assert!(!yaml.contains("exclude-filter:"));
    assert!(
        yaml.contains("url: \"https://subscription.example.invalid/client?token=fixture-secret\"")
    );
    assert!(yaml.contains("type: \"select\""));
    assert!(profile.rules.is_empty());
    assert!(!yaml.contains("GEOIP,CN,DIRECT"));
    assert!(!yaml.contains("MATCH,__MANIS_GLOBAL__"));
    assert!(
        yaml.find("proxy-providers:").expect("providers section")
            < yaml.find("proxy-groups:").expect("groups section")
    );
}

#[test]
fn subscription_proxy_dns_replaces_generic_proxy_resolvers() {
    let mut profile = Profile::qx_default(fixture_secret()).expect("default profile is valid");
    profile.set_proxy_server_nameservers(vec![
        ProxyDnsServer::parse_https("https://192.0.2.10:8443/dns-query?site=fixture")
            .expect("fixture proxy DNS is valid"),
        ProxyDnsServer::parse_https("https://198.51.100.20/dns-query")
            .expect("fixture proxy DNS is valid"),
    ]);

    let yaml = render_mihomo_yaml(&profile).expect("profile with proxy DNS renders");

    assert!(yaml.contains("https://192.0.2.10:8443/dns-query?site=fixture"));
    assert!(yaml.contains("https://198.51.100.20/dns-query"));
    assert!(!yaml.contains("proxy-server-nameserver:\n    - \"https://223.5.5.5/dns-query\""));
    assert!(ProxyDnsServer::parse_https("http://192.0.2.10/dns-query").is_err());
    assert!(ProxyDnsServer::parse_https("https://192.0.2.10/dns-query\nproxies:").is_err());
}

#[test]
fn tun_runtime_render_only_changes_the_managed_enable_flag() {
    let profile = Profile::qx_default(fixture_secret()).expect("default profile is valid");
    let disabled = render_mihomo_yaml(&profile).expect("disabled profile renders");
    let enabled = render_mihomo_yaml_with_tun(&profile, true).expect("enabled profile renders");

    assert!(disabled.contains("tun:\n  enable: false\n"));
    assert!(enabled.contains("tun:\n  enable: true\n"));
    assert_eq!(
        disabled.replacen("tun:\n  enable: false\n", "tun:\n  enable: true\n", 1),
        enabled
    );
}

#[test]
fn generated_profiles_default_to_rule_mode_for_compatibility() {
    let preview = Profile::subscription_preview(fixture_secret(), 17_891)
        .expect("preview profile should build");
    let vless = VlessProxy::parse_share_link(
        "vless://00000000-0000-4000-8000-000000000000@edge.example.invalid:443?encryption=none&security=tls&type=ws&path=%2Fmanis#Saved",
    )
    .expect("fixture VLESS should parse");
    let sources = Profile::qx_sources(vec![fixture_secret()], vec![vless], 17_890)
        .expect("source profile should build");

    assert_eq!(ProfileMode::default(), ProfileMode::Rule);
    assert_eq!(preview.mode, ProfileMode::Rule);
    assert_eq!(sources.mode, ProfileMode::Rule);
    assert_eq!(preview.log_level, LogLevel::Silent);
    assert_eq!(sources.log_level, LogLevel::Info);
}

#[test]
fn profile_mode_can_be_persisted_and_rendered_for_all_mihomo_modes() {
    let cases = [
        (ProfileMode::Direct, "direct"),
        (ProfileMode::Global, "global"),
        (ProfileMode::Rule, "rule"),
    ];

    for (mode, wire_label) in cases {
        let mut profile = Profile::qx_default(fixture_secret()).expect("default profile is valid");
        profile.set_mode(mode);

        let yaml = render_mihomo_yaml(&profile).expect("profile should render");

        assert_eq!(profile.mode, mode);
        assert_eq!(mode.as_mihomo_mode(), wire_label);
        assert!(yaml.contains(&format!("mode: \"{wire_label}\"")));
    }
}

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

#[test]
fn qx_sources_only_generates_the_hidden_global_exit_group() {
    let subscription = fixture_secret();
    let vless = VlessProxy::parse_share_link(
        "vless://00000000-0000-4000-8000-000000000000@edge.example.invalid:443?encryption=none&security=tls&type=ws&path=%2Fmanis#Saved",
    )
    .expect("fixture VLESS should parse");

    let profile = Profile::qx_sources(vec![subscription], vec![vless], 17_890)
        .expect("combined QX-style profile should build");
    let yaml = render_mihomo_yaml(&profile).expect("combined profile should render");

    assert!(yaml.contains("mixed-port: 17890"));
    assert!(yaml.contains("name: \"Saved\""));
    assert_eq!(yaml.matches("- \"Saved\"").count(), 1);
    assert!(yaml.contains("- \"Subscription 1\""));
    assert!(yaml.contains("name: \"__MANIS_GLOBAL__\""));
    assert!(!yaml.contains("name: \"Auto\""));
    assert!(!yaml.contains("name: \"Proxy\""));
    assert!(profile.rules.is_empty());
    assert!(!yaml.contains("- \"MATCH,__MANIS_GLOBAL__\""));
}

#[test]
fn qx_sources_with_groups_only_compiles_user_groups_and_the_hidden_global_exit() {
    let subscription = fixture_secret();
    let saved = VlessProxy::parse_share_link(
        "vless://00000000-0000-4000-8000-000000000000@edge.example.invalid:443?encryption=none&security=tls&type=ws&path=%2Fmanis#Saved",
    )
    .expect("fixture VLESS should parse");
    let saved_name = saved.name().clone();
    let streaming = UserPolicyGroup {
        name: Name::parse("Streaming").expect("valid user group"),
        icon: Some("https://assets.example.invalid/streaming.png".to_owned()),
        kind: UserPolicyGroupKind::UrlTest {
            tolerance: 80,
            interval_secs: 300,
        },
        provider_indexes: vec![0],
        direct_proxies: vec![saved_name],
        direct_policies: Vec::new(),
        filter: Some("HK|JP|US".to_owned()),
    };
    let manual = UserPolicyGroup {
        name: Name::parse("Manual Saved").expect("valid user group"),
        icon: None,
        kind: UserPolicyGroupKind::Select,
        provider_indexes: Vec::new(),
        direct_proxies: vec![Name::parse("Saved").expect("valid proxy")],
        direct_policies: vec![PolicyRef::Group(
            Name::parse("Streaming").expect("valid group"),
        )],
        filter: None,
    };

    let profile = Profile::qx_sources_with_groups(
        vec![subscription],
        vec![saved],
        vec![streaming, manual],
        17_890,
    )
    .expect("profile with user groups should build");
    let yaml = render_mihomo_yaml(&profile).expect("profile should render");

    assert!(yaml.contains("name: \"Streaming\""));
    assert!(yaml.contains("type: \"url-test\""));
    assert!(yaml.contains("icon: \"https://assets.example.invalid/streaming.png\""));
    assert!(yaml.contains("filter: \"HK|JP|US\""));
    assert!(yaml.contains("tolerance: 80"));
    assert!(yaml.contains("interval: 300"));
    assert!(yaml.contains("name: \"Manual Saved\""));
    assert!(yaml.contains("type: \"select\""));
    assert!(yaml.contains("- \"Streaming\""));
    assert!(yaml.find("name: \"Streaming\"") < yaml.find("name: \"__MANIS_GLOBAL__\""));
    assert!(yaml.find("name: \"Manual Saved\"") < yaml.find("name: \"__MANIS_GLOBAL__\""));
    assert!(!yaml.contains("name: \"Auto\""));
    assert!(!yaml.contains("name: \"Proxy\""));
    assert!(profile.rules.is_empty());
    assert!(!yaml.contains("- \"MATCH,__MANIS_GLOBAL__\""));
}

#[test]
fn user_policy_groups_can_target_the_node_page_proxy_exit() {
    let subscription = fixture_secret();
    let saved = VlessProxy::parse_share_link(
        "vless://00000000-0000-4000-8000-000000000000@edge.example.invalid:443?encryption=none&security=tls&type=ws&path=%2Fmanis#Saved",
    )
    .expect("fixture VLESS should parse");
    let select = UserPolicyGroup {
        name: Name::parse("Proxy Select").expect("valid user group"),
        icon: None,
        kind: UserPolicyGroupKind::Select,
        provider_indexes: Vec::new(),
        direct_proxies: Vec::new(),
        direct_policies: vec![global_exit_policy()],
        filter: None,
    };
    let auto = UserPolicyGroup {
        name: Name::parse("Proxy Auto").expect("valid user group"),
        icon: None,
        kind: UserPolicyGroupKind::UrlTest {
            tolerance: 120,
            interval_secs: 600,
        },
        provider_indexes: Vec::new(),
        direct_proxies: Vec::new(),
        direct_policies: vec![global_exit_policy()],
        filter: None,
    };

    let profile = Profile::qx_sources_with_groups(
        vec![subscription],
        vec![saved],
        vec![select, auto],
        17_890,
    )
    .expect("global exit should be accepted as a generated group reference");
    let yaml = render_mihomo_yaml(&profile).expect("profile should render");
    let hidden_global = yaml
        .split("name: \"__MANIS_GLOBAL__\"")
        .nth(1)
        .expect("hidden global selector should render")
        .split("\nrules:")
        .next()
        .expect("proxy groups should appear before rules");

    assert!(yaml.contains("name: \"Proxy Select\""));
    assert!(yaml.contains("type: \"select\""));
    assert!(yaml.contains("- \"__MANIS_GLOBAL__\""));
    assert!(yaml.contains("name: \"Proxy Auto\""));
    assert!(yaml.contains("type: \"url-test\""));
    assert!(yaml.contains("tolerance: 120"));
    assert!(hidden_global.contains("- \"Saved\""));
    assert!(hidden_global.contains("- \"Subscription 1\""));
    assert!(!hidden_global.contains("Proxy Select"));
    assert!(!hidden_global.contains("Proxy Auto"));
}

#[test]
fn allowing_the_proxy_exit_does_not_allow_invalid_group_references() {
    for (direct_policies, expected) in [
        (
            vec![PolicyRef::Group(Name::parse("Missing").unwrap())],
            ProfileError::DanglingReference,
        ),
        (
            vec![PolicyRef::Group(Name::parse("Follow").unwrap())],
            ProfileError::DanglingReference,
        ),
        (
            vec![global_exit_policy(), global_exit_policy()],
            ProfileError::DuplicateName,
        ),
    ] {
        let group = UserPolicyGroup {
            name: Name::parse("Follow").unwrap(),
            icon: None,
            kind: UserPolicyGroupKind::Select,
            provider_indexes: Vec::new(),
            direct_proxies: Vec::new(),
            direct_policies,
            filter: None,
        };
        let error = Profile::qx_sources_with_groups(
            vec![fixture_secret()],
            Vec::new(),
            vec![group],
            17_890,
        )
        .expect_err("invalid reference must be rejected");
        assert_eq!(error, expected);
    }
}

#[test]
fn qx_sources_with_groups_rejects_bad_user_group_references_and_names() {
    let subscription = fixture_secret();
    let saved = VlessProxy::parse_share_link(
        "vless://00000000-0000-4000-8000-000000000000@edge.example.invalid:443?encryption=none&security=tls&type=ws&path=%2Fmanis#Saved",
    )
    .expect("fixture VLESS should parse");
    let valid = UserPolicyGroup {
        name: Name::parse("Valid").expect("valid user group"),
        icon: None,
        kind: UserPolicyGroupKind::Select,
        provider_indexes: vec![0],
        direct_proxies: vec![Name::parse("Saved").expect("valid proxy")],
        direct_policies: Vec::new(),
        filter: None,
    };

    let mut bad_provider = valid.clone();
    bad_provider.provider_indexes = vec![1];
    assert!(
        Profile::qx_sources_with_groups(
            vec![subscription.clone()],
            vec![saved.clone()],
            vec![bad_provider],
            17_890,
        )
        .is_err()
    );

    let mut bad_proxy = valid.clone();
    bad_proxy.direct_proxies = vec![Name::parse("Missing").expect("valid name")];
    assert!(
        Profile::qx_sources_with_groups(
            vec![subscription.clone()],
            vec![saved.clone()],
            vec![bad_proxy],
            17_890,
        )
        .is_err()
    );

    let mut reserved = valid.clone();
    reserved.name = Name::parse("GLOBAL").expect("valid reserved name");
    assert!(
        Profile::qx_sources_with_groups(
            vec![subscription.clone()],
            vec![saved.clone()],
            vec![reserved],
            17_890,
        )
        .is_err()
    );

    let mut reserved_internal = valid.clone();
    reserved_internal.name =
        Name::parse(MANIS_GLOBAL_GROUP_NAME).expect("valid internal reserved name");
    assert!(
        Profile::qx_sources_with_groups(
            vec![subscription.clone()],
            vec![saved.clone()],
            vec![reserved_internal],
            17_890,
        )
        .is_err()
    );

    let duplicate = valid.clone();
    assert!(
        Profile::qx_sources_with_groups(
            vec![subscription],
            vec![saved],
            vec![valid, duplicate],
            17_890,
        )
        .is_err()
    );
}

#[test]
fn managed_profile_rejects_manual_nodes_named_like_system_groups() {
    let reserved = VlessProxy::parse_share_link(
        "vless://00000000-0000-4000-8000-000000000000@198.51.100.7:443?encryption=none&security=reality&type=tcp&sni=cdn.example.invalid&pbk=fixture-key#GLOBAL",
    )
    .expect("reserved-name fixture should parse before profile validation");

    assert_eq!(
        Profile::qx_sources(Vec::new(), vec![reserved], 17_890),
        Err(ProfileError::InvalidValue("reserved proxy name"))
    );
}

#[test]
fn domain_keyword_rules_validate_and_render_to_mihomo_yaml() {
    let mut profile = Profile::qx_default(fixture_secret()).expect("default profile is valid");
    profile.rules.insert(
        0,
        Rule::DomainKeyword {
            value: "google".to_owned(),
            policy: global_exit_policy(),
        },
    );

    let yaml = render_mihomo_yaml(&profile).expect("profile should render");

    assert!(yaml.contains("- \"DOMAIN-KEYWORD,google,__MANIS_GLOBAL__\""));
    assert!(!yaml.contains("- \"MATCH,__MANIS_GLOBAL__\""));
}

#[test]
fn destination_port_rules_render_to_mihomo_yaml_without_a_generated_fallback() {
    let mut profile = Profile::qx_default(fixture_secret()).expect("default profile is valid");
    profile.rules.insert(
        0,
        Rule::DstPort {
            port: 22,
            policy: PolicyRef::Direct,
        },
    );

    let yaml = render_mihomo_yaml(&profile).expect("profile should render");

    assert!(yaml.contains("- \"DST-PORT,22,DIRECT\""));
    assert!(!yaml.contains("- \"MATCH,__MANIS_GLOBAL__\""));
}

#[test]
fn destination_port_rules_render_to_sing_box_json() {
    let vless = VlessProxy::parse_share_link(
        "vless://00000000-0000-4000-8000-000000000000@198.51.100.7:443?security=reality&encryption=none&pbk=fixture_reality-public-key&headerType=&fp=chrome&type=tcp&flow=xtls-rprx-vision&sni=cdn.example.invalid#Reality%20TCP",
    )
    .expect("Reality TCP fixture should parse");
    let mut profile = Profile::qx_sources(Vec::new(), vec![vless], 17_890)
        .expect("manual VLESS fixture should build a profile");
    profile.rules.insert(
        0,
        Rule::DstPort {
            port: 22,
            policy: PolicyRef::Direct,
        },
    );

    let json = render_sing_box_json(
        &profile,
        &SingBoxOptions::new("127.0.0.1:19090", "fixture-controller-secret"),
    )
    .expect("supported profile should render");

    let document: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(
        document["route"]["rules"][2],
        serde_json::json!({"port": [22], "action": "route", "outbound": "direct"})
    );
}

#[test]
fn domain_and_port_rules_render_exactly_for_both_kernels() {
    let mut mihomo_profile =
        Profile::qx_default(fixture_secret()).expect("default profile is valid");
    mihomo_profile.rules.insert(
        0,
        Rule::All {
            conditions: vec![
                manis_profile::RuleCondition::DomainSuffix("github.com".to_owned()),
                manis_profile::RuleCondition::DstPort(22),
            ],
            policy: PolicyRef::Direct,
        },
    );
    let yaml = render_mihomo_yaml(&mihomo_profile).expect("compound rule should render");
    assert!(yaml.contains("AND,((DOMAIN-SUFFIX,github.com),(DST-PORT,22)),DIRECT"));

    let vless = VlessProxy::parse_share_link(
        "vless://00000000-0000-4000-8000-000000000000@198.51.100.7:443?security=reality&encryption=none&pbk=fixture_reality-public-key&headerType=&fp=chrome&type=tcp&flow=xtls-rprx-vision&sni=cdn.example.invalid#Reality%20TCP",
    )
    .expect("Reality TCP fixture should parse");
    let mut sing_box_profile = Profile::qx_sources(Vec::new(), vec![vless], 17_890)
        .expect("manual VLESS fixture should build a profile");
    sing_box_profile.rules.insert(
        0,
        Rule::All {
            conditions: vec![
                manis_profile::RuleCondition::IpCidr {
                    value: "192.0.2.10/32".to_owned(),
                    no_resolve: true,
                },
                manis_profile::RuleCondition::DstPort(22),
            ],
            policy: PolicyRef::Direct,
        },
    );
    let json = render_sing_box_json(
        &sing_box_profile,
        &SingBoxOptions::new("127.0.0.1:19090", "fixture-controller-secret"),
    )
    .expect("compound rule should render");
    let document: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(
        document["route"]["rules"][2],
        serde_json::json!({
            "type": "logical", "mode": "and",
            "rules": [{"ip_cidr": ["192.0.2.10/32"]}, {"port": [22]}],
            "action": "route", "outbound": "direct"
        })
    );
}

#[test]
fn quantumult_x_rule_list_parses_rules_and_reports_actionable_diagnostics() {
    let parsed = QxRuleList::parse(
        r"
# comment
DOMAIN-KEYWORD,google,PROXY
DOMAIN-SUFFIX,githubusercontent.com,PROXY
DOMAIN,example.com,proxy
IP-CIDR,192.0.2.0/24,DIRECT
DOMAIN-SUFFIX,,PROXY
DOMAIN,bad.example,DIRECT,unexpected
",
    );

    assert_eq!(parsed.rules.len(), 3);
    assert_eq!(parsed.rules[0].kind, QxRuleKind::DomainKeyword);
    assert_eq!(parsed.rules[0].value, "google");
    assert_eq!(parsed.rules[0].source_policy.as_str(), "PROXY");
    assert_eq!(parsed.rules[1].kind, QxRuleKind::DomainSuffix);
    assert_eq!(parsed.rules[2].kind, QxRuleKind::Domain);
    assert_eq!(parsed.rules[2].source_policy.as_str(), "proxy");
    assert_eq!(parsed.source_policies().len(), 1);
    assert!(
        parsed
            .source_policies()
            .iter()
            .any(|name| name.as_str() == "PROXY")
    );
    assert_eq!(parsed.diagnostics.len(), 3);
    assert_eq!(parsed.diagnostics[0].line_number, 6);
    assert_eq!(
        parsed.diagnostics[0].kind,
        QxRuleDiagnosticKind::UnsupportedRuleType
    );
    assert_eq!(parsed.diagnostics[1].line_number, 7);
    assert_eq!(
        parsed.diagnostics[1].kind,
        QxRuleDiagnosticKind::InvalidValue
    );
    assert_eq!(parsed.diagnostics[2].line_number, 8);
    assert_eq!(
        parsed.diagnostics[2].kind,
        QxRuleDiagnosticKind::InvalidFieldCount
    );
}

#[test]
fn quantumult_x_host_aliases_match_domain_rules_case_insensitively() {
    for (host_kind, domain_kind, value) in [
        ("HOST", "DOMAIN", "service.example.com"),
        ("HOST-SUFFIX", "DOMAIN-SUFFIX", "example.com"),
        ("HOST-KEYWORD", "DOMAIN-KEYWORD", "example"),
    ] {
        let expected = QxRuleList::parse(&format!("{domain_kind},{value},Example"));
        assert_eq!(expected.rules.len(), 1);
        for spelling in [host_kind.to_owned(), host_kind.to_ascii_lowercase()] {
            let parsed = QxRuleList::parse(&format!("  {spelling}, {value}, Example  \r\n"));
            assert_eq!(
                parsed, expected,
                "QX alias {spelling} must retain its semantics"
            );
        }
    }
}

#[test]
fn quantumult_x_rule_list_maps_source_policies_to_local_profile_rules() {
    let parsed = QxRuleList::parse(
        r"
HOST-KEYWORD,google,PROXY
host-suffix,githubusercontent.com,PROXY
HoSt,example.com,proxy
",
    );
    assert!(parsed.diagnostics.is_empty());
    let proxy = Name::parse(MANIS_GLOBAL_GROUP_NAME).expect("valid internal group");
    let mapped_rules = parsed
        .to_profile_rules(|source| {
            (source.as_str().eq_ignore_ascii_case("PROXY")).then(|| PolicyRef::Group(proxy.clone()))
        })
        .expect("all fixture policies should map");

    let mut profile = Profile::qx_default(fixture_secret()).expect("default profile is valid");
    profile.rules.splice(0..0, mapped_rules);
    let yaml = render_mihomo_yaml(&profile).expect("mapped QX rules should render");

    assert!(yaml.contains("- \"DOMAIN-KEYWORD,google,__MANIS_GLOBAL__\""));
    assert!(yaml.contains("- \"DOMAIN-SUFFIX,githubusercontent.com,__MANIS_GLOBAL__\""));
    assert!(yaml.contains("- \"DOMAIN,example.com,__MANIS_GLOBAL__\""));
}

#[test]
fn quantumult_x_rule_list_requires_policy_mapping_before_import() {
    let parsed = QxRuleList::parse("DOMAIN-KEYWORD,google,PROXY\n");

    let error = parsed
        .to_profile_rules(|_source| None)
        .expect_err("unmapped source policy should be actionable");

    assert_eq!(error.source_policy().as_str(), "PROXY");
    assert!(error.to_string().contains("PROXY"));
}

#[test]
fn renderer_escapes_double_quoted_yaml_scalars() {
    let profile = Profile {
        mode: ProfileMode::Rule,
        mixed_port: 7891,
        log_level: LogLevel::Silent,
        store_selected: true,
        proxy_server_nameservers: vec![
            ProxyDnsServer::parse_https("https://223.5.5.5/dns-query").expect("valid DNS"),
        ],
        proxies: Vec::new(),
        providers: vec![ProxyProvider {
            name: Name::parse("subscription").expect("valid name"),
            source: manis_profile::ProxyProviderSource::Http(fixture_secret()),
            interval_secs: 86_400,
            path: "providers/subscription.yaml".to_owned(),
            health_check: HealthCheck {
                enabled: true,
                interval_secs: 600,
                url: "https://www.gstatic.com/generate_204?x=\"slash\\path".to_owned(),
            },
        }],
        groups: vec![PolicyGroup {
            name: Name::parse("Proxy").expect("valid group"),
            icon: None,
            kind: PolicyGroupKind::Select {
                proxies: vec![PolicyRef::Direct],
                use_providers: vec![Name::parse("subscription").expect("valid provider")],
                filter: None,
            },
        }],
        rules: vec![Rule::Match {
            policy: PolicyRef::Group(Name::parse("Proxy").expect("valid group")),
        }],
    };

    let yaml = render_mihomo_yaml(&profile).expect("profile renders");

    assert!(yaml.contains("log-level: \"silent\""));
    assert!(yaml.contains("url: \"https://www.gstatic.com/generate_204?x=\\\"slash\\\\path\""));
}

#[test]
fn validation_rejects_invalid_names_duplicates_dangling_refs_and_misplaced_match() {
    assert!(Name::parse("bad,name").is_err());
    assert!(Name::parse("bad\u{7f}name").is_err());

    let provider_name = Name::parse("subscription").expect("valid provider");
    let group_name = Name::parse("Proxy").expect("valid group");
    let duplicate_groups = Profile {
        mode: ProfileMode::Rule,
        mixed_port: 7890,
        log_level: LogLevel::Warning,
        store_selected: true,
        proxy_server_nameservers: vec![
            ProxyDnsServer::parse_https("https://223.5.5.5/dns-query").expect("valid DNS"),
        ],
        proxies: Vec::new(),
        providers: vec![ProxyProvider {
            name: provider_name.clone(),
            source: manis_profile::ProxyProviderSource::Http(fixture_secret()),
            interval_secs: 86_400,
            path: "providers/subscription.yaml".to_owned(),
            health_check: HealthCheck {
                enabled: true,
                interval_secs: 600,
                url: "https://www.gstatic.com/generate_204".to_owned(),
            },
        }],
        groups: vec![
            PolicyGroup {
                name: group_name.clone(),
                icon: None,
                kind: PolicyGroupKind::Select {
                    proxies: vec![PolicyRef::Group(
                        Name::parse("Missing").expect("valid group"),
                    )],
                    use_providers: vec![provider_name.clone()],
                    filter: None,
                },
            },
            PolicyGroup {
                name: group_name.clone(),
                icon: None,
                kind: PolicyGroupKind::Select {
                    proxies: vec![PolicyRef::Direct],
                    use_providers: vec![provider_name],
                    filter: None,
                },
            },
        ],
        rules: vec![Rule::DomainSuffix {
            value: "example.com".to_owned(),
            policy: PolicyRef::Direct,
        }],
    };

    assert!(duplicate_groups.validate().is_err());

    let message = duplicate_groups
        .validate()
        .expect_err("invalid profile")
        .to_string();
    assert!(!message.contains("fixture-secret"));

    let mut misplaced_match = Profile::qx_default(fixture_secret()).expect("default profile");
    misplaced_match.rules.push(Rule::Match {
        policy: PolicyRef::Direct,
    });
    misplaced_match.rules.push(Rule::DomainSuffix {
        value: "example.com".to_owned(),
        policy: PolicyRef::Direct,
    });
    assert!(misplaced_match.validate().is_err());

    let mut control_character = Profile::qx_default(fixture_secret()).expect("default profile");
    control_character.providers[0].health_check.url.push('\t');
    assert!(control_character.validate().is_err());
}

#[test]
fn atomic_writer_creates_private_directory_and_replaces_file() {
    let temp = test_temp_dir("manis-profile-atomic");
    let runtime = temp.join("runtime");

    let written =
        write_private_atomic(&runtime, "manis-generated.yaml", b"first").expect("initial write");
    assert_eq!(written, runtime.join("manis-generated.yaml"));
    assert_eq!(fs::read(&written).expect("read initial"), b"first");

    write_private_atomic(&runtime, "manis-generated.yaml", b"second").expect("replace");
    assert_eq!(fs::read(&written).expect("read replacement"), b"second");

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let dir_mode = fs::metadata(&runtime)
            .expect("runtime metadata")
            .permissions()
            .mode()
            & 0o777;
        let file_mode = fs::metadata(&written)
            .expect("file metadata")
            .permissions()
            .mode()
            & 0o777;

        assert_eq!(dir_mode, 0o700);
        assert_eq!(file_mode, 0o600);
    }

    fs::remove_dir_all(temp).expect("cleanup temp");
}

#[test]
fn atomic_writer_rejects_symlink_runtime_and_final_file() {
    let temp = test_temp_dir("manis-profile-symlink");
    let target = temp.join("target");
    let runtime_link = temp.join("runtime-link");
    fs::create_dir(&target).expect("create target");

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&target, &runtime_link).expect("create symlink");
        assert!(write_private_atomic(&runtime_link, "manis-generated.yaml", b"data").is_err());

        let runtime = temp.join("runtime");
        fs::create_dir(&runtime).expect("create runtime");
        let final_target = temp.join("outside.yaml");
        fs::write(&final_target, b"outside").expect("write outside");
        std::os::unix::fs::symlink(&final_target, runtime.join("manis-generated.yaml"))
            .expect("final symlink");
        assert!(write_private_atomic(&runtime, "manis-generated.yaml", b"data").is_err());
    }

    assert!(write_private_atomic(&target, "../escape.yaml", b"data").is_err());

    fs::remove_dir_all(temp).expect("cleanup temp");
}

#[test]
fn empty_managed_profile_is_a_direct_only_bootstrap_config() {
    let profile = Profile::managed_empty(17_890).expect("empty managed profile should build");
    let yaml = render_mihomo_yaml(&profile).expect("empty managed profile should render");

    assert!(yaml.contains("mixed-port: 17890"));
    let document: serde_json::Value = serde_saphyr::from_str(&yaml).unwrap();
    assert_eq!(document["proxies"], serde_json::json!([]));
    assert_eq!(document["proxy-providers"], serde_json::json!({}));
    assert_eq!(document["proxy-groups"], serde_json::json!([]));
    assert!(yaml.contains("rules:\n  - \"MATCH,DIRECT\""));
    assert!(!yaml.contains("__MANIS_GLOBAL__"));
}

fn test_temp_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("{name}-{}", std::process::id()));
    if Path::new(&dir).exists() {
        fs::remove_dir_all(&dir).expect("cleanup stale temp");
    }
    fs::create_dir(&dir).expect("create temp");
    dir
}
