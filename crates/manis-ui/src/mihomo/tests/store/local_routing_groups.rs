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

use crate::manual_rule::{ManualRule, ManualRuleKind, save_manual_rules_in};
use manis_engine::ControllerEndpoint;

fn rule(kind: ManualRuleKind, parameter: &str, target: &str) -> ManualRule {
    ManualRule::parse(kind, parameter, target).expect("valid rule")
}

fn saved_group(store: &Path, name: &str, target: &str, rules: Vec<ManualRule>) -> String {
    let mut group = super::LocalRoutingRuleGroup::new(name, target).expect("valid group");
    group.rules = rules;
    save_local_routing_rule_groups_in(store, std::slice::from_ref(&group)).expect("save group");
    group.id
}

#[test]
fn saved_local_group_rules_compile_in_the_dragged_group_order()
-> Result<(), Box<dyn std::error::Error>> {
    let root = test_temp_dir("manis-local-group-compile");
    let store = root.join("subscriptions");
    save_manual_rules_in(
        &store,
        &[rule(ManualRuleKind::Host, "manual.example", "DIRECT")],
    )?;
    let group_id = saved_group(
        &store,
        "Streaming",
        "DIRECT",
        vec![rule(ManualRuleKind::HostSuffix, "stream.example", "DIRECT")],
    );

    super::save_routing_rule_group_order_in(
        &store,
        &[
            group_id.clone(),
            super::MANUAL_ROUTING_RULE_GROUP_ID.to_owned(),
        ],
    )?;
    let yaml = manis_profile::render_mihomo_yaml(&super::compile_saved_profile(&store, None)?)?;
    assert!(
        yaml.find("stream.example").expect("local rule")
            < yaml.find("manual.example").expect("manual rule"),
        "the stored order puts the local group first: {yaml}"
    );

    super::save_routing_rule_group_order_in(
        &store,
        &[super::MANUAL_ROUTING_RULE_GROUP_ID.to_owned(), group_id],
    )?;
    let yaml = manis_profile::render_mihomo_yaml(&super::compile_saved_profile(&store, None)?)?;
    assert!(
        yaml.find("manual.example").expect("manual rule")
            < yaml.find("stream.example").expect("local rule"),
        "the manual group moves back to the front: {yaml}"
    );

    fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn a_disabled_local_group_contributes_nothing() -> Result<(), Box<dyn std::error::Error>> {
    let root = test_temp_dir("manis-local-group-disabled");
    let store = root.join("subscriptions");
    let mut group = super::LocalRoutingRuleGroup::new("Off", "DIRECT")?;
    group.enabled = false;
    group.rules = vec![rule(ManualRuleKind::HostSuffix, "off.example", "DIRECT")];
    save_local_routing_rule_groups_in(&store, std::slice::from_ref(&group))?;

    let yaml = manis_profile::render_mihomo_yaml(&super::compile_saved_profile(&store, None)?)?;

    assert!(!yaml.contains("off.example"), "{yaml}");
    fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn a_stored_final_rule_in_a_local_group_warns_and_is_skipped()
-> Result<(), Box<dyn std::error::Error>> {
    let root = test_temp_dir("manis-local-group-final");
    let store = root.join("subscriptions");
    saved_group(
        &store,
        "Mixed",
        "DIRECT",
        vec![
            ManualRule::final_rule("REJECT")?,
            rule(ManualRuleKind::HostSuffix, "kept.example", "DIRECT"),
        ],
    );

    let yaml = manis_profile::render_mihomo_yaml(&super::compile_saved_profile(&store, None)?)?;

    assert!(yaml.contains("kept.example"), "{yaml}");
    assert!(!yaml.contains("MATCH,REJECT"), "{yaml}");
    assert!(
        crate::diagnostics::recent_ui_logs()
            .iter()
            .any(|entry| entry.event == "routing.local_group.final_skipped"),
        "the skipped FINAL is reported as a warning"
    );
    fs::remove_dir_all(root)?;
    Ok(())
}
