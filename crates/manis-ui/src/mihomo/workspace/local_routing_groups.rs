use std::collections::BTreeSet;

use super::{
    LOCAL_ROUTING_RULE_GROUP_PREFIX, LOCAL_ROUTING_RULE_GROUPS_FILE,
    LOCAL_ROUTING_RULE_GROUPS_VERSION, MAX_LOCAL_ROUTING_RULE_GROUP_RULES,
    MAX_LOCAL_ROUTING_RULE_GROUPS_FILE_BYTES, MAX_ROUTING_RULE_GROUPS, Name, Path,
    SubscriptionStoreError, next_stored_source_id, valid_stored_id,
    validate_subscription_source_name,
};
#[cfg(not(windows))]
use super::{require_clean_absolute_store, write_config_entry_bytes};
use crate::manual_rule::{ManualRule, ManualRuleKind};

/// A local routing rule group: a name, one target policy and its own rules, stored without a URL.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LocalRoutingRuleGroup {
    pub id: String,
    pub name: String,
    pub target_policy: Name,
    pub enabled: bool,
    pub rules: Vec<ManualRule>,
}

impl LocalRoutingRuleGroup {
    /// Creates an enabled, empty group with a freshly generated id.
    pub(crate) fn new(name: &str, target_policy: &str) -> Result<Self, SubscriptionStoreError> {
        let name = validate_subscription_source_name(name)?;
        let target_policy =
            Name::parse(target_policy).map_err(|_error| SubscriptionStoreError::InvalidSource)?;
        Ok(Self {
            id: next_stored_source_id(LOCAL_ROUTING_RULE_GROUP_PREFIX),
            name,
            target_policy,
            enabled: true,
            rules: Vec::new(),
        })
    }

    /// Every rule in the group routes to the group target, so a change re-points them all.
    pub(crate) fn rename(&mut self, name: &str) -> Result<(), SubscriptionStoreError> {
        self.name = validate_subscription_source_name(name)?;
        Ok(())
    }

    pub(crate) fn retarget(&mut self, target_policy: &str) -> Result<(), SubscriptionStoreError> {
        let target_policy =
            Name::parse(target_policy).map_err(|_error| SubscriptionStoreError::InvalidSource)?;
        for rule in &mut self.rules {
            rule.retarget(target_policy.as_str())
                .map_err(|_error| SubscriptionStoreError::InvalidSource)?;
        }
        self.target_policy = target_policy;
        Ok(())
    }
}

fn encode_local_routing_rule_groups(
    groups: &[LocalRoutingRuleGroup],
) -> Result<String, SubscriptionStoreError> {
    if groups.len() > MAX_ROUTING_RULE_GROUPS {
        return Err(SubscriptionStoreError::InvalidSource);
    }
    let mut seen = BTreeSet::new();
    let mut contents = String::from(LOCAL_ROUTING_RULE_GROUPS_VERSION);
    for group in groups {
        if !valid_stored_id(&group.id, LOCAL_ROUTING_RULE_GROUP_PREFIX)
            || !seen.insert(group.id.as_str())
            || group.rules.len() > MAX_LOCAL_ROUTING_RULE_GROUP_RULES
        {
            return Err(SubscriptionStoreError::InvalidSource);
        }
        let name = validate_subscription_source_name(&group.name)?;
        contents.push_str("\ngroup\t");
        contents.push_str(&group.id);
        contents.push('\t');
        contents.push_str(if group.enabled { "1" } else { "0" });
        contents.push('\t');
        contents.push_str(group.target_policy.as_str());
        contents.push('\t');
        contents.push_str(&name);
        for rule in &group.rules {
            contents.push_str("\nrule\t");
            contents.push_str(if rule.is_enabled() { "1" } else { "0" });
            if rule.is_final() {
                contents.push_str("\tfinal\t");
                continue;
            }
            for condition in rule.conditions() {
                contents.push('\t');
                contents.push_str(condition.kind().storage_key());
                contents.push('\t');
                contents.push_str(condition.parameter());
            }
        }
    }
    if contents.len() as u64 > MAX_LOCAL_ROUTING_RULE_GROUPS_FILE_BYTES {
        return Err(SubscriptionStoreError::InvalidSource);
    }
    Ok(contents)
}

fn decode_local_routing_rule_groups(
    contents: &str,
) -> Result<Vec<LocalRoutingRuleGroup>, SubscriptionStoreError> {
    let mut lines = contents.lines();
    if lines.next() != Some(LOCAL_ROUTING_RULE_GROUPS_VERSION) {
        return Err(SubscriptionStoreError::StoredSourceUnavailable);
    }
    let mut groups = Vec::new();
    let mut seen = BTreeSet::new();
    for line in lines.filter(|line| !line.is_empty()) {
        let fields = line.split('\t').collect::<Vec<_>>();
        match fields.as_slice() {
            ["group", id, enabled @ ("0" | "1"), target_policy, name] => {
                if groups.len() == MAX_ROUTING_RULE_GROUPS
                    || !valid_stored_id(id, LOCAL_ROUTING_RULE_GROUP_PREFIX)
                    || !seen.insert(*id)
                {
                    return Err(SubscriptionStoreError::StoredSourceUnavailable);
                }
                groups.push(LocalRoutingRuleGroup {
                    id: (*id).to_owned(),
                    name: validate_subscription_source_name(name)
                        .map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)?,
                    target_policy: Name::parse(target_policy)
                        .map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)?,
                    enabled: *enabled == "1",
                    rules: Vec::new(),
                });
            }
            ["rule", enabled @ ("0" | "1"), conditions @ ..] => {
                let Some(group) = groups.last_mut() else {
                    return Err(SubscriptionStoreError::StoredSourceUnavailable);
                };
                if group.rules.len() == MAX_LOCAL_ROUTING_RULE_GROUP_RULES
                    || conditions.is_empty()
                    || !conditions.len().is_multiple_of(2)
                {
                    return Err(SubscriptionStoreError::StoredSourceUnavailable);
                }
                let conditions = conditions
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|pair| {
                        ManualRuleKind::from_storage_key(pair[0])
                            .map(|kind| (kind, (*pair[1]).to_owned()))
                            .ok_or(SubscriptionStoreError::StoredSourceUnavailable)
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let mut rule =
                    ManualRule::parse_conditions(conditions, group.target_policy.as_str())
                        .map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)?;
                rule.set_enabled(*enabled == "1");
                group.rules.push(rule);
            }
            _ => return Err(SubscriptionStoreError::StoredSourceUnavailable),
        }
    }
    Ok(groups)
}

#[cfg(not(windows))]
pub(crate) fn load_local_routing_rule_groups_in(
    directory: &Path,
) -> Result<Vec<LocalRoutingRuleGroup>, SubscriptionStoreError> {
    require_clean_absolute_store(directory)?;
    let Some(contents) = crate::config_toml::read_entry(
        directory,
        LOCAL_ROUTING_RULE_GROUPS_FILE,
        MAX_LOCAL_ROUTING_RULE_GROUPS_FILE_BYTES,
    )
    .map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)?
    else {
        return Ok(Vec::new());
    };
    decode_local_routing_rule_groups(&contents)
}

#[cfg(windows)]
pub(crate) fn load_local_routing_rule_groups_in(
    _directory: &Path,
) -> Result<Vec<LocalRoutingRuleGroup>, SubscriptionStoreError> {
    Ok(Vec::new())
}

#[cfg(not(windows))]
pub(crate) fn save_local_routing_rule_groups_in(
    directory: &Path,
    groups: &[LocalRoutingRuleGroup],
) -> Result<(), SubscriptionStoreError> {
    let contents = encode_local_routing_rule_groups(groups)?;
    write_config_entry_bytes(
        directory,
        LOCAL_ROUTING_RULE_GROUPS_FILE,
        contents.as_bytes(),
    )
    .map(|_path| ())
    .map_err(|_error| SubscriptionStoreError::StoreUnavailable)
}

#[cfg(windows)]
pub(crate) fn save_local_routing_rule_groups_in(
    _directory: &Path,
    _groups: &[LocalRoutingRuleGroup],
) -> Result<(), SubscriptionStoreError> {
    Err(SubscriptionStoreError::StoreUnavailable)
}

#[cfg(all(test, not(windows)))]
mod tests {
    use super::{
        LOCAL_ROUTING_RULE_GROUPS_FILE, LOCAL_ROUTING_RULE_GROUPS_VERSION,
        MAX_LOCAL_ROUTING_RULE_GROUPS_FILE_BYTES, MAX_ROUTING_RULE_GROUPS,
        decode_local_routing_rule_groups, encode_local_routing_rule_groups,
        load_local_routing_rule_groups_in, save_local_routing_rule_groups_in,
    };
    use crate::manual_rule::{ManualRule, ManualRuleKind};
    use crate::mihomo::LocalRoutingRuleGroup;
    use std::fmt::Write as _;
    use std::fs;
    use std::path::PathBuf;

    fn store(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "manis-local-rule-groups-{name}-{}-{:x}",
            std::process::id(),
            crate::mihomo::current_unix_secs()
        ));
        fs::create_dir_all(&root).expect("create fixture root");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            fs::set_permissions(&root, fs::Permissions::from_mode(0o700))
                .expect("make fixture private");
        }
        root
    }

    fn group(name: &str, target: &str) -> LocalRoutingRuleGroup {
        LocalRoutingRuleGroup::new(name, target).expect("valid group")
    }

    fn rule(kind: ManualRuleKind, parameter: &str, target: &str) -> ManualRule {
        ManualRule::parse(kind, parameter, target).expect("valid rule")
    }

    #[test]
    fn groups_round_trip_with_rules_targets_and_enabled_state() {
        let store = store("round-trip");
        let mut disabled = group("Disabled group", "REJECT");
        disabled.enabled = false;
        let mut first = group("Streaming", "DIRECT");
        first.rules = vec![
            rule(ManualRuleKind::HostSuffix, "example.com", "DIRECT"),
            rule(ManualRuleKind::DstPort, "443", "DIRECT"),
        ];
        let mut second = group("Blocked", "REJECT");
        second.rules = vec![rule(ManualRuleKind::HostKeyword, "ads", "REJECT")];

        save_local_routing_rule_groups_in(&store, &[first, second, disabled]).expect("save groups");
        let restored = load_local_routing_rule_groups_in(&store).expect("load groups");

        assert_eq!(restored.len(), 3);
        assert_eq!(restored[0].name, "Streaming");
        assert_eq!(restored[0].target_policy.as_str(), "DIRECT");
        assert!(restored[0].enabled);
        assert_eq!(
            restored[0].rules[0].conditions()[0].parameter(),
            "example.com"
        );
        assert_eq!(
            restored[0].rules[1].conditions()[0].kind(),
            ManualRuleKind::DstPort
        );
        assert_eq!(restored[1].rules[0].conditions()[0].parameter(), "ads");
        assert!(!restored[2].enabled);
        assert!(restored[2].rules.is_empty());
        fs::remove_dir_all(store).expect("remove fixture");
    }

    #[test]
    fn an_empty_store_loads_no_groups() {
        let store = store("empty");
        assert!(
            load_local_routing_rule_groups_in(&store)
                .expect("load empty store")
                .is_empty()
        );
        fs::remove_dir_all(store).expect("remove fixture");
    }

    #[test]
    fn a_saved_disabled_rule_stays_disabled_and_a_final_rule_stays_readable() {
        let mut stored = group("Mixed", "DIRECT");
        let mut disabled = rule(ManualRuleKind::HostSuffix, "off.example", "DIRECT");
        disabled.set_enabled(false);
        stored.rules = vec![
            disabled,
            ManualRule::final_rule("DIRECT").expect("valid final rule"),
        ];

        let encoded =
            encode_local_routing_rule_groups(std::slice::from_ref(&stored)).expect("encode groups");
        let decoded = decode_local_routing_rule_groups(&encoded).expect("decode groups");

        assert!(!decoded[0].rules[0].is_enabled());
        assert!(decoded[0].rules[1].is_final());
        assert_eq!(decoded, std::slice::from_ref(&stored).to_vec());
    }

    #[test]
    fn a_rule_before_its_group_and_unknown_kinds_are_rejected() {
        for contents in [
            format!("{LOCAL_ROUTING_RULE_GROUPS_VERSION}\nrule\t1\thost\texample.com"),
            format!("{LOCAL_ROUTING_RULE_GROUPS_VERSION}\nrule\t1\thost"),
            format!(
                "{LOCAL_ROUTING_RULE_GROUPS_VERSION}\ngroup\tlocal-rule-1\t1\tDIRECT\tGroup\nrule\t1\tunknown\texample.com"
            ),
            format!("{LOCAL_ROUTING_RULE_GROUPS_VERSION}\ngroup\tlocal-rule-1\t2\tDIRECT\tGroup"),
            format!(
                "{LOCAL_ROUTING_RULE_GROUPS_VERSION}\ngroup\tlocal-rule-1\t1\tDIRECT\tGroup\textra"
            ),
            format!("{LOCAL_ROUTING_RULE_GROUPS_VERSION}\ngroup\tlocal-rule-1\t1\tDIRECT\t"),
            "manis.local-routing-rule-groups.v2\ngroup\tlocal-rule-1\t1\tDIRECT\tGroup".to_owned(),
        ] {
            assert!(
                decode_local_routing_rule_groups(&contents).is_err(),
                "{contents}"
            );
        }
        // A rule that belongs to a group above it is accepted.
        assert!(
            decode_local_routing_rule_groups(&format!(
                "{LOCAL_ROUTING_RULE_GROUPS_VERSION}\ngroup\tlocal-rule-1\t1\tDIRECT\tGroup\nrule\t1\thost\texample.com"
            ))
            .is_ok()
        );
    }

    #[test]
    fn malformed_and_duplicate_ids_are_rejected() {
        for id in [
            "local-rule-",
            "group-1",
            "local-rule-0x1",
            "local-rule-1\t1",
        ] {
            assert!(
                decode_local_routing_rule_groups(&format!(
                    "{LOCAL_ROUTING_RULE_GROUPS_VERSION}\ngroup\t{id}\t1\tDIRECT\tGroup"
                ))
                .is_err(),
                "{id}"
            );
        }
        assert!(
            decode_local_routing_rule_groups(&format!(
                "{LOCAL_ROUTING_RULE_GROUPS_VERSION}\ngroup\tlocal-rule-1\t1\tDIRECT\tA\ngroup\tlocal-rule-1\t1\tDIRECT\tB"
            ))
            .is_err()
        );
    }

    #[test]
    fn too_many_groups_or_rules_in_one_group_are_rejected() {
        let mut groups = String::from(LOCAL_ROUTING_RULE_GROUPS_VERSION);
        for index in 0..=MAX_ROUTING_RULE_GROUPS {
            let _ = write!(groups, "\ngroup\tlocal-rule-{index:x}\t1\tDIRECT\tGroup");
        }
        assert!(decode_local_routing_rule_groups(&groups).is_err());

        let mut rules =
            format!("{LOCAL_ROUTING_RULE_GROUPS_VERSION}\ngroup\tlocal-rule-1\t1\tDIRECT\tGroup");
        for _ in 0..=super::MAX_LOCAL_ROUTING_RULE_GROUP_RULES {
            rules.push_str("\nrule\t1\thost\texample.com");
        }
        assert!(decode_local_routing_rule_groups(&rules).is_err());
    }

    #[test]
    fn a_store_over_the_size_cap_is_rejected() {
        let store = store("oversized");
        let mut contents = String::from(LOCAL_ROUTING_RULE_GROUPS_VERSION);
        for index in 0..20 {
            let _ = write!(contents, "\ngroup\tlocal-rule-{index:x}\t1\tDIRECT\tGroup");
            for _ in 0..super::MAX_LOCAL_ROUTING_RULE_GROUP_RULES {
                contents.push_str("\nrule\t1\thost-keyword\tpadding");
            }
        }
        assert!(contents.len() as u64 > MAX_LOCAL_ROUTING_RULE_GROUPS_FILE_BYTES);
        // The document is structurally valid, so only the size cap can reject it.
        assert!(decode_local_routing_rule_groups(&contents).is_ok());
        crate::config_toml::write_entry(&store, LOCAL_ROUTING_RULE_GROUPS_FILE, &contents)
            .expect("write oversized fixture");
        assert!(load_local_routing_rule_groups_in(&store).is_err());
        fs::remove_dir_all(store).expect("remove fixture");
    }

    #[test]
    fn a_group_without_a_name_or_target_policy_is_refused() {
        assert!(LocalRoutingRuleGroup::new("", "DIRECT").is_err());
        assert!(LocalRoutingRuleGroup::new("Group", "").is_err());
        // Policy names are Mihomo scalars, so a comma cannot appear in one.
        assert!(LocalRoutingRuleGroup::new("Group", "Policy, direct").is_err());
        let mut stored = group("Group", "DIRECT");
        assert!(stored.rename("").is_err());
        assert!(stored.retarget("").is_err());
    }

    #[test]
    fn retargeting_a_group_repoints_every_rule_it_holds() {
        let mut stored = group("Group", "DIRECT");
        stored.rules = vec![rule(ManualRuleKind::HostSuffix, "example.com", "DIRECT")];

        stored.retarget("REJECT").expect("retarget");

        assert_eq!(stored.target_policy.as_str(), "REJECT");
        assert_eq!(stored.rules[0].target(), "REJECT");
    }

    #[test]
    fn a_failed_save_keeps_the_previous_document() {
        let store = store("failed-save");
        let good = group("Good", "DIRECT");
        save_local_routing_rule_groups_in(&store, std::slice::from_ref(&good))
            .expect("save groups");
        let mut duplicated = good.clone();
        duplicated.name = "Duplicate id".to_owned();
        assert!(
            save_local_routing_rule_groups_in(&store, &[good.clone(), duplicated]).is_err(),
            "a duplicate id must not be written"
        );
        assert_eq!(
            load_local_routing_rule_groups_in(&store).expect("reload groups"),
            std::slice::from_ref(&good)
        );
        fs::remove_dir_all(store).expect("remove fixture");
    }

    #[test]
    fn a_corrupt_document_is_reported_instead_of_silently_dropped() {
        let store = store("corrupt");
        crate::config_toml::write_entry(
            &store,
            LOCAL_ROUTING_RULE_GROUPS_FILE,
            "not the expected version",
        )
        .expect("write corrupt fixture");
        assert!(load_local_routing_rule_groups_in(&store).is_err());
        fs::remove_dir_all(store).expect("remove fixture");
    }
}
