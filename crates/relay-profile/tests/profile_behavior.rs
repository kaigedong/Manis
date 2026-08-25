use std::fs;
use std::path::Path;

use relay_profile::{
    HealthCheck, LogLevel, Name, OutboundProxy, PolicyGroup, PolicyGroupKind, PolicyRef, Profile,
    ProxyProvider, Rule, SecretUrl, UserPolicyGroup, UserPolicyGroupKind, VlessProxy,
    render_mihomo_yaml, write_private_atomic,
};

fn fixture_secret() -> SecretUrl {
    SecretUrl::parse_https("https://subscription.example.invalid/client?token=fixture-secret")
        .expect("fixture url is valid")
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
        "https://subscription.example.invalid/client?token=fixture-secret&name=NaiU_Net",
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

    assert_eq!(named.subscription_name().as_deref(), Some("NaiU_Net"));
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
}

#[test]
fn qx_default_renders_ordered_minimal_mihomo_yaml() {
    let profile = Profile::qx_default(fixture_secret()).expect("default profile is valid");
    let yaml = render_mihomo_yaml(&profile).expect("default profile renders");

    assert!(yaml.contains("mode: \"rule\""));
    assert!(yaml.contains("allow-lan: false"));
    assert!(yaml.contains("bind-address: \"127.0.0.1\""));
    assert!(yaml.contains("mixed-port: 7890"));
    assert!(yaml.contains("log-level: \"warning\""));
    assert!(yaml.contains("store-selected: true"));
    assert!(yaml.contains("proxy-providers:"));
    assert!(
        yaml.contains("url: \"https://subscription.example.invalid/client?token=fixture-secret\"")
    );
    assert!(yaml.contains("type: \"select\""));
    assert!(yaml.contains("type: \"url-test\""));
    assert!(yaml.contains("- \"GEOIP,CN,DIRECT,no-resolve\""));
    assert!(yaml.contains("- \"MATCH,Proxy\""));
    assert!(
        yaml.find("proxy-providers:").expect("providers section")
            < yaml.find("proxy-groups:").expect("groups section")
    );
    assert!(yaml.find("- \"GEOIP,CN,DIRECT,no-resolve\"") < yaml.find("- \"MATCH,Proxy\""));
}

#[test]
fn vless_share_link_compiles_into_a_direct_proxy_and_policy_reference() {
    let vless = VlessProxy::parse_share_link(
        "vless://00000000-0000-4000-8000-000000000000@edge.example.invalid:443?encryption=none&security=reality&type=grpc&sni=cdn.example.invalid&fp=chrome&pbk=fixture-public-key&sid=0123456789abcdef&serviceName=relay#Tokyo%20Edge",
    )
    .expect("supported fixture VLESS link should parse");
    let proxy_name = vless.name().clone();
    let mut profile = Profile::qx_default(fixture_secret()).expect("default profile is valid");
    profile.proxies.push(OutboundProxy::Vless(vless));
    let PolicyGroupKind::Select { proxies, .. } = &mut profile.groups[1].kind else {
        panic!("Proxy is a select group");
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
    assert!(yaml.contains("grpc-service-name: \"relay\""));
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
    let input = "vless://00000000-0000-4000-8000-000000000000@edge.example.invalid:443?encryption=none&security=tls&type=ws&path=%2Frelay&host=cdn.example.invalid#Saved";
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
fn qx_sources_routes_subscriptions_and_saved_nodes_through_auto_and_proxy() {
    let subscription = fixture_secret();
    let vless = VlessProxy::parse_share_link(
        "vless://00000000-0000-4000-8000-000000000000@edge.example.invalid:443?encryption=none&security=tls&type=ws&path=%2Frelay#Saved",
    )
    .expect("fixture VLESS should parse");

    let profile = Profile::qx_sources(vec![subscription], vec![vless], 17_890)
        .expect("combined QX-style profile should build");
    let yaml = render_mihomo_yaml(&profile).expect("combined profile should render");

    assert!(yaml.contains("mixed-port: 17890"));
    assert!(yaml.contains("name: \"Saved\""));
    assert_eq!(yaml.matches("- \"Saved\"").count(), 2);
    assert!(yaml.contains("- \"Subscription 1\""));
    assert!(yaml.contains("- \"MATCH,Proxy\""));
}

#[test]
fn qx_sources_with_groups_compiles_user_groups_before_auto_and_proxy() {
    let subscription = fixture_secret();
    let saved = VlessProxy::parse_share_link(
        "vless://00000000-0000-4000-8000-000000000000@edge.example.invalid:443?encryption=none&security=tls&type=ws&path=%2Frelay#Saved",
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
        filter: Some("HK|JP|US".to_owned()),
    };
    let manual = UserPolicyGroup {
        name: Name::parse("Manual Saved").expect("valid user group"),
        icon: None,
        kind: UserPolicyGroupKind::Select,
        provider_indexes: Vec::new(),
        direct_proxies: vec![Name::parse("Saved").expect("valid proxy")],
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
    assert!(yaml.find("name: \"Streaming\"") < yaml.find("name: \"Auto\""));
    assert!(yaml.find("name: \"Manual Saved\"") < yaml.find("name: \"Auto\""));
    assert!(yaml.find("name: \"Auto\"") < yaml.find("name: \"Proxy\""));
    assert!(yaml.contains("      - \"Streaming\""));
    assert!(yaml.contains("      - \"Manual Saved\""));
}

#[test]
fn qx_sources_with_groups_rejects_bad_user_group_references_and_names() {
    let subscription = fixture_secret();
    let saved = VlessProxy::parse_share_link(
        "vless://00000000-0000-4000-8000-000000000000@edge.example.invalid:443?encryption=none&security=tls&type=ws&path=%2Frelay#Saved",
    )
    .expect("fixture VLESS should parse");
    let valid = UserPolicyGroup {
        name: Name::parse("Valid").expect("valid user group"),
        icon: None,
        kind: UserPolicyGroupKind::Select,
        provider_indexes: vec![0],
        direct_proxies: vec![Name::parse("Saved").expect("valid proxy")],
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
fn renderer_escapes_double_quoted_yaml_scalars() {
    let profile = Profile {
        mixed_port: 7891,
        log_level: LogLevel::Silent,
        store_selected: true,
        proxies: Vec::new(),
        providers: vec![ProxyProvider {
            name: Name::parse("subscription").expect("valid name"),
            url: fixture_secret(),
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
fn validation_rejects_invalid_names_duplicates_dangling_refs_and_missing_match() {
    assert!(Name::parse("bad,name").is_err());
    assert!(Name::parse("bad\u{7f}name").is_err());

    let provider_name = Name::parse("subscription").expect("valid provider");
    let group_name = Name::parse("Proxy").expect("valid group");
    let duplicate_groups = Profile {
        mixed_port: 7890,
        log_level: LogLevel::Warning,
        store_selected: true,
        proxies: Vec::new(),
        providers: vec![ProxyProvider {
            name: provider_name.clone(),
            url: fixture_secret(),
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

    let mut missing_match = Profile::qx_default(fixture_secret()).expect("default profile");
    missing_match.rules.pop();
    assert!(missing_match.validate().is_err());

    let mut control_character = Profile::qx_default(fixture_secret()).expect("default profile");
    control_character.providers[0].health_check.url.push('\t');
    assert!(control_character.validate().is_err());
}

#[test]
fn atomic_writer_creates_private_directory_and_replaces_file() {
    let temp = test_temp_dir("relay-profile-atomic");
    let runtime = temp.join("runtime");

    let written =
        write_private_atomic(&runtime, "relay-generated.yaml", b"first").expect("initial write");
    assert_eq!(written, runtime.join("relay-generated.yaml"));
    assert_eq!(fs::read(&written).expect("read initial"), b"first");

    write_private_atomic(&runtime, "relay-generated.yaml", b"second").expect("replace");
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
    let temp = test_temp_dir("relay-profile-symlink");
    let target = temp.join("target");
    let runtime_link = temp.join("runtime-link");
    fs::create_dir(&target).expect("create target");

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&target, &runtime_link).expect("create symlink");
        assert!(write_private_atomic(&runtime_link, "relay-generated.yaml", b"data").is_err());

        let runtime = temp.join("runtime");
        fs::create_dir(&runtime).expect("create runtime");
        let final_target = temp.join("outside.yaml");
        fs::write(&final_target, b"outside").expect("write outside");
        std::os::unix::fs::symlink(&final_target, runtime.join("relay-generated.yaml"))
            .expect("final symlink");
        assert!(write_private_atomic(&runtime, "relay-generated.yaml", b"data").is_err());
    }

    assert!(write_private_atomic(&target, "../escape.yaml", b"data").is_err());

    fs::remove_dir_all(temp).expect("cleanup temp");
}

fn test_temp_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("{name}-{}", std::process::id()));
    if Path::new(&dir).exists() {
        fs::remove_dir_all(&dir).expect("cleanup stale temp");
    }
    fs::create_dir(&dir).expect("create temp");
    dir
}
