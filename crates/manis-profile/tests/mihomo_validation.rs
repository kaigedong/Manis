use std::fs;
use std::process::Command;

use manis_profile::{
    Name, Profile, SecretUrl, UserPolicyGroup, UserPolicyGroupKind, VlessProxy, render_mihomo_yaml,
    write_private_atomic,
};

#[test]
#[ignore = "requires MANIS_MIHOMO_TEST_BINARY pointing to a local Mihomo executable"]
fn generated_qx_profile_passes_mihomo_validation() -> Result<(), Box<dyn std::error::Error>> {
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
    let yaml = render_mihomo_yaml(&profile)?;
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
    let group = UserPolicyGroup {
        name: Name::parse("HK Auto")?,
        icon: None,
        kind: UserPolicyGroupKind::UrlTest {
            tolerance: 50,
            interval_secs: 600,
        },
        provider_indexes: vec![0],
        direct_proxies: Vec::new(),
        filter: Some("(?i)Hong Kong".to_owned()),
    };
    let profile =
        Profile::qx_sources_with_groups(vec![subscription], Vec::new(), vec![group], 17_890)?;
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
fn generated_direct_rules_pass_mihomo_validation() -> Result<(), Box<dyn std::error::Error>> {
    use manis_profile::{PolicyRef, Rule};

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
        Rule::DomainSuffix {
            value: "github.com".to_owned(),
            policy: PolicyRef::Direct,
        },
    );
    profile.rules.insert(
        0,
        Rule::DstPort {
            port: 22,
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
    assert!(status.success(), "Mihomo rejected generated direct rules");
    Ok(())
}
