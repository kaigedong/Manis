use std::fs;
use std::process::Command;

use manis_profile::{
    MANIS_GLOBAL_GROUP_NAME, Name, PolicyRef, Profile, SecretUrl, UserPolicyGroup,
    UserPolicyGroupKind, VlessProxy, render_mihomo_yaml, render_mihomo_yaml_with_tun,
    write_private_atomic,
};

#[test]
#[ignore = "requires MANIS_MIHOMO_TEST_BINARY pointing to a local Mihomo executable"]
fn generated_tun_enabled_profile_passes_mihomo_validation() -> Result<(), Box<dyn std::error::Error>>
{
    let binary = std::env::var_os("MANIS_MIHOMO_TEST_BINARY")
        .ok_or("MANIS_MIHOMO_TEST_BINARY is required")?;
    let root = std::env::temp_dir().join(format!(
        "manis-profile-mihomo-validation-{}",
        std::process::id()
    ));
    if root.exists() {
        fs::remove_dir_all(&root)?;
    }
    let profile = Profile::qx_default(SecretUrl::parse_https(
        "https://subscription.example.invalid/client?token=fixture-secret",
    )?)?;
    let yaml = render_mihomo_yaml_with_tun(&profile, true)?;
    let config = write_private_atomic(&root, "manis-generated.yaml", yaml.as_bytes())?;

    let status = Command::new(binary)
        .args(["-t", "-d"])
        .arg(&root)
        .arg("-f")
        .arg(&config)
        .status()?;

    fs::remove_dir_all(root)?;
    assert!(status.success(), "Mihomo rejected generated fixture YAML");
    Ok(())
}

#[test]
#[ignore = "requires MANIS_MIHOMO_TEST_BINARY pointing to a local Mihomo executable"]
fn generated_vless_profile_passes_mihomo_validation() -> Result<(), Box<dyn std::error::Error>> {
    let binary = std::env::var_os("MANIS_MIHOMO_TEST_BINARY")
        .ok_or("MANIS_MIHOMO_TEST_BINARY is required")?;
    let root = std::env::temp_dir().join(format!(
        "manis-vless-mihomo-validation-{}",
        std::process::id()
    ));
    if root.exists() {
        fs::remove_dir_all(&root)?;
    }
    let vless = VlessProxy::parse_share_link(
        "vless://00000000-0000-4000-8000-000000000000@edge.example.invalid:443?encryption=none&security=reality&type=grpc&sni=cdn.example.invalid&fp=chrome&pbk=Qs24XU-ibEZ3LWDjGBITKdQvualLy0pi_PI0qoF79A8&sid=0123456789abcdef&serviceName=manis#Saved",
    )?;
    let reality_tcp = VlessProxy::parse_share_link(
        "vless://10000000-0000-4000-8000-000000000001@198.51.100.7:443?security=reality&encryption=none&pbk=Qs24XU-ibEZ3LWDjGBITKdQvualLy0pi_PI0qoF79A8&headerType=&fp=chrome&type=tcp&flow=xtls-rprx-vision&sni=cdn.example.invalid#Reality%20TCP",
    )?;
    let profile = Profile::qx_sources(Vec::new(), vec![vless, reality_tcp], 17_890)?;
    let yaml = render_mihomo_yaml(&profile)?;
    let config = write_private_atomic(&root, "manis-generated.yaml", yaml.as_bytes())?;

    let status = Command::new(binary)
        .args(["-t", "-d"])
        .arg(&root)
        .arg("-f")
        .arg(&config)
        .status()?;

    fs::remove_dir_all(root)?;
    assert!(
        status.success(),
        "Mihomo rejected generated VLESS fixture YAML"
    );
    Ok(())
}

#[test]
#[ignore = "requires MANIS_MIHOMO_TEST_BINARY pointing to a local Mihomo executable"]
fn generated_user_policy_groups_pass_mihomo_validation() -> Result<(), Box<dyn std::error::Error>> {
    let binary = std::env::var_os("MANIS_MIHOMO_TEST_BINARY")
        .ok_or("MANIS_MIHOMO_TEST_BINARY is required")?;
    let root = std::env::temp_dir().join(format!(
        "manis-groups-mihomo-validation-{}",
        std::process::id()
    ));
    if root.exists() {
        fs::remove_dir_all(&root)?;
    }
    let subscription =
        SecretUrl::parse_https("https://subscription.example.invalid/client?token=fixture-secret")?;
    let global_exit = PolicyRef::Group(Name::parse(MANIS_GLOBAL_GROUP_NAME)?);
    let group = UserPolicyGroup {
        name: Name::parse("HK Auto")?,
        icon: None,
        kind: UserPolicyGroupKind::UrlTest {
            tolerance: 50,
            interval_secs: 600,
        },
        provider_indexes: vec![0],
        direct_proxies: Vec::new(),
        direct_policies: vec![global_exit.clone(), PolicyRef::Direct, PolicyRef::Reject],
        filter: Some("(?i)Hong Kong".to_owned()),
    };
    let manual = UserPolicyGroup {
        name: Name::parse("Manual Proxy")?,
        icon: None,
        kind: UserPolicyGroupKind::Select,
        provider_indexes: Vec::new(),
        direct_proxies: Vec::new(),
        direct_policies: vec![global_exit],
        filter: None,
    };
    let profile = Profile::qx_sources_with_groups(
        vec![subscription],
        Vec::new(),
        vec![group, manual],
        17_890,
    )?;
    let yaml = render_mihomo_yaml(&profile)?;
    let config = write_private_atomic(&root, "manis-generated.yaml", yaml.as_bytes())?;

    let status = Command::new(binary)
        .args(["-t", "-d"])
        .arg(&root)
        .arg("-f")
        .arg(&config)
        .status()?;

    fs::remove_dir_all(root)?;
    assert!(
        status.success(),
        "Mihomo rejected generated policy-group YAML"
    );
    Ok(())
}

#[test]
#[ignore = "requires MANIS_MIHOMO_TEST_BINARY pointing to a local Mihomo executable"]
fn generated_compound_rule_passes_mihomo_validation() -> Result<(), Box<dyn std::error::Error>> {
    use manis_profile::{PolicyRef, Rule, RuleCondition};

    let binary = std::env::var_os("MANIS_MIHOMO_TEST_BINARY")
        .ok_or("MANIS_MIHOMO_TEST_BINARY is required")?;
    let root = std::env::temp_dir().join(format!(
        "manis-profile-direct-rules-validation-{}",
        std::process::id()
    ));
    if root.exists() {
        fs::remove_dir_all(&root)?;
    }
    let mut profile = Profile::qx_default(SecretUrl::parse_https(
        "https://subscription.example.invalid/client?token=fixture-secret",
    )?)?;
    profile.rules.insert(
        0,
        Rule::All {
            conditions: vec![
                RuleCondition::DomainSuffix("github.com".to_owned()),
                RuleCondition::DstPort(22),
            ],
            policy: PolicyRef::Direct,
        },
    );
    let yaml = render_mihomo_yaml(&profile)?;
    let config = write_private_atomic(&root, "manis-generated.yaml", yaml.as_bytes())?;

    let status = Command::new(binary)
        .args(["-t", "-d"])
        .arg(&root)
        .arg("-f")
        .arg(&config)
        .status()?;

    fs::remove_dir_all(root)?;
    assert!(status.success(), "Mihomo rejected generated compound rule");
    Ok(())
}
