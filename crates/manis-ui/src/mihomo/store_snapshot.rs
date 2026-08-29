use std::collections::BTreeMap;
#[cfg(not(windows))]
use std::fs;
use std::path::Path;

use super::SubscriptionStoreError;
#[cfg(not(windows))]
use super::{private_store_entries, require_clean_absolute_store, write_private_atomic};

const MAX_SNAPSHOT_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Clone)]
pub(crate) struct SubscriptionStoreSnapshot {
    files: BTreeMap<String, Vec<u8>>,
}

impl SubscriptionStoreSnapshot {
    #[cfg(not(windows))]
    pub(crate) fn capture(directory: &Path) -> Result<Self, SubscriptionStoreError> {
        let mut files = BTreeMap::new();
        let mut total_bytes = 0_u64;
        for path in private_store_entries(directory)?.unwrap_or_default() {
            let metadata = fs::symlink_metadata(&path)
                .map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)?;
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(SubscriptionStoreError::StoredSourceUnavailable);
            }
            total_bytes = total_bytes
                .checked_add(metadata.len())
                .filter(|total| *total <= MAX_SNAPSHOT_BYTES)
                .ok_or(SubscriptionStoreError::StoreUnavailable)?;
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or(SubscriptionStoreError::StoredSourceUnavailable)?
                .to_owned();
            let bytes =
                fs::read(path).map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)?;
            files.insert(name, bytes);
        }
        Ok(Self { files })
    }

    #[cfg(windows)]
    pub(crate) fn capture(_directory: &Path) -> Result<Self, SubscriptionStoreError> {
        Err(SubscriptionStoreError::StoreUnavailable)
    }

    #[cfg(not(windows))]
    pub(crate) fn restore(self, directory: &Path) -> Result<(), SubscriptionStoreError> {
        require_clean_absolute_store(directory)?;
        for path in private_store_entries(directory)?.unwrap_or_default() {
            let metadata = fs::symlink_metadata(&path)
                .map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)?;
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(SubscriptionStoreError::StoredSourceUnavailable);
            }
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or(SubscriptionStoreError::StoredSourceUnavailable)?;
            if !self.files.contains_key(name) {
                fs::remove_file(path).map_err(|_error| SubscriptionStoreError::StoreUnavailable)?;
            }
        }
        for (name, bytes) in self.files {
            write_private_atomic(directory, &name, &bytes)
                .map_err(|_error| SubscriptionStoreError::StoreUnavailable)?;
        }
        Ok(())
    }

    #[cfg(windows)]
    pub(crate) fn restore(self, _directory: &Path) -> Result<(), SubscriptionStoreError> {
        Err(SubscriptionStoreError::StoreUnavailable)
    }
}

#[cfg(all(test, not(windows)))]
mod tests {
    use std::fs;

    use manis_profile::write_private_atomic;

    use super::SubscriptionStoreSnapshot;

    #[test]
    fn restore_reinstates_changed_deleted_and_new_files() -> Result<(), Box<dyn std::error::Error>>
    {
        let root = std::env::temp_dir().join(format!(
            "manis-store-snapshot-{}-{}",
            std::process::id(),
            super::super::current_unix_nanos()
        ));
        let store = root.join("subscriptions");
        write_private_atomic(&store, "changed.state", b"before")?;
        write_private_atomic(&store, "deleted.state", b"keep")?;
        let snapshot = SubscriptionStoreSnapshot::capture(&store)?;

        write_private_atomic(&store, "changed.state", b"after")?;
        fs::remove_file(store.join("deleted.state"))?;
        write_private_atomic(&store, "new.state", b"remove")?;
        snapshot.restore(&store)?;

        assert_eq!(fs::read(store.join("changed.state"))?, b"before");
        assert_eq!(fs::read(store.join("deleted.state"))?, b"keep");
        assert!(!store.join("new.state").exists());
        fs::remove_dir_all(root)?;
        Ok(())
    }
}
