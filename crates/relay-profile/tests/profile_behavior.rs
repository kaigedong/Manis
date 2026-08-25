use std::fs;
use std::path::Path;

use relay_profile::{
    HealthCheck, LogLevel, Name, PolicyGroup, PolicyGroupKind, PolicyRef, Profile, ProxyProvider,
    Rule, SecretUrl, render_mihomo_yaml, write_private_atomic,
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
fn renderer_escapes_double_quoted_yaml_scalars() {
    let profile = Profile {
        mixed_port: 7891,
        log_level: LogLevel::Silent,
        store_selected: true,
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
            kind: PolicyGroupKind::Select {
                proxies: vec![PolicyRef::Direct],
                use_providers: vec![Name::parse("subscription").expect("valid provider")],
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
                kind: PolicyGroupKind::Select {
                    proxies: vec![PolicyRef::Group(
                        Name::parse("Missing").expect("valid group"),
                    )],
                    use_providers: vec![provider_name.clone()],
                },
            },
            PolicyGroup {
                name: group_name.clone(),
                kind: PolicyGroupKind::Select {
                    proxies: vec![PolicyRef::Direct],
                    use_providers: vec![provider_name],
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
