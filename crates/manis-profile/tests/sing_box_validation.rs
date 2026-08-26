use std::fs;
use std::process::Command;

use manis_profile::{
    Profile, SingBoxOptions, VlessProxy, render_sing_box_json, write_private_atomic,
};

#[test]
#[ignore = "requires MANIS_SING_BOX_TEST_BINARY pointing to a local sing-box executable"]
fn generated_reality_profile_passes_sing_box_check() -> Result<(), Box<dyn std::error::Error>> {
    let binary = std::env::var_os("MANIS_SING_BOX_TEST_BINARY")
        .ok_or("MANIS_SING_BOX_TEST_BINARY is required")?;
    let root = std::env::temp_dir().join(format!(
        "manis-profile-sing-box-validation-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    let vless = VlessProxy::parse_share_link(
        "vless://00000000-0000-4000-8000-000000000000@198.51.100.7:443?security=reality&encryption=none&pbk=Qs24XU-ibEZ3LWDjGBITKdQvualLy0pi_PI0qoF79A8&headerType=&fp=chrome&type=tcp&flow=xtls-rprx-vision&sni=cdn.example.invalid#Reality%20TCP",
    )?;
    let profile = Profile::qx_sources(Vec::new(), vec![vless], 17_890)?;
    let json = render_sing_box_json(
        &profile,
        &SingBoxOptions::new("127.0.0.1:19090", "fixture-controller-secret"),
    )?;
    let config = write_private_atomic(&root, "manis-generated.json", json.as_bytes())?;

    let status = Command::new(binary)
        .arg("check")
        .arg("-c")
        .arg(&config)
        .arg("-D")
        .arg(&root)
        .status()?;

    fs::remove_dir_all(root)?;
    assert!(status.success(), "sing-box rejected generated fixture JSON");
    Ok(())
}

#[test]
#[ignore = "requires MANIS_SING_BOX_TEST_BINARY pointing to a local sing-box executable"]
fn generated_direct_rules_pass_sing_box_check() -> Result<(), Box<dyn std::error::Error>> {
    use manis_profile::{PolicyRef, Rule};

    let binary = std::env::var_os("MANIS_SING_BOX_TEST_BINARY")
        .ok_or("MANIS_SING_BOX_TEST_BINARY is required")?;
    let root = std::env::temp_dir().join(format!(
        "manis-profile-sing-box-direct-rules-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    let vless = VlessProxy::parse_share_link(
        "vless://00000000-0000-4000-8000-000000000000@198.51.100.7:443?security=reality&encryption=none&pbk=Qs24XU-ibEZ3LWDjGBITKdQvualLy0pi_PI0qoF79A8&headerType=&fp=chrome&type=tcp&flow=xtls-rprx-vision&sni=cdn.example.invalid#Reality%20TCP",
    )?;
    let mut profile = Profile::qx_sources(Vec::new(), vec![vless], 17_890)?;
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
    let json = render_sing_box_json(
        &profile,
        &SingBoxOptions::new("127.0.0.1:19090", "fixture-controller-secret"),
    )?;
    let config = write_private_atomic(&root, "manis-generated.json", json.as_bytes())?;

    let status = Command::new(binary)
        .arg("check")
        .arg("-c")
        .arg(&config)
        .arg("-D")
        .arg(&root)
        .status()?;

    fs::remove_dir_all(root)?;
    assert!(status.success(), "sing-box rejected generated direct rules");
    Ok(())
}
