#![allow(unused_imports)]

use super::*;
use std::collections::BTreeSet;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use manis_engine::ControllerEndpoint;

#[test]
fn managed_mihomo_rejects_a_controller_with_a_failed_mixed_listener() {
    let spec = super::ManagedGeneratedProfile {
        kernel: manis_core::KernelKind::Mihomo,
        binary: PathBuf::from("/Applications/Manis.app/Contents/Resources/mihomo/mihomo"),
        data_dir: PathBuf::from("/tmp/manis-runtime"),
        controller: ControllerEndpoint::UnixSocket(PathBuf::from(
            "/tmp/manis-runtime/controller.sock",
        )),
        expected_mixed_port: Some(17_890),
        profile_store_dir: None,
        controller_secret: None,
    };
    let failed = manis_mihomo::RuntimeConfig {
        mixed_port: Some(0),
        ..manis_mihomo::RuntimeConfig::default()
    };
    let ready = manis_mihomo::RuntimeConfig {
        mixed_port: Some(17_890),
        ..manis_mihomo::RuntimeConfig::default()
    };

    assert!(super::validate_managed_runtime(&spec, &failed).is_err());
    assert!(super::validate_managed_runtime(&spec, &ready).is_ok());
}

#[test]
fn external_controller_and_custom_config_overrides_are_not_runtime_inputs() {
    assert_eq!(
        super::first_unsupported_runtime_override(|name| name == super::CONTROLLER_ENV),
        Some(super::CONTROLLER_ENV)
    );
    assert_eq!(
        super::first_unsupported_runtime_override(|name| name == super::CONFIG_ENV),
        Some(super::CONFIG_ENV)
    );
    assert_eq!(
        super::first_unsupported_runtime_override(|name| name == super::CONTROLLER_SECRET_ENV),
        Some(super::CONTROLLER_SECRET_ENV)
    );
    assert_eq!(
        super::first_unsupported_runtime_override(|name| name == super::SUBSCRIPTION_FILE_ENV),
        Some(super::SUBSCRIPTION_FILE_ENV)
    );
    assert_eq!(
        super::first_unsupported_runtime_override(|name| name == super::BINARY_ENV),
        None
    );
}

#[test]
fn saved_sources_build_a_managed_mihomo_runtime_without_starting_kernel()
-> Result<(), Box<dyn std::error::Error>> {
    let root = test_temp_dir("manis-managed-mihomo-saved-sources");
    let store = root.join("subscriptions");
    let data_dir = root.join("runtime");
    let binary = root.join("mihomo");
    fs::write(&binary, "#!/bin/sh\nexit 0\n")?;
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o700))?;
    let saved = super::save_single_node_source_in(
        &store,
        "vless://00000000-0000-4000-8000-000000000000@edge.example.invalid:443?security=tls#Private%20Edge",
    )?;

    let runtime = super::build_saved_sources_mihomo_runtime_in(
        &store,
        &binary.canonicalize()?,
        &data_dir,
        &ControllerEndpoint::UnixSocket(data_dir.join("controller.sock")),
    )?;
    let cloned_runtime = runtime.clone();

    match (&runtime, &cloned_runtime) {
        (
            super::ControllerRuntime::Managed {
                apply_lock: left, ..
            },
            super::ControllerRuntime::Managed {
                apply_lock: right, ..
            },
        ) => assert!(std::sync::Arc::ptr_eq(left, right)),
        _ => panic!("saved sources should share a managed apply lock"),
    }

    assert_eq!(
        runtime.managed_health()?,
        super::ManagedRuntimeHealth::Stopped
    );

    match runtime {
        super::ControllerRuntime::Managed {
            profile_source,
            generated_profile,
            ..
        } => {
            assert_eq!(profile_source, super::RuntimeProfileSource::SavedSources);
            assert_eq!(
                generated_profile.expect("generated profile").kernel,
                manis_core::KernelKind::Mihomo
            );
        }
        _ => panic!("saved sources should build a managed runtime"),
    }
    let generated = fs::read_to_string(data_dir.join(super::GENERATED_PROFILE_FILE))?;
    assert!(generated.contains("type: \"file\""));
    assert!(generated.contains(&format!("single_nodes/{}.txt", saved.id)));
    assert!(
        fs::read_to_string(
            data_dir
                .join("single_nodes")
                .join(format!("{}.txt", saved.id))
        )?
        .contains("Private%20Edge")
    );
    assert!(generated.contains("mixed-port: 17890"));

    fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn empty_workspace_builds_a_managed_direct_only_mihomo_runtime()
-> Result<(), Box<dyn std::error::Error>> {
    let root = test_temp_dir("manis-managed-mihomo-empty-workspace");
    let store = root.join("subscriptions");
    let data_dir = root.join("runtime");
    let binary = root.join("mihomo");
    fs::create_dir_all(&store)?;
    fs::set_permissions(&store, fs::Permissions::from_mode(0o700))?;
    fs::write(&binary, "#!/bin/sh\nexit 0\n")?;
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o700))?;

    let runtime = super::build_saved_sources_mihomo_runtime_in(
        &store,
        &binary.canonicalize()?,
        &data_dir,
        &ControllerEndpoint::UnixSocket(data_dir.join("controller.sock")),
    )?;

    assert!(matches!(runtime, super::ControllerRuntime::Managed { .. }));
    let generated = fs::read_to_string(data_dir.join(super::GENERATED_PROFILE_FILE))?;
    assert!(generated.contains("rules:\n"));
    assert!(generated.ends_with("  - \"MATCH,DIRECT\"\n"));
    assert!(!generated.contains("__MANIS_GLOBAL__"));

    fs::remove_dir_all(root)?;
    Ok(())
}
