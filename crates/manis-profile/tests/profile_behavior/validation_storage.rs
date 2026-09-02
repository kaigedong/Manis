use super::*;

#[test]
fn validation_checks_each_policy_reference_kind() {
    let saved = VlessProxy::parse_share_link(
        "vless://00000000-0000-4000-8000-000000000000@edge.example.invalid:443?security=tls#Saved",
    )
    .expect("valid saved proxy");
    let mut profile =
        Profile::qx_sources_with_groups(vec![fixture_secret()], vec![saved], Vec::new(), 17_890)
            .expect("valid profile");
    let group = profile.groups[0].name.clone();
    let proxy = Name::parse("Saved").expect("valid proxy name");
    let missing = Name::parse("Missing").expect("valid missing name");

    for (policy, expected) in [
        (PolicyRef::Direct, Ok(())),
        (PolicyRef::Reject, Ok(())),
        (PolicyRef::Group(group), Ok(())),
        (PolicyRef::Proxy(proxy), Ok(())),
        (
            PolicyRef::Group(missing.clone()),
            Err(ProfileError::DanglingReference),
        ),
        (
            PolicyRef::Proxy(missing),
            Err(ProfileError::DanglingReference),
        ),
    ] {
        profile.rules = vec![Rule::Match { policy }];
        assert_eq!(profile.validate(), expected);
    }
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
    fs::create_dir(&target).expect("create target");

    #[cfg(unix)]
    {
        let runtime_link = temp.join("runtime-link");
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
