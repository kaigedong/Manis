use std::fs;
use std::path::Path;
use std::process::Command;

use super::AppUpdateError;

pub(super) fn install_package(package: &Path, version: &str) -> Result<(), AppUpdateError> {
    validate_package(package, version)?;
    let status = Command::new("/usr/bin/pkexec")
        .arg("/usr/bin/pacman")
        .args(["-U", "--noconfirm"])
        .arg(package)
        .status()
        .map_err(|_error| AppUpdateError::InstallFailed)?;
    if !status.success() {
        return if matches!(status.code(), Some(126) | Some(127)) {
            Err(AppUpdateError::PermissionDenied)
        } else {
            Err(AppUpdateError::InstallFailed)
        };
    }
    let output = Command::new("/usr/bin/manis")
        .arg("--version")
        .output()
        .map_err(|_error| AppUpdateError::InstallFailed)?;
    if output.status.success()
        && String::from_utf8_lossy(&output.stdout).trim() == format!("Manis {version}")
    {
        Ok(())
    } else {
        Err(AppUpdateError::InstallFailed)
    }
}

fn validate_package(package: &Path, version: &str) -> Result<(), AppUpdateError> {
    let metadata =
        fs::symlink_metadata(package).map_err(|_error| AppUpdateError::InvalidPackage)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(AppUpdateError::InvalidPackage);
    }
    let output = Command::new("/usr/bin/pacman")
        .arg("-Qip")
        .arg(package)
        .env("LC_ALL", "C")
        .output()
        .map_err(|_error| AppUpdateError::InvalidPackage)?;
    if !output.status.success() {
        return Err(AppUpdateError::InvalidPackage);
    }
    let metadata = String::from_utf8_lossy(&output.stdout);
    let expected_version = format!("{version}-1");
    if package_field(&metadata, "Name") == Some("manis")
        && package_field(&metadata, "Version") == Some(expected_version.as_str())
    {
        Ok(())
    } else {
        Err(AppUpdateError::InvalidPackage)
    }
}

fn package_field<'a>(metadata: &'a str, wanted: &str) -> Option<&'a str> {
    metadata.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        (name.trim() == wanted).then(|| value.trim())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pacman_metadata_fields_are_matched_exactly() {
        let metadata = "Name            : manis\nVersion         : 0.1.101-1\n";
        assert_eq!(package_field(metadata, "Name"), Some("manis"));
        assert_eq!(package_field(metadata, "Version"), Some("0.1.101-1"));
        assert_eq!(package_field(metadata, "Packager"), None);
    }
}
