#[cfg(not(windows))]
use super::subscription_sources::decode_subscription_source;
use super::{
    IMPORTED_SUBSCRIPTION_FILE, MAX_SUBSCRIPTION_FILE_BYTES, Path, SecretUrl,
    SubscriptionStoreError, require_clean_absolute_store, write_private_atomic,
};

#[cfg(test)]
pub(crate) fn save_imported_subscription_in(
    directory: &Path,
    input: &str,
) -> Result<SecretUrl, SubscriptionStoreError> {
    let subscription =
        SecretUrl::parse_subscription(input).map_err(|_| SubscriptionStoreError::InvalidSource)?;
    write_private_atomic(directory, IMPORTED_SUBSCRIPTION_FILE, input.as_bytes())
        .map_err(|_| SubscriptionStoreError::StoreUnavailable)?;
    Ok(subscription)
}

pub(crate) fn load_imported_subscription_in(
    directory: &Path,
) -> Result<Option<SecretUrl>, SubscriptionStoreError> {
    require_clean_absolute_store(directory)?;
    let Some(contents) = crate::config_toml::read_entry(
        directory,
        IMPORTED_SUBSCRIPTION_FILE,
        MAX_SUBSCRIPTION_FILE_BYTES,
    )
    .map_err(|_| SubscriptionStoreError::StoredSourceUnavailable)?
    else {
        return Ok(None);
    };
    decode_imported_subscription(&contents).map(Some)
}

#[cfg(not(windows))]
fn decode_imported_subscription(contents: &str) -> Result<SecretUrl, SubscriptionStoreError> {
    decode_subscription_source(contents, "subscription:legacy").map(|decoded| decoded.stored.source)
}

#[cfg(windows)]
fn decode_imported_subscription(contents: &str) -> Result<SecretUrl, SubscriptionStoreError> {
    SecretUrl::parse_subscription(contents)
        .map_err(|_| SubscriptionStoreError::StoredSourceUnavailable)
}

#[cfg(test)]
pub(crate) fn remove_imported_subscription_in(
    directory: &Path,
) -> Result<(), SubscriptionStoreError> {
    require_clean_absolute_store(directory)?;
    crate::config_toml::remove_entry(directory, IMPORTED_SUBSCRIPTION_FILE)
        .map_err(|_| SubscriptionStoreError::StoreUnavailable)
}
