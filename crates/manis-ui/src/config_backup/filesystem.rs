use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::{BACKUP_DIRECTORY_NAME, BackupError};

pub(super) fn backup_current_store(directory: &Path, backup_dir: &Path) -> Result<(), BackupError> {
    require_clean_absolute_path(directory)?;
    require_clean_absolute_path(backup_dir)?;
    let source = crate::config_toml::read_source(directory)?;
    manis_profile::write_private_atomic(backup_dir, "previous.toml", source.as_bytes())
        .map(|_| ())
        .map_err(|_| BackupError::Unavailable)
}

pub(super) fn create_backup_dir(directory: &Path) -> Result<PathBuf, BackupError> {
    let root = backup_root(directory);
    require_clean_absolute_path(&root)?;
    fs::create_dir_all(&root).map_err(|_| BackupError::Unavailable)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700))
            .map_err(|_| BackupError::Unavailable)?;
    }
    for _ in 0..80 {
        let candidate = root.join(format!("backup-{:x}", current_unix_nanos()));
        match fs::create_dir(&candidate) {
            Ok(()) => {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt as _;
                    fs::set_permissions(&candidate, fs::Permissions::from_mode(0o700))
                        .map_err(|_| BackupError::Unavailable)?;
                }
                return Ok(candidate);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(_) => return Err(BackupError::Unavailable),
        }
    }
    Err(BackupError::Unavailable)
}

pub(super) fn backup_root(configuration_directory: &Path) -> PathBuf {
    if crate::brand::config_dir().as_deref() == Some(configuration_directory) {
        crate::brand::data_dir().map_or_else(
            || configuration_directory.join(BACKUP_DIRECTORY_NAME),
            |directory| directory.join(BACKUP_DIRECTORY_NAME),
        )
    } else {
        configuration_directory.join(BACKUP_DIRECTORY_NAME)
    }
}

pub(super) fn require_clean_absolute_path(path: &Path) -> Result<(), BackupError> {
    if !path.is_absolute()
        || !path.components().all(|component| {
            matches!(
                component,
                Component::Prefix(_) | Component::RootDir | Component::Normal(_)
            )
        })
    {
        return Err(BackupError::UnsafePath);
    }
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(BackupError::UnsafePath),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(BackupError::Unavailable),
    }
}

pub(super) fn current_unix_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}
