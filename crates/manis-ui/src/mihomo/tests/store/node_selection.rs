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

#[cfg(not(windows))]
#[test]
fn node_selection_preferences_missing_file_defaults() -> Result<(), Box<dyn std::error::Error>> {
    let root = test_temp_dir("manis-node-selection-missing");
    let store = root.join("subscriptions");

    assert_eq!(
        super::load_node_selection_preferences_in(&store)?,
        super::NodeSelectionPreferences::default()
    );

    fs::remove_dir_all(root)?;
    Ok(())
}

#[cfg(not(windows))]
#[test]
fn node_selection_preferences_round_trip_privately() -> Result<(), Box<dyn std::error::Error>> {
    use manis_core::NodeIdentity;

    let root = test_temp_dir("manis-node-selection-store");
    let store = root.join("subscriptions");
    let global = NodeIdentity::new("subscription:source-1", "Hong Kong Edge")?;
    let mut preferences = super::NodeSelectionPreferences::default();
    preferences.set_global(global.clone());
    preferences.set_policy_target("视频服务", "Tokyo Manual")?;

    super::save_node_selection_preferences_in(&store, &preferences)?;
    let loaded = super::load_node_selection_preferences_in(&store)?;

    assert_eq!(loaded.global(), Some(&global));
    assert_eq!(loaded.policy_target("视频服务"), Some("Tokyo Manual"));
    assert_eq!(
        loaded.iter_policy_targets().collect::<Vec<_>>(),
        vec![("视频服务", "Tokyo Manual")]
    );

    #[cfg(unix)]
    {
        assert_eq!(fs::metadata(&store)?.permissions().mode() & 0o077, 0);
        let path = store.join("config.toml");
        assert_eq!(fs::metadata(&path)?.permissions().mode() & 0o077, 0);
        let stored_text = crate::config_toml::read_entry(
            &store,
            super::NODE_SELECTION_PREFERENCES_FILE,
            1024 * 1024,
        )?
        .expect("stored preferences");
        assert!(!stored_text.contains("Hong Kong Edge"));
        assert!(!stored_text.contains("Tokyo Manual"));
    }

    fs::remove_dir_all(root)?;
    Ok(())
}

#[cfg(not(windows))]
#[test]
fn node_selection_preferences_reject_malformed_and_duplicate_records()
-> Result<(), Box<dyn std::error::Error>> {
    let root = test_temp_dir("manis-node-selection-invalid");
    let store = root.join("subscriptions");
    fs::create_dir(&store)?;
    fs::set_permissions(&store, fs::Permissions::from_mode(0o700))?;
    let path = store.join(super::NODE_SELECTION_PREFERENCES_FILE);

    let duplicate = [
        super::NODE_SELECTION_PREFERENCES_VERSION.to_owned(),
        format!(
            "policy\t{}\t{}",
            super::encode_hex("Proxy"),
            super::encode_hex("Hong Kong Edge")
        ),
        format!(
            "policy\t{}\t{}",
            super::encode_hex("Proxy"),
            super::encode_hex("Tokyo Edge")
        ),
    ]
    .join("\n");
    fs::write(&path, duplicate)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    assert!(super::load_node_selection_preferences_in(&store).is_err());

    let malformed = [
        super::NODE_SELECTION_PREFERENCES_VERSION.to_owned(),
        "global\tnot-hex\talso-not-hex".to_owned(),
    ]
    .join("\n");
    fs::write(&path, malformed)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    assert!(super::load_node_selection_preferences_in(&store).is_err());

    let invalid_target = [
        super::NODE_SELECTION_PREFERENCES_VERSION.to_owned(),
        format!(
            "policy\t{}\t{}",
            super::encode_hex("Proxy"),
            super::encode_hex(" bad target ")
        ),
    ]
    .join("\n");
    fs::write(&path, invalid_target)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    assert!(super::load_node_selection_preferences_in(&store).is_err());

    fs::remove_dir_all(root)?;
    Ok(())
}

#[cfg(unix)]
#[test]
fn node_selection_preferences_reject_group_readable_files() -> Result<(), Box<dyn std::error::Error>>
{
    let root = test_temp_dir("manis-node-selection-permission");
    let store = root.join("subscriptions");
    fs::create_dir(&store)?;
    fs::set_permissions(&store, fs::Permissions::from_mode(0o700))?;
    let path = store.join(super::NODE_SELECTION_PREFERENCES_FILE);
    fs::write(&path, super::NODE_SELECTION_PREFERENCES_VERSION)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644))?;

    assert!(super::load_node_selection_preferences_in(&store).is_err());

    fs::remove_dir_all(root)?;
    Ok(())
}
