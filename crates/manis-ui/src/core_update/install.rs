use super::{
    CoreUpdateError, CoreUpdateFailureKind, InstalledCore, Ordering, Path, PathBuf, ReleaseAsset,
    STAGE_COUNTER, SeedInstallOutcome, download_asset_package, fetch_current_release_asset, fs,
    unpack_core_binary, validate_binary_version, verify_asset_digest,
};
use std::io::Write as _;

pub(crate) fn write_staged_core(target: &Path, binary: &[u8]) -> Result<PathBuf, CoreUpdateError> {
    let parent = target.parent().ok_or(CoreUpdateError::Io)?;
    fs::create_dir_all(parent).map_err(|error| {
        CoreUpdateError::caused(CoreUpdateFailureKind::Io, "create core directory", error)
    })?;
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
                file.write_all(binary).map_err(|error| {
                    CoreUpdateError::caused(CoreUpdateFailureKind::Io, "write staged core", error)
                })?;
                file.sync_all().map_err(|error| {
                    CoreUpdateError::caused(CoreUpdateFailureKind::Io, "sync staged core", error)
                })?;
                make_executable(&staged)?;
                return Ok(staged);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => {
                return Err(CoreUpdateError::caused(
                    CoreUpdateFailureKind::Io,
                    "create staged core",
                    error,
                ));
            }
        }
    }

    Err(CoreUpdateError::Io)
}

pub(super) fn require_safe_core_parent(
    parent: &Path,
    target: &Path,
) -> Result<(), CoreUpdateError> {
    let metadata = fs::symlink_metadata(parent).map_err(|error| {
        CoreUpdateError::caused(CoreUpdateFailureKind::Io, "inspect core directory", error)
    })?;
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
pub(super) fn secure_core_directory(path: &Path) -> Result<(), CoreUpdateError> {
    use std::os::unix::fs::PermissionsExt as _;

    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|error| {
        CoreUpdateError::caused(CoreUpdateFailureKind::Io, "secure core directory", error)
    })
}

#[cfg(not(unix))]
pub(super) fn secure_core_directory(_path: &Path) -> Result<(), CoreUpdateError> {
    Ok(())
}

pub(super) fn zip_entry_is_mihomo_binary(name: &str) -> bool {
    let Some(file_name) = Path::new(name).file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    matches!(
        file_name,
        "mihomo" | "mihomo.exe" | "verge-mihomo" | "verge-mihomo.exe"
    )
}

pub(super) fn unique_sibling_path(target: &Path, purpose: &str) -> PathBuf {
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
pub(super) fn make_executable(path: &Path) -> Result<(), CoreUpdateError> {
    use std::os::unix::fs::PermissionsExt as _;

    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).map_err(|error| {
        CoreUpdateError::caused(
            CoreUpdateFailureKind::Io,
            "make staged core executable",
            error,
        )
    })
}

#[cfg(not(unix))]
pub(super) fn make_executable(_path: &Path) -> Result<(), CoreUpdateError> {
    Ok(())
}

#[cfg(unix)]
pub(super) fn rename_replace(from: &Path, to: &Path) -> std::io::Result<()> {
    fs::rename(from, to)
}

#[cfg(not(unix))]
pub(super) fn rename_replace(from: &Path, to: &Path) -> std::io::Result<()> {
    fs::rename(from, to)
}

pub(super) fn remove_file_if_exists(path: &Path) {
    if path.exists() {
        let _ = fs::remove_file(path);
    }
}

pub(super) fn managed_core_path_in(data_dir: &Path) -> PathBuf {
    data_dir.join("core").join(core_binary_name())
}

pub(super) fn install_seed_if_missing_from(
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

    let binary = fs::read(seed).map_err(|error| {
        CoreUpdateError::caused(CoreUpdateFailureKind::Io, "read bundled core", error)
    })?;
    let staged = write_staged_core(target, &binary)?;
    publish_staged_core(target, &staged, || Ok(()))?;
    Ok(SeedInstallOutcome::Installed(target.to_owned()))
}

pub(super) fn require_managed_core_file(path: &Path) -> Result<(), CoreUpdateError> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        CoreUpdateError::caused(CoreUpdateFailureKind::Io, "inspect managed core", error)
    })?;
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

pub(super) fn app_data_dir() -> Option<PathBuf> {
    crate::brand::data_dir()
}

pub(super) fn core_binary_name() -> &'static str {
    if cfg!(windows) {
        "mihomo.exe"
    } else {
        "mihomo"
    }
}

#[cfg_attr(test, allow(dead_code))]
pub(crate) fn bundled_seed_path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let exe_dir = exe.parent()?;
    if cfg!(target_os = "macos") {
        Some(exe_dir.join("../Resources/mihomo/mihomo"))
    } else if cfg!(target_os = "linux") {
        Some(exe_dir.join("../lib/manis/mihomo"))
    } else if cfg!(windows) {
        Some(exe_dir.join("mihomo.exe"))
    } else {
        None
    }
}

pub(crate) fn publish_staged_core(
    target: &Path,
    staged: &Path,
    health_check: impl FnOnce() -> Result<(), CoreUpdateError>,
) -> Result<(), CoreUpdateError> {
    let backup = unique_sibling_path(target, "backup");
    let had_target = target.exists();
    if had_target {
        fs::hard_link(target, &backup).map_err(|error| {
            CoreUpdateError::caused(
                CoreUpdateFailureKind::PublishFailed,
                "create core update backup",
                error,
            )
        })?;
    }

    if let Err(error) = rename_replace(staged, target) {
        remove_file_if_exists(&backup);
        return Err(CoreUpdateError::caused(
            CoreUpdateFailureKind::PublishFailed,
            "publish staged core",
            error,
        ));
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

#[cfg_attr(test, allow(dead_code))]
pub(crate) fn install_bundled_seed_if_missing() -> Result<SeedInstallOutcome, CoreUpdateError> {
    let target = managed_core_path()?;
    install_seed_if_missing_from(bundled_seed_path().as_deref(), &target)
}

pub(crate) fn install_latest_core_update(
    health_check: impl FnOnce() -> Result<(), CoreUpdateError>,
) -> Result<InstalledCore, CoreUpdateError> {
    let asset = fetch_current_release_asset()?;
    let package = download_asset_package(&asset)?;
    install_asset_package(&managed_core_path()?, &asset, &package, health_check)
}

pub(crate) fn install_asset_package(
    target: &Path,
    asset: &ReleaseAsset,
    package: &[u8],
    health_check: impl FnOnce() -> Result<(), CoreUpdateError>,
) -> Result<InstalledCore, CoreUpdateError> {
    verify_asset_digest(asset, package)?;
    let binary = unpack_core_binary(asset.archive, package)?;
    let staged = write_staged_core(target, &binary)?;
    if let Err(error) = validate_binary_version(&staged, &asset.version) {
        remove_file_if_exists(&staged);
        return Err(error);
    }
    if let Err(error) = publish_staged_core(target, &staged, health_check) {
        remove_file_if_exists(&staged);
        return Err(error);
    }

    Ok(InstalledCore {
        path: target.to_owned(),
        version: asset.version.clone(),
    })
}
