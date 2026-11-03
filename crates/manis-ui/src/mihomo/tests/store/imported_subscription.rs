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
fn imported_subscription_round_trips_privately_and_replaces_atomically()
-> Result<(), Box<dyn std::error::Error>> {
    let root = test_temp_dir("manis-import-store");
    let store = root.join("subscriptions");
    let first = "https://first.example.invalid/client?token=fixture-one";
    let second = "https://second.example.invalid/client?token=fixture-two";

    let first_secret = super::save_imported_subscription_in(&store, first)?;
    assert_eq!(
        super::load_imported_subscription_in(&store)?,
        Some(first_secret)
    );
    let second_secret = super::save_imported_subscription_in(&store, second)?;
    assert_eq!(
        super::load_imported_subscription_in(&store)?,
        Some(second_secret)
    );

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(fs::metadata(&store)?.permissions().mode() & 0o077, 0);
        assert_eq!(
            fs::metadata(store.join("config.toml"))?
                .permissions()
                .mode()
                & 0o077,
            0
        );
    }

    fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn imported_subscription_load_rejects_symlinks_and_redacts_corruption()
-> Result<(), Box<dyn std::error::Error>> {
    let root = test_temp_dir("manis-import-corrupt");
    let store = root.join("subscriptions");
    fs::create_dir(&store)?;
    let secret_path = store.join("subscription.url");
    fs::write(
        &secret_path,
        "https://example.invalid/?token=private-fixture\nsecond-line",
    )?;

    let error = super::load_imported_subscription_in(&store)
        .expect_err("multi-line stored input must fail closed");
    assert!(!error.to_string().contains("private-fixture"));
    assert!(!format!("{error:?}").contains("private-fixture"));

    #[cfg(unix)]
    {
        fs::remove_file(&secret_path)?;
        let outside = root.join("outside.url");
        fs::write(&outside, "https://example.invalid/subscription")?;
        std::os::unix::fs::symlink(&outside, &secret_path)?;
        assert!(super::load_imported_subscription_in(&store).is_err());
    }

    fs::remove_dir_all(root)?;
    Ok(())
}

#[cfg(not(windows))]
#[test]
fn removing_an_imported_subscription_is_idempotent() -> Result<(), Box<dyn std::error::Error>> {
    let root = test_temp_dir("manis-import-remove");
    let store = root.join("subscriptions");
    super::save_imported_subscription_in(&store, "https://example.invalid/client?token=fixture")?;

    super::remove_imported_subscription_in(&store)?;
    super::remove_imported_subscription_in(&store)?;
    assert_eq!(super::load_imported_subscription_in(&store)?, None);

    fs::remove_dir_all(root)?;
    Ok(())
}
