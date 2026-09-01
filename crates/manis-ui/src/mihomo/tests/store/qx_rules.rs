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
fn qx_rule_sources_round_trip_privately_with_counts() -> Result<(), Box<dyn std::error::Error>> {
    let root = test_temp_dir("manis-qx-rule-store");
    let store = root.join("subscriptions");
    let url = "https://rules.example.invalid/airports.list?token=fixture-secret";
    let content = r"
# QX rule source fixture
HOST-KEYWORD,google,PROXY
HOST-SUFFIX,githubusercontent.com,PROXY
IP-CIDR,192.0.2.0/24,DIRECT
";

    let stored = super::save_qx_rule_source_in(&store, url, "Proxy", content)?.into_source();
    let loaded = super::load_qx_rule_sources_in(&store)?;

    assert_eq!(loaded, vec![stored.clone()]);
    assert_eq!(stored.name, None);
    assert_eq!(stored.target_policy.as_str(), "Proxy");
    assert_eq!(stored.content, content);
    assert_eq!(stored.rule_count, 2);
    assert_eq!(stored.diagnostic_count, 1);
    assert_eq!(
        stored.refresh_interval,
        super::RemoteSourceRefreshInterval::Manual
    );
    assert!(stored.last_successful_update_unix_secs > 0);
    assert!(!format!("{stored:?}").contains("fixture-secret"));

    let duplicate = super::save_qx_rule_source_in(
        &store,
        url,
        "DIRECT",
        "DOMAIN-SUFFIX,duplicate.example,DIRECT",
    )?;
    let super::SaveQxRuleSourceOutcome::Existing(existing) = duplicate else {
        return Err("duplicate QX rule URL was stored twice".into());
    };
    assert_eq!(existing.id, stored.id);
    assert_eq!(existing.name, None);
    assert_eq!(existing.target_policy.as_str(), "Proxy");
    assert_eq!(existing.content, content);
    assert_eq!(super::load_qx_rule_sources_in(&store)?.len(), 1);

    #[cfg(unix)]
    {
        assert_eq!(fs::metadata(&store)?.permissions().mode() & 0o077, 0);
        let entry = fs::read_dir(&store)?.next().ok_or("stored QX file")??;
        assert_eq!(entry.metadata()?.permissions().mode() & 0o077, 0);
        let stored_bytes = fs::read(entry.path())?;
        let stored_text = String::from_utf8(stored_bytes)?;
        assert!(!stored_text.contains("fixture-secret"));
    }

    super::remove_qx_rule_source_in(&store, &stored.id)?;
    assert!(super::load_qx_rule_sources_in(&store)?.is_empty());
    fs::remove_dir_all(root)?;
    Ok(())
}

#[cfg(not(windows))]
#[test]
fn qx_rule_source_custom_name_round_trips_and_can_reset() -> Result<(), Box<dyn std::error::Error>>
{
    let root = test_temp_dir("manis-qx-rule-name");
    let store = root.join("subscriptions");
    let stored = super::save_named_qx_rule_source_in(
        &store,
        "https://rules.example.invalid/airports.list?token=fixture-secret",
        "  机场规则  ",
        "Proxy",
        "DOMAIN-SUFFIX,example.com,PROXY\n",
    )?
    .into_source();

    assert_eq!(stored.name.as_deref(), Some("机场规则"));
    let loaded = super::load_qx_rule_sources_in(&store)?;
    assert_eq!(loaded, vec![stored.clone()]);
    let entry = fs::read_dir(&store)?.next().ok_or("stored QX file")??;
    let stored_text = fs::read_to_string(entry.path())?;
    assert!(stored_text.starts_with(super::QX_RULE_SOURCE_VERSION));
    assert!(stored_text.contains(&format!("name\t{}", super::encode_hex("机场规则"))));

    let reset = super::update_qx_rule_source_name_in(&store, &stored.id, "   ")?;
    assert_eq!(reset.name, None);
    assert_eq!(
        super::load_qx_rule_sources_in(&store)?
            .into_iter()
            .next()
            .and_then(|source| source.name),
        None
    );

    fs::remove_dir_all(root)?;
    Ok(())
}

#[cfg(not(windows))]
#[test]
fn qx_rule_source_name_survives_existing_mutations() -> Result<(), Box<dyn std::error::Error>> {
    let root = test_temp_dir("manis-qx-rule-name-preserve");
    let store = root.join("subscriptions");
    let stored = super::save_named_qx_rule_source_in(
        &store,
        "https://rules.example.invalid/old.list?token=fixture-secret",
        "初始规则",
        "Proxy",
        "DOMAIN-SUFFIX,old.example,PROXY\n",
    )?
    .into_source();
    let interval = super::update_qx_rule_source_refresh_interval_in(
        &store,
        &stored.id,
        super::RemoteSourceRefreshInterval::Hourly,
    )?;
    let target = super::update_qx_rule_source_target_in(&store, &stored.id, "DIRECT")?;
    let disabled = super::update_qx_rule_source_enabled_in(&store, &stored.id, false)?;
    let refreshed = super::replace_qx_rule_source_content_in(
        &store,
        &stored.id,
        "DOMAIN-SUFFIX,refresh.example,DIRECT\n",
        321,
    )?;
    let edited = super::replace_qx_rule_source_definition_in(
        &store,
        &stored.id,
        "编辑后的规则",
        "https://rules.example.invalid/new.list?token=fixture-secret",
        "Proxy",
        "DOMAIN-SUFFIX,new.example,PROXY\n",
        super::RemoteSourceRefreshInterval::Daily,
        456,
    )?;

    assert_eq!(interval.name.as_deref(), Some("初始规则"));
    assert_eq!(target.name.as_deref(), Some("初始规则"));
    assert_eq!(disabled.name.as_deref(), Some("初始规则"));
    assert_eq!(refreshed.name.as_deref(), Some("初始规则"));
    assert_eq!(edited.id, stored.id);
    assert_eq!(edited.name.as_deref(), Some("编辑后的规则"));
    assert!(!edited.enabled);
    assert_eq!(edited.target_policy.as_str(), "Proxy");
    assert_eq!(
        edited.source.expose_to(str::to_owned),
        "https://rules.example.invalid/new.list?token=fixture-secret"
    );
    assert_eq!(
        super::load_qx_rule_sources_in(&store)?
            .into_iter()
            .next()
            .and_then(|source| source.name),
        Some("编辑后的规则".to_owned())
    );

    fs::remove_dir_all(root)?;
    Ok(())
}

#[cfg(not(windows))]
#[test]
fn qx_rule_source_invalid_name_does_not_damage_existing_file()
-> Result<(), Box<dyn std::error::Error>> {
    let root = test_temp_dir("manis-qx-rule-invalid-name");
    let store = root.join("subscriptions");
    let stored = super::save_named_qx_rule_source_in(
        &store,
        "https://rules.example.invalid/list?token=fixture-secret",
        "Valid",
        "Proxy",
        "DOMAIN-SUFFIX,example.com,PROXY\n",
    )?
    .into_source();
    let path = store.join(format!("{}{}", stored.id, super::QX_RULE_SOURCE_SUFFIX));
    let before = fs::read_to_string(&path)?;

    assert!(super::update_qx_rule_source_name_in(&store, &stored.id, "bad\nname").is_err());
    assert_eq!(fs::read_to_string(&path)?, before);
    assert_eq!(
        super::load_qx_rule_sources_in(&store)?
            .into_iter()
            .next()
            .and_then(|source| source.name),
        Some("Valid".to_owned())
    );
    assert!(
        super::save_named_qx_rule_source_in(
            &store,
            "https://rules.example.invalid/other.list",
            &"长".repeat(49),
            "Proxy",
            "DOMAIN-SUFFIX,other.example,PROXY\n",
        )
        .is_err()
    );
    assert_eq!(super::load_qx_rule_sources_in(&store)?.len(), 1);

    fs::remove_dir_all(root)?;
    Ok(())
}

#[cfg(not(windows))]
#[test]
fn routing_rule_group_order_round_trips_and_appends_new_groups()
-> Result<(), Box<dyn std::error::Error>> {
    let root = test_temp_dir("manis-routing-rule-group-order");
    let store = root.join("subscriptions");
    let first = super::save_qx_rule_source_in(
        &store,
        "https://rules.example.invalid/first.list",
        "DIRECT",
        "DOMAIN-SUFFIX,first.example,DIRECT\n",
    )?
    .into_source();
    let second = super::save_qx_rule_source_in(
        &store,
        "https://rules.example.invalid/second.list",
        "DIRECT",
        "DOMAIN-SUFFIX,second.example,DIRECT\n",
    )?
    .into_source();
    let stored_order = vec![
        second.id.clone(),
        super::MANUAL_ROUTING_RULE_GROUP_ID.to_owned(),
        first.id.clone(),
    ];

    super::save_routing_rule_group_order_in(&store, &stored_order)?;
    assert_eq!(
        super::load_routing_rule_group_order_in(&store)?,
        stored_order
    );

    let sources = super::load_qx_rule_sources_in(&store)?;
    let normalized = super::normalized_routing_rule_group_order(
        &[second.id.clone(), "qx-rule-removed".to_owned()],
        true,
        &sources,
    );
    assert_eq!(normalized[0], super::MANUAL_ROUTING_RULE_GROUP_ID);
    assert_eq!(normalized[1], second.id);
    assert_eq!(normalized[2], first.id);

    let mut moved = normalized.clone();
    assert!(super::move_routing_rule_group(
        &mut moved,
        &second.id,
        super::MoveDirection::Up,
    ));
    assert_eq!(moved[0], second.id);
    assert_eq!(moved[1], super::MANUAL_ROUTING_RULE_GROUP_ID);
    assert!(!super::move_routing_rule_group(
        &mut moved,
        &second.id,
        super::MoveDirection::Up,
    ));
    assert!(!super::move_routing_rule_group(
        &mut moved,
        &first.id,
        super::MoveDirection::Down,
    ));

    fs::remove_dir_all(root)?;
    Ok(())
}

#[cfg(not(windows))]
#[test]
fn saved_rule_group_order_controls_compiled_rule_priority() -> Result<(), Box<dyn std::error::Error>>
{
    let root = test_temp_dir("manis-compiled-rule-group-order");
    let store = root.join("subscriptions");
    super::save_subscription_source_in(
        &store,
        "https://subscription.example.invalid/client?token=fixture",
    )?;
    let first = super::save_qx_rule_source_in(
        &store,
        "https://rules.example.invalid/first.list",
        "DIRECT",
        "DOMAIN-SUFFIX,first.example,DIRECT\n",
    )?
    .into_source();
    let second = super::save_qx_rule_source_in(
        &store,
        "https://rules.example.invalid/second.list",
        "DIRECT",
        "DOMAIN-SUFFIX,second.example,DIRECT\n",
    )?
    .into_source();
    let manual = crate::manual_rule::ManualRule::parse(
        crate::manual_rule::ManualRuleKind::Host,
        "manual.example",
        "DIRECT",
    )?;
    let final_rule = crate::manual_rule::ManualRule::final_rule("DIRECT")?;
    crate::manual_rule::save_manual_rules_in(&store, &[final_rule, manual])?;
    super::save_routing_rule_group_order_in(
        &store,
        &[
            second.id,
            super::MANUAL_ROUTING_RULE_GROUP_ID.to_owned(),
            first.id,
        ],
    )?;

    let profile = super::compile_saved_profile(&store, None, manis_core::KernelKind::Mihomo)?;
    let yaml = manis_profile::render_mihomo_yaml(&profile)?;
    let second_index = yaml.find("DOMAIN-SUFFIX,second.example,DIRECT");
    let manual_index = yaml.find("DOMAIN,manual.example,DIRECT");
    let first_index = yaml.find("DOMAIN-SUFFIX,first.example,DIRECT");
    let final_index = yaml.find("MATCH,DIRECT");
    assert!(second_index < manual_index && manual_index < first_index);
    assert!(first_index < final_index);
    assert!(yaml.trim_end().ends_with("\"MATCH,DIRECT\""));
    assert!(!yaml.contains("GEOIP,CN,DIRECT"));
    assert!(!yaml.contains("MATCH,__MANIS_GLOBAL__"));

    fs::remove_dir_all(root)?;
    Ok(())
}

#[cfg(not(windows))]
#[test]
fn qx_rule_sources_update_interval_and_success_atomically() -> Result<(), Box<dyn std::error::Error>>
{
    let root = test_temp_dir("manis-qx-rule-refresh");
    let store = root.join("subscriptions");
    let url = "https://rules.example.invalid/airports.list?token=fixture-secret";
    let initial = "DOMAIN-KEYWORD,google,PROXY\n";
    let updated_content = "DOMAIN-SUFFIX,github.com,PROXY\nDOMAIN-KEYWORD,youtube,PROXY\n";

    let stored = super::save_qx_rule_source_in(&store, url, "Proxy", initial)?.into_source();
    let interval_updated = super::update_qx_rule_source_refresh_interval_in(
        &store,
        &stored.id,
        super::RemoteSourceRefreshInterval::Hourly,
    )?;
    assert_eq!(
        interval_updated.refresh_interval,
        super::RemoteSourceRefreshInterval::Hourly
    );
    assert_eq!(
        interval_updated.last_successful_update_unix_secs,
        stored.last_successful_update_unix_secs
    );

    let refreshed =
        super::replace_qx_rule_source_content_in(&store, &stored.id, updated_content, 123)?;
    assert_eq!(refreshed.content, updated_content);
    assert_eq!(refreshed.rule_count, 2);
    assert_eq!(
        refreshed.refresh_interval,
        super::RemoteSourceRefreshInterval::Hourly
    );
    assert_eq!(refreshed.last_successful_update_unix_secs, 123);

    assert!(
        super::replace_qx_rule_source_content_in(&store, &stored.id, "# empty\n", 456).is_err()
    );
    let after_failed_refresh = super::load_qx_rule_sources_in(&store)?;
    assert_eq!(after_failed_refresh, vec![refreshed]);

    fs::remove_dir_all(root)?;
    Ok(())
}

#[cfg(not(windows))]
#[test]
fn qx_rule_source_definition_can_be_edited_without_changing_its_identity()
-> Result<(), Box<dyn std::error::Error>> {
    let root = test_temp_dir("manis-qx-rule-definition-edit");
    let store = root.join("subscriptions");
    let stored = super::save_qx_rule_source_in(
        &store,
        "https://rules.example.invalid/old.list",
        "Proxy",
        "DOMAIN-SUFFIX,old.example,PROXY\n",
    )?
    .into_source();
    let disabled = super::update_qx_rule_source_enabled_in(&store, &stored.id, false)?;
    assert!(!disabled.enabled);

    let edited = super::replace_qx_rule_source_definition_in(
        &store,
        &stored.id,
        "",
        "https://rules.example.invalid/new.list",
        "DIRECT",
        "DOMAIN-SUFFIX,new.example,DIRECT\n",
        super::RemoteSourceRefreshInterval::SixHours,
        456,
    )?;

    assert_eq!(edited.id, stored.id);
    assert!(!edited.enabled);
    assert_eq!(
        edited.source.expose_to(str::to_owned),
        "https://rules.example.invalid/new.list"
    );
    assert_eq!(edited.target_policy.as_str(), "DIRECT");
    assert_eq!(edited.rule_count, 1);
    assert_eq!(
        edited.refresh_interval,
        super::RemoteSourceRefreshInterval::SixHours
    );
    assert_eq!(edited.last_successful_update_unix_secs, 456);
    assert_eq!(super::load_qx_rule_sources_in(&store)?, vec![edited]);

    fs::remove_dir_all(root)?;
    Ok(())
}

#[cfg(not(windows))]
#[test]
fn qx_rule_source_target_update_preserves_source_and_refresh_metadata()
-> Result<(), Box<dyn std::error::Error>> {
    let root = test_temp_dir("manis-qx-rule-target");
    let store = root.join("subscriptions");
    let url = "https://rules.example.invalid/airports.list?token=fixture-secret";
    let content = "DOMAIN-KEYWORD,google,PROXY\nDOMAIN-SUFFIX,youtube.com,PROXY\n";
    let stored = super::save_qx_rule_source_in(&store, url, "Old policy", content)?.into_source();
    let with_interval = super::update_qx_rule_source_refresh_interval_in(
        &store,
        &stored.id,
        super::RemoteSourceRefreshInterval::Daily,
    )?;

    let updated = super::update_qx_rule_source_target_in(&store, &stored.id, "Streaming")?;

    assert_eq!(updated.id, stored.id);
    assert_eq!(updated.source, stored.source);
    assert_eq!(updated.target_policy.as_str(), "Streaming");
    assert_eq!(updated.content, content);
    assert_eq!(updated.rule_count, stored.rule_count);
    assert_eq!(updated.diagnostic_count, stored.diagnostic_count);
    assert_eq!(updated.refresh_interval, with_interval.refresh_interval);
    assert_eq!(
        updated.last_successful_update_unix_secs,
        stored.last_successful_update_unix_secs
    );
    assert_eq!(super::load_qx_rule_sources_in(&store)?, vec![updated]);

    fs::remove_dir_all(root)?;
    Ok(())
}

#[cfg(not(windows))]
#[test]
fn qx_rule_sources_read_legacy_v1_without_refresh_metadata()
-> Result<(), Box<dyn std::error::Error>> {
    let root = test_temp_dir("manis-qx-rule-legacy-refresh");
    let store = root.join("subscriptions");
    fs::create_dir(&store)?;
    fs::set_permissions(&store, fs::Permissions::from_mode(0o700))?;
    let id = "qx-rule-feed";
    let content = "DOMAIN-KEYWORD,google,PROXY\n";
    let legacy = [
        super::LEGACY_MANIS_QX_RULE_SOURCE_VERSION.to_owned(),
        format!("id\t{id}"),
        format!(
            "url\t{}",
            super::encode_hex("https://rules.example.invalid/list?token=fixture-secret")
        ),
        format!("target\t{}", super::encode_hex("Proxy")),
        format!("content\t{}", super::encode_hex(content)),
    ]
    .join("\n");
    let path = store.join(format!("{id}{}", super::QX_RULE_SOURCE_SUFFIX));
    fs::write(&path, legacy)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;

    let loaded = super::load_qx_rule_sources_in(&store)?
        .into_iter()
        .next()
        .ok_or("legacy qx source")?;
    assert_eq!(loaded.id, id);
    assert_eq!(loaded.name, None);
    assert_eq!(loaded.content, content);
    assert!(loaded.enabled);
    assert_eq!(
        loaded.refresh_interval,
        super::RemoteSourceRefreshInterval::Manual
    );
    assert_eq!(loaded.last_successful_update_unix_secs, 0);

    fs::remove_dir_all(root)?;
    Ok(())
}

#[cfg(not(windows))]
#[test]
fn qx_rule_sources_read_legacy_v2_without_custom_name() -> Result<(), Box<dyn std::error::Error>> {
    let root = test_temp_dir("manis-qx-rule-legacy-v2-name");
    let store = root.join("subscriptions");
    fs::create_dir(&store)?;
    fs::set_permissions(&store, fs::Permissions::from_mode(0o700))?;
    let id = "qx-rule-feed";
    let content = "DOMAIN-KEYWORD,google,PROXY\n";
    let legacy = [
        super::LEGACY_MANIS_QX_RULE_SOURCE_VERSION_V2.to_owned(),
        format!("id\t{id}"),
        format!(
            "url\t{}",
            super::encode_hex("https://rules.example.invalid/list?token=fixture-secret")
        ),
        format!("target\t{}", super::encode_hex("Proxy")),
        format!("content\t{}", super::encode_hex(content)),
        "enabled\t1".to_owned(),
        "refresh\t1h".to_owned(),
        "last-success\t123".to_owned(),
    ]
    .join("\n");
    let path = store.join(format!("{id}{}", super::QX_RULE_SOURCE_SUFFIX));
    fs::write(&path, legacy)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;

    let loaded = super::load_qx_rule_sources_in(&store)?
        .into_iter()
        .next()
        .ok_or("legacy v2 qx source")?;
    assert_eq!(loaded.id, id);
    assert_eq!(loaded.name, None);
    assert_eq!(loaded.content, content);
    assert!(loaded.enabled);
    assert_eq!(
        loaded.refresh_interval,
        super::RemoteSourceRefreshInterval::Hourly
    );
    assert_eq!(loaded.last_successful_update_unix_secs, 123);

    fs::remove_dir_all(root)?;
    Ok(())
}

#[cfg(not(windows))]
#[test]
fn qx_rule_sources_reject_invalid_inputs() -> Result<(), Box<dyn std::error::Error>> {
    let root = test_temp_dir("manis-qx-rule-invalid");
    let store = root.join("subscriptions");
    let valid_content = "DOMAIN-KEYWORD,google,PROXY\n";

    assert!(
        super::save_qx_rule_source_in(
            &store,
            "http://rules.example.invalid/list?token=fixture-secret",
            "Proxy",
            valid_content,
        )
        .is_err()
    );
    assert!(
        super::save_qx_rule_source_in(
            &store,
            "https://rules.example.invalid/list?token=fixture-secret",
            "bad,name",
            valid_content,
        )
        .is_err()
    );
    assert!(
        super::save_qx_rule_source_in(
            &store,
            "https://rules.example.invalid/list?token=fixture-secret",
            "Proxy",
            "# comments only\n",
        )
        .is_err()
    );
    assert!(
        super::save_qx_rule_source_in(
            &store,
            "https://rules.example.invalid/list?token=fixture-secret",
            "Proxy",
            &"x".repeat(super::MAX_QX_RULE_SOURCE_CONTENT_BYTES + 1),
        )
        .is_err()
    );

    fs::remove_dir_all(root)?;
    Ok(())
}

#[cfg(not(windows))]
#[test]
fn qx_rule_source_errors_redact_secret_inputs() {
    let root = test_temp_dir("manis-qx-rule-redaction");
    let store = root.join("subscriptions");
    let error = super::save_qx_rule_source_in(
        &store,
        "http://rules.example.invalid/list?token=private-fixture",
        "Proxy",
        "DOMAIN-KEYWORD,google,PROXY\n",
    )
    .expect_err("plain HTTP QX rule source must fail");

    assert!(!error.to_string().contains("private-fixture"));
    assert!(!format!("{error:?}").contains("private-fixture"));
    fs::remove_dir_all(root).expect("remove temp root");
}

#[test]
fn qx_rule_sources_compile_in_source_order_without_generated_fallbacks()
-> Result<(), Box<dyn std::error::Error>> {
    let source = super::StoredQxRuleSource {
        id: "qx-rule-fixture-1".to_owned(),
        name: None,
        source: manis_profile::SecretUrl::parse_https(
            "https://rules.example.invalid/airports.list?token=fixture-secret",
        )?,
        enabled: true,
        target_policy: manis_profile::Name::parse("Proxy")?,
        content: "DOMAIN-KEYWORD,google,PROXY\nDOMAIN-SUFFIX,githubusercontent.com,proxy\n"
            .to_owned(),
        rule_count: 2,
        diagnostic_count: 0,
        refresh_interval: super::RemoteSourceRefreshInterval::Manual,
        last_successful_update_unix_secs: 0,
    };
    let mut profile = manis_profile::Profile::qx_default(manis_profile::SecretUrl::parse_https(
        "https://subscription.example.invalid/client?token=fixture-secret",
    )?)?;
    let mut disabled_source = source.clone();
    disabled_source.enabled = false;

    super::apply_qx_rule_sources(&mut profile, &[source])?;
    let yaml = manis_profile::render_mihomo_yaml(&profile)?;

    assert!(
        yaml.find("- \"DOMAIN-KEYWORD,google,__MANIS_GLOBAL__\"")
            < yaml.find("- \"DOMAIN-SUFFIX,githubusercontent.com,__MANIS_GLOBAL__\"")
    );
    assert!(!yaml.contains("GEOIP,CN,DIRECT"));
    assert!(!yaml.contains("MATCH,__MANIS_GLOBAL__"));

    let mut disabled_profile = manis_profile::Profile::qx_default(
        manis_profile::SecretUrl::parse_https("https://subscription.example.invalid/client")?,
    )?;
    super::apply_qx_rule_sources(&mut disabled_profile, &[disabled_source])?;
    let disabled_yaml = manis_profile::render_mihomo_yaml(&disabled_profile)?;
    assert!(!disabled_yaml.contains("DOMAIN-KEYWORD,google"));
    Ok(())
}

#[test]
fn legacy_proxy_rule_targets_resolve_to_the_first_user_policy_group()
-> Result<(), Box<dyn std::error::Error>> {
    let source = super::StoredQxRuleSource {
        id: "qx-rule-fixture-legacy-target".to_owned(),
        name: None,
        source: manis_profile::SecretUrl::parse_https("https://rules.example.invalid/legacy.list")?,
        enabled: true,
        target_policy: manis_profile::Name::parse("Proxy")?,
        content: "DOMAIN-SUFFIX,google.com,PROXY\n".to_owned(),
        rule_count: 1,
        diagnostic_count: 0,
        refresh_interval: super::RemoteSourceRefreshInterval::Manual,
        last_successful_update_unix_secs: 0,
    };
    let group = manis_profile::UserPolicyGroup {
        name: manis_profile::Name::parse("香港")?,
        icon: None,
        kind: manis_profile::UserPolicyGroupKind::UrlTest {
            tolerance: 50,
            interval_secs: 300,
        },
        provider_indexes: vec![0],
        direct_proxies: Vec::new(),
        direct_policies: Vec::new(),
        filter: None,
    };
    let mut profile = manis_profile::Profile::qx_sources_with_groups(
        vec![manis_profile::SecretUrl::parse_https(
            "https://subscription.example.invalid/client",
        )?],
        Vec::new(),
        vec![group],
        17_890,
    )?;

    super::apply_qx_rule_sources(&mut profile, &[source])?;
    let yaml = manis_profile::render_mihomo_yaml(&profile)?;

    assert!(yaml.contains("- \"DOMAIN-SUFFIX,google.com,香港\""));
    assert!(!yaml.contains("name: \"Proxy\""));
    Ok(())
}
