use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use super::SubscriptionStoreError;

const MAX_SNAPSHOT_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Clone)]
pub(crate) struct SubscriptionStoreSnapshot {
    files: BTreeMap<String, Vec<u8>>,
}

impl SubscriptionStoreSnapshot {
    pub(crate) fn capture(directory: &Path) -> Result<Self, SubscriptionStoreError> {
        let entries = crate::config_toml::entries(directory)
            .map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)?;
        let total_bytes = entries.values().try_fold(0_u64, |total, contents| {
            total
                .checked_add(contents.len() as u64)
                .filter(|total| *total <= MAX_SNAPSHOT_BYTES)
                .ok_or(SubscriptionStoreError::StoreUnavailable)
        })?;
        let _ = total_bytes;
        let files = entries
            .into_iter()
            .map(|(name, contents)| (name, contents.into_bytes()))
            .collect();
        Ok(Self { files })
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
                        crate::config_toml::write_entry(
                            &transaction.staged,
                            name,
                            std::str::from_utf8(bytes)
                                .map_err(|_| SubscriptionStoreError::StoredSourceUnavailable)?,
                        )
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
            if !crate::config_toml::replace_entry_if_unchanged(
                directory,
                &change.name,
                change
                    .before
                    .as_deref()
                    .map(std::str::from_utf8)
                    .transpose()
                    .map_err(|_| SubscriptionStoreError::StoredSourceUnavailable)?,
                change
                    .after
                    .as_deref()
                    .map(std::str::from_utf8)
                    .transpose()
                    .map_err(|_| SubscriptionStoreError::StoredSourceUnavailable)?,
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
                if crate::config_toml::replace_entry_if_unchanged(
                    directory,
                    &change.name,
                    change
                        .after
                        .as_deref()
                        .map(std::str::from_utf8)
                        .transpose()
                        .map_err(|_| SubscriptionStoreError::StoredSourceUnavailable)?,
                    change
                        .before
                        .as_deref()
                        .map(std::str::from_utf8)
                        .transpose()
                        .map_err(|_| SubscriptionStoreError::StoredSourceUnavailable)?,
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
    crate::config_toml::read_entry(directory, name, MAX_SNAPSHOT_BYTES)
        .map(|contents| contents.map(String::into_bytes))
        .map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)
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

    fn write(directory: &Path, name: &str, contents: &str) {
        crate::config_toml::write_entry(directory, name, contents).expect("write entry");
    }

    fn remove(directory: &Path, name: &str) {
        crate::config_toml::remove_entry(directory, name).expect("remove entry");
    }

    fn read(directory: &Path, name: &str) -> Option<String> {
        crate::config_toml::read_entry(directory, name, MAX_SNAPSHOT_BYTES).expect("read entry")
    }

    #[test]
    fn rollback_only_restores_installed_changes() {
        let store = store("rollback");
        write(&store, "routing.mode", "rule");
        write(&store, "kernel.kind", "mihomo");
        write(&store, "language.preference", "zh-CN");
        let staged = SourceStoreTransaction::begin(&store).unwrap();
        write(staged.directory(), "routing.mode", "global");
        remove(staged.directory(), "kernel.kind");
        write(staged.directory(), "workspace.state", "new");
        let mut changes = staged.changes().unwrap();
        write(&store, "language.preference", "en");
        changes.install(&store).unwrap();
        assert_eq!(read(&store, "routing.mode").as_deref(), Some("global"));
        assert_eq!(read(&store, "kernel.kind"), None);
        write(&store, "node-selection.state", "independent setting");
        changes.rollback(&store).unwrap();
        assert_eq!(read(&store, "routing.mode").as_deref(), Some("rule"));
        assert_eq!(read(&store, "kernel.kind").as_deref(), Some("mihomo"));
        assert_eq!(read(&store, "workspace.state"), None);
        assert_eq!(read(&store, "language.preference").as_deref(), Some("en"));
        assert_eq!(
            read(&store, "node-selection.state").as_deref(),
            Some("independent setting")
        );
        fs::remove_dir_all(store).unwrap();
    }

    #[test]
    fn commit_rejects_a_concurrent_change_to_the_same_file() {
        let store = store("conflict");
        write(&store, "routing.mode", "rule");
        let staged = SourceStoreTransaction::begin(&store).unwrap();
        write(staged.directory(), "routing.mode", "global");
        let mut changes = staged.changes().unwrap();
        write(&store, "routing.mode", "direct");
        assert!(changes.install(&store).is_err());
        changes.rollback(&store).unwrap();
        assert_eq!(read(&store, "routing.mode").as_deref(), Some("direct"));
        fs::remove_dir_all(store).unwrap();
    }

    #[test]
    fn rollback_reports_conflicts_and_still_restores_other_files() {
        let store = store("rollback-conflict");
        write(&store, "routing.mode", "rule");
        write(&store, "language.preference", "system");
        let staged = SourceStoreTransaction::begin(&store).unwrap();
        write(staged.directory(), "routing.mode", "global");
        write(staged.directory(), "language.preference", "zh-CN");
        let mut changes = staged.changes().unwrap();
        changes.install(&store).unwrap();
        write(&store, "language.preference", "en");
        assert!(changes.rollback(&store).is_err());
        assert_eq!(read(&store, "routing.mode").as_deref(), Some("rule"));
        assert_eq!(read(&store, "language.preference").as_deref(), Some("en"));
        fs::remove_dir_all(store).unwrap();
    }

    #[test]
    fn abandoned_staging_removes_private_files_without_touching_live_store() {
        let store = store("abandoned");
        write(&store, "routing.mode", "rule");
        let staged = SourceStoreTransaction::begin(&store).unwrap();
        let path = staged.directory().to_owned();
        write(&path, "routing.mode", "global");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o700
            );
            assert_eq!(
                fs::metadata(path.join("config.toml"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
        drop(staged);
        assert!(!path.exists());
        assert_eq!(read(&store, "routing.mode").as_deref(), Some("rule"));
        fs::remove_dir_all(store).unwrap();
    }
}
