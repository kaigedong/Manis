use std::{
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use crate::brand;
use crate::localization::{Language, copy};

use super::SystemProxyError;

const RECOVERY_FILE: &str = "system-proxy.recovery";
pub(super) const RECOVERY_VERSION: &str = "manis-system-proxy-v1";
pub(super) const LEGACY_RELAY_RECOVERY_VERSION: &str = "relay-system-proxy-v1";
const TUN_DNS_RECOVERY_FILE: &str = "tun-dns.recovery";
pub(super) const TUN_DNS_RECOVERY_VERSION: &str = "manis-tun-dns-v1";
const MAX_RECOVERY_BYTES: u64 = 1024 * 1024;

pub(super) fn recovery_version_supported(version: &str) -> bool {
    matches!(version, RECOVERY_VERSION | LEGACY_RELAY_RECOVERY_VERSION)
}

fn recovery_snapshot_path(language: Language) -> Result<PathBuf, SystemProxyError> {
    brand::data_dir()
        .map(|directory| directory.join(RECOVERY_FILE))
        .ok_or_else(|| {
            SystemProxyError::Unavailable(
                language
                    .localized(copy::system_proxy::COULD_NOT_DETERMINE_MANIS_DATA_DIRECTORY_FOR_SYSTEM_PROXY_RECOVERY)
                    .to_owned(),
            )
        })
}

fn tun_dns_recovery_snapshot_path(language: Language) -> Result<PathBuf, SystemProxyError> {
    brand::data_dir()
        .map(|directory| directory.join(TUN_DNS_RECOVERY_FILE))
        .ok_or_else(|| {
            SystemProxyError::Unavailable(
                language
                    .localized(copy::system_proxy::COULD_NOT_DETERMINE_MANIS_DATA_DIRECTORY_FOR_TUN_DNS_RECOVERY)
                    .to_owned(),
            )
        })
}

#[cfg(all(any(target_os = "macos", target_os = "linux"), not(test)))]
pub(super) fn read_tun_dns_recovery_snapshot(
    language: Language,
) -> Result<Option<String>, SystemProxyError> {
    let path = tun_dns_recovery_snapshot_path(language)?;
    read_recovery_snapshot_at(&path, language)
}

pub(super) fn write_tun_dns_recovery_snapshot(
    contents: &str,
    language: Language,
) -> Result<(), SystemProxyError> {
    let path = tun_dns_recovery_snapshot_path(language)?;
    write_recovery_snapshot_at(&path, contents, language)
}

pub(super) fn delete_tun_dns_recovery_snapshot(language: Language) -> Result<(), SystemProxyError> {
    let path = tun_dns_recovery_snapshot_path(language)?;
    delete_recovery_snapshot_at(&path, language)
}

#[cfg(any(
    target_os = "linux",
    target_os = "windows",
    all(target_os = "macos", not(test))
))]
pub(super) fn read_recovery_snapshot(
    language: Language,
) -> Result<Option<String>, SystemProxyError> {
    let path = recovery_snapshot_path(language)?;
    read_recovery_snapshot_at(&path, language)
}

pub(super) fn read_recovery_snapshot_at(
    path: &Path,
    language: Language,
) -> Result<Option<String>, SystemProxyError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(_error) => return Err(recovery_read_error(language)),
    };
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.len() > MAX_RECOVERY_BYTES
    {
        return Err(recovery_read_error(language));
    }
    #[cfg(unix)]
    if metadata.permissions().mode() & 0o077 != 0 {
        return Err(recovery_read_error(language));
    }
    let file = fs::File::open(path).map_err(|_error| recovery_read_error(language))?;
    let mut contents = String::new();
    file.take(MAX_RECOVERY_BYTES + 1)
        .read_to_string(&mut contents)
        .map_err(|_error| recovery_read_error(language))?;
    if contents.len() as u64 > MAX_RECOVERY_BYTES {
        return Err(recovery_read_error(language));
    }
    Ok(Some(contents))
}

fn recovery_read_error(language: Language) -> SystemProxyError {
    SystemProxyError::CommandFailed(
        language
            .localized(
                copy::system_proxy::COULD_NOT_SAFELY_READ_MANIS_SYSTEM_PROXY_RECOVERY_SNAPSHOT,
            )
            .to_owned(),
    )
}

pub(super) fn write_recovery_snapshot(
    contents: &str,
    language: Language,
) -> Result<(), SystemProxyError> {
    let path = recovery_snapshot_path(language)?;
    write_recovery_snapshot_at(&path, contents, language)
}

pub(super) fn write_recovery_snapshot_at(
    path: &Path,
    contents: &str,
    language: Language,
) -> Result<(), SystemProxyError> {
    let Some(directory) = path.parent() else {
        return Err(SystemProxyError::Unavailable(
            language
                .localized(
                    copy::system_proxy::COULD_NOT_DETERMINE_MANIS_SYSTEM_PROXY_RECOVERY_DIRECTORY,
                )
                .to_owned(),
        ));
    };
    prepare_recovery_directory(directory, language)?;

    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(RECOVERY_FILE);
    let temporary = directory.join(format!(".{file_name}.{}.tmp", std::process::id()));
    let _ = fs::remove_file(&temporary);
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|_error| {
            SystemProxyError::CommandFailed(
                language
                    .localized(
                        copy::system_proxy::COULD_NOT_CREATE_MANIS_SYSTEM_PROXY_RECOVERY_SNAPSHOT,
                    )
                    .to_owned(),
            )
        })?;
    #[cfg(unix)]
    file.set_permissions(fs::Permissions::from_mode(0o600))
        .map_err(|_error| {
            SystemProxyError::CommandFailed(
                language
                    .localized(
                        copy::system_proxy::COULD_NOT_PROTECT_MANIS_SYSTEM_PROXY_RECOVERY_SNAPSHOT,
                    )
                    .to_owned(),
            )
        })?;
    file.write_all(contents.as_bytes()).map_err(|_error| {
        SystemProxyError::CommandFailed(
            language
                .localized(copy::system_proxy::COULD_NOT_WRITE_MANIS_SYSTEM_PROXY_RECOVERY_SNAPSHOT)
                .to_owned(),
        )
    })?;
    file.sync_all().map_err(|_error| {
        SystemProxyError::CommandFailed(
            language
                .localized(copy::system_proxy::COULD_NOT_FLUSH_MANIS_SYSTEM_PROXY_RECOVERY_SNAPSHOT)
                .to_owned(),
        )
    })?;
    drop(file);
    fs::rename(&temporary, path).map_err(|_error| {
        let _ = fs::remove_file(&temporary);
        SystemProxyError::CommandFailed(
            language
                .localized(
                    copy::system_proxy::COULD_NOT_REPLACE_MANIS_SYSTEM_PROXY_RECOVERY_SNAPSHOT,
                )
                .to_owned(),
        )
    })
}

fn prepare_recovery_directory(
    directory: &Path,
    language: Language,
) -> Result<(), SystemProxyError> {
    match fs::symlink_metadata(directory) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            return Err(SystemProxyError::CommandFailed(
                language
                    .localized(copy::system_proxy::MANIS_SYSTEM_PROXY_RECOVERY_DIRECTORY_IS_UNSAFE)
                    .to_owned(),
            ));
        }
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            fs::create_dir_all(directory).map_err(|_error| {
                SystemProxyError::CommandFailed(
                    language
                        .localized(copy::system_proxy::COULD_NOT_CREATE_MANIS_SYSTEM_PROXY_RECOVERY_DIRECTORY)
                        .to_owned(),
                )
            })?;
        }
        Err(_error) => {
            return Err(SystemProxyError::CommandFailed(
                language
                    .localized(
                        copy::system_proxy::COULD_NOT_INSPECT_MANIS_SYSTEM_PROXY_RECOVERY_DIRECTORY,
                    )
                    .to_owned(),
            ));
        }
    }
    #[cfg(unix)]
    fs::set_permissions(directory, fs::Permissions::from_mode(0o700)).map_err(|_error| {
        SystemProxyError::CommandFailed(
            language
                .localized(
                    copy::system_proxy::COULD_NOT_PROTECT_MANIS_SYSTEM_PROXY_RECOVERY_DIRECTORY,
                )
                .to_owned(),
        )
    })?;
    Ok(())
}

pub(super) fn delete_recovery_snapshot(language: Language) -> Result<(), SystemProxyError> {
    let path = recovery_snapshot_path(language)?;
    delete_recovery_snapshot_at(&path, language)
}

pub(super) fn delete_recovery_snapshot_at(
    path: &Path,
    language: Language,
) -> Result<(), SystemProxyError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(_error) => Err(SystemProxyError::CommandFailed(
            language
                .localized(
                    copy::system_proxy::COULD_NOT_REMOVE_MANIS_SYSTEM_PROXY_RECOVERY_SNAPSHOT,
                )
                .to_owned(),
        )),
    }
}

pub(super) fn rollback_failed_message(language: Language) -> SystemProxyError {
    SystemProxyError::CommandFailed(
        language
            .localized(
                copy::system_proxy::COULD_NOT_APPLY_THE_SYSTEM_PROXY_OR_RESTORE_EVERY_PREVIOUS,
            )
            .to_owned(),
    )
}

pub(super) fn encode_string(value: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(value.len() * 2);
    for byte in value.as_bytes() {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

pub(super) fn decode_string(value: &str) -> Option<String> {
    if !value.len().is_multiple_of(2) {
        return None;
    }
    let mut bytes = Vec::with_capacity(value.len() / 2);
    for index in (0..value.len()).step_by(2) {
        bytes.push(u8::from_str_radix(&value[index..index + 2], 16).ok()?);
    }
    String::from_utf8(bytes).ok()
}
