use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use zip::ZipArchive;

use super::{AppUpdateError, unique_sibling};

const MAX_EXTRACTED_BYTES: u64 = 1024 * 1024 * 1024;

pub(super) fn prepare_bundle(
    root: &Path,
    archive: &Path,
    version: &str,
) -> Result<PathBuf, AppUpdateError> {
    preflight_zip(archive)?;
    let staged = root.join(format!("Manis-{version}.app"));
    if validate_bundle(&staged, version).is_ok() {
        return Ok(staged);
    }
    remove_dir_if_exists(&staged);
    let extraction_root = unique_sibling(&staged, "extract");
    fs::create_dir(&extraction_root).map_err(|_error| AppUpdateError::Io)?;
    let result = (|| {
        let status = Command::new("/usr/bin/ditto")
            .args(["-x", "-k"])
            .arg(archive)
            .arg(&extraction_root)
            .status()
            .map_err(|_error| AppUpdateError::InvalidPackage)?;
        if !status.success() {
            return Err(AppUpdateError::InvalidPackage);
        }
        let extracted = extraction_root.join("Manis.app");
        validate_bundle(&extracted, version)?;
        fs::rename(&extracted, &staged).map_err(|_error| AppUpdateError::Io)?;
        Ok(staged.clone())
    })();
    remove_dir_if_exists(&extraction_root);
    result
}

pub(super) fn install_bundle(
    staged: &Path,
    current: &Path,
    version: &str,
) -> Result<(), AppUpdateError> {
    validate_bundle(staged, version)?;
    let current_metadata =
        fs::symlink_metadata(current).map_err(|_error| AppUpdateError::UnsupportedInstallation)?;
    if current_metadata.file_type().is_symlink() || !current_metadata.is_dir() {
        return Err(AppUpdateError::UnsupportedInstallation);
    }
    let incoming = unique_sibling(current, "incoming");
    let backup = unique_sibling(current, "previous");
    remove_dir_if_exists(&incoming);
    if fs::symlink_metadata(&backup).is_ok() {
        return Err(AppUpdateError::InstallFailed);
    }
    fs::create_dir(&incoming).map_err(|_error| AppUpdateError::PermissionDenied)?;
    let mut source = OsString::from(staged.as_os_str());
    source.push("/");
    let copied = Command::new("/usr/bin/rsync")
        .args(["-a", "--delete", "--exclude", "Icon?"])
        .arg(source)
        .arg(&incoming)
        .status()
        .map_err(|_error| AppUpdateError::InstallFailed)?;
    if !copied.success() || validate_bundle(&incoming, version).is_err() {
        remove_dir_if_exists(&incoming);
        return Err(AppUpdateError::InstallFailed);
    }
    fs::rename(current, &backup).map_err(|_error| {
        remove_dir_if_exists(&incoming);
        AppUpdateError::PermissionDenied
    })?;
    if fs::rename(&incoming, current).is_err() {
        let _ = fs::rename(&backup, current);
        remove_dir_if_exists(&incoming);
        return Err(AppUpdateError::InstallFailed);
    }
    remove_dir_if_exists(&backup);
    Ok(())
}

fn preflight_zip(path: &Path) -> Result<(), AppUpdateError> {
    let file = fs::File::open(path).map_err(|_error| AppUpdateError::Io)?;
    let mut archive = ZipArchive::new(file).map_err(|_error| AppUpdateError::InvalidPackage)?;
    let mut total = 0_u64;
    let mut found_binary = false;
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|_error| AppUpdateError::InvalidPackage)?;
        let enclosed = entry
            .enclosed_name()
            .ok_or(AppUpdateError::InvalidPackage)?;
        if entry
            .unix_mode()
            .is_some_and(|mode| mode & 0o170_000 == 0o120_000)
        {
            return Err(AppUpdateError::InvalidPackage);
        }
        total = total
            .checked_add(entry.size())
            .ok_or(AppUpdateError::PackageTooLarge)?;
        if total > MAX_EXTRACTED_BYTES {
            return Err(AppUpdateError::PackageTooLarge);
        }
        found_binary |= enclosed == Path::new("Manis.app/Contents/MacOS/Manis");
    }
    if found_binary {
        Ok(())
    } else {
        Err(AppUpdateError::InvalidPackage)
    }
}

fn validate_bundle(bundle: &Path, version: &str) -> Result<(), AppUpdateError> {
    let metadata = fs::symlink_metadata(bundle).map_err(|_error| AppUpdateError::InvalidPackage)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(AppUpdateError::InvalidPackage);
    }
    let binary = bundle.join("Contents/MacOS/Manis");
    let output = Command::new(&binary)
        .arg("--version")
        .output()
        .map_err(|_error| AppUpdateError::InvalidPackage)?;
    if !output.status.success()
        || String::from_utf8_lossy(&output.stdout).trim() != format!("Manis {version}")
    {
        return Err(AppUpdateError::InvalidPackage);
    }
    let plist_version = Command::new("/usr/bin/plutil")
        .args(["-extract", "CFBundleShortVersionString", "raw", "-o", "-"])
        .arg(bundle.join("Contents/Info.plist"))
        .output()
        .map_err(|_error| AppUpdateError::InvalidPackage)?;
    if !plist_version.status.success()
        || String::from_utf8_lossy(&plist_version.stdout).trim() != version
    {
        return Err(AppUpdateError::InvalidPackage);
    }
    let status = Command::new("/usr/bin/codesign")
        .args(["--verify", "--deep", "--strict"])
        .arg(bundle)
        .status()
        .map_err(|_error| AppUpdateError::InvalidPackage)?;
    status
        .success()
        .then_some(())
        .ok_or(AppUpdateError::InvalidPackage)
}

fn remove_dir_if_exists(path: &Path) {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return;
    };
    if metadata.file_type().is_symlink() {
        let _ = fs::remove_file(path);
    } else if metadata.is_dir() {
        let _ = fs::remove_dir_all(path);
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;

    use super::*;

    fn zip_with_entry(name: &str) -> PathBuf {
        let path = unique_sibling(&std::env::temp_dir().join("manis-update.zip"), "test");
        let file = fs::File::create(&path).expect("create zip fixture");
        let mut writer = zip::ZipWriter::new(file);
        writer
            .start_file(name, zip::write::SimpleFileOptions::default())
            .expect("start zip entry");
        writer.write_all(b"fixture").expect("write zip entry");
        writer.finish().expect("finish zip fixture");
        path
    }

    #[test]
    fn zip_preflight_requires_the_expected_bundle_binary() {
        let valid = zip_with_entry("Manis.app/Contents/MacOS/Manis");
        assert_eq!(preflight_zip(&valid), Ok(()));
        fs::remove_file(valid).expect("remove valid zip fixture");

        let unrelated = zip_with_entry("Manis.app/Contents/Info.plist");
        assert_eq!(
            preflight_zip(&unrelated),
            Err(AppUpdateError::InvalidPackage)
        );
        fs::remove_file(unrelated).expect("remove unrelated zip fixture");
    }

    #[test]
    fn zip_preflight_rejects_parent_traversal() {
        let path = zip_with_entry("../Manis.app/Contents/MacOS/Manis");
        assert_eq!(preflight_zip(&path), Err(AppUpdateError::InvalidPackage));
        fs::remove_file(path).expect("remove unsafe zip fixture");
    }
}
