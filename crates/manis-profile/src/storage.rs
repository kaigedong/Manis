use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write as _},
    path::{Component, Path, PathBuf},
};

use crate::WriteError;

/// Writes secret bytes with private permissions using a same-directory temporary file.
///
/// # Errors
/// Returns a path-safety or I/O error without including `bytes`.
pub fn write_private_atomic(
    runtime_dir: &Path,
    file_name: &str,
    bytes: &[u8],
) -> Result<PathBuf, WriteError> {
    let _guard = private_write_lock()?;
    write_private_atomic_inner(runtime_dir, file_name, bytes)
}

static PRIVATE_WRITE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn private_write_lock() -> Result<std::sync::MutexGuard<'static, ()>, WriteError> {
    PRIVATE_WRITE_LOCK.lock().map_err(|_| {
        WriteError::Io(io::Error::other(
            "private configuration write lock is poisoned",
        ))
    })
}

/// Replaces or deletes a private file only while its contents still match.
///
/// Returns `false` on a conflict without changing the file. Comparison and
/// replacement are serialized with other private writes in this process.
/// `None` represents an absent file, for both the expectation and replacement.
///
/// # Errors
/// Returns a path-safety or I/O error without exposing the file contents.
pub fn replace_private_if_unchanged(
    runtime_dir: &Path,
    file_name: &str,
    expected: Option<&[u8]>,
    replacement: Option<&[u8]>,
) -> Result<bool, WriteError> {
    let _guard = private_write_lock()?;
    let path = private_write_path(runtime_dir, file_name)?;
    let matches = match (fs::symlink_metadata(&path), expected) {
        (Ok(metadata), Some(expected)) if metadata.len() == expected.len() as u64 => {
            fs::read(&path).map_err(WriteError::Io)? == expected
        }
        (Err(error), None) if error.kind() == io::ErrorKind::NotFound => true,
        (Err(error), _) if error.kind() != io::ErrorKind::NotFound => {
            return Err(WriteError::Io(error));
        }
        _ => false,
    };
    if !matches {
        return Ok(false);
    }
    match replacement {
        Some(bytes) => {
            write_private_atomic_inner(runtime_dir, file_name, bytes)?;
        }
        None if expected.is_some() => {
            fs::remove_file(path).map_err(WriteError::Io)?;
            sync_runtime_dir(runtime_dir)?;
        }
        None => {}
    }
    Ok(true)
}

fn private_write_path(runtime_dir: &Path, file_name: &str) -> Result<PathBuf, WriteError> {
    if !runtime_dir.is_absolute()
        || !runtime_dir.components().all(|component| {
            matches!(
                component,
                Component::Prefix(_) | Component::RootDir | Component::Normal(_)
            )
        })
    {
        return Err(WriteError::InvalidRuntimePath);
    }
    if Path::new(file_name).components().count() != 1
        || !matches!(
            Path::new(file_name).components().next(),
            Some(Component::Normal(_))
        )
    {
        return Err(WriteError::InvalidFileName);
    }
    prepare_runtime_dir(runtime_dir)?;
    let final_path = runtime_dir.join(file_name);
    if let Ok(metadata) = fs::symlink_metadata(&final_path) {
        if metadata.file_type().is_symlink() {
            return Err(WriteError::FinalPathSymlink);
        }
        if !metadata.is_file() {
            return Err(WriteError::FinalPathNotFile);
        }
    }
    Ok(final_path)
}

fn write_private_atomic_inner(
    runtime_dir: &Path,
    file_name: &str,
    bytes: &[u8],
) -> Result<PathBuf, WriteError> {
    let final_path = private_write_path(runtime_dir, file_name)?;
    let (temp_path, mut temp_file) = create_private_temp(runtime_dir, file_name)?;
    let write_result = (|| -> Result<(), WriteError> {
        temp_file.write_all(bytes).map_err(WriteError::Io)?;
        temp_file.sync_all().map_err(WriteError::Io)?;
        drop(temp_file);
        replace_file(&temp_path, &final_path)?;
        harden_file(&final_path)?;
        sync_runtime_dir(runtime_dir)?;
        Ok(())
    })();
    if write_result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    write_result?;
    Ok(final_path)
}

fn prepare_runtime_dir(path: &Path) -> Result<(), WriteError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(WriteError::RuntimeDirSymlink);
        }
        Ok(metadata) if !metadata.is_dir() => return Err(WriteError::RuntimeDirNotDirectory),
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            fs::create_dir_all(path).map_err(WriteError::Io)?;
        }
        Err(error) => return Err(WriteError::Io(error)),
    }
    let metadata = fs::symlink_metadata(path).map_err(WriteError::Io)?;
    if metadata.file_type().is_symlink() {
        return Err(WriteError::RuntimeDirSymlink);
    }
    if !metadata.is_dir() {
        return Err(WriteError::RuntimeDirNotDirectory);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(WriteError::Io)?;
    }
    Ok(())
}

fn create_private_temp(runtime_dir: &Path, file_name: &str) -> Result<(PathBuf, File), WriteError> {
    for sequence in 0..64_u8 {
        let temp_path = runtime_dir.join(format!(
            ".{file_name}.{}.{}.tmp",
            std::process::id(),
            sequence
        ));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&temp_path) {
            Ok(file) => return Ok((temp_path, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(WriteError::Io(error)),
        }
    }
    Err(WriteError::Io(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "no private temporary profile name was available",
    )))
}

fn harden_file(path: &Path) -> Result<(), WriteError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(WriteError::Io)?;
    }
    Ok(())
}

#[cfg(not(windows))]
fn replace_file(source: &Path, destination: &Path) -> Result<(), WriteError> {
    fs::rename(source, destination).map_err(WriteError::Io)
}

#[cfg(windows)]
fn replace_file(source: &Path, destination: &Path) -> Result<(), WriteError> {
    match fs::rename(source, destination) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            fs::remove_file(destination).map_err(WriteError::Io)?;
            fs::rename(source, destination).map_err(WriteError::Io)
        }
        Err(error) => Err(WriteError::Io(error)),
    }
}

#[cfg(unix)]
fn sync_runtime_dir(path: &Path) -> Result<(), WriteError> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(WriteError::Io)
}

#[cfg(not(unix))]
fn sync_runtime_dir(_path: &Path) -> Result<(), WriteError> {
    Ok(())
}

#[cfg(test)]
mod conditional_write_tests {
    use super::*;

    #[test]
    fn conditional_writes_reject_later_saves_and_handle_create_delete() {
        let dir = std::env::temp_dir().join(format!(
            "manis-conditional-write-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        assert!(replace_private_if_unchanged(&dir, "state", None, Some(b"first")).unwrap());
        write_private_atomic(&dir, "state", b"newer").unwrap();
        assert!(!replace_private_if_unchanged(&dir, "state", Some(b"first"), None).unwrap());
        assert_eq!(fs::read(dir.join("state")).unwrap(), b"newer");
        assert!(replace_private_if_unchanged(&dir, "state", Some(b"newer"), None).unwrap());
        assert!(!dir.join("state").exists());
        assert!(
            !replace_private_if_unchanged(&dir, "state", Some(b"newer"), Some(b"wrong")).unwrap()
        );
        assert!(replace_private_if_unchanged(&dir, "state", None, Some(b"restored")).unwrap());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn conditional_writes_reject_unsafe_names() {
        let dir = std::env::temp_dir();
        assert!(matches!(
            replace_private_if_unchanged(&dir, "../state", None, Some(b"bytes")),
            Err(WriteError::InvalidFileName)
        ));
    }
}
