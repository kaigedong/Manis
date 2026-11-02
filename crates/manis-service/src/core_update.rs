//! Managed Mihomo core paths and the first-run seed install.

use std::error::Error;
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static STAGE_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Failure while locating or installing the managed Mihomo core.
#[derive(Debug)]
pub(crate) enum CoreUpdateError {
    DataDirUnavailable,
    Io,
    PublishFailed,
    RollbackFailed,
    Caused {
        operation: &'static str,
        source: Box<dyn Error + Send + Sync>,
    },
}

impl CoreUpdateError {
    fn caused(operation: &'static str, source: impl Error + Send + Sync + 'static) -> Self {
        Self::Caused {
            operation,
            source: Box::new(source),
        }
    }
}

impl std::fmt::Display for CoreUpdateError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Caused { operation, source } => {
                write!(formatter, "could not {operation}: {source}")
            }
            Self::DataDirUnavailable => {
                formatter.write_str("the Manis data directory is unavailable")
            }
            Self::Io => formatter.write_str("the managed Mihomo core is not a usable file"),
            Self::PublishFailed => {
                formatter.write_str("the staged Mihomo core could not be published")
            }
            Self::RollbackFailed => {
                formatter.write_str("the previous Mihomo core could not be restored")
            }
        }
    }
}

impl Error for CoreUpdateError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Caused { source, .. } => Some(source.as_ref()),
            Self::DataDirUnavailable | Self::Io | Self::PublishFailed | Self::RollbackFailed => {
                None
            }
        }
    }
}

/// What the first-run seed install found.
#[derive(Debug)]
pub(crate) enum SeedInstallOutcome {
    AlreadyPresent(PathBuf),
    Installed(PathBuf),
    MissingSeed { target: PathBuf },
}

pub(crate) fn install_bundled_seed_if_missing() -> Result<SeedInstallOutcome, CoreUpdateError> {
    let target = managed_core_path()?;
    install_seed_if_missing_from(bundled_seed_path().as_deref(), &target)
}

pub(crate) fn managed_core_path() -> Result<PathBuf, CoreUpdateError> {
    Ok(managed_core_path_in(
        &app_data_dir().ok_or(CoreUpdateError::DataDirUnavailable)?,
    ))
}

pub(crate) fn managed_core_binary_path() -> Result<PathBuf, CoreUpdateError> {
    let path = managed_core_path()?;
    require_managed_core_file(&path)?;
    Ok(path)
}

fn managed_core_path_in(data_dir: &Path) -> PathBuf {
    data_dir.join("core").join(core_binary_name())
}

fn app_data_dir() -> Option<PathBuf> {
    crate::brand::data_dir()
}

fn core_binary_name() -> &'static str {
    if cfg!(windows) {
        "mihomo.exe"
    } else {
        "mihomo"
    }
}

pub(crate) fn bundled_seed_path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let exe_dir = exe.parent()?;
    if cfg!(target_os = "macos") {
        Some(exe_dir.join("../Resources/mihomo/mihomo"))
    } else if cfg!(target_os = "linux") {
        Some(exe_dir.join("../lib/manis/mihomo"))
    } else if cfg!(target_os = "windows") {
        Some(exe_dir.join("mihomo.exe"))
    } else {
        None
    }
}

fn install_seed_if_missing_from(
    seed: Option<&Path>,
    target: &Path,
) -> Result<SeedInstallOutcome, CoreUpdateError> {
    if target.exists() {
        require_managed_core_file(target)?;
        return Ok(SeedInstallOutcome::AlreadyPresent(target.to_owned()));
    }

    let Some(seed) = seed.filter(|path| path.is_file()) else {
        return Ok(SeedInstallOutcome::MissingSeed {
            target: target.to_owned(),
        });
    };

    let binary =
        fs::read(seed).map_err(|error| CoreUpdateError::caused("read bundled core", error))?;
    let staged = write_staged_core(target, &binary)?;
    publish_staged_core(target, &staged, || Ok(()))?;
    Ok(SeedInstallOutcome::Installed(target.to_owned()))
}

fn write_staged_core(target: &Path, binary: &[u8]) -> Result<PathBuf, CoreUpdateError> {
    let parent = target.parent().ok_or(CoreUpdateError::Io)?;
    fs::create_dir_all(parent)
        .map_err(|error| CoreUpdateError::caused("create core directory", error))?;
    require_safe_core_parent(parent, target)?;
    secure_core_directory(parent)?;

    for _attempt in 0..16 {
        let staged = unique_sibling_path(target, "stage");
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&staged)
        {
            Ok(mut file) => {
                file.write_all(binary)
                    .map_err(|error| CoreUpdateError::caused("write staged core", error))?;
                file.sync_all()
                    .map_err(|error| CoreUpdateError::caused("sync staged core", error))?;
                make_executable(&staged)?;
                return Ok(staged);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => {
                return Err(CoreUpdateError::caused("create staged core", error));
            }
        }
    }

    Err(CoreUpdateError::Io)
}

fn publish_staged_core(
    target: &Path,
    staged: &Path,
    health_check: impl FnOnce() -> Result<(), CoreUpdateError>,
) -> Result<(), CoreUpdateError> {
    let backup = unique_sibling_path(target, "backup");
    let had_target = target.exists();
    if had_target {
        fs::hard_link(target, &backup)
            .map_err(|error| CoreUpdateError::caused("create core update backup", error))?;
    }

    if let Err(error) = rename_replace(staged, target) {
        remove_file_if_exists(&backup);
        return Err(CoreUpdateError::caused("publish staged core", error));
    }

    match health_check() {
        Ok(()) => {
            remove_file_if_exists(&backup);
            Ok(())
        }
        Err(error) => {
            let failed = unique_sibling_path(target, "failed");
            let removed_new = fs::rename(target, &failed)
                .or_else(|_rename_error| fs::remove_file(target))
                .is_ok();
            let restored_old = if had_target {
                fs::rename(&backup, target).is_ok()
            } else {
                true
            };
            remove_file_if_exists(&failed);
            if removed_new && restored_old {
                Err(error)
            } else {
                Err(CoreUpdateError::RollbackFailed)
            }
        }
    }
}

fn require_safe_core_parent(parent: &Path, target: &Path) -> Result<(), CoreUpdateError> {
    let metadata = fs::symlink_metadata(parent)
        .map_err(|error| CoreUpdateError::caused("inspect core directory", error))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(CoreUpdateError::Io);
    }
    if let Ok(metadata) = fs::symlink_metadata(target)
        && (metadata.file_type().is_symlink() || !metadata.is_file())
    {
        return Err(CoreUpdateError::Io);
    }
    Ok(())
}

#[cfg(unix)]
fn secure_core_directory(path: &Path) -> Result<(), CoreUpdateError> {
    use std::os::unix::fs::PermissionsExt as _;

    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .map_err(|error| CoreUpdateError::caused("secure core directory", error))
}

#[cfg(not(unix))]
fn secure_core_directory(_path: &Path) -> Result<(), CoreUpdateError> {
    Ok(())
}

fn unique_sibling_path(target: &Path, purpose: &str) -> PathBuf {
    let parent = target.parent().unwrap_or_else(|| Path::new("."));
    let file_name = target
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("mihomo");
    let counter = STAGE_COUNTER.fetch_add(1, Ordering::Relaxed);
    parent.join(format!(
        ".{file_name}.{purpose}.{}.{}",
        std::process::id(),
        counter
    ))
}

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<(), CoreUpdateError> {
    use std::os::unix::fs::PermissionsExt as _;

    fs::set_permissions(path, fs::Permissions::from_mode(0o755))
        .map_err(|error| CoreUpdateError::caused("make staged core executable", error))
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> Result<(), CoreUpdateError> {
    Ok(())
}

fn rename_replace(from: &Path, to: &Path) -> std::io::Result<()> {
    fs::rename(from, to)
}

fn remove_file_if_exists(path: &Path) {
    if path.exists() {
        let _ = fs::remove_file(path);
    }
}

fn require_managed_core_file(path: &Path) -> Result<(), CoreUpdateError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(CoreUpdateError::Io);
        }
        Err(error) => return Err(CoreUpdateError::caused("inspect managed core", error)),
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(CoreUpdateError::Io);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        if metadata.permissions().mode() & 0o111 == 0 {
            return Err(CoreUpdateError::Io);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{SeedInstallOutcome, install_seed_if_missing_from};

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("{name}-{}", std::process::id()));
        if root.exists() {
            fs::remove_dir_all(&root).expect("remove stale fixture");
        }
        root
    }

    #[test]
    fn seed_install_reports_a_missing_bundle_without_writing() {
        let root = temp_dir("manis-seed-missing");
        let target = root.join("core/mihomo");
        let outcome =
            install_seed_if_missing_from(None, &target).expect("missing seed is not an error");
        assert!(matches!(outcome, SeedInstallOutcome::MissingSeed { .. }));
        assert!(!target.exists());
    }

    #[test]
    fn seed_install_copies_and_publishes_the_bundle_once() {
        let root = temp_dir("manis-seed-install");
        fs::create_dir_all(&root).expect("create fixture");
        let seed = root.join("mihomo-bundle");
        fs::write(&seed, b"#!fixture").expect("write seed");
        let target = root.join("core/mihomo");

        let first = install_seed_if_missing_from(Some(&seed), &target).expect("install seed");
        assert!(matches!(first, SeedInstallOutcome::Installed(_)));
        assert_eq!(
            fs::read(&target).expect("read installed core"),
            b"#!fixture"
        );

        let second = install_seed_if_missing_from(Some(&seed), &target).expect("reuse seed");
        assert!(matches!(second, SeedInstallOutcome::AlreadyPresent(_)));

        fs::remove_dir_all(&root).expect("remove fixture");
    }
}
