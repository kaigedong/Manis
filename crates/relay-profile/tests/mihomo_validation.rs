use std::fs;
use std::process::Command;

use relay_profile::{Profile, SecretUrl, VlessProxy, render_mihomo_yaml, write_private_atomic};

#[test]
#[ignore = "requires RELAY_MIHOMO_TEST_BINARY pointing to a local Mihomo executable"]
fn generated_qx_profile_passes_mihomo_validation() -> Result<(), Box<dyn std::error::Error>> {
    let binary = std::env::var_os("RELAY_MIHOMO_TEST_BINARY")
        .ok_or("RELAY_MIHOMO_TEST_BINARY is required")?;
    let root = std::env::temp_dir().join(format!(
        "relay-profile-mihomo-validation-{}",
        std::process::id()
    ));
    if root.exists() {
        fs::remove_dir_all(&root)?;
    }
    let profile = Profile::qx_default(SecretUrl::parse_https(
        "https://subscription.example.invalid/client?token=fixture-secret",
    )?)?;
    let yaml = render_mihomo_yaml(&profile)?;
    let config = write_private_atomic(&root, "relay-generated.yaml", yaml.as_bytes())?;

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
#[ignore = "requires RELAY_MIHOMO_TEST_BINARY pointing to a local Mihomo executable"]
fn generated_vless_profile_passes_mihomo_validation() -> Result<(), Box<dyn std::error::Error>> {
    let binary = std::env::var_os("RELAY_MIHOMO_TEST_BINARY")
        .ok_or("RELAY_MIHOMO_TEST_BINARY is required")?;
    let root = std::env::temp_dir().join(format!(
        "relay-vless-mihomo-validation-{}",
        std::process::id()
    ));
    if root.exists() {
        fs::remove_dir_all(&root)?;
    }
    let vless = VlessProxy::parse_share_link(
        "vless://00000000-0000-4000-8000-000000000000@edge.example.invalid:443?encryption=none&security=reality&type=grpc&sni=cdn.example.invalid&fp=chrome&pbk=Qs24XU-ibEZ3LWDjGBITKdQvualLy0pi_PI0qoF79A8&sid=0123456789abcdef&serviceName=relay#Saved",
    )?;
    let profile = Profile::qx_sources(Vec::new(), vec![vless], 17_890)?;
    let yaml = render_mihomo_yaml(&profile)?;
    let config = write_private_atomic(&root, "relay-generated.yaml", yaml.as_bytes())?;

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
