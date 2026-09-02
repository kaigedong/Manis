use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use crate::localization::LanguagePreference;
use crate::mihomo::{self, SubscriptionStoreError};

const MAX_BACKUP_TEXT_BYTES: u64 = 64 * 1024 * 1024;
const BACKUP_DIRECTORY_NAME: &str = "configuration-backups";

mod filesystem;
mod restore;

use filesystem::{
    backup_current_store, backup_root as backup_storage_root, create_backup_dir,
    current_unix_nanos, require_clean_absolute_path,
};
use restore::validate_store;
pub(crate) use restore::{backup_root, restore};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct BackupSummary {
    pub(crate) subscriptions: usize,
    pub(crate) single_nodes: usize,
    pub(crate) policy_groups: usize,
    pub(crate) rule_sources: usize,
    pub(crate) manual_rules: usize,
}

#[derive(Clone)]
pub(crate) struct PreparedBackup {
    source: String,
    files: BTreeMap<String, String>,
    summary: BackupSummary,
}

impl PreparedBackup {
    #[must_use]
    pub(crate) fn summary(&self) -> &BackupSummary {
        &self.summary
    }
}

impl fmt::Debug for PreparedBackup {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PreparedBackup")
            .field("source_bytes", &self.source.len())
            .field("entry_count", &self.files.len())
            .field("summary", &self.summary)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ImportResult {
    pub(crate) backup_dir: PathBuf,
}

#[derive(Debug)]
pub(crate) struct ImportError {
    pub(crate) backup_dir: Option<PathBuf>,
    pub(crate) rollback_failed: bool,
    kind: BackupError,
}

impl ImportError {
    fn new(kind: BackupError) -> Self {
        Self {
            backup_dir: None,
            rollback_failed: false,
            kind,
        }
    }

    fn with_backup(kind: BackupError, backup_dir: PathBuf, rollback_failed: bool) -> Self {
        Self {
            backup_dir: Some(backup_dir),
            rollback_failed,
            kind,
        }
    }
}

impl fmt::Display for ImportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.kind.fmt(formatter)
    }
}

impl std::error::Error for ImportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.kind)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BackupError {
    Unavailable,
    UnsafePath,
    Oversized,
    InvalidFormat,
    InvalidConfiguration,
}

impl fmt::Display for BackupError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Unavailable => "configuration is unavailable",
            Self::UnsafePath => "configuration path is not safe",
            Self::Oversized => "configuration is too large",
            Self::InvalidFormat => "configuration TOML is invalid or unsupported",
            Self::InvalidConfiguration => "configuration contains invalid values or references",
        })
    }
}

impl std::error::Error for BackupError {}

impl From<crate::config_toml::ConfigTomlError> for BackupError {
    fn from(error: crate::config_toml::ConfigTomlError) -> Self {
        match error {
            crate::config_toml::ConfigTomlError::Unavailable => Self::Unavailable,
            crate::config_toml::ConfigTomlError::UnsafePath => Self::UnsafePath,
            crate::config_toml::ConfigTomlError::InvalidFormat => Self::InvalidFormat,
            crate::config_toml::ConfigTomlError::Oversized => Self::Oversized,
        }
    }
}

impl From<SubscriptionStoreError> for BackupError {
    fn from(_: SubscriptionStoreError) -> Self {
        Self::InvalidConfiguration
    }
}

pub(crate) fn read_configuration_for_editing(directory: &Path) -> Result<String, BackupError> {
    crate::config_toml::read_source(directory).map_err(Into::into)
}

#[cfg(test)]
pub(crate) fn export_backup(directory: &Path) -> Result<String, BackupError> {
    let source = read_configuration_for_editing(directory)?;
    prepare_import(&source)?;
    Ok(source)
}

pub(crate) fn prepare_import(text: &str) -> Result<PreparedBackup, BackupError> {
    if text.len() as u64 > MAX_BACKUP_TEXT_BYTES {
        return Err(BackupError::Oversized);
    }
    let files = crate::config_toml::entries_from_source(text)?;
    let temp = TempStore::new()?;
    crate::config_toml::replace_source(temp.store_dir(), text)?;
    let summary = validate_store(temp.store_dir(), &files)?;
    Ok(PreparedBackup {
        source: text.to_owned(),
        files,
        summary,
    })
}

struct TempStore {
    root: PathBuf,
    store: PathBuf,
}

impl TempStore {
    fn new() -> Result<Self, BackupError> {
        for _ in 0..80 {
            let root = std::env::temp_dir().join(format!(
                "manis-configuration-{}-{:x}",
                std::process::id(),
                current_unix_nanos()
            ));
            match fs::create_dir(&root) {
                Ok(()) => {
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt as _;
                        fs::set_permissions(&root, fs::Permissions::from_mode(0o700))
                            .map_err(|_| BackupError::Unavailable)?;
                    }
                    return Ok(Self {
                        store: root.join("config"),
                        root,
                    });
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(_) => return Err(BackupError::Unavailable),
            }
        }
        Err(BackupError::Unavailable)
    }

    fn store_dir(&self) -> &Path {
        &self.store
    }
}

impl Drop for TempStore {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "manis-config-backup-{name}-{}-{:x}",
            std::process::id(),
            current_unix_nanos()
        ))
    }

    #[test]
    fn editor_round_trip_preserves_toml_comments() -> Result<(), Box<dyn std::error::Error>> {
        let source = store("source");
        let target = store("target");
        let text = r#"schema_version = 1

# 用户注释
[configuration]
"routing.mode" = "global" # 模式注释
"#;
        crate::config_toml::replace_source(&source, text)?;
        let prepared = prepare_import(&read_configuration_for_editing(&source)?)?;
        restore(&target, &prepared)?;
        assert_eq!(crate::config_toml::read_source(&target)?, text);
        fs::remove_dir_all(source)?;
        fs::remove_dir_all(target)?;
        Ok(())
    }

    #[test]
    fn invalid_toml_and_invalid_values_are_rejected() {
        assert!(matches!(
            prepare_import("{"),
            Err(BackupError::InvalidFormat)
        ));
        let invalid = r#"schema_version = 1
[configuration]
"routing.mode" = "impossible"
"#;
        assert!(matches!(
            prepare_import(invalid),
            Err(BackupError::InvalidConfiguration)
        ));
    }

    #[test]
    fn restore_saves_the_previous_toml() -> Result<(), Box<dyn std::error::Error>> {
        let target = store("previous");
        let before = r#"schema_version = 1
# before
[configuration]
"routing.mode" = "direct"
"#;
        let after = r#"schema_version = 1
# after
[configuration]
"routing.mode" = "global"
"#;
        crate::config_toml::replace_source(&target, before)?;
        let result = restore(&target, &prepare_import(after)?)?;
        assert_eq!(
            fs::read_to_string(result.backup_dir.join("previous.toml"))?,
            before
        );
        assert_eq!(crate::config_toml::read_source(&target)?, after);
        fs::remove_dir_all(target)?;
        Ok(())
    }
}
