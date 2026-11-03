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
fn legacy_relay_storage_versions_remain_readable() {
    assert!(super::storage_version_supported(
        Some(super::LEGACY_RELAY_STORED_SUBSCRIPTION_VERSION),
        super::STORED_SUBSCRIPTION_VERSION,
        super::LEGACY_RELAY_STORED_SUBSCRIPTION_VERSION,
    ));
    assert!(super::storage_version_supported(
        Some(super::LEGACY_RELAY_QX_RULE_SOURCE_VERSION),
        super::QX_RULE_SOURCE_VERSION,
        super::LEGACY_RELAY_QX_RULE_SOURCE_VERSION,
    ));
    assert!(super::storage_version_supported(
        Some(super::LEGACY_RELAY_NODE_SELECTION_PREFERENCES_VERSION),
        super::NODE_SELECTION_PREFERENCES_VERSION,
        super::LEGACY_RELAY_NODE_SELECTION_PREFERENCES_VERSION,
    ));
}

#[test]
fn remote_source_refresh_intervals_cycle_and_respect_last_success() {
    use super::RemoteSourceRefreshInterval as Interval;

    assert!(!Interval::Manual.is_due(0, u64::MAX));
    assert!(Interval::Hourly.is_due(0, 1));
    assert!(!Interval::Hourly.is_due(10_000, 13_599));
    assert!(Interval::Hourly.is_due(10_000, 13_600));
    assert!(!Interval::Daily.is_due(100_000, 99_000));
}

#[test]
fn routing_mode_round_trips_in_the_private_workspace_store()
-> Result<(), Box<dyn std::error::Error>> {
    use manis_core::RoutingMode;

    let root = test_temp_dir("manis-routing-mode-store");
    let store = root.join("subscriptions");
    assert_eq!(super::load_routing_mode_in(&store)?, RoutingMode::Rule);

    super::save_routing_mode_in(&store, RoutingMode::Global)?;
    assert_eq!(super::load_routing_mode_in(&store)?, RoutingMode::Global);

    super::save_routing_mode_in(&store, RoutingMode::Direct)?;
    assert_eq!(super::load_routing_mode_in(&store)?, RoutingMode::Direct);
    fs::remove_dir_all(root)?;
    Ok(())
}
