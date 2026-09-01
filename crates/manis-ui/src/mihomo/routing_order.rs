use std::collections::BTreeSet;
#[cfg(not(windows))]
use std::fs;
use std::path::Path;

use super::{
    MANUAL_ROUTING_RULE_GROUP_ID, MAX_ROUTING_RULE_GROUP_ORDER_FILE_BYTES, MAX_ROUTING_RULE_GROUPS,
    QX_RULE_SOURCE_PREFIX, ROUTING_RULE_GROUP_ORDER_FILE, ROUTING_RULE_GROUP_ORDER_VERSION,
    StoredQxRuleSource, SubscriptionStoreError, valid_stored_id,
};
#[cfg(not(windows))]
use super::{
    read_private_source_allow_empty_max, require_clean_absolute_store, write_private_atomic,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MoveDirection {
    Up,
    Down,
}

pub(crate) fn normalized_routing_rule_group_order(
    stored_order: &[String],
    has_manual_rules: bool,
    sources: &[StoredQxRuleSource],
) -> Vec<String> {
    let source_ids = sources
        .iter()
        .map(|source| source.id.as_str())
        .collect::<BTreeSet<_>>();
    let mut seen = BTreeSet::new();
    let mut order = stored_order
        .iter()
        .filter_map(|id| {
            let id = id.as_str();
            let retained =
                (has_manual_rules && id == MANUAL_ROUTING_RULE_GROUP_ID) || source_ids.contains(id);
            retained.then_some(id)
        })
        .filter(|id| seen.insert(*id))
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if has_manual_rules && seen.insert(MANUAL_ROUTING_RULE_GROUP_ID) {
        order.insert(0, MANUAL_ROUTING_RULE_GROUP_ID.to_owned());
    }
    for source in sources {
        if seen.insert(source.id.as_str()) {
            order.push(source.id.clone());
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

fn valid_routing_rule_group_id(id: &str) -> bool {
    id == MANUAL_ROUTING_RULE_GROUP_ID || valid_stored_id(id, QX_RULE_SOURCE_PREFIX)
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
    write_private_atomic(
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
    let path = directory.join(ROUTING_RULE_GROUP_ORDER_FILE);
    let contents = match fs::symlink_metadata(&path) {
        Ok(_) => {
            read_private_source_allow_empty_max(&path, MAX_ROUTING_RULE_GROUP_ORDER_FILE_BYTES)?
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(_error) => return Err(SubscriptionStoreError::StoredSourceUnavailable),
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
