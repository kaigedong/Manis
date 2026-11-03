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
fn source_store_round_trips_editable_managed_policy_groups()
-> Result<(), Box<dyn std::error::Error>> {
    use manis_core::{
        ManagedPolicyGroup, ManagedPolicyIcon, ManagedPolicyStrategy, NodeIdentity,
        PolicyCandidateMatcher,
    };

    let root = test_temp_dir("manis-managed-policies");
    let store = root.join("subscriptions");
    let mut group = ManagedPolicyGroup::new("policy-a-1", "香港优选")?;
    group.icon = ManagedPolicyIcon::Globe;
    group.strategy = ManagedPolicyStrategy::LowestLatency;
    group.set_test_interval_secs(1_800)?;
    group.switch_tolerance_ms = 150;
    group.set_matcher(PolicyCandidateMatcher::name_contains("Hong Kong")?)?;
    super::save_managed_policy_in(&store, &group).expect("save first policy");

    let mut explicit = ManagedPolicyGroup::new("policy-b-2", "手动出口")?;
    explicit.icon = ManagedPolicyIcon::Shield;
    explicit.set_matcher(PolicyCandidateMatcher::Explicit(BTreeSet::default()))?;
    explicit.toggle_member(NodeIdentity::new("subscription:source-1", "Tokyo Edge")?);
    explicit.toggle_member(NodeIdentity::new("saved", "Private Edge")?);
    explicit.toggle_member(NodeIdentity::new("builtin", "PROXY")?);
    super::save_managed_policy_in(&store, &explicit).expect("save second policy");

    let groups = super::load_managed_policy_groups_in(&store).expect("load policies");
    assert_eq!(groups, vec![group.clone(), explicit.clone()]);

    group.rename("香港 · 自动")?;
    group.icon = ManagedPolicyIcon::Compass;
    super::save_managed_policy_in(&store, &group).expect("update policy");
    let updated = super::load_managed_policy_groups_in(&store).expect("load updated policies");
    assert_eq!(updated.len(), 2);
    assert_eq!(updated[0], group);

    super::remove_managed_policy_in(&store, &explicit.id).expect("remove policy");
    assert_eq!(super::load_managed_policy_groups_in(&store)?.len(), 1);
    fs::remove_dir_all(root)?;
    Ok(())
}

#[cfg(not(windows))]
#[test]
fn legacy_node_group_file_migrates_to_managed_policy_file() -> Result<(), Box<dyn std::error::Error>>
{
    use std::os::unix::fs::PermissionsExt;

    let root = test_temp_dir("manis-managed-policy-migration");
    let store = root.join("subscriptions");
    fs::create_dir_all(&store)?;
    fs::set_permissions(&store, fs::Permissions::from_mode(0o700))?;
    let legacy = store.join("group-deadbeef-1.group");
    fs::write(
        &legacy,
        concat!(
            "manis-node-group-v1\n",
            "id\tgroup-deadbeef-1\n",
            "name\t4c656761637920506f6c696379\n",
            "icon\tbolt\n",
            "strategy\tmanual\n",
            "interval\t600\n",
            "matcher\tall\n",
            "filter\t"
        ),
    )?;
    fs::set_permissions(&legacy, fs::Permissions::from_mode(0o600))?;

    let policies = super::load_managed_policy_groups_in(&store)?;
    assert_eq!(policies.len(), 1);
    assert_eq!(policies[0].name, "Legacy Policy");
    assert!(!legacy.exists());
    let migrated = crate::config_toml::read_entry(&store, "group-deadbeef-1.policy", 1024 * 1024)?
        .expect("migrated policy");
    assert!(migrated.starts_with("manis-policy-group-v1\n"));

    fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn managed_policy_groups_compile_matchers_into_mihomo_candidates()
-> Result<(), Box<dyn std::error::Error>> {
    use std::collections::HashMap;

    use manis_core::{
        ManagedPolicyGroup, ManagedPolicyStrategy, NodeIdentity, PolicyCandidateMatcher,
    };
    use manis_profile::{Name, PolicyRef, UserPolicyGroupKind, VlessProxy};

    let saved = VlessProxy::parse_share_link(
        "vless://00000000-0000-4000-8000-000000000000@edge.example.invalid:443?security=tls#Private%20Edge",
    )?;
    let indexes = HashMap::from([("source-a".to_owned(), 1_usize)]);

    let mut latency = ManagedPolicyGroup::new("group-a-1", "香港优选")?;
    latency.strategy = ManagedPolicyStrategy::LowestLatency;
    latency.set_test_interval_secs(300)?;
    latency.switch_tolerance_ms = 200;
    latency.set_matcher(PolicyCandidateMatcher::name_contains("Hong Kong")?)?;

    let mut explicit = ManagedPolicyGroup::new("group-b-2", "手动出口")?;
    explicit.set_matcher(PolicyCandidateMatcher::Explicit(BTreeSet::default()))?;
    explicit.toggle_member(NodeIdentity::new("subscription:source-a", "Tokyo (Fast)")?);
    explicit.toggle_member(NodeIdentity::new("saved", "Private Edge")?);
    explicit.toggle_member(NodeIdentity::new("policy:group-a-1", "香港优选")?);
    explicit.toggle_member(NodeIdentity::new("builtin", "DIRECT")?);
    explicit.toggle_member(NodeIdentity::new("builtin", "REJECT")?);
    explicit.toggle_member(NodeIdentity::new("builtin", "PROXY")?);

    let compiled =
        super::compile_managed_policy_groups(&[latency, explicit], &indexes, &[], &[saved], 2)?;

    assert_eq!(
        compiled[0].kind,
        UserPolicyGroupKind::UrlTest {
            tolerance: 200,
            interval_secs: 300,
        }
    );
    assert_eq!(compiled[0].provider_indexes, vec![0, 1]);
    assert_eq!(compiled[0].filter.as_deref(), Some("(?i)Hong Kong"));
    assert_eq!(compiled[1].provider_indexes, vec![1]);
    assert_eq!(
        compiled[1].filter.as_deref(),
        Some("^(?:Tokyo \\(Fast\\))$")
    );
    assert_eq!(compiled[1].direct_proxies.len(), 1);
    assert!(compiled[1].direct_policies.contains(&PolicyRef::Direct));
    assert!(compiled[1].direct_policies.contains(&PolicyRef::Reject));
    assert!(
        compiled[1]
            .direct_policies
            .contains(&PolicyRef::Group(Name::parse("香港优选")?))
    );
    assert!(
        compiled[1]
            .direct_policies
            .contains(&PolicyRef::Group(Name::parse(
                super::MANIS_GLOBAL_GROUP_NAME
            )?))
    );
    Ok(())
}
