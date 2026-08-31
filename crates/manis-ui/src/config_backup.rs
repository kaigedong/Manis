use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use manis_profile::write_private_atomic;
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
    UnsafePath,
    Oversized,
    InvalidFormat,
    InvalidConfiguration,
}

impl fmt::Display for BackupError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Unavailable => "configuration backup is unavailable",
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
    prepare_import(&text)?;
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
    write_external_file_atomic(path, contents.as_bytes())
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

pub(crate) fn restore(
    directory: &Path,
    prepared: &PreparedBackup,
) -> Result<ImportResult, ImportError> {
    require_clean_absolute_path(directory).map_err(ImportError::new)?;
    let snapshot = mihomo::SubscriptionStoreSnapshot::capture(directory)
        .map_err(|error| ImportError::new(error.into()))?;
    let backup_dir = create_backup_dir(directory).map_err(ImportError::new)?;
    backup_current_store(directory, &backup_dir)
        .map_err(|error| ImportError::with_backup(error, backup_dir.clone(), false))?;

    let restore_result = (|| -> Result<(), BackupError> {
        remove_current_store_files(directory)?;
        write_files(directory, &prepared.files)?;
        validate_store(directory, &prepared.files)?;
        Ok(())
    })();

    if let Err(error) = restore_result {
        let rollback_failed = snapshot.restore(directory).is_err();
        return Err(ImportError::with_backup(error, backup_dir, rollback_failed));
    }

    Ok(ImportResult { backup_dir })
}

pub(crate) fn backup_root(directory: &Path) -> Result<PathBuf, BackupError> {
    let parent = directory.parent().ok_or(BackupError::UnsafePath)?;
    Ok(parent.join(BACKUP_DIRECTORY_NAME))
}

fn validate_store(
    directory: &Path,
    files: &BTreeMap<String, String>,
) -> Result<BackupSummary, BackupError> {
    let subscriptions = mihomo::load_subscription_sources_in(directory)?.len();
    let single_nodes = mihomo::load_single_node_sources_in(directory)?.len();
    let rule_sources = mihomo::load_qx_rule_sources_in(directory)?.len();
    let policies = mihomo::load_managed_policy_groups_in(directory)?;
    mihomo::validate_managed_policy_references(&policies)
        .map_err(|_error| BackupError::InvalidConfiguration)?;
    mihomo::load_routing_rule_group_order_in(directory)?;
    mihomo::load_collapsed_groups_in(directory)?;
    mihomo::load_node_selection_preferences_in(directory)?;
    mihomo::load_routing_mode_in(directory)?;
    crate::kernel::load_kernel_kind_in(directory)
        .map(|_kind| ())
        .map_err(|_error| BackupError::InvalidConfiguration)?;
    crate::localization::load_language_preference_in(directory)
        .map(|_preference: LanguagePreference| ())
        .map_err(|_error| BackupError::InvalidConfiguration)?;
    let manual_rules = if files.contains_key("manual-routing-rules.state")
        || files.contains_key("direct-rules.state")
    {
        crate::manual_rule::load_manual_rules_in(directory)
            .map_err(|_error| BackupError::InvalidConfiguration)?
            .len()
    } else {
        0
    };
    Ok(BackupSummary {
        subscriptions,
        single_nodes,
        policy_groups: policies.len(),
        rule_sources,
        manual_rules,
    })
}

fn portable_store_paths(directory: &Path) -> Result<Vec<PathBuf>, BackupError> {
    require_clean_absolute_path(directory)?;
    let iterator = match fs::read_dir(directory) {
        Ok(iterator) => iterator,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(_error) => return Err(BackupError::Unavailable),
    };
    let mut paths = Vec::new();
    for entry in iterator {
        let path = entry.map_err(|_error| BackupError::Unavailable)?.path();
        let Some(name) = path.file_name().and_then(std::ffi::OsStr::to_str) else {
            return Err(BackupError::UnsafePath);
        };
        if is_portable_file_name(name) {
            paths.push(path);
        }
    }
    paths.sort();
    Ok(paths)
}

fn read_private_portable_file(path: &Path, max_bytes: u64) -> Result<String, BackupError> {
    let metadata = fs::symlink_metadata(path).map_err(|_error| BackupError::Unavailable)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() > max_bytes {
        return Err(BackupError::UnsafePath);
    }
    let file = fs::File::open(path).map_err(|_error| BackupError::Unavailable)?;
    let opened_metadata = file.metadata().map_err(|_error| BackupError::Unavailable)?;
    if opened_metadata.len() > max_bytes {
        return Err(BackupError::Oversized);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};

        if metadata.dev() != opened_metadata.dev()
            || metadata.ino() != opened_metadata.ino()
            || opened_metadata.permissions().mode() & 0o077 != 0
        {
            return Err(BackupError::UnsafePath);
        }
    }
    read_from_file(file, max_bytes)
}

fn read_bounded_text_file(path: &Path, max_bytes: u64) -> Result<String, BackupError> {
    let metadata = fs::symlink_metadata(path).map_err(|_error| BackupError::Unavailable)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(BackupError::UnsafePath);
    }
    let file = fs::File::open(path).map_err(|_error| BackupError::Unavailable)?;
    let opened_metadata = file.metadata().map_err(|_error| BackupError::Unavailable)?;
    if opened_metadata.len() > max_bytes {
        return Err(BackupError::Oversized);
    }
    read_from_file(file, max_bytes)
}

fn read_from_file(file: fs::File, max_bytes: u64) -> Result<String, BackupError> {
    let mut contents = String::new();
    file.take(max_bytes + 1)
        .read_to_string(&mut contents)
        .map_err(|_error| BackupError::InvalidFormat)?;
    if contents.len() as u64 > max_bytes {
        return Err(BackupError::Oversized);
    }
    Ok(contents)
}

fn write_files(directory: &Path, files: &BTreeMap<String, String>) -> Result<(), BackupError> {
    require_clean_absolute_path(directory)?;
    for (name, contents) in files {
        validate_file_name(name)?;
        write_private_atomic(directory, name, contents.as_bytes())
            .map_err(|_error| BackupError::Unavailable)?;
    }
    Ok(())
}

fn remove_current_store_files(directory: &Path) -> Result<(), BackupError> {
    require_clean_absolute_path(directory)?;
    let iterator = match fs::read_dir(directory) {
        Ok(iterator) => iterator,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_error) => return Err(BackupError::Unavailable),
    };
    for entry in iterator {
        let path = entry.map_err(|_error| BackupError::Unavailable)?.path();
        let metadata = fs::symlink_metadata(&path).map_err(|_error| BackupError::Unavailable)?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(BackupError::UnsafePath);
        }
        let Some(name) = path.file_name().and_then(std::ffi::OsStr::to_str) else {
            return Err(BackupError::UnsafePath);
        };
        if is_restorable_store_file_name(name) {
            fs::remove_file(path).map_err(|_error| BackupError::Unavailable)?;
        }
    }
    Ok(())
}

fn backup_current_store(directory: &Path, backup_dir: &Path) -> Result<(), BackupError> {
    require_clean_absolute_path(directory)?;
    let paths = match fs::read_dir(directory) {
        Ok(iterator) => iterator
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_error| BackupError::Unavailable)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(_error) => return Err(BackupError::Unavailable),
    };
    for path in paths {
        let metadata = fs::symlink_metadata(&path).map_err(|_error| BackupError::Unavailable)?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(BackupError::UnsafePath);
        }
        let name = path
            .file_name()
            .and_then(std::ffi::OsStr::to_str)
            .ok_or(BackupError::UnsafePath)?;
        let bytes = read_bounded_binary_file(&path, MAX_BACKUP_TEXT_BYTES)?;
        write_private_atomic(backup_dir, name, &bytes)
            .map_err(|_error| BackupError::Unavailable)?;
    }
    if let Ok(text) = export_backup(directory) {
        write_private_atomic(backup_dir, "previous.manis.json", text.as_bytes())
            .map_err(|_error| BackupError::Unavailable)?;
    }
    Ok(())
}

fn create_backup_dir(directory: &Path) -> Result<PathBuf, BackupError> {
    let root = backup_root(directory)?;
    require_clean_absolute_path(&root)?;
    fs::create_dir_all(&root).map_err(|_error| BackupError::Unavailable)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700))
            .map_err(|_error| BackupError::Unavailable)?;
    }
    for _ in 0..80 {
        let candidate = root.join(format!("backup-{:x}", current_unix_nanos()));
        match fs::create_dir(&candidate) {
            Ok(()) => {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt as _;
                    fs::set_permissions(&candidate, fs::Permissions::from_mode(0o700))
                        .map_err(|_error| BackupError::Unavailable)?;
                }
                return Ok(candidate);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(_error) => return Err(BackupError::Unavailable),
        }
    }
    Err(BackupError::Unavailable)
}

fn write_external_file_atomic(path: &Path, bytes: &[u8]) -> Result<(), BackupError> {
    if !path.is_absolute() {
        return Err(BackupError::UnsafePath);
    }
    let parent = path.parent().ok_or(BackupError::UnsafePath)?;
    let name = path
        .file_name()
        .and_then(std::ffi::OsStr::to_str)
        .ok_or(BackupError::UnsafePath)?;
    let parent_metadata =
        fs::symlink_metadata(parent).map_err(|_error| BackupError::Unavailable)?;
    if parent_metadata.file_type().is_symlink() || !parent_metadata.is_dir() {
        return Err(BackupError::UnsafePath);
    }
    if let Ok(metadata) = fs::symlink_metadata(path)
        && (metadata.file_type().is_symlink() || !metadata.is_file())
    {
        return Err(BackupError::UnsafePath);
    }
    validate_clean_absolute_path(parent)?;
    let temporary = parent.join(format!(
        ".{name}.tmp-{}-{:x}",
        std::process::id(),
        current_unix_nanos()
    ));
    #[cfg(unix)]
    let mut file = {
        use std::os::unix::fs::OpenOptionsExt as _;
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)
            .map_err(|_error| BackupError::Unavailable)?
    };
    #[cfg(not(unix))]
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|_error| BackupError::Unavailable)?;
    let result = (|| -> Result<(), BackupError> {
        file.write_all(bytes)
            .map_err(|_error| BackupError::Unavailable)?;
        file.sync_all().map_err(|_error| BackupError::Unavailable)?;
        drop(file);
        fs::rename(&temporary, path).map_err(|_error| BackupError::Unavailable)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn validate_file_name(name: &str) -> Result<(), BackupError> {
    if is_portable_file_name(name) {
        Ok(())
    } else {
        Err(BackupError::InvalidFormat)
    }
}

fn is_portable_file_name(name: &str) -> bool {
    matches!(
        name,
        "subscription.url"
            | "routing-rule-group-order.state"
            | "workspace.state"
            | "routing.mode"
            | "node-selection.state"
            | "manual-routing-rules.state"
            | "direct-rules.state"
            | "kernel.kind"
            | "language.preference"
    ) || portable_prefixed_name(name, "source-", ".url")
        || portable_prefixed_name(name, "saved-", ".vless")
        || portable_prefixed_name(name, "qx-rule-", ".qxrules")
        || portable_prefixed_name(name, "policy-", ".policy")
        || portable_prefixed_name(name, "group-", ".group")
}

fn is_restorable_store_file_name(name: &str) -> bool {
    is_portable_file_name(name)
        || matches!(
            name,
            "benchmarks.state"
                | "manis-generated.yaml"
                | "manis-generated.candidate.yaml"
                | "manis-generated.json"
                | "manis-generated.candidate.json"
        )
}

fn portable_prefixed_name(name: &str, prefix: &str, suffix: &str) -> bool {
    let Some(id) = name.strip_suffix(suffix) else {
        return false;
    };
    id.strip_prefix(prefix).is_some_and(|tail| {
        !tail.is_empty()
            && tail
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() || byte == b'-')
    })
}

fn max_file_bytes(name: &str) -> Result<u64, BackupError> {
    validate_file_name(name)?;
    if portable_prefixed_name(name, "qx-rule-", ".qxrules") {
        Ok(2 * 1024 * 1024 + 64 * 1024)
    } else if matches!(name, "manual-routing-rules.state" | "direct-rules.state") {
        Ok(MAX_SMALL_STATE_BYTES)
    } else {
        Ok(MAX_PORTABLE_FILE_BYTES)
    }
}

fn require_clean_absolute_path(path: &Path) -> Result<(), BackupError> {
    if path.is_absolute()
        && path.components().all(|component| {
            matches!(
                component,
                std::path::Component::Prefix(_)
                    | std::path::Component::RootDir
                    | std::path::Component::Normal(_)
            )
        })
    {
        match fs::symlink_metadata(path) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                Err(BackupError::UnsafePath)
            }
            Ok(_) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(BackupError::Unavailable),
        }
    } else {
        Err(BackupError::UnsafePath)
    }
}

fn validate_clean_absolute_path(path: &Path) -> Result<(), BackupError> {
    require_clean_absolute_path(path)?;
    let canonical = fs::canonicalize(path).map_err(|_error| BackupError::Unavailable)?;
    require_clean_absolute_path(&canonical)
}

fn read_bounded_binary_file(path: &Path, max_bytes: u64) -> Result<Vec<u8>, BackupError> {
    let metadata = fs::symlink_metadata(path).map_err(|_error| BackupError::Unavailable)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(BackupError::UnsafePath);
    }
    let file = fs::File::open(path).map_err(|_error| BackupError::Unavailable)?;
    let opened_metadata = file.metadata().map_err(|_error| BackupError::Unavailable)?;
    if opened_metadata.len() > max_bytes {
        return Err(BackupError::Oversized);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;

        if metadata.dev() != opened_metadata.dev() || metadata.ino() != opened_metadata.ino() {
            return Err(BackupError::UnsafePath);
        }
    }
    read_binary_from_file(file, max_bytes)
}

fn read_binary_from_file(file: fs::File, max_bytes: u64) -> Result<Vec<u8>, BackupError> {
    let mut bytes = Vec::new();
    file.take(max_bytes + 1)
        .read_to_end(&mut bytes)
        .map_err(|_error| BackupError::Unavailable)?;
    if bytes.len() as u64 > max_bytes {
        return Err(BackupError::Oversized);
    }
    Ok(bytes)
}

fn current_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn current_unix_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
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
    fn oversized_files_and_symlinked_store_roots_are_rejected_before_changes()
    -> Result<(), Box<dyn std::error::Error>> {
        let store = temp_store("manis-backup-boundaries");
        crate::mihomo::save_routing_mode_in(&store, RoutingMode::Direct)?;
        let root = store.parent().expect("root");
        let oversized = root.join("oversized.manis.json");
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
        let previous = fs::read_to_string(result.backup_dir.join("previous.manis.json"))?;
        let previous = super::prepare_import(&previous)?;
        assert_eq!(previous.summary(), &super::BackupSummary::default());

        fs::remove_dir_all(target.parent().expect("target root"))?;
        Ok(())
    }
}
