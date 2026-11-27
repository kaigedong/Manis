use std::collections::BTreeSet;
use std::path::Path;

#[cfg(not(windows))]
use super::require_clean_absolute_store;
use super::{
    LOCAL_ROUTING_RULE_GROUP_PREFIX, LocalRoutingRuleGroup, MANUAL_ROUTING_RULE_GROUP_ID,
    MAX_ROUTING_RULE_GROUP_ORDER_FILE_BYTES, MAX_ROUTING_RULE_GROUPS, QX_RULE_SOURCE_PREFIX,
    ROUTING_RULE_GROUP_ORDER_FILE, ROUTING_RULE_GROUP_ORDER_VERSION, StoredQxRuleSource,
    SubscriptionStoreError, valid_stored_id,
};

#[cfg(not(windows))]
fn write_config_entry_bytes(
    directory: &Path,
    file_name: &str,
    bytes: &[u8],
) -> Result<std::path::PathBuf, crate::config_toml::ConfigTomlError> {
    let contents = std::str::from_utf8(bytes)
        .map_err(|_error| crate::config_toml::ConfigTomlError::InvalidFormat)?;
    crate::config_toml::write_entry(directory, file_name, contents)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MoveDirection {
    Up,
    Down,
}

/// One ordered list of rule groups: the manual group, remote sources and local groups.
pub(crate) fn normalized_routing_rule_group_order(
    stored_order: &[String],
    has_manual_rules: bool,
    sources: &[StoredQxRuleSource],
    local_groups: &[LocalRoutingRuleGroup],
) -> Vec<String> {
    let known = sources
        .iter()
        .map(|source| source.id.as_str())
        .chain(local_groups.iter().map(|group| group.id.as_str()))
        .collect::<BTreeSet<_>>();
    let mut seen = BTreeSet::new();
    let mut order = stored_order
        .iter()
        .filter_map(|id| {
            let id = id.as_str();
            let retained =
                (has_manual_rules && id == MANUAL_ROUTING_RULE_GROUP_ID) || known.contains(id);
            retained.then_some(id)
        })
        .filter(|id| seen.insert(*id))
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if has_manual_rules && seen.insert(MANUAL_ROUTING_RULE_GROUP_ID) {
        order.insert(0, MANUAL_ROUTING_RULE_GROUP_ID.to_owned());
    }
    for id in sources
        .iter()
        .map(|source| source.id.as_str())
        .chain(local_groups.iter().map(|group| group.id.as_str()))
    {
        if seen.insert(id) {
            order.push(id.to_owned());
        }
    }
    order
}

pub(crate) fn move_routing_rule_group(
    order: &mut [String],
    group_id: &str,
    direction: MoveDirection,
) -> bool {
    let Some(index) = order.iter().position(|id| id == group_id) else {
        return false;
    };
    let target = match direction {
        MoveDirection::Up => index.checked_sub(1),
        MoveDirection::Down => index.checked_add(1).filter(|target| *target < order.len()),
    };
    let Some(target) = target else {
        return false;
    };
    order.swap(index, target);
    true
}

/// Place a group before or after an existing anchor without disturbing other groups.
pub(crate) fn place_routing_rule_group(
    order: &mut [String],
    group_id: &str,
    anchor_id: &str,
    after: bool,
) -> bool {
    let Some(source) = order.iter().position(|id| id == group_id) else {
        return false;
    };
    let Some(anchor) = order.iter().position(|id| id == anchor_id) else {
        return false;
    };
    if source == anchor {
        return false;
    }
    let slot = anchor + usize::from(after);
    let target = slot - usize::from(source < slot);
    match source.cmp(&target) {
        std::cmp::Ordering::Less => order[source..=target].rotate_left(1),
        std::cmp::Ordering::Greater => order[target..=source].rotate_right(1),
        std::cmp::Ordering::Equal => return false,
    }
    true
}

fn valid_routing_rule_group_id(id: &str) -> bool {
    id == MANUAL_ROUTING_RULE_GROUP_ID
        || valid_stored_id(id, QX_RULE_SOURCE_PREFIX)
        || valid_stored_id(id, LOCAL_ROUTING_RULE_GROUP_PREFIX)
}

#[cfg(not(windows))]
pub(crate) fn save_routing_rule_group_order_in(
    directory: &Path,
    order: &[String],
) -> Result<(), SubscriptionStoreError> {
    if order.len() > MAX_ROUTING_RULE_GROUPS {
        return Err(SubscriptionStoreError::StoreUnavailable);
    }
    let mut seen = BTreeSet::new();
    if order
        .iter()
        .any(|id| !valid_routing_rule_group_id(id) || !seen.insert(id.as_str()))
    {
        return Err(SubscriptionStoreError::StoreUnavailable);
    }
    let mut contents = ROUTING_RULE_GROUP_ORDER_VERSION.to_owned();
    for id in order {
        contents.push('\n');
        contents.push_str(id);
    }
    write_config_entry_bytes(
        directory,
        ROUTING_RULE_GROUP_ORDER_FILE,
        contents.as_bytes(),
    )
    .map(|_path| ())
    .map_err(|_error| SubscriptionStoreError::StoreUnavailable)
}

#[cfg(windows)]
pub(crate) fn save_routing_rule_group_order_in(
    _directory: &Path,
    _order: &[String],
) -> Result<(), SubscriptionStoreError> {
    Err(SubscriptionStoreError::StoreUnavailable)
}

#[cfg(not(windows))]
pub(crate) fn load_routing_rule_group_order_in(
    directory: &Path,
) -> Result<Vec<String>, SubscriptionStoreError> {
    require_clean_absolute_store(directory)?;
    let Some(contents) = crate::config_toml::read_entry(
        directory,
        ROUTING_RULE_GROUP_ORDER_FILE,
        MAX_ROUTING_RULE_GROUP_ORDER_FILE_BYTES,
    )
    .map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)?
    else {
        return Ok(Vec::new());
    };
    let mut lines = contents.lines();
    if lines.next() != Some(ROUTING_RULE_GROUP_ORDER_VERSION) {
        return Err(SubscriptionStoreError::StoredSourceUnavailable);
    }
    let mut seen = BTreeSet::new();
    lines
        .map(|id| {
            if valid_routing_rule_group_id(id) && seen.insert(id) {
                Ok(id.to_owned())
            } else {
                Err(SubscriptionStoreError::StoredSourceUnavailable)
            }
        })
        .collect()
}

#[cfg(windows)]
pub(crate) fn load_routing_rule_group_order_in(
    _directory: &Path,
) -> Result<Vec<String>, SubscriptionStoreError> {
    Ok(Vec::new())
}

#[cfg(test)]
mod reorder_tests {
    use super::place_routing_rule_group;

    #[test]
    fn dragging_across_multiple_groups_preserves_the_other_priorities() {
        for (source, anchor, after, expected) in [
            ("a", "d", true, ["b", "c", "d", "a"]),
            ("a", "d", false, ["b", "c", "a", "d"]),
            ("d", "a", false, ["d", "a", "b", "c"]),
            ("d", "a", true, ["a", "d", "b", "c"]),
        ] {
            let mut order = ["a", "b", "c", "d"].map(str::to_owned);
            assert!(place_routing_rule_group(&mut order, source, anchor, after));
            assert_eq!(order, expected);
        }
    }

    #[test]
    fn stale_self_and_unchanged_drops_do_not_reorder() {
        for (source, anchor, after) in [
            ("missing", "a", false),
            ("a", "missing", false),
            ("a", "a", true),
            ("a", "b", false),
            ("b", "a", true),
        ] {
            let mut order = ["a", "b", "c"].map(str::to_owned);
            assert!(!place_routing_rule_group(&mut order, source, anchor, after));
            assert_eq!(order, ["a", "b", "c"]);
        }
    }
}

#[cfg(all(test, not(windows)))]
mod local_group_order_tests {
    use super::{load_routing_rule_group_order_in, save_routing_rule_group_order_in};
    use crate::mihomo::{
        LocalRoutingRuleGroup, MANUAL_ROUTING_RULE_GROUP_ID, Name, SecretUrl, StoredQxRuleSource,
        normalized_routing_rule_group_order,
    };
    use std::fs;

    fn source(id: &str) -> StoredQxRuleSource {
        StoredQxRuleSource {
            id: id.to_owned(),
            name: None,
            source: SecretUrl::parse_https("https://example.invalid/rules").expect("source"),
            enabled: true,
            target_policy: Name::parse("DIRECT").expect("target"),
            content: "DOMAIN-SUFFIX,example.invalid,DIRECT".to_owned(),
            rule_count: 1,
            diagnostic_count: 0,
            refresh_interval: crate::mihomo::RemoteSourceRefreshInterval::Manual,
            last_successful_update_unix_secs: 0,
        }
    }

    fn local(id: &str) -> LocalRoutingRuleGroup {
        LocalRoutingRuleGroup {
            id: id.to_owned(),
            name: "Local".to_owned(),
            target_policy: Name::parse("DIRECT").expect("target"),
            enabled: true,
            rules: Vec::new(),
        }
    }

    fn order_ids(order: &[String]) -> Vec<&str> {
        order.iter().map(String::as_str).collect()
    }

    #[test]
    fn a_local_group_joins_the_shared_order_without_disturbing_the_stored_one() {
        let sources = [source("qx-rule-b")];
        let locals = [local("local-rule-1")];

        assert_eq!(
            order_ids(&normalized_routing_rule_group_order(
                &[
                    "qx-rule-b".to_owned(),
                    MANUAL_ROUTING_RULE_GROUP_ID.to_owned()
                ],
                true,
                &sources,
                &locals,
            )),
            ["qx-rule-b", MANUAL_ROUTING_RULE_GROUP_ID, "local-rule-1"]
        );
        assert_eq!(
            order_ids(&normalized_routing_rule_group_order(
                &["local-rule-1".to_owned()],
                false,
                &sources,
                &locals,
            )),
            ["local-rule-1", "qx-rule-b"]
        );
    }

    #[test]
    fn the_manual_group_comes_first_by_default_and_dropped_groups_disappear() {
        let sources = [source("qx-rule-b")];
        let locals = [local("local-rule-1")];

        assert_eq!(
            order_ids(&normalized_routing_rule_group_order(
                &[],
                true,
                &sources,
                &locals
            )),
            [MANUAL_ROUTING_RULE_GROUP_ID, "qx-rule-b", "local-rule-1"]
        );
        assert_eq!(
            order_ids(&normalized_routing_rule_group_order(
                &["local-rule-9".to_owned(), "local-rule-1".to_owned()],
                false,
                &[],
                &locals,
            )),
            ["local-rule-1"]
        );
    }

    #[test]
    fn the_group_order_file_stores_local_group_ids() {
        let store = std::env::temp_dir().join(format!(
            "manis-local-group-order-{}-{:x}",
            std::process::id(),
            crate::mihomo::current_unix_secs()
        ));
        fs::create_dir_all(&store).expect("create fixture");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            fs::set_permissions(&store, fs::Permissions::from_mode(0o700))
                .expect("make fixture private");
        }
        let order = [
            MANUAL_ROUTING_RULE_GROUP_ID.to_owned(),
            "local-rule-1".to_owned(),
            "qx-rule-b".to_owned(),
        ];
        save_routing_rule_group_order_in(&store, &order).expect("save order");
        assert_eq!(
            load_routing_rule_group_order_in(&store).expect("load order"),
            order
        );
        fs::remove_dir_all(store).expect("remove fixture");
    }
}
