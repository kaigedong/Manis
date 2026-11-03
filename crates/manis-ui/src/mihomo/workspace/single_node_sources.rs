#[cfg(not(windows))]
use super::{
    LEGACY_SAVED_SINGLE_NODE_VERSION, MAX_STORED_SUBSCRIPTION_FILE_BYTES, SAVED_SINGLE_NODE_PREFIX,
    SAVED_SINGLE_NODE_SUFFIX, SAVED_SINGLE_NODE_VERSION, SingleNodeSource, decode_hex, encode_hex,
    next_stored_source_id, private_store_entries, read_private_source_allow_empty_max,
    remove_private_source, require_clean_absolute_store, valid_stored_id,
    validate_subscription_source_name, write_private_atomic,
};
use super::{Path, StoredSingleNode, SubscriptionStoreError};

#[cfg(all(not(windows), test))]
pub(crate) fn save_single_node_source_in(
    directory: &Path,
    input: &str,
) -> Result<StoredSingleNode, SubscriptionStoreError> {
    let source =
        SingleNodeSource::parse(input).map_err(|_error| SubscriptionStoreError::InvalidSource)?;
    let name = source.preview().name.clone();
    save_single_node_source_with_options_in(directory, input, &name, true)
}

#[cfg(not(windows))]
pub(crate) fn save_single_node_source_with_options_in(
    directory: &Path,
    input: &str,
    name: &str,
    enabled: bool,
) -> Result<StoredSingleNode, SubscriptionStoreError> {
    let source =
        SingleNodeSource::parse(input).map_err(|_error| SubscriptionStoreError::InvalidSource)?;
    if let Some(existing) = load_single_node_sources_in(directory)?
        .into_iter()
        .find(|stored| stored.source == source)
    {
        if existing.enabled == enabled && existing.name == name.trim() {
            return Ok(existing);
        }
        return update_single_node_source_in(directory, &existing.id, input, name, enabled);
    }
    let id = next_stored_source_id(SAVED_SINGLE_NODE_PREFIX);
    let file_name = format!("{id}{SAVED_SINGLE_NODE_SUFFIX}");
    let name = validate_subscription_source_name(name)?;
    let encoded = encode_single_node_source(&id, input, &name, enabled)?;
    write_private_atomic(directory, &file_name, encoded.as_bytes())
        .map_err(|_error| SubscriptionStoreError::StoreUnavailable)?;
    Ok(StoredSingleNode {
        id,
        name,
        source,
        enabled,
    })
}

#[cfg(windows)]
pub(crate) fn save_single_node_source_in(
    _directory: &Path,
    _input: &str,
) -> Result<StoredSingleNode, SubscriptionStoreError> {
    Err(SubscriptionStoreError::StoreUnavailable)
}

#[cfg(windows)]
pub(crate) fn save_single_node_source_with_options_in(
    _directory: &Path,
    _input: &str,
    _name: &str,
    _enabled: bool,
) -> Result<StoredSingleNode, SubscriptionStoreError> {
    Err(SubscriptionStoreError::StoreUnavailable)
}

#[cfg(not(windows))]
pub(crate) fn update_single_node_source_in(
    directory: &Path,
    id: &str,
    input: &str,
    name: &str,
    enabled: bool,
) -> Result<StoredSingleNode, SubscriptionStoreError> {
    require_clean_absolute_store(directory)?;
    if !valid_stored_id(id, SAVED_SINGLE_NODE_PREFIX) {
        return Err(SubscriptionStoreError::InvalidSource);
    }
    let source =
        SingleNodeSource::parse(input).map_err(|_error| SubscriptionStoreError::InvalidSource)?;
    if load_single_node_sources_in(directory)?
        .into_iter()
        .any(|stored| stored.id != id && stored.source == source)
    {
        return Err(SubscriptionStoreError::InvalidSource);
    }
    let name = validate_subscription_source_name(name)?;
    let encoded = encode_single_node_source(id, input, &name, enabled)?;
    write_private_atomic(
        directory,
        &format!("{id}{SAVED_SINGLE_NODE_SUFFIX}"),
        encoded.as_bytes(),
    )
    .map_err(|_error| SubscriptionStoreError::StoreUnavailable)?;
    Ok(StoredSingleNode {
        id: id.to_owned(),
        name,
        source,
        enabled,
    })
}

#[cfg(windows)]
pub(crate) fn update_single_node_source_in(
    _directory: &Path,
    _id: &str,
    _input: &str,
    _name: &str,
    _enabled: bool,
) -> Result<StoredSingleNode, SubscriptionStoreError> {
    Err(SubscriptionStoreError::StoreUnavailable)
}

pub(crate) fn update_single_node_source_enabled_in(
    directory: &Path,
    id: &str,
    enabled: bool,
) -> Result<StoredSingleNode, SubscriptionStoreError> {
    let stored = load_single_node_sources_in(directory)?
        .into_iter()
        .find(|stored| stored.id == id)
        .ok_or(SubscriptionStoreError::StoredSourceUnavailable)?;
    let input = stored.source.expose_to(str::to_owned);
    update_single_node_source_in(directory, id, &input, &stored.name, enabled)
}

#[cfg(not(windows))]
pub(crate) fn load_single_node_sources_in(
    directory: &Path,
) -> Result<Vec<StoredSingleNode>, SubscriptionStoreError> {
    let mut nodes = Vec::new();
    let Some(entries) = private_store_entries(directory)? else {
        return Ok(nodes);
    };
    for path in entries {
        let Some(file_name) = path.file_name().and_then(std::ffi::OsStr::to_str) else {
            continue;
        };
        let Some(id) = file_name.strip_suffix(SAVED_SINGLE_NODE_SUFFIX) else {
            continue;
        };
        if !valid_stored_id(id, SAVED_SINGLE_NODE_PREFIX) {
            continue;
        }
        let contents =
            read_private_source_allow_empty_max(&path, MAX_STORED_SUBSCRIPTION_FILE_BYTES)?;
        nodes.push(decode_single_node_source(&contents, id)?);
    }
    nodes.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(nodes)
}

#[cfg(windows)]
pub(crate) fn load_single_node_sources_in(
    _directory: &Path,
) -> Result<Vec<StoredSingleNode>, SubscriptionStoreError> {
    Ok(Vec::new())
}

#[cfg(not(windows))]
fn encode_single_node_source(
    id: &str,
    input: &str,
    name: &str,
    enabled: bool,
) -> Result<String, SubscriptionStoreError> {
    if !valid_stored_id(id, SAVED_SINGLE_NODE_PREFIX)
        || input.len() > crate::subscription::MAX_SUBSCRIPTION_BYTES
    {
        return Err(SubscriptionStoreError::InvalidSource);
    }
    SingleNodeSource::parse(input).map_err(|_error| SubscriptionStoreError::InvalidSource)?;
    let name = validate_subscription_source_name(name)?;
    Ok([
        SAVED_SINGLE_NODE_VERSION.to_owned(),
        format!("id\t{id}"),
        format!("name\t{}", encode_hex(&name)),
        format!("enabled\t{}", if enabled { "true" } else { "false" }),
        format!("url\t{}", encode_hex(input)),
    ]
    .join("\n"))
}

#[cfg(not(windows))]
fn decode_single_node_source(
    contents: &str,
    expected_id: &str,
) -> Result<StoredSingleNode, SubscriptionStoreError> {
    if !matches!(
        contents.lines().next(),
        Some(SAVED_SINGLE_NODE_VERSION | LEGACY_SAVED_SINGLE_NODE_VERSION)
    ) {
        let source = SingleNodeSource::parse(contents)
            .map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)?;
        return Ok(StoredSingleNode {
            id: expected_id.to_owned(),
            name: source.preview().name.clone(),
            source,
            enabled: true,
        });
    }
    let mut id = None;
    let mut name = None;
    let mut enabled = None;
    let mut url = None;
    for line in contents.lines().skip(1) {
        let fields = line.split('\t').collect::<Vec<_>>();
        match fields.as_slice() {
            ["id", value] if id.is_none() => id = Some(*value),
            ["name", value] if name.is_none() => {
                name = Some(validate_subscription_source_name(&decode_hex(value)?)?);
            }
            ["enabled", value] if enabled.is_none() => {
                enabled = Some(match *value {
                    "true" => true,
                    "false" => false,
                    _ => return Err(SubscriptionStoreError::StoredSourceUnavailable),
                });
            }
            ["url", value] if url.is_none() => url = Some(decode_hex(value)?),
            _ => return Err(SubscriptionStoreError::StoredSourceUnavailable),
        }
    }
    if id != Some(expected_id) || !valid_stored_id(expected_id, SAVED_SINGLE_NODE_PREFIX) {
        return Err(SubscriptionStoreError::StoredSourceUnavailable);
    }
    let source = SingleNodeSource::parse(
        url.as_deref()
            .ok_or(SubscriptionStoreError::StoredSourceUnavailable)?,
    )
    .map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)?;
    Ok(StoredSingleNode {
        id: expected_id.to_owned(),
        name: name.unwrap_or_else(|| source.preview().name.clone()),
        source,
        enabled: enabled.ok_or(SubscriptionStoreError::StoredSourceUnavailable)?,
    })
}

#[cfg(not(windows))]
pub(crate) fn remove_single_node_source_in(
    directory: &Path,
    id: &str,
) -> Result<(), SubscriptionStoreError> {
    require_clean_absolute_store(directory)?;
    if !valid_stored_id(id, SAVED_SINGLE_NODE_PREFIX) {
        return Err(SubscriptionStoreError::StoreUnavailable);
    }
    remove_private_source(&directory.join(format!("{id}{SAVED_SINGLE_NODE_SUFFIX}")))
}

#[cfg(windows)]
pub(crate) fn remove_single_node_source_in(
    _directory: &Path,
    _id: &str,
) -> Result<(), SubscriptionStoreError> {
    Err(SubscriptionStoreError::StoreUnavailable)
}
