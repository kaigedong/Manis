use super::*;

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
