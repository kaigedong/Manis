use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::localization::LanguagePreference;
use crate::mihomo::{self, SubscriptionStoreError};

const BACKUP_SCHEMA: &str = "manis.configuration-backup";
const BACKUP_VERSION: u8 = 1;
const MAX_BACKUP_TEXT_BYTES: u64 = 64 * 1024 * 1024;
const MAX_BACKUP_FILES: usize = 512;
const MAX_PORTABLE_FILE_BYTES: u64 = 4 * 1024 * 1024;
const MAX_SMALL_STATE_BYTES: u64 = 512 * 1024;
const BACKUP_DIRECTORY_NAME: &str = "configuration-backups";

mod filesystem;
mod restore;

use filesystem::{
    backup_current_store, create_backup_dir, current_unix_nanos, current_unix_secs, max_file_bytes,
    portable_store_paths, read_bounded_text_file, read_private_portable_file,
    remove_current_store_files, require_clean_absolute_path, validate_file_name,
    write_authorized_external_file, write_files,
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
            .field("file_count", &self.files.len())
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
    PermissionDenied,
    UnsafePath,
    Oversized,
    InvalidFormat,
    InvalidConfiguration,
}

impl fmt::Display for BackupError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Unavailable => "configuration backup is unavailable",
            Self::PermissionDenied => "permission to write the configuration backup was denied",
            Self::UnsafePath => "configuration backup path is not safe",
            Self::Oversized => "configuration backup is too large",
            Self::InvalidFormat => "configuration backup format is invalid or unsupported",
            Self::InvalidConfiguration => "configuration backup contains invalid configuration",
        })
    }
}

impl std::error::Error for BackupError {}

impl From<SubscriptionStoreError> for BackupError {
    fn from(_error: SubscriptionStoreError) -> Self {
        Self::InvalidConfiguration
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct BackupDocument {
    schema: String,
    version: u8,
    created_unix_secs: u64,
    files: Vec<BackupFile>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct BackupFile {
    name: String,
    contents: String,
}

pub(crate) fn export_backup(directory: &Path) -> Result<String, BackupError> {
    let text = read_configuration_for_editing(directory)?;
    prepare_import(&text)?;
    Ok(text)
}

// Reading a draft must not require valid configuration: the editor is also used to
// repair stale references. Keep path, permission and size checks here; validate on apply.
pub(crate) fn read_configuration_for_editing(directory: &Path) -> Result<String, BackupError> {
    let mut files = Vec::new();
    let mut total = 0_u64;
    for path in portable_store_paths(directory)? {
        let name = path
            .file_name()
            .and_then(std::ffi::OsStr::to_str)
            .ok_or(BackupError::UnsafePath)?
            .to_owned();
        let contents = read_private_portable_file(&path, max_file_bytes(&name)?)?;
        total = total
            .checked_add(contents.len() as u64)
            .filter(|total| *total <= MAX_BACKUP_TEXT_BYTES)
            .ok_or(BackupError::Oversized)?;
        files.push(BackupFile { name, contents });
        if files.len() > MAX_BACKUP_FILES {
            return Err(BackupError::Oversized);
        }
    }
    let document = BackupDocument {
        schema: BACKUP_SCHEMA.to_owned(),
        version: BACKUP_VERSION,
        created_unix_secs: current_unix_secs(),
        files,
    };
    let text =
        serde_json::to_string_pretty(&document).map_err(|_error| BackupError::Unavailable)?;
    if text.len() as u64 > MAX_BACKUP_TEXT_BYTES {
        return Err(BackupError::Oversized);
    }
    Ok(text)
}

pub(crate) fn read_backup(path: &Path) -> Result<PreparedBackup, BackupError> {
    let text = read_bounded_text_file(path, MAX_BACKUP_TEXT_BYTES)?;
    prepare_import(&text)
}

pub(crate) fn export_to_file(directory: &Path, path: &Path) -> Result<(), BackupError> {
    require_clean_absolute_path(directory)?;
    let destination_parent = path.parent().ok_or(BackupError::UnsafePath)?;
    let destination_parent =
        fs::canonicalize(destination_parent).map_err(|_| BackupError::Unavailable)?;
    let store = resolve_directory(directory)?;
    if destination_parent.starts_with(store) {
        return Err(BackupError::UnsafePath);
    }
    let contents = export_backup(directory)?;
    write_authorized_external_file(path, contents.as_bytes())
}

// A fresh install may not have a source directory yet. Resolve its existing ancestors too,
// so a symlinked parent cannot bypass the export destination check.
fn resolve_directory(path: &Path) -> Result<PathBuf, BackupError> {
    let mut ancestor = path;
    let mut missing = Vec::new();
    let mut resolved = loop {
        match fs::canonicalize(ancestor) {
            Ok(path) => break path,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                missing.push(ancestor.file_name().ok_or(BackupError::UnsafePath)?);
                ancestor = ancestor.parent().ok_or(BackupError::UnsafePath)?;
            }
            Err(_) => return Err(BackupError::Unavailable),
        }
    };
    for component in missing.into_iter().rev() {
        resolved.push(component);
    }
    Ok(resolved)
}

pub(crate) fn prepare_import(text: &str) -> Result<PreparedBackup, BackupError> {
    if text.len() as u64 > MAX_BACKUP_TEXT_BYTES {
        return Err(BackupError::Oversized);
    }
    let document: BackupDocument =
        serde_json::from_str(text).map_err(|_error| BackupError::InvalidFormat)?;
    if document.schema != BACKUP_SCHEMA || document.version != BACKUP_VERSION {
        return Err(BackupError::InvalidFormat);
    }
    if document.files.len() > MAX_BACKUP_FILES {
        return Err(BackupError::Oversized);
    }

    let mut files = BTreeMap::new();
    let mut total = 0_u64;
    for file in document.files {
        validate_file_name(&file.name)?;
        let max_bytes = max_file_bytes(&file.name)?;
        let len = file.contents.len() as u64;
        if len > max_bytes {
            return Err(BackupError::Oversized);
        }
        total = total
            .checked_add(len)
            .filter(|total| *total <= MAX_BACKUP_TEXT_BYTES)
            .ok_or(BackupError::Oversized)?;
        if files.insert(file.name, file.contents).is_some() {
            return Err(BackupError::InvalidFormat);
        }
    }

    let temp = TempStore::new()?;
    write_files(temp.store_dir(), &files)?;
    let summary = validate_store(temp.store_dir(), &files)?;
    Ok(PreparedBackup { files, summary })
}

struct TempStore {
    root: PathBuf,
    store: PathBuf,
}

impl TempStore {
    fn new() -> Result<Self, BackupError> {
        for _ in 0..80 {
            let root = std::env::temp_dir().join(format!(
                "manis-configuration-backup-{}-{:x}",
                std::process::id(),
                current_unix_nanos()
            ));
            match fs::create_dir(&root) {
                Ok(()) => {
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt as _;
                        fs::set_permissions(&root, fs::Permissions::from_mode(0o700))
                            .map_err(|_error| BackupError::Unavailable)?;
                    }
                    let store = root.join("subscriptions");
                    return Ok(Self { root, store });
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(_error) => return Err(BackupError::Unavailable),
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

#[cfg(all(test, not(windows)))]
mod tests {
    use std::fs;

    use manis_core::{ManagedPolicyGroup, ManagedPolicyStrategy, RoutingMode};

    fn temp_store(name: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "{name}-{}-{:x}",
            std::process::id(),
            super::current_unix_nanos()
        ));
        let _ = fs::remove_dir_all(&root);
        root.join("subscriptions")
    }

    #[test]
    fn backup_round_trips_portable_configuration() -> Result<(), Box<dyn std::error::Error>> {
        let source = temp_store("manis-backup-source");
        let target = temp_store("manis-backup-target");

        crate::mihomo::save_subscription_source_with_options_in(
            &source,
            "https://example.invalid/sub?token=private",
            "Private subscription",
            crate::mihomo::RemoteSourceRefreshInterval::Daily,
            true,
        )?;
        crate::mihomo::save_single_node_source_with_options_in(
            &source,
            "vless://00000000-0000-0000-0000-000000000000@example.invalid:443?encryption=none#HK",
            "HK",
            true,
        )?;
        crate::mihomo::save_named_qx_rule_source_in(
            &source,
            "https://example.invalid/rules.list?token=private",
            "Work",
            "DIRECT",
            "DOMAIN-SUFFIX,example.com,DIRECT\n",
        )?;
        let mut group = ManagedPolicyGroup::new("policy-1", "Auto")?;
        group.strategy = ManagedPolicyStrategy::LowestLatency;
        group.switch_tolerance_ms = 200;
        crate::mihomo::save_managed_policy_in(&source, &group)?;
        crate::mihomo::save_routing_mode_in(&source, RoutingMode::Global)?;
        crate::manual_rule::save_manual_rules_in(
            &source,
            &[crate::manual_rule::ManualRule::parse(
                crate::manual_rule::ManualRuleKind::HostSuffix,
                "example.com",
                "DIRECT",
            )?],
        )?;
        crate::kernel::save_kernel_kind_in(&source, manis_core::KernelKind::Mihomo)?;
        crate::localization::save_language_preference_in(
            &source,
            crate::localization::LanguagePreference::SimplifiedChinese,
        )?;
        let node = crate::mihomo::load_single_node_sources_in(&source)?.remove(0);
        let mut selections = crate::mihomo::NodeSelectionPreferences::default();
        selections.set_global(manis_core::NodeIdentity::new(&node.id, &node.name)?);
        crate::mihomo::save_node_selection_preferences_in(&source, &selections)?;
        crate::mihomo::save_collapsed_groups_in(&source, [node.id.as_str()])?;

        let text = super::export_backup(&source)?;
        assert!(text.contains("manis.configuration-backup"));
        assert!(!text.contains("benchmarks.state"));
        let prepared = super::prepare_import(&text)?;
        assert_eq!(prepared.summary().subscriptions, 1);
        assert_eq!(prepared.summary().single_nodes, 1);
        assert_eq!(prepared.summary().rule_sources, 1);
        assert_eq!(prepared.summary().policy_groups, 1);
        assert_eq!(prepared.summary().manual_rules, 1);

        crate::mihomo::save_routing_mode_in(&target, RoutingMode::Direct)?;
        let result = super::restore(&target, &prepared)?;
        assert!(result.backup_dir.is_dir());
        assert_eq!(
            crate::mihomo::load_subscription_sources_in(&target)?.len(),
            1
        );
        assert_eq!(
            crate::mihomo::load_single_node_sources_in(&target)?.len(),
            1
        );
        assert_eq!(crate::mihomo::load_qx_rule_sources_in(&target)?.len(), 1);
        assert_eq!(
            crate::mihomo::load_managed_policy_groups_in(&target)?.len(),
            1
        );
        assert_eq!(
            crate::mihomo::load_routing_mode_in(&target)?,
            RoutingMode::Global
        );
        let exported_again = super::prepare_import(&super::export_backup(&target)?)?;
        assert_eq!(
            prepared.files, exported_again.files,
            "all portable configuration values survive migration to a different directory"
        );

        fs::remove_dir_all(source.parent().expect("source root"))?;
        fs::remove_dir_all(target.parent().expect("target root"))?;
        Ok(())
    }

    #[test]
    fn export_cannot_overwrite_the_live_store_even_through_a_directory_alias()
    -> Result<(), Box<dyn std::error::Error>> {
        let store = temp_store("manis-backup-protected-target");
        crate::mihomo::save_routing_mode_in(&store, RoutingMode::Direct)?;
        let path = store.join("routing.mode");
        let before = fs::read(&path)?;
        assert_eq!(
            super::export_to_file(&store, &path),
            Err(super::BackupError::UnsafePath)
        );
        let alias = store.parent().expect("root").join("alias");
        std::os::unix::fs::symlink(&store, &alias)?;
        assert_eq!(
            super::export_to_file(&store, &alias.join("routing.mode")),
            Err(super::BackupError::UnsafePath)
        );
        assert_eq!(fs::read(&path)?, before);
        fs::remove_dir_all(store.parent().expect("root"))?;
        Ok(())
    }

    #[test]
    fn export_writes_the_authorized_file_without_creating_a_sibling()
    -> Result<(), Box<dyn std::error::Error>> {
        use std::os::unix::fs::PermissionsExt as _;

        let store = temp_store("manis-backup-authorized-target");
        crate::mihomo::save_routing_mode_in(&store, RoutingMode::Direct)?;
        let root = store.parent().expect("root");
        let destination = root.join("Manis.json");
        fs::write(&destination, "stale")?;
        fs::set_permissions(root, fs::Permissions::from_mode(0o500))?;

        let result = super::export_to_file(&store, &destination);
        let unauthorized = super::export_to_file(&store, &root.join("not-authorized.json"));

        fs::set_permissions(root, fs::Permissions::from_mode(0o700))?;
        assert_eq!(result, Ok(()));
        assert_eq!(unauthorized, Err(super::BackupError::PermissionDenied));
        let exported = super::read_backup(&destination)?;
        assert_eq!(
            exported.files.get("routing.mode").map(String::as_str),
            Some("direct")
        );
        fs::set_permissions(&destination, fs::Permissions::from_mode(0o000))?;
        assert!(matches!(
            super::read_backup(&destination),
            Err(super::BackupError::PermissionDenied)
        ));
        fs::set_permissions(&destination, fs::Permissions::from_mode(0o600))?;
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn oversized_files_and_symlinked_store_roots_are_rejected_before_changes()
    -> Result<(), Box<dyn std::error::Error>> {
        let store = temp_store("manis-backup-boundaries");
        crate::mihomo::save_routing_mode_in(&store, RoutingMode::Direct)?;
        let root = store.parent().expect("root");
        let oversized = root.join("oversized.json");
        fs::File::create(&oversized)?.set_len(super::MAX_BACKUP_TEXT_BYTES + 1)?;
        assert!(matches!(
            super::read_backup(&oversized),
            Err(super::BackupError::Oversized)
        ));
        let alias = root.join("alias");
        std::os::unix::fs::symlink(&store, &alias)?;
        assert!(matches!(
            super::export_backup(&alias),
            Err(super::BackupError::UnsafePath)
        ));
        let prepared = super::prepare_import(
            r#"{"schema":"manis.configuration-backup","version":1,"created_unix_secs":0,"files":[]}"#,
        )?;
        assert!(super::restore(&alias, &prepared).is_err());
        assert_eq!(
            crate::mihomo::load_routing_mode_in(&store)?,
            RoutingMode::Direct
        );
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn invalid_and_unsafe_backup_input_is_rejected() {
        assert!(matches!(
            super::prepare_import(
                r#"{"schema":"wrong","version":1,"created_unix_secs":0,"files":[]}"#
            ),
            Err(super::BackupError::InvalidFormat)
        ));
        assert!(matches!(
            super::prepare_import(
                r#"{"schema":"manis.configuration-backup","version":1,"created_unix_secs":0,"files":[{"name":"../evil","contents":""}]}"#
            ),
            Err(super::BackupError::InvalidFormat)
        ));
        assert!(matches!(
            super::prepare_import(
                r#"{"schema":"manis.configuration-backup","version":1,"created_unix_secs":0,"files":[{"name":"routing.mode","contents":"bad"}]}"#
            ),
            Err(super::BackupError::InvalidConfiguration)
        ));
        assert!(matches!(
            super::prepare_import(
                r#"{"schema":"manis.configuration-backup","version":1,"created_unix_secs":0,"files":[],"surprise":true}"#
            ),
            Err(super::BackupError::InvalidFormat)
        ));
    }

    #[test]
    fn failed_restore_rolls_back_the_previous_store() -> Result<(), Box<dyn std::error::Error>> {
        let target = temp_store("manis-backup-rollback");
        crate::mihomo::save_routing_mode_in(&target, RoutingMode::Direct)?;
        let before = fs::read_to_string(target.join("routing.mode"))?;
        let text = r#"{
  "schema": "manis.configuration-backup",
  "version": 1,
  "created_unix_secs": 0,
  "files": [
    { "name": "routing.mode", "contents": "global" },
    { "name": "node-selection.state", "contents": "bad-version" }
  ]
}"#;
        let prepared = super::PreparedBackup {
            files: serde_json::from_str::<super::BackupDocument>(text)?
                .files
                .into_iter()
                .map(|file| (file.name, file.contents))
                .collect(),
            summary: super::BackupSummary::default(),
        };

        let error = super::restore(&target, &prepared).expect_err("restore must fail");
        assert!(error.backup_dir.is_some());
        assert!(!error.rollback_failed);
        assert_eq!(fs::read_to_string(target.join("routing.mode"))?, before);

        fs::remove_dir_all(target.parent().expect("target root"))?;
        Ok(())
    }

    #[test]
    fn export_rejects_symlinked_store_files() -> Result<(), Box<dyn std::error::Error>> {
        let store = temp_store("manis-backup-symlink");
        fs::create_dir_all(&store)?;
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink("/tmp/not-moved", store.join("routing.mode"))?;
            assert!(matches!(
                super::export_backup(&store),
                Err(super::BackupError::UnsafePath)
            ));
        }
        fs::remove_dir_all(store.parent().expect("store root"))?;
        Ok(())
    }

    #[test]
    fn restore_keeps_unknown_files_and_saves_importable_previous_backup()
    -> Result<(), Box<dyn std::error::Error>> {
        let target = temp_store("manis-backup-previous");
        crate::mihomo::save_routing_mode_in(&target, RoutingMode::Direct)?;
        manis_profile::write_private_atomic(&target, "benchmarks.state", b"device cache")?;
        manis_profile::write_private_atomic(&target, "future-local.state", b"keep me")?;

        let prepared = super::prepare_import(
            r#"{
  "schema": "manis.configuration-backup",
  "version": 1,
  "created_unix_secs": 0,
  "files": [
    { "name": "routing.mode", "contents": "global" }
  ]
}"#,
        )?;
        let result = super::restore(&target, &prepared)?;

        assert_eq!(
            crate::mihomo::load_routing_mode_in(&target)?,
            RoutingMode::Global
        );
        assert!(!target.join("benchmarks.state").exists());
        assert_eq!(
            fs::read_to_string(target.join("future-local.state"))?,
            "keep me"
        );
        let previous = fs::read_to_string(result.backup_dir.join("previous.json"))?;
        let previous = super::prepare_import(&previous)?;
        assert_eq!(previous.summary(), &super::BackupSummary::default());

        fs::remove_dir_all(target.parent().expect("target root"))?;
        Ok(())
    }
}
