use super::{
    Path, ProfileMode, ROUTING_MODE_FILE, RoutingMode, SubscriptionStoreError,
    WORKSPACE_STATE_FILE, valid_workspace_group_id,
};
#[cfg(not(windows))]
use super::{require_clean_absolute_store, write_private_atomic};

#[cfg(not(windows))]
pub(crate) fn save_collapsed_groups_in<'a>(
    directory: &Path,
    group_ids: impl IntoIterator<Item = &'a str>,
) -> Result<(), SubscriptionStoreError> {
    let mut ids: Vec<_> = group_ids
        .into_iter()
        .filter(|id| valid_workspace_group_id(id))
        .collect();
    ids.sort_unstable();
    ids.dedup();
    let contents = ids.join("\n");
    write_private_atomic(directory, WORKSPACE_STATE_FILE, contents.as_bytes())
        .map(|_path| ())
        .map_err(|_error| SubscriptionStoreError::StoreUnavailable)
}

#[cfg(windows)]
pub(crate) fn save_collapsed_groups_in<'a>(
    _directory: &Path,
    _group_ids: impl IntoIterator<Item = &'a str>,
) -> Result<(), SubscriptionStoreError> {
    Err(SubscriptionStoreError::StoreUnavailable)
}

#[cfg(not(windows))]
pub(crate) fn load_collapsed_groups_in(
    directory: &Path,
) -> Result<Vec<String>, SubscriptionStoreError> {
    require_clean_absolute_store(directory)?;
    let contents = crate::config_toml::read_entry(
        directory,
        WORKSPACE_STATE_FILE,
        super::MAX_SUBSCRIPTION_FILE_BYTES,
    )
    .map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)?
    .unwrap_or_default();
    contents
        .lines()
        .map(str::to_owned)
        .map(|id| {
            valid_workspace_group_id(&id)
                .then_some(id)
                .ok_or(SubscriptionStoreError::StoredSourceUnavailable)
        })
        .collect()
}

#[cfg(windows)]
pub(crate) fn load_collapsed_groups_in(
    _directory: &Path,
) -> Result<Vec<String>, SubscriptionStoreError> {
    Ok(Vec::new())
}

#[cfg(not(windows))]
pub(crate) fn save_routing_mode_in(
    directory: &Path,
    mode: RoutingMode,
) -> Result<(), SubscriptionStoreError> {
    write_private_atomic(directory, ROUTING_MODE_FILE, mode.wire_value().as_bytes())
        .map(|_path| ())
        .map_err(|_error| SubscriptionStoreError::StoreUnavailable)
}

#[cfg(windows)]
pub(crate) fn save_routing_mode_in(
    _directory: &Path,
    _mode: RoutingMode,
) -> Result<(), SubscriptionStoreError> {
    Err(SubscriptionStoreError::StoreUnavailable)
}

#[cfg(not(windows))]
pub(crate) fn load_routing_mode_in(
    directory: &Path,
) -> Result<RoutingMode, SubscriptionStoreError> {
    require_clean_absolute_store(directory)?;
    let Some(contents) = crate::config_toml::read_entry(
        directory,
        ROUTING_MODE_FILE,
        super::MAX_SUBSCRIPTION_FILE_BYTES,
    )
    .map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)?
    else {
        return Ok(RoutingMode::Rule);
    };
    RoutingMode::parse_wire_value(contents.trim())
        .ok_or(SubscriptionStoreError::StoredSourceUnavailable)
}

#[cfg(windows)]
pub(crate) fn load_routing_mode_in(
    _directory: &Path,
) -> Result<RoutingMode, SubscriptionStoreError> {
    Ok(RoutingMode::Rule)
}

pub(crate) fn profile_mode(mode: RoutingMode) -> ProfileMode {
    match mode {
        RoutingMode::Direct => ProfileMode::Direct,
        RoutingMode::Global => ProfileMode::Global,
        RoutingMode::Rule => ProfileMode::Rule,
    }
}
