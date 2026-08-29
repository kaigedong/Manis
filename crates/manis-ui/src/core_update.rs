use std::error::Error;
use std::fmt;
use std::fs;
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use flate2::read::GzDecoder;
use serde::Deserialize;
use sha2::{Digest as _, Sha256};
use ureq::{Agent, ResponseExt as _};
use zip::ZipArchive;

use crate::localization::{Language, LocalizedText, copy};

pub(crate) const LATEST_STABLE_RELEASE_API: &str =
    "https://api.github.com/repos/MetaCubeX/mihomo/releases/latest";
pub(crate) const MAX_CORE_PACKAGE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_CORE_BINARY_BYTES: u64 = 128 * 1024 * 1024;
const MAX_RELEASE_METADATA_BYTES: u64 = 1024 * 1024;
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(120);
const VERSION_PROBE_TIMEOUT: Duration = Duration::from_secs(5);
const VERSION_PROBE_POLL_INTERVAL: Duration = Duration::from_millis(20);
const MAX_VERSION_OUTPUT_BYTES: u64 = 64 * 1024;
const MAX_REDIRECTS: u32 = 5;
static STAGE_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Platform {
    MacosArm64,
    MacosX64,
    LinuxX64,
    WindowsX64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ArchiveKind {
    Gzip,
    Zip,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ReleaseAsset {
    pub(crate) version: String,
    pub(crate) name: String,
    pub(crate) download_url: String,
    pub(crate) archive: ArchiveKind,
    pub(crate) sha256: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CoreUpdateError {
    UnsupportedPlatform,
    DataDirUnavailable,
    NetworkUnavailable,
    InvalidReleaseMetadata,
    InsecureRedirect,
    MissingAsset,
    MissingDigest,
    InvalidDigest,
    DigestMismatch,
    PackageTooLarge,
    InvalidArchive,
    Io,
    VersionMismatch,
    PublishFailed,
    RollbackFailed,
}

impl fmt::Display for CoreUpdateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.localized_message(Language::English))
    }
}

impl CoreUpdateError {
    pub(crate) const fn localized_message(self, language: Language) -> &'static str {
        language.localized(self.message())
    }

    const fn message(self) -> LocalizedText {
        match self {
            Self::UnsupportedPlatform => copy::core_update::UNSUPPORTED_PLATFORM,
            Self::DataDirUnavailable => copy::core_update::DATA_DIR_UNAVAILABLE,
            Self::NetworkUnavailable => copy::core_update::NETWORK_UNAVAILABLE,
            Self::InvalidReleaseMetadata => copy::core_update::INVALID_RELEASE_METADATA,
            Self::InsecureRedirect => copy::core_update::INSECURE_REDIRECT,
            Self::MissingAsset => copy::core_update::MISSING_ASSET,
            Self::MissingDigest => copy::core_update::MISSING_DIGEST,
            Self::InvalidDigest => copy::core_update::INVALID_DIGEST,
            Self::DigestMismatch => copy::core_update::DIGEST_MISMATCH,
            Self::PackageTooLarge => copy::core_update::PACKAGE_TOO_LARGE,
            Self::InvalidArchive => copy::core_update::INVALID_ARCHIVE,
            Self::Io => copy::core_update::IO,
            Self::VersionMismatch => copy::core_update::VERSION_MISMATCH,
            Self::PublishFailed => copy::core_update::PUBLISH_FAILED,
            Self::RollbackFailed => copy::core_update::ROLLBACK_FAILED,
        }
    }
}

impl Error for CoreUpdateError {}

impl Platform {
    pub(crate) fn current() -> Result<Self, CoreUpdateError> {
        match (std::env::consts::OS, std::env::consts::ARCH) {
            ("macos", "aarch64") => Ok(Self::MacosArm64),
            ("macos", "x86_64") => Ok(Self::MacosX64),
            ("linux", "x86_64") => Ok(Self::LinuxX64),
            ("windows", "x86_64") => Ok(Self::WindowsX64),
            _ => Err(CoreUpdateError::UnsupportedPlatform),
        }
    }
}

#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    prerelease: bool,
    assets: Vec<GithubAsset>,
}

#[derive(Debug, Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
    digest: Option<String>,
}

pub(crate) fn fetch_current_release_asset() -> Result<ReleaseAsset, CoreUpdateError> {
    fetch_release_asset_for_platform(Platform::current()?)
}

pub(crate) fn fetch_release_asset_for_platform(
    platform: Platform,
) -> Result<ReleaseAsset, CoreUpdateError> {
    let metadata = download_text(LATEST_STABLE_RELEASE_API, MAX_RELEASE_METADATA_BYTES)?;
    select_release_asset(&metadata, platform)
}

pub(crate) fn download_asset_package(asset: &ReleaseAsset) -> Result<Vec<u8>, CoreUpdateError> {
    download_bytes(&asset.download_url, MAX_CORE_PACKAGE_BYTES)
        .and_then(|package| verify_asset_digest(asset, &package).map(|()| package))
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum SeedInstallOutcome {
    AlreadyPresent(PathBuf),
    Installed(PathBuf),
    MissingSeed { target: PathBuf },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct InstalledCore {
    pub(crate) path: PathBuf,
    pub(crate) version: String,
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

pub(crate) fn select_release_asset(
    release_json: &str,
    platform: Platform,
) -> Result<ReleaseAsset, CoreUpdateError> {
    let release: GithubRelease = serde_json::from_str(release_json)
        .map_err(|_error| CoreUpdateError::InvalidReleaseMetadata)?;
    if release.prerelease {
        return Err(CoreUpdateError::MissingAsset);
    }

    let selected = release
        .assets
        .iter()
        .filter_map(|asset| asset_priority(&asset.name, platform).map(|priority| (priority, asset)))
        .min_by_key(|(priority, _asset)| *priority)
        .map(|(_priority, asset)| asset)
        .ok_or(CoreUpdateError::MissingAsset)?;

    let digest = selected
        .digest
        .as_deref()
        .ok_or(CoreUpdateError::MissingDigest)
        .and_then(parse_sha256_digest)?;
    let archive =
        ArchiveKind::from_asset_name(&selected.name).ok_or(CoreUpdateError::MissingAsset)?;

    Ok(ReleaseAsset {
        version: release.tag_name,
        name: selected.name.clone(),
        download_url: selected.browser_download_url.clone(),
        archive,
        sha256: digest,
    })
}

pub(crate) fn verify_asset_digest(
    asset: &ReleaseAsset,
    package: &[u8],
) -> Result<(), CoreUpdateError> {
    if !is_valid_sha256_hex(&asset.sha256) {
        return Err(CoreUpdateError::InvalidDigest);
    }

    let mut hasher = Sha256::new();
    hasher.update(package);
    let actual = format!("{:x}", hasher.finalize());
    if actual.eq_ignore_ascii_case(&asset.sha256) {
        Ok(())
    } else {
        Err(CoreUpdateError::DigestMismatch)
    }
}

pub(crate) fn unpack_core_binary(
    archive: ArchiveKind,
    package: &[u8],
) -> Result<Vec<u8>, CoreUpdateError> {
    match archive {
        ArchiveKind::Gzip => {
            let mut decoder = GzDecoder::new(package);
            read_bounded_archive_entry(&mut decoder)
        }
        ArchiveKind::Zip => unpack_zip_core_binary(package),
    }
}

pub(crate) fn write_staged_core(target: &Path, binary: &[u8]) -> Result<PathBuf, CoreUpdateError> {
    let parent = target.parent().ok_or(CoreUpdateError::Io)?;
    fs::create_dir_all(parent).map_err(|_error| CoreUpdateError::Io)?;
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
                    .map_err(|_error| CoreUpdateError::Io)?;
                file.sync_all().map_err(|_error| CoreUpdateError::Io)?;
                make_executable(&staged)?;
                return Ok(staged);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(_error) => return Err(CoreUpdateError::Io),
        }
    }

    Err(CoreUpdateError::Io)
}

pub(crate) fn validate_binary_version(
    binary: &Path,
    expected_version: &str,
) -> Result<(), CoreUpdateError> {
    let reported = reported_binary_version_with_timeout(binary, VERSION_PROBE_TIMEOUT)?;
    let expected = format!("v{}", expected_version.trim_start_matches('v'));
    if reported == expected {
        Ok(())
    } else {
        Err(CoreUpdateError::VersionMismatch)
    }
}

fn reported_binary_version_with_timeout(
    binary: &Path,
    timeout: Duration,
) -> Result<String, CoreUpdateError> {
    let stdout_path = unique_sibling_path(binary, "version-stdout");
    let stderr_path = unique_sibling_path(binary, "version-stderr");
    let stdout = create_version_output_file(&stdout_path)?;
    let stderr = match create_version_output_file(&stderr_path) {
        Ok(file) => file,
        Err(error) => {
            remove_file_if_exists(&stdout_path);
            return Err(error);
        }
    };
    let result = (|| {
        let mut child = Command::new(binary)
            .arg("-v")
            .stdin(Stdio::null())
            .stdout(Stdio::from(stdout))
            .stderr(Stdio::from(stderr))
            .spawn()
            .map_err(|_error| CoreUpdateError::VersionMismatch)?;
        let started = Instant::now();
        let status = loop {
            match child
                .try_wait()
                .map_err(|_error| CoreUpdateError::VersionMismatch)?
            {
                Some(status) => break status,
                None if started.elapsed() < timeout => thread::sleep(VERSION_PROBE_POLL_INTERVAL),
                None => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(CoreUpdateError::VersionMismatch);
                }
            }
        };
        if !status.success() {
            return Err(CoreUpdateError::VersionMismatch);
        }
        let stdout = read_bounded_version_output(&stdout_path)?;
        let stderr = read_bounded_version_output(&stderr_path)?;
        parse_reported_version(&format!("{stdout} {stderr}"))
            .ok_or(CoreUpdateError::VersionMismatch)
    })();
    remove_file_if_exists(&stdout_path);
    remove_file_if_exists(&stderr_path);
    result
}

fn create_version_output_file(path: &Path) -> Result<fs::File, CoreUpdateError> {
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|_error| CoreUpdateError::VersionMismatch)
}

fn read_bounded_version_output(path: &Path) -> Result<String, CoreUpdateError> {
    let metadata = fs::metadata(path).map_err(|_error| CoreUpdateError::VersionMismatch)?;
    if metadata.len() > MAX_VERSION_OUTPUT_BYTES {
        return Err(CoreUpdateError::VersionMismatch);
    }
    fs::read_to_string(path).map_err(|_error| CoreUpdateError::VersionMismatch)
}

fn parse_reported_version(reported: &str) -> Option<String> {
    reported
        .split_whitespace()
        .find(|field| {
            let version = field.strip_prefix('v').unwrap_or(field);
            let mut parts = version.split('.');
            parts.next().is_some_and(|part| part.parse::<u64>().is_ok())
                && parts.next().is_some_and(|part| part.parse::<u64>().is_ok())
                && parts.next().is_some_and(|part| {
                    part.trim_end_matches(|character: char| !character.is_ascii_digit())
                        .parse::<u64>()
                        .is_ok()
                })
        })
        .map(|field| {
            let trimmed = field.trim_end_matches(|character: char| {
                !character.is_ascii_alphanumeric() && character != '.' && character != '-'
            });
            if trimmed.starts_with('v') {
                trimmed.to_owned()
            } else {
                format!("v{trimmed}")
            }
        })
}

pub(crate) fn publish_staged_core(
    target: &Path,
    staged: &Path,
    health_check: impl FnOnce() -> Result<(), CoreUpdateError>,
) -> Result<(), CoreUpdateError> {
    let backup = unique_sibling_path(target, "backup");
    let had_target = target.exists();
    if had_target {
        fs::hard_link(target, &backup).map_err(|_error| CoreUpdateError::PublishFailed)?;
    }

    if rename_replace(staged, target).is_err() {
        remove_file_if_exists(&backup);
        return Err(CoreUpdateError::PublishFailed);
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

impl ArchiveKind {
    fn from_asset_name(name: &str) -> Option<Self> {
        let extension = Path::new(name).extension()?.to_str()?;
        if extension.eq_ignore_ascii_case("gz") {
            Some(Self::Gzip)
        } else if extension.eq_ignore_ascii_case("zip") {
            Some(Self::Zip)
        } else {
            None
        }
    }
}

fn download_text(url: &str, max_bytes: u64) -> Result<String, CoreUpdateError> {
    let bytes = download_bytes(url, max_bytes)?;
    String::from_utf8(bytes).map_err(|_error| CoreUpdateError::InvalidReleaseMetadata)
}

fn download_bytes(url: &str, max_bytes: u64) -> Result<Vec<u8>, CoreUpdateError> {
    let config = Agent::config_builder()
        .https_only(true)
        .max_redirects(MAX_REDIRECTS)
        .timeout_global(Some(DOWNLOAD_TIMEOUT))
        .user_agent("Manis/0.1 Mihomo-Core-Updater")
        .build();
    let agent: Agent = config.into();
    let mut response = agent
        .get(url)
        .call()
        .map_err(|error| map_request_error(&error))?;
    if response.get_uri().scheme_str() != Some("https") {
        return Err(CoreUpdateError::InsecureRedirect);
    }
    response
        .body_mut()
        .with_config()
        .limit(max_bytes + 1)
        .read_to_vec()
        .map_err(|error| map_body_error(&error))
        .and_then(|bytes| {
            if bytes.len() as u64 > max_bytes {
                Err(CoreUpdateError::PackageTooLarge)
            } else {
                Ok(bytes)
            }
        })
}

fn map_request_error(error: &ureq::Error) -> CoreUpdateError {
    match error {
        ureq::Error::RequireHttpsOnly(_) => CoreUpdateError::InsecureRedirect,
        _ => CoreUpdateError::NetworkUnavailable,
    }
}

fn map_body_error(error: &ureq::Error) -> CoreUpdateError {
    match error {
        ureq::Error::BodyExceedsLimit(_) => CoreUpdateError::PackageTooLarge,
        _ => CoreUpdateError::NetworkUnavailable,
    }
}

#[cfg(test)]
fn read_limited_body(reader: &mut impl Read, max_bytes: u64) -> Result<Vec<u8>, CoreUpdateError> {
    let mut limited = reader.take(max_bytes + 1);
    let mut body = Vec::new();
    limited
        .read_to_end(&mut body)
        .map_err(|_error| CoreUpdateError::NetworkUnavailable)?;
    if body.len() as u64 > max_bytes {
        Err(CoreUpdateError::PackageTooLarge)
    } else {
        Ok(body)
    }
}

fn managed_core_path_in(data_dir: &Path) -> PathBuf {
    data_dir.join("core").join(core_binary_name())
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

    let binary = fs::read(seed).map_err(|_error| CoreUpdateError::Io)?;
    let staged = write_staged_core(target, &binary)?;
    publish_staged_core(target, &staged, || Ok(()))?;
    Ok(SeedInstallOutcome::Installed(target.to_owned()))
}

fn require_managed_core_file(path: &Path) -> Result<(), CoreUpdateError> {
    let metadata = fs::symlink_metadata(path).map_err(|_error| CoreUpdateError::Io)?;
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

#[cfg_attr(test, allow(dead_code))]
fn bundled_seed_path() -> Option<PathBuf> {
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

fn asset_priority(name: &str, platform: Platform) -> Option<usize> {
    let name = name.to_ascii_lowercase();
    if name.contains("alpha")
        || name.contains("compatible")
        || name.ends_with(".sha256")
        || ArchiveKind::from_asset_name(&name).is_none()
    {
        return None;
    }

    platform
        .preferred_asset_markers()
        .iter()
        .position(|marker| name.contains(marker))
}

impl Platform {
    fn preferred_asset_markers(self) -> &'static [&'static str] {
        match self {
            Self::MacosArm64 => &["mihomo-darwin-arm64-go122", "mihomo-darwin-arm64"],
            Self::MacosX64 => &["mihomo-darwin-amd64-v2-go122", "mihomo-darwin-amd64-v2"],
            Self::LinuxX64 => &["mihomo-linux-amd64-v2", "mihomo-linux-amd64"],
            Self::WindowsX64 => &["mihomo-windows-amd64-v2", "mihomo-windows-amd64"],
        }
    }
}

fn parse_sha256_digest(value: &str) -> Result<String, CoreUpdateError> {
    let digest = value
        .trim()
        .strip_prefix("sha256:")
        .ok_or(CoreUpdateError::InvalidDigest)?;
    if is_valid_sha256_hex(digest) {
        Ok(digest.to_ascii_lowercase())
    } else {
        Err(CoreUpdateError::InvalidDigest)
    }
}

fn is_valid_sha256_hex(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn unpack_zip_core_binary(package: &[u8]) -> Result<Vec<u8>, CoreUpdateError> {
    let reader = Cursor::new(package);
    let mut archive = ZipArchive::new(reader).map_err(|_error| CoreUpdateError::InvalidArchive)?;
    for index in 0..archive.len() {
        let mut file = archive
            .by_index(index)
            .map_err(|_error| CoreUpdateError::InvalidArchive)?;
        if !zip_entry_is_mihomo_binary(file.name()) {
            continue;
        }
        if file.size() > MAX_CORE_BINARY_BYTES {
            return Err(CoreUpdateError::PackageTooLarge);
        }
        return read_bounded_archive_entry(&mut file);
    }

    Err(CoreUpdateError::InvalidArchive)
}

fn read_bounded_archive_entry(reader: &mut impl Read) -> Result<Vec<u8>, CoreUpdateError> {
    let mut binary = Vec::new();
    reader
        .take(MAX_CORE_BINARY_BYTES + 1)
        .read_to_end(&mut binary)
        .map_err(|_error| CoreUpdateError::InvalidArchive)?;
    if binary.len() as u64 > MAX_CORE_BINARY_BYTES {
        Err(CoreUpdateError::PackageTooLarge)
    } else if binary.is_empty() {
        Err(CoreUpdateError::InvalidArchive)
    } else {
        Ok(binary)
    }
}

fn require_safe_core_parent(parent: &Path, target: &Path) -> Result<(), CoreUpdateError> {
    let metadata = fs::symlink_metadata(parent).map_err(|_error| CoreUpdateError::Io)?;
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
        .map_err(|_error| CoreUpdateError::Io)
}

#[cfg(not(unix))]
fn secure_core_directory(_path: &Path) -> Result<(), CoreUpdateError> {
    Ok(())
}

fn zip_entry_is_mihomo_binary(name: &str) -> bool {
    let Some(file_name) = Path::new(name).file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    matches!(
        file_name,
        "mihomo" | "mihomo.exe" | "verge-mihomo" | "verge-mihomo.exe"
    )
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
        .map_err(|_error| CoreUpdateError::Io)
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> Result<(), CoreUpdateError> {
    Ok(())
}

#[cfg(unix)]
fn rename_replace(from: &Path, to: &Path) -> std::io::Result<()> {
    fs::rename(from, to)
}

#[cfg(not(unix))]
fn rename_replace(from: &Path, to: &Path) -> std::io::Result<()> {
    if to.exists() {
        fs::remove_file(to)?;
    }
    fs::rename(from, to)
}

fn remove_file_if_exists(path: &Path) {
    if path.exists() {
        let _ = fs::remove_file(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn core_update_errors_have_language_specific_user_messages() {
        assert_eq!(
            CoreUpdateError::NetworkUnavailable.localized_message(Language::English),
            "The Mihomo release download failed; check the network and try again"
        );
        assert_eq!(
            CoreUpdateError::NetworkUnavailable.localized_message(Language::SimplifiedChinese),
            "Mihomo release 下载失败，请检查网络后重试"
        );
        assert_eq!(
            CoreUpdateError::PublishFailed.to_string(),
            "The Mihomo core update could not be published"
        );
    }
    use flate2::{Compression, write::GzEncoder};
    use std::fs;
    use std::io::{Cursor, Write};

    #[cfg(unix)]
    use std::os::unix::fs::{PermissionsExt as _, symlink};

    #[test]
    fn selects_platform_asset_with_sha256_digest() {
        let release = r#"{
            "tag_name": "v1.19.30",
            "prerelease": false,
            "assets": [
                {
                    "name": "mihomo-linux-amd64-v2-v1.19.30.gz",
                    "browser_download_url": "https://example.test/linux.gz",
                    "digest": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                },
                {
                    "name": "mihomo-darwin-arm64-v1.19.30.gz",
                    "browser_download_url": "https://example.test/darwin.gz",
                    "digest": "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
                }
            ]
        }"#;

        let asset = select_release_asset(release, Platform::MacosArm64).expect("select asset");

        assert_eq!(asset.version, "v1.19.30");
        assert_eq!(asset.name, "mihomo-darwin-arm64-v1.19.30.gz");
        assert_eq!(asset.archive, ArchiveKind::Gzip);
        assert_eq!(
            asset.sha256,
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
        );
    }

    #[test]
    fn rejects_release_asset_without_sha256_digest() {
        let release = r#"{
            "tag_name": "v1.19.30",
            "prerelease": false,
            "assets": [{
                "name": "mihomo-darwin-arm64-v1.19.30.gz",
                "browser_download_url": "https://example.test/darwin.gz"
            }]
        }"#;

        assert_eq!(
            select_release_asset(release, Platform::MacosArm64),
            Err(CoreUpdateError::MissingDigest)
        );
    }

    #[test]
    fn verifies_package_digest_exactly() {
        let package = b"fixture package";
        let asset = ReleaseAsset {
            version: "v1.19.30".to_string(),
            name: "mihomo-darwin-arm64-v1.19.30.gz".to_string(),
            download_url: "https://example.test/darwin.gz".to_string(),
            archive: ArchiveKind::Gzip,
            sha256: "6cbac7deb54ff07ea9f5220277632d66b740cc1c04e44b485471aad82aa51042".to_string(),
        };

        assert_eq!(verify_asset_digest(&asset, package), Ok(()));

        let mut wrong = asset.clone();
        wrong.sha256 =
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string();
        assert_eq!(
            verify_asset_digest(&wrong, package),
            Err(CoreUpdateError::DigestMismatch)
        );
    }

    #[test]
    fn reads_download_bodies_with_a_hard_limit() {
        let mut body = Cursor::new(vec![7_u8; 5]);

        assert_eq!(
            super::read_limited_body(&mut body, 4),
            Err(CoreUpdateError::PackageTooLarge)
        );
    }

    #[test]
    fn managed_core_lives_under_the_manis_data_dir() {
        let data_dir = PathBuf::from("/tmp/manis-data");

        assert_eq!(
            super::managed_core_path_in(&data_dir),
            data_dir.join("core").join(super::core_binary_name())
        );
    }

    #[test]
    fn seed_install_reports_missing_seed_without_failing() {
        let root = test_temp_dir("manis-core-missing-seed");
        let target = root.join("core").join(super::core_binary_name());

        assert_eq!(
            super::install_seed_if_missing_from(None, &target).expect("missing seed is not fatal"),
            SeedInstallOutcome::MissingSeed {
                target: target.clone()
            }
        );
        assert!(!target.exists());

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn seed_install_copies_seed_when_target_is_missing() {
        let root = test_temp_dir("manis-core-seed");
        let seed = root.join("seed-mihomo");
        let target = root.join("core").join(super::core_binary_name());
        fs::write(&seed, b"seed core").expect("write seed");

        assert_eq!(
            super::install_seed_if_missing_from(Some(&seed), &target).expect("install seed"),
            SeedInstallOutcome::Installed(target.clone())
        );
        assert_eq!(fs::read(&target).expect("read target"), b"seed core");

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    #[cfg(unix)]
    fn managed_core_rejects_a_symlink_target() {
        let root = test_temp_dir("manis-core-managed-symlink");
        let external = root.join("external-mihomo");
        let target = root.join("core").join(super::core_binary_name());
        fs::create_dir_all(target.parent().expect("target parent")).expect("create core dir");
        fs::write(&external, b"external").expect("write external core");
        symlink(&external, &target).expect("link managed core");

        assert_eq!(
            super::require_managed_core_file(&target),
            Err(CoreUpdateError::Io)
        );

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn unpacks_gzip_core_package() {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder
            .write_all(b"#!/bin/sh\n")
            .expect("write gzip fixture");
        let package = encoder.finish().expect("finish gzip fixture");

        assert_eq!(
            unpack_core_binary(ArchiveKind::Gzip, &package).expect("unpack gzip"),
            b"#!/bin/sh\n"
        );
    }

    #[test]
    fn unpacks_zip_core_package_by_mihomo_file_name() {
        let package = zip_package(&[
            ("README.txt", b"ignore me".as_slice()),
            ("mihomo", b"binary bytes".as_slice()),
        ]);

        assert_eq!(
            unpack_core_binary(ArchiveKind::Zip, &package).expect("unpack zip"),
            b"binary bytes"
        );
    }

    #[test]
    fn staged_core_paths_are_unique_and_executable() {
        let root = test_temp_dir("manis-core-stage");
        let target = root.join("mihomo");

        let first = write_staged_core(&target, b"first").expect("stage first");
        let second = write_staged_core(&target, b"second").expect("stage second");

        assert_ne!(first, second);
        assert_eq!(fs::read(&first).expect("read first"), b"first");
        assert_eq!(fs::read(&second).expect("read second"), b"second");
        #[cfg(unix)]
        assert_eq!(
            fs::metadata(&first).expect("metadata").permissions().mode() & 0o777,
            0o755
        );

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    #[cfg(unix)]
    fn validates_binary_version_from_v_output() {
        let root = test_temp_dir("manis-core-version");
        let binary = root.join("mihomo");
        fs::write(&binary, "#!/bin/sh\necho 'Mihomo Meta v1.19.30'\n").expect("write script");
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o755)).expect("chmod script");

        assert_eq!(validate_binary_version(&binary, "v1.19.30"), Ok(()));
        assert_eq!(
            validate_binary_version(&binary, "v1.19.3"),
            Err(CoreUpdateError::VersionMismatch)
        );
        assert_eq!(
            validate_binary_version(&binary, "v1.19.31"),
            Err(CoreUpdateError::VersionMismatch)
        );

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    #[cfg(unix)]
    fn version_probe_times_out_and_terminates_a_hung_binary() {
        let root = test_temp_dir("manis-core-version-timeout");
        let binary = root.join("mihomo");
        fs::write(&binary, "#!/bin/sh\nsleep 10\n").expect("write script");
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o755)).expect("chmod script");

        assert_eq!(
            super::reported_binary_version_with_timeout(&binary, Duration::from_millis(50)),
            Err(CoreUpdateError::VersionMismatch)
        );

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn parses_reported_mihomo_version() {
        assert_eq!(
            super::parse_reported_version("Mihomo Meta v1.19.30 linux amd64"),
            Some("v1.19.30".to_owned())
        );
        assert_eq!(
            super::parse_reported_version("mihomo development build"),
            None
        );
    }

    #[test]
    #[cfg(unix)]
    fn install_asset_package_verifies_unpacks_validates_and_publishes() {
        let root = test_temp_dir("manis-core-install-package");
        let target = root.join("mihomo");
        let package = gzip_package(b"#!/bin/sh\necho 'Mihomo Meta v1.19.30'\n");
        let asset = ReleaseAsset {
            version: "v1.19.30".to_string(),
            name: "mihomo-darwin-arm64-v1.19.30.gz".to_string(),
            download_url: "https://example.test/darwin.gz".to_string(),
            archive: ArchiveKind::Gzip,
            sha256: sha256_hex(&package),
        };

        let installed =
            install_asset_package(&target, &asset, &package, || Ok(())).expect("install package");

        assert_eq!(
            installed,
            InstalledCore {
                path: target.clone(),
                version: "v1.19.30".to_string()
            }
        );
        assert_eq!(validate_binary_version(&target, "v1.19.30"), Ok(()));

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn publish_rolls_back_when_health_check_fails() {
        let root = test_temp_dir("manis-core-rollback");
        let target = root.join("mihomo");
        fs::write(&target, b"old core").expect("write target");
        let staged = write_staged_core(&target, b"new core").expect("write staged");

        assert_eq!(
            publish_staged_core(&target, &staged, || Err(CoreUpdateError::PublishFailed)),
            Err(CoreUpdateError::PublishFailed)
        );

        assert_eq!(
            fs::read(&target).expect("read restored target"),
            b"old core"
        );
        assert!(!staged.exists());

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn publish_keeps_new_core_after_successful_health_check() {
        let root = test_temp_dir("manis-core-publish");
        let target = root.join("mihomo");
        fs::write(&target, b"old core").expect("write target");
        let staged = write_staged_core(&target, b"new core").expect("write staged");

        assert_eq!(publish_staged_core(&target, &staged, || Ok(())), Ok(()));

        assert_eq!(fs::read(&target).expect("read target"), b"new core");
        assert!(!staged.exists());

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    #[cfg(unix)]
    fn staged_core_refuses_to_follow_target_symlink_parentless_paths() {
        let root = test_temp_dir("manis-core-symlink");
        let real = root.join("real");
        let link = root.join("mihomo-link");
        fs::create_dir(&real).expect("create real");
        symlink(&real, &link).expect("create symlink");

        assert_eq!(
            write_staged_core(&link.join("mihomo"), b"binary"),
            Err(CoreUpdateError::Io)
        );
        fs::remove_dir_all(root).expect("cleanup");
    }

    fn zip_package(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut buffer = Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut buffer);
            let options = zip::write::SimpleFileOptions::default();
            for (name, content) in files {
                writer.start_file(*name, options).expect("start zip file");
                writer.write_all(content).expect("write zip file");
            }
            writer.finish().expect("finish zip");
        }
        buffer.into_inner()
    }

    fn gzip_package(content: &[u8]) -> Vec<u8> {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(content).expect("write gzip fixture");
        encoder.finish().expect("finish gzip fixture")
    }

    fn sha256_hex(content: &[u8]) -> String {
        format!("{:x}", sha2::Sha256::digest(content))
    }

    fn test_temp_dir(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("{name}-{}", std::process::id()));
        if path.exists() {
            fs::remove_dir_all(&path).expect("remove stale temp dir");
        }
        fs::create_dir(&path).expect("create temp dir");
        path
    }
}
