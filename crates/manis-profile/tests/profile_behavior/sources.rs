use super::*;

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
