use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use manis_profile::write_private_atomic;

use super::SubscriptionStoreError;
#[cfg(not(windows))]
use super::{private_store_entries, require_clean_absolute_store};

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

/// Mutations run against a private copy, so a failed multi-file write never
/// exposes a partially saved source in the live store.
pub(crate) struct SourceStoreTransaction {
    staged: PathBuf,
    before: SubscriptionStoreSnapshot,
}

impl SourceStoreTransaction {
    pub(crate) fn begin(directory: &Path) -> Result<Self, SubscriptionStoreError> {
        let before = SubscriptionStoreSnapshot::capture(directory)?;
        for _ in 0..80 {
            let staged = std::env::temp_dir().join(format!(
                "manis-source-transaction-{}-{:x}",
                std::process::id(),
                super::current_unix_nanos()
            ));
            let builder = fs::DirBuilder::new();
            #[cfg(unix)]
            let builder = {
                use std::os::unix::fs::DirBuilderExt as _;
                let mut builder = builder;
                builder.mode(0o700);
                builder
            };
            match builder.create(&staged) {
                Ok(()) => {
                    let transaction = Self { staged, before };
                    for (name, bytes) in &transaction.before.files {
                        write_private_atomic(&transaction.staged, name, bytes)
                            .map_err(|_| SubscriptionStoreError::StoreUnavailable)?;
                    }
                    return Ok(transaction);
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(_) => return Err(SubscriptionStoreError::StoreUnavailable),
            }
        }
        Err(SubscriptionStoreError::StoreUnavailable)
    }

    pub(crate) fn directory(&self) -> &Path {
        &self.staged
    }

    pub(crate) fn changes(&self) -> Result<SourceStoreChanges, SubscriptionStoreError> {
        let after = SubscriptionStoreSnapshot::capture(&self.staged)?;
        let names: BTreeSet<_> = self.before.files.keys().chain(after.files.keys()).collect();
        let changes = names
            .into_iter()
            .filter(|name| self.before.files.get(*name) != after.files.get(*name))
            .map(|name| StoreChange {
                name: name.clone(),
                before: self.before.files.get(name).cloned(),
                after: after.files.get(name).cloned(),
            })
            .collect();
        Ok(SourceStoreChanges {
            changes,
            installed: 0,
        })
    }
}

impl Drop for SourceStoreTransaction {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.staged);
    }
}

struct StoreChange {
    name: String,
    before: Option<Vec<u8>>,
    after: Option<Vec<u8>>,
}

pub(crate) struct SourceStoreChanges {
    changes: Vec<StoreChange>,
    installed: usize,
}

impl SourceStoreChanges {
    pub(crate) fn install(&mut self, directory: &Path) -> Result<(), SubscriptionStoreError> {
        // Do not overwrite a change made since staging began. In particular,
        // unrelated preferences are never part of this write set at all.
        for change in &self.changes {
            if read_file(directory, &change.name)? != change.before {
                return Err(SubscriptionStoreError::StoreUnavailable);
            }
        }
        for change in &self.changes {
            // A write can fail after rename (for example while syncing the
            // directory), so include the attempted file in rollback as well.
            self.installed += 1;
            if !manis_profile::replace_private_if_unchanged(
                directory,
                &change.name,
                change.before.as_deref(),
                change.after.as_deref(),
            )
            .map_err(|_| SubscriptionStoreError::StoreUnavailable)?
            {
                return Err(SubscriptionStoreError::StoreUnavailable);
            }
        }
        Ok(())
    }

    pub(crate) fn rollback(&self, directory: &Path) -> Result<(), SubscriptionStoreError> {
        let mut result = Ok(());
        for change in self.changes[..self.installed].iter().rev() {
            let restored = (|| {
                // Preserve a later writer even when it touched the same file.
                if manis_profile::replace_private_if_unchanged(
                    directory,
                    &change.name,
                    change.after.as_deref(),
                    change.before.as_deref(),
                )
                .map_err(|_| SubscriptionStoreError::StoreUnavailable)?
                {
                    return Ok(());
                }
                if read_file(directory, &change.name)? == change.before {
                    Ok(())
                } else {
                    Err(SubscriptionStoreError::StoreUnavailable)
                }
            })();
            if let Err(error) = restored {
                result = Err(error);
            }
        }
        result
    }
}

fn read_file(directory: &Path, name: &str) -> Result<Option<Vec<u8>>, SubscriptionStoreError> {
    let path = directory.join(name);
    match fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.is_file() && metadata.len() <= MAX_SNAPSHOT_BYTES => {
            fs::read(path)
                .map(Some)
                .map_err(|_| SubscriptionStoreError::StoreUnavailable)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        _ => Err(SubscriptionStoreError::StoredSourceUnavailable),
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

#[cfg(all(test, not(windows)))]
mod transaction_tests {
    use super::*;

    fn store(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "manis-store-{name}-{}-{}",
            std::process::id(),
            super::super::current_unix_nanos()
        ))
    }

    #[test]
    fn rollback_only_restores_installed_changes() {
        let store = store("rollback");
        write_private_atomic(&store, "changed", b"before").unwrap();
        write_private_atomic(&store, "removed", b"original").unwrap();
        write_private_atomic(&store, "language.preference", b"zh-CN").unwrap();
        let staged = SourceStoreTransaction::begin(&store).unwrap();
        write_private_atomic(staged.directory(), "changed", b"after").unwrap();
        fs::remove_file(staged.directory().join("removed")).unwrap();
        write_private_atomic(staged.directory(), "added", b"new").unwrap();
        let mut changes = staged.changes().unwrap();
        write_private_atomic(&store, "language.preference", b"en").unwrap();
        changes.install(&store).unwrap();
        assert_eq!(fs::read(store.join("changed")).unwrap(), b"after");
        assert!(!store.join("removed").exists());
        write_private_atomic(&store, "unrelated", b"independent setting").unwrap();
        changes.rollback(&store).unwrap();
        assert_eq!(fs::read(store.join("changed")).unwrap(), b"before");
        assert_eq!(fs::read(store.join("removed")).unwrap(), b"original");
        assert!(!store.join("added").exists());
        assert_eq!(fs::read(store.join("language.preference")).unwrap(), b"en");
        assert_eq!(
            fs::read(store.join("unrelated")).unwrap(),
            b"independent setting"
        );
        fs::remove_dir_all(store).unwrap();
    }

    #[test]
    fn commit_rejects_a_concurrent_change_to_the_same_file() {
        let store = store("conflict");
        write_private_atomic(&store, "source", b"before").unwrap();
        let staged = SourceStoreTransaction::begin(&store).unwrap();
        write_private_atomic(staged.directory(), "source", b"candidate").unwrap();
        let mut changes = staged.changes().unwrap();
        write_private_atomic(&store, "source", b"newer save").unwrap();
        assert!(changes.install(&store).is_err());
        changes.rollback(&store).unwrap();
        assert_eq!(fs::read(store.join("source")).unwrap(), b"newer save");
        fs::remove_dir_all(store).unwrap();
    }

    #[test]
    fn rollback_reports_conflicts_and_still_restores_other_files() {
        let store = store("rollback-conflict");
        write_private_atomic(&store, "a", b"before a").unwrap();
        write_private_atomic(&store, "b", b"before b").unwrap();
        let staged = SourceStoreTransaction::begin(&store).unwrap();
        write_private_atomic(staged.directory(), "a", b"candidate a").unwrap();
        write_private_atomic(staged.directory(), "b", b"candidate b").unwrap();
        let mut changes = staged.changes().unwrap();
        changes.install(&store).unwrap();
        write_private_atomic(&store, "b", b"newer save").unwrap();
        assert!(changes.rollback(&store).is_err());
        assert_eq!(fs::read(store.join("a")).unwrap(), b"before a");
        assert_eq!(fs::read(store.join("b")).unwrap(), b"newer save");
        fs::remove_dir_all(store).unwrap();
    }

    #[test]
    fn abandoned_staging_removes_private_files_without_touching_live_store() {
        let store = store("abandoned");
        write_private_atomic(&store, "source", b"before").unwrap();
        let staged = SourceStoreTransaction::begin(&store).unwrap();
        let path = staged.directory().to_owned();
        write_private_atomic(&path, "source", b"partial save").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o700
            );
            assert_eq!(
                fs::metadata(path.join("source"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
        drop(staged);
        assert!(!path.exists());
        assert_eq!(fs::read(store.join("source")).unwrap(), b"before");
        fs::remove_dir_all(store).unwrap();
    }
}
