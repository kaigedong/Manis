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
fn source_store_keeps_multiple_subscriptions_saved_nodes_and_fold_state()
-> Result<(), Box<dyn std::error::Error>> {
    let root = test_temp_dir("manis-multi-source-store");
    let store = root.join("subscriptions");
    let imported_before = super::current_unix_secs();
    let first = super::save_subscription_source_in(
        &store,
        "https://first.example.invalid/client?token=fixture-one&name=First",
    )?;
    let second = super::save_subscription_source_in(
        &store,
        "https://second.example.invalid/client?token=fixture-two&name=Second",
    )?;
    let duplicate = super::save_subscription_source_in(
        &store,
        "https://first.example.invalid/client?token=fixture-one&name=First",
    )?;
    let saved = super::save_single_node_source_in(
        &store,
        "vless://00000000-0000-4000-8000-000000000000@edge.example.invalid:443?security=tls#Saved",
    )?;
    super::save_collapsed_groups_in(&store, [first.id.as_str(), "saved", "../../unsafe"])?;

    let subscriptions = super::load_subscription_sources_in(&store)?;
    let nodes = super::load_single_node_sources_in(&store)?;
    assert_eq!(subscriptions.len(), 2);
    assert_ne!(first.id, second.id);
    assert_eq!(duplicate.id, first.id);
    assert_eq!(
        subscriptions[0].refresh_interval,
        super::RemoteSourceRefreshInterval::Manual
    );
    assert!(subscriptions[0].last_successful_update_unix_secs >= imported_before);
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0].source.preview().name, "Saved");
    assert!(!format!("{first:?}").contains("fixture-one"));
    assert!(!format!("{saved:?}").contains("00000000"));
    assert_eq!(
        super::load_collapsed_groups_in(&store)?,
        vec!["saved".to_owned(), first.id.clone()]
    );

    super::remove_subscription_source_in(&store, &first.id)?;
    super::remove_single_node_source_in(&store, &saved.id)?;
    assert_eq!(super::load_subscription_sources_in(&store)?.len(), 1);
    assert!(super::load_single_node_sources_in(&store)?.is_empty());

    fs::remove_dir_all(root)?;
    Ok(())
}

#[cfg(not(windows))]
#[test]
fn subscription_name_and_enabled_state_round_trip_and_control_compilation()
-> Result<(), Box<dyn std::error::Error>> {
    let root = test_temp_dir("manis-subscription-enabled-state");
    let store = root.join("subscriptions");
    let stored = super::save_subscription_source_with_options_in(
        &store,
        "https://disabled.example.invalid/client?token=private",
        "备用订阅",
        super::RemoteSourceRefreshInterval::SixHours,
        false,
    )?;

    let loaded = super::load_subscription_sources_in(&store)?;
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].name, "备用订阅");
    assert!(!loaded[0].enabled);
    assert_eq!(
        loaded[0].refresh_interval,
        super::RemoteSourceRefreshInterval::SixHours
    );
    let disabled_profile =
        super::compile_saved_profile(&store, None, manis_core::KernelKind::Mihomo)?;
    assert!(disabled_profile.providers.is_empty());

    super::update_subscription_source_enabled_in(&store, &stored.id, true)?;
    let enabled_profile =
        super::compile_saved_profile(&store, None, manis_core::KernelKind::Mihomo)?;
    assert_eq!(enabled_profile.providers.len(), 1);

    fs::remove_dir_all(root)?;
    Ok(())
}

#[cfg(not(windows))]
#[test]
fn single_node_sources_are_protocol_agnostic_editable_and_disableable()
-> Result<(), Box<dyn std::error::Error>> {
    let root = test_temp_dir("manis-single-node-source");
    let store = root.join("subscriptions");
    let stored = super::save_single_node_source_with_options_in(
        &store,
        "trojan://fixture-password@example.invalid:443?security=tls#Original",
        "家庭节点",
        false,
    )?;

    let loaded = super::load_single_node_sources_in(&store)?;
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].name, "家庭节点");
    assert!(!loaded[0].enabled);
    assert!(
        super::compile_saved_profile(&store, None, manis_core::KernelKind::Mihomo,)?
            .providers
            .is_empty()
    );

    let updated = super::update_single_node_source_in(
        &store,
        &stored.id,
        "ss://fixture@example.invalid:8388#Edited",
        "办公节点",
        true,
    )?;
    assert_eq!(updated.name, "办公节点");
    assert!(updated.enabled);
    let profile = super::compile_saved_profile(&store, None, manis_core::KernelKind::Mihomo)?;
    assert_eq!(profile.providers.len(), 1);
    assert!(matches!(
        profile.providers[0].source,
        manis_profile::ProxyProviderSource::File
    ));

    fs::remove_dir_all(root)?;
    Ok(())
}

#[cfg(not(windows))]
#[test]
fn subscription_sources_support_refresh_metadata_and_legacy_url_files()
-> Result<(), Box<dyn std::error::Error>> {
    let root = test_temp_dir("manis-source-refresh-metadata");
    let store = root.join("subscriptions");
    fs::create_dir(&store)?;
    fs::set_permissions(&store, fs::Permissions::from_mode(0o700))?;
    let legacy_path = store.join("source-feed.url");
    fs::write(
        &legacy_path,
        "https://legacy.example.invalid/client?token=fixture-legacy",
    )?;
    fs::set_permissions(&legacy_path, fs::Permissions::from_mode(0o600))?;

    let legacy = super::load_subscription_sources_in(&store)?
        .into_iter()
        .next()
        .ok_or("legacy source")?;
    assert_eq!(legacy.id, "source-feed");
    assert_eq!(
        legacy.refresh_interval,
        super::RemoteSourceRefreshInterval::Manual
    );
    assert_eq!(legacy.last_successful_update_unix_secs, 0);

    let updated = super::update_subscription_source_refresh_interval_in(
        &store,
        &legacy.id,
        super::RemoteSourceRefreshInterval::SixHours,
    )?;
    assert_eq!(
        updated.refresh_interval,
        super::RemoteSourceRefreshInterval::SixHours
    );
    assert_eq!(updated.last_successful_update_unix_secs, 0);

    let refreshed = super::mark_subscription_source_update_success_in(&store, &legacy.id, 42)?;
    assert_eq!(
        refreshed.refresh_interval,
        super::RemoteSourceRefreshInterval::SixHours
    );
    assert_eq!(refreshed.last_successful_update_unix_secs, 42);
    let reloaded = super::load_subscription_sources_in(&store)?;
    assert_eq!(reloaded, vec![refreshed]);

    let long_url = format!("https://long.example.invalid/{}", "a".repeat(9 * 1024));
    let long_source = super::save_subscription_source_in(&store, &long_url)?;
    assert!(
        super::load_subscription_sources_in(&store)?
            .iter()
            .any(|source| source.id == long_source.id)
    );

    fs::remove_dir_all(root)?;
    Ok(())
}

#[cfg(not(windows))]
#[test]
fn subscription_proxy_dns_is_extracted_and_persisted_across_v1_upgrade()
-> Result<(), Box<dyn std::error::Error>> {
    let document = r#"
mixed-port: 7890
dns:
  enable: true
  proxy-server-nameserver:
    - 'https://192.0.2.10:8443/dns-query/clash?site=fixture'
    - "https://198.51.100.20/dns-query"
    - http://192.0.2.30/unsafe
proxies: []
"#;
    let nameservers = super::extract_subscription_proxy_nameservers(document);
    assert_eq!(nameservers.len(), 2);
    assert_eq!(
        super::extract_subscription_proxy_nameservers(
            "dns:\n  proxy-server-nameserver: [https://203.0.113.1/dns-query]\n"
        )
        .len(),
        1
    );

    let root = test_temp_dir("manis-subscription-proxy-dns");
    let store = root.join("subscriptions");
    fs::create_dir(&store)?;
    fs::set_permissions(&store, fs::Permissions::from_mode(0o700))?;
    let path = store.join("source-feed.url");
    let legacy_v1 = [
        super::LEGACY_RELAY_STORED_SUBSCRIPTION_VERSION.to_owned(),
        "id\tsource-feed".to_owned(),
        format!(
            "url\t{}",
            super::encode_hex("https://legacy.example.invalid/client?token=fixture")
        ),
        "refresh\tmanual".to_owned(),
        "last-success\t42".to_owned(),
    ]
    .join("\n");
    fs::write(&path, legacy_v1)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;

    let upgraded = super::update_subscription_source_proxy_nameservers_in(
        &store,
        "source-feed",
        &nameservers,
    )?;
    assert_eq!(upgraded.proxy_server_nameservers, nameservers);
    assert_eq!(upgraded.last_successful_update_unix_secs, 42);

    let contents = fs::read_to_string(&path)?;
    assert!(contents.starts_with(super::STORED_SUBSCRIPTION_VERSION));
    let reloaded = super::load_subscription_sources_in(&store)?;
    assert_eq!(reloaded, vec![upgraded]);
    assert!(!format!("{:?}", reloaded[0]).contains("192.0.2.10"));
    let profile = super::compile_saved_profile(&store, None, manis_core::KernelKind::Mihomo)?;
    let yaml = manis_profile::render_mihomo_yaml(&profile)?;
    assert!(yaml.contains("https://192.0.2.10:8443/dns-query/clash?site=fixture"));

    fs::remove_dir_all(root)?;
    Ok(())
}
