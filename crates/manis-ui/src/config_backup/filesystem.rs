use std::collections::BTreeMap;
use std::fs;
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use manis_profile::write_private_atomic;

use super::{
    BackupError, MAX_BACKUP_TEXT_BYTES, MAX_PORTABLE_FILE_BYTES, MAX_SMALL_STATE_BYTES,
    backup_root, export_backup,
};

pub(super) fn portable_store_paths(directory: &Path) -> Result<Vec<PathBuf>, BackupError> {
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

pub(super) fn read_private_portable_file(
    path: &Path,
    max_bytes: u64,
) -> Result<String, BackupError> {
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

pub(super) fn read_bounded_text_file(path: &Path, max_bytes: u64) -> Result<String, BackupError> {
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

pub(super) fn read_from_file(file: fs::File, max_bytes: u64) -> Result<String, BackupError> {
    let mut contents = String::new();
    file.take(max_bytes + 1)
        .read_to_string(&mut contents)
        .map_err(|_error| BackupError::InvalidFormat)?;
    if contents.len() as u64 > max_bytes {
        return Err(BackupError::Oversized);
    }
    Ok(contents)
}

pub(super) fn write_files(
    directory: &Path,
    files: &BTreeMap<String, String>,
) -> Result<(), BackupError> {
    require_clean_absolute_path(directory)?;
    for (name, contents) in files {
        validate_file_name(name)?;
        write_private_atomic(directory, name, contents.as_bytes())
            .map_err(|_error| BackupError::Unavailable)?;
    }
    Ok(())
}

pub(super) fn remove_current_store_files(directory: &Path) -> Result<(), BackupError> {
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

pub(super) fn backup_current_store(directory: &Path, backup_dir: &Path) -> Result<(), BackupError> {
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
        write_private_atomic(backup_dir, "previous.json", text.as_bytes())
            .map_err(|_error| BackupError::Unavailable)?;
    }
    Ok(())
}

pub(super) fn create_backup_dir(directory: &Path) -> Result<PathBuf, BackupError> {
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

pub(super) fn write_authorized_external_file(path: &Path, bytes: &[u8]) -> Result<(), BackupError> {
    if !path.is_absolute() {
        return Err(BackupError::UnsafePath);
    }
    let parent = path.parent().ok_or(BackupError::UnsafePath)?;
    path.file_name()
        .and_then(std::ffi::OsStr::to_str)
        .ok_or(BackupError::UnsafePath)?;
    let parent_metadata =
        fs::symlink_metadata(parent).map_err(|error| map_external_write_error(&error))?;
    if parent_metadata.file_type().is_symlink() || !parent_metadata.is_dir() {
        return Err(BackupError::UnsafePath);
    }
    if let Ok(metadata) = fs::symlink_metadata(path)
        && (metadata.file_type().is_symlink() || !metadata.is_file())
    {
        return Err(BackupError::UnsafePath);
    }
    validate_clean_absolute_path(parent)?;
    #[cfg(unix)]
    let mut file = {
        use std::os::unix::fs::OpenOptionsExt as _;
        fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(path)
            .map_err(|error| map_external_write_error(&error))?
    };
    #[cfg(not(unix))]
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .map_err(|error| map_external_write_error(&error))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        file.set_permissions(fs::Permissions::from_mode(0o600))
            .map_err(|error| map_external_write_error(&error))?;
    }
    file.set_len(0)
        .map_err(|error| map_external_write_error(&error))?;
    file.write_all(bytes)
        .map_err(|error| map_external_write_error(&error))?;
    file.sync_all()
        .map_err(|error| map_external_write_error(&error))
}

fn map_external_write_error(error: &std::io::Error) -> BackupError {
    if error.kind() == std::io::ErrorKind::PermissionDenied {
        BackupError::PermissionDenied
    } else {
        BackupError::Unavailable
    }
}

pub(super) fn validate_file_name(name: &str) -> Result<(), BackupError> {
    if is_portable_file_name(name) {
        Ok(())
    } else {
        Err(BackupError::InvalidFormat)
    }
}

pub(super) fn is_portable_file_name(name: &str) -> bool {
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

pub(super) fn is_restorable_store_file_name(name: &str) -> bool {
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

pub(super) fn portable_prefixed_name(name: &str, prefix: &str, suffix: &str) -> bool {
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

pub(super) fn max_file_bytes(name: &str) -> Result<u64, BackupError> {
    validate_file_name(name)?;
    if portable_prefixed_name(name, "qx-rule-", ".qxrules") {
        Ok(2 * 1024 * 1024 + 64 * 1024)
    } else if matches!(name, "manual-routing-rules.state" | "direct-rules.state") {
        Ok(MAX_SMALL_STATE_BYTES)
    } else {
        Ok(MAX_PORTABLE_FILE_BYTES)
    }
}

pub(super) fn require_clean_absolute_path(path: &Path) -> Result<(), BackupError> {
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

pub(super) fn validate_clean_absolute_path(path: &Path) -> Result<(), BackupError> {
    require_clean_absolute_path(path)?;
    let canonical = fs::canonicalize(path).map_err(|_error| BackupError::Unavailable)?;
    require_clean_absolute_path(&canonical)
}

pub(super) fn read_bounded_binary_file(
    path: &Path,
    max_bytes: u64,
) -> Result<Vec<u8>, BackupError> {
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

pub(super) fn read_binary_from_file(
    file: fs::File,
    max_bytes: u64,
) -> Result<Vec<u8>, BackupError> {
    let mut bytes = Vec::new();
    file.take(max_bytes + 1)
        .read_to_end(&mut bytes)
        .map_err(|_error| BackupError::Unavailable)?;
    if bytes.len() as u64 > max_bytes {
        return Err(BackupError::Oversized);
    }
    Ok(bytes)
}

pub(super) fn current_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub(super) fn current_unix_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}
