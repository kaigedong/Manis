#[cfg(not(windows))]
use super::subscription_sources::decode_subscription_source;
use super::{
    IMPORTED_SUBSCRIPTION_FILE, MAX_SUBSCRIPTION_FILE_BYTES, Path, SecretUrl,
    SubscriptionStoreError,
};
#[cfg(not(windows))]
use super::{fs, require_clean_absolute_store, write_private_atomic};
#[cfg(not(windows))]
use std::io::Read as _;

#[cfg(all(not(windows), test))]
pub(crate) fn save_imported_subscription_in(
    directory: &Path,
    input: &str,
) -> Result<SecretUrl, SubscriptionStoreError> {
    let subscription = SecretUrl::parse_subscription(input)
        .map_err(|_error| SubscriptionStoreError::InvalidSource)?;
    write_private_atomic(directory, IMPORTED_SUBSCRIPTION_FILE, input.as_bytes())
        .map_err(|_error| SubscriptionStoreError::StoreUnavailable)?;
    Ok(subscription)
}

#[cfg(all(windows, test))]
pub(crate) fn save_imported_subscription_in(
    _directory: &Path,
    _input: &str,
) -> Result<SecretUrl, SubscriptionStoreError> {
    Err(SubscriptionStoreError::StoreUnavailable)
}

#[cfg(all(not(windows), test))]
pub(crate) fn load_imported_subscription_in(
    directory: &Path,
) -> Result<Option<SecretUrl>, SubscriptionStoreError> {
    require_clean_absolute_store(directory)?;
    let path = directory.join(IMPORTED_SUBSCRIPTION_FILE);
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_error) => return Err(SubscriptionStoreError::StoredSourceUnavailable),
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(SubscriptionStoreError::StoredSourceUnavailable);
    }
    let file =
        fs::File::open(&path).map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)?;
    let opened_metadata = file
        .metadata()
        .map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)?;
    if opened_metadata.len() > MAX_SUBSCRIPTION_FILE_BYTES {
        return Err(SubscriptionStoreError::StoredSourceUnavailable);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};

        if metadata.dev() != opened_metadata.dev()
            || metadata.ino() != opened_metadata.ino()
            || opened_metadata.permissions().mode() & 0o077 != 0
        {
            return Err(SubscriptionStoreError::StoredSourceUnavailable);
        }
    }
    let mut contents = String::new();
    file.take(MAX_SUBSCRIPTION_FILE_BYTES + 1)
        .read_to_string(&mut contents)
        .map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)?;
    if contents.len() as u64 > MAX_SUBSCRIPTION_FILE_BYTES {
        return Err(SubscriptionStoreError::StoredSourceUnavailable);
    }
    decode_subscription_source(&contents, "subscription:legacy")
        .map(|decoded| Some(decoded.stored.source))
}

#[cfg(windows)]
pub(crate) fn load_imported_subscription_in(
    directory: &Path,
) -> Result<Option<SecretUrl>, SubscriptionStoreError> {
    if directory.join(IMPORTED_SUBSCRIPTION_FILE).exists() {
        Err(SubscriptionStoreError::StoredSourceUnavailable)
    } else {
        Ok(None)
    }
}

#[cfg(all(not(windows), test))]
pub(crate) fn remove_imported_subscription_in(
    directory: &Path,
) -> Result<(), SubscriptionStoreError> {
    require_clean_absolute_store(directory)?;
    let path = directory.join(IMPORTED_SUBSCRIPTION_FILE);
    match fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            Err(SubscriptionStoreError::StoredSourceUnavailable)
        }
        Ok(_) => fs::remove_file(path).map_err(|_error| SubscriptionStoreError::StoreUnavailable),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_error) => Err(SubscriptionStoreError::StoreUnavailable),
    }
}

#[cfg(all(windows, test))]
pub(crate) fn remove_imported_subscription_in(
    _directory: &Path,
) -> Result<(), SubscriptionStoreError> {
    Err(SubscriptionStoreError::StoreUnavailable)
}
