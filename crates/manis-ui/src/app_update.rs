use std::error::Error;
use std::fmt;
use std::fs;
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde::Deserialize;
use sha2::{Digest as _, Sha256};
use ureq::{Agent, ResponseExt as _};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;

const RELEASE_API: &str = "https://api.github.com/repos/kaigedong/Manis/releases/tags/latest";
const MANIFEST_NAME: &str = "manis-update.json";
const MANIFEST_SCHEMA_VERSION: u32 = 1;
const MAX_RELEASE_METADATA_BYTES: u64 = 1024 * 1024;
const MAX_MANIFEST_BYTES: u64 = 256 * 1024;
const MAX_PACKAGE_BYTES: u64 = 512 * 1024 * 1024;
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(180);
const MAX_REDIRECTS: u32 = 5;
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct Version([u64; 3]);

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AvailableUpdate {
    pub(crate) version: String,
    asset: ManifestAsset,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct StagedUpdate {
    pub(crate) version: String,
    payload: PathBuf,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AppUpdateError {
    UnsupportedInstallation,
    DataDirUnavailable,
    NetworkUnavailable,
    InvalidMetadata,
    InsecureRedirect,
    MissingAsset,
    InvalidDigest,
    DigestMismatch,
    PackageTooLarge,
    InvalidPackage,
    PermissionDenied,
    InstallFailed,
    Io,
}

impl fmt::Display for AppUpdateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::UnsupportedInstallation => "this installation cannot be updated automatically",
            Self::DataDirUnavailable => "the Manis data directory is unavailable",
            Self::NetworkUnavailable => "the update service is unavailable",
            Self::InvalidMetadata => "the update metadata is invalid",
            Self::InsecureRedirect => "the update download left HTTPS",
            Self::MissingAsset => "no update is available for this platform",
            Self::InvalidDigest => "the update checksum is invalid",
            Self::DigestMismatch => "the downloaded update failed verification",
            Self::PackageTooLarge => "the update package exceeds the safety limit",
            Self::InvalidPackage => "the update package is invalid",
            Self::PermissionDenied => "administrator authorization was cancelled or denied",
            Self::InstallFailed => "the operating system could not install the update",
            Self::Io => "the update could not be staged on this device",
        })
    }
}

impl Error for AppUpdateError {}

#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    target_commitish: String,
    draft: bool,
    prerelease: bool,
    assets: Vec<GithubAsset>,
}

#[derive(Debug, Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
    digest: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UpdateManifest {
    schema_version: u32,
    version: String,
    commit: String,
    assets: Vec<ManifestAsset>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
struct ManifestAsset {
    platform: String,
    architecture: String,
    name: String,
    sha256: String,
    size: u64,
}

pub(crate) const fn current_version() -> &'static str {
    match option_env!("MANIS_BUILD_VERSION") {
        Some(version) => version,
        None => env!("CARGO_PKG_VERSION"),
    }
}

pub(crate) fn installation_supported(app_path: &Path) -> bool {
    if cfg!(target_os = "macos") {
        app_path
            .extension()
            .is_some_and(|extension| extension == "app")
            && app_path.file_name().is_some_and(|name| name == "Manis.app")
    } else if cfg!(target_os = "linux") {
        fs::canonicalize(app_path).is_ok_and(|path| path == Path::new("/usr/bin/manis"))
            && Path::new("/usr/bin/pacman").is_file()
            && Path::new("/usr/bin/pkexec").is_file()
    } else {
        false
    }
}

pub(crate) fn check_for_update() -> Result<Option<AvailableUpdate>, AppUpdateError> {
    let release_bytes = download_bytes(RELEASE_API, MAX_RELEASE_METADATA_BYTES)?;
    let release: GithubRelease =
        serde_json::from_slice(&release_bytes).map_err(|_error| AppUpdateError::InvalidMetadata)?;
    if release.draft
        || !release.prerelease
        || release.tag_name != "latest"
        || !is_git_commit(&release.target_commitish)
    {
        return Err(AppUpdateError::InvalidMetadata);
    }
    let manifest_asset = release
        .assets
        .iter()
        .find(|asset| asset.name == MANIFEST_NAME)
        .ok_or(AppUpdateError::MissingAsset)?;
    let manifest_digest = manifest_asset
        .digest
        .as_deref()
        .ok_or(AppUpdateError::InvalidDigest)
        .and_then(parse_github_digest)?;
    let manifest_bytes = download_bytes(&manifest_asset.browser_download_url, MAX_MANIFEST_BYTES)?;
    verify_digest(&manifest_bytes, &manifest_digest)?;
    select_available_update(
        &manifest_bytes,
        current_version(),
        Some(&release.target_commitish),
    )
}

fn select_available_update(
    manifest_bytes: &[u8],
    installed_version: &str,
    release_commit: Option<&str>,
) -> Result<Option<AvailableUpdate>, AppUpdateError> {
    let manifest: UpdateManifest =
        serde_json::from_slice(manifest_bytes).map_err(|_error| AppUpdateError::InvalidMetadata)?;
    let fetched = Version::parse(&manifest.version)?;
    let installed = Version::parse(installed_version)?;
    if manifest.schema_version != MANIFEST_SCHEMA_VERSION
        || !is_git_commit(&manifest.commit)
        || release_commit.is_some_and(|commit| !commit.eq_ignore_ascii_case(&manifest.commit))
        || fetched <= installed
    {
        return if fetched <= installed {
            Ok(None)
        } else {
            Err(AppUpdateError::InvalidMetadata)
        };
    }
    let platform = std::env::consts::OS;
    let architecture = std::env::consts::ARCH;
    let asset = manifest
        .assets
        .into_iter()
        .find(|asset| asset.platform == platform && asset.architecture == architecture)
        .ok_or(AppUpdateError::MissingAsset)?;
    validate_manifest_asset(&asset, &manifest.version)?;
    Ok(Some(AvailableUpdate {
        version: manifest.version,
        asset,
    }))
}

pub(crate) fn stage_update(update: &AvailableUpdate) -> Result<StagedUpdate, AppUpdateError> {
    let root = update_root()?;
    let archive = root.join(&update.asset.name);
    if !verified_file(&archive, update.asset.size, &update.asset.sha256) {
        remove_file_if_exists(&archive);
        download_package(&update.asset, &archive)?;
    }

    #[cfg(target_os = "macos")]
    let payload = macos::prepare_bundle(&root, &archive, &update.version)?;
    #[cfg(not(target_os = "macos"))]
    let payload = archive;

    Ok(StagedUpdate {
        version: update.version.clone(),
        payload,
    })
}

pub(crate) fn install_staged_update(
    update: &StagedUpdate,
    app_path: &Path,
) -> Result<Option<PathBuf>, AppUpdateError> {
    if !installation_supported(app_path) {
        return Err(AppUpdateError::UnsupportedInstallation);
    }
    #[cfg(target_os = "macos")]
    {
        macos::install_bundle(&update.payload, app_path, &update.version)?;
        Ok(None)
    }
    #[cfg(target_os = "linux")]
    {
        linux::install_package(&update.payload, &update.version)?;
        Ok(Some(PathBuf::from("/usr/bin/manis")))
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = update;
        Err(AppUpdateError::UnsupportedInstallation)
    }
}

impl Version {
    fn parse(value: &str) -> Result<Self, AppUpdateError> {
        let mut components = value.split('.');
        let major = parse_version_component(components.next())?;
        let minor = parse_version_component(components.next())?;
        let patch = parse_version_component(components.next())?;
        if components.next().is_some() {
            return Err(AppUpdateError::InvalidMetadata);
        }
        Ok(Self([major, minor, patch]))
    }
}

fn parse_version_component(component: Option<&str>) -> Result<u64, AppUpdateError> {
    component
        .filter(|component| {
            !component.is_empty() && component.bytes().all(|byte| byte.is_ascii_digit())
        })
        .and_then(|component| component.parse().ok())
        .ok_or(AppUpdateError::InvalidMetadata)
}

fn is_git_commit(value: &str) -> bool {
    value.len() == 40 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn validate_manifest_asset(asset: &ManifestAsset, version: &str) -> Result<(), AppUpdateError> {
    if asset.size == 0 || asset.size > MAX_PACKAGE_BYTES || !is_sha256(&asset.sha256) {
        return Err(AppUpdateError::InvalidMetadata);
    }
    let safe_name = Path::new(&asset.name)
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name == asset.name);
    let expected_name = match (asset.platform.as_str(), asset.architecture.as_str()) {
        ("macos", "aarch64") => format!("Manis-{version}-macos-arm64-unsigned.zip"),
        ("macos", "x86_64") => format!("Manis-{version}-macos-x86_64-unsigned.zip"),
        ("linux", "x86_64") => format!("manis-{version}-1-x86_64.pkg.tar.zst"),
        _ => return Err(AppUpdateError::MissingAsset),
    };
    if safe_name && asset.name == expected_name {
        Ok(())
    } else {
        Err(AppUpdateError::InvalidMetadata)
    }
}

fn update_root() -> Result<PathBuf, AppUpdateError> {
    let root = crate::brand::data_dir()
        .ok_or(AppUpdateError::DataDirUnavailable)?
        .join("updates");
    fs::create_dir_all(&root).map_err(|_error| AppUpdateError::Io)?;
    let metadata = fs::symlink_metadata(&root).map_err(|_error| AppUpdateError::Io)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(AppUpdateError::Io);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700))
            .map_err(|_error| AppUpdateError::Io)?;
    }
    Ok(root)
}

fn download_package(asset: &ManifestAsset, target: &Path) -> Result<(), AppUpdateError> {
    let temporary = unique_sibling(target, "part");
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|_error| AppUpdateError::Io)?;
        let agent = https_agent();
        let mut response = agent
            .get(format!(
                "https://github.com/kaigedong/Manis/releases/download/latest/{}",
                asset.name
            ))
            .call()
            .map_err(|error| map_request_error(&error))?;
        require_https(&response)?;
        let mut reader = response.body_mut().as_reader();
        let mut hasher = Sha256::new();
        let mut received = 0_u64;
        let mut buffer = [0_u8; 16 * 1024];
        loop {
            let count = reader
                .read(&mut buffer)
                .map_err(|_error| AppUpdateError::NetworkUnavailable)?;
            if count == 0 {
                break;
            }
            received = received
                .checked_add(count as u64)
                .ok_or(AppUpdateError::PackageTooLarge)?;
            if received > asset.size || received > MAX_PACKAGE_BYTES {
                return Err(AppUpdateError::PackageTooLarge);
            }
            hasher.update(&buffer[..count]);
            file.write_all(&buffer[..count])
                .map_err(|_error| AppUpdateError::Io)?;
        }
        if received != asset.size
            || !format!("{:x}", hasher.finalize()).eq_ignore_ascii_case(&asset.sha256)
        {
            return Err(AppUpdateError::DigestMismatch);
        }
        file.sync_all().map_err(|_error| AppUpdateError::Io)?;
        fs::rename(&temporary, target).map_err(|_error| AppUpdateError::Io)
    })();
    if result.is_err() {
        remove_file_if_exists(&temporary);
    }
    result
}

fn download_bytes(url: &str, max_bytes: u64) -> Result<Vec<u8>, AppUpdateError> {
    let agent = https_agent();
    let mut response = agent
        .get(url)
        .call()
        .map_err(|error| map_request_error(&error))?;
    require_https(&response)?;
    response
        .body_mut()
        .with_config()
        .limit(max_bytes + 1)
        .read_to_vec()
        .map_err(|error| match error {
            ureq::Error::BodyExceedsLimit(_) => AppUpdateError::PackageTooLarge,
            _ => AppUpdateError::NetworkUnavailable,
        })
        .and_then(|bytes| {
            if bytes.len() as u64 > max_bytes {
                Err(AppUpdateError::PackageTooLarge)
            } else {
                Ok(bytes)
            }
        })
}

fn https_agent() -> Agent {
    Agent::config_builder()
        .https_only(true)
        .max_redirects(MAX_REDIRECTS)
        .timeout_global(Some(DOWNLOAD_TIMEOUT))
        .user_agent(concat!("Manis/", env!("CARGO_PKG_VERSION"), " App-Updater"))
        .build()
        .into()
}

fn require_https(response: &ureq::http::Response<ureq::Body>) -> Result<(), AppUpdateError> {
    if response.get_uri().scheme_str() == Some("https") {
        Ok(())
    } else {
        Err(AppUpdateError::InsecureRedirect)
    }
}

fn map_request_error(error: &ureq::Error) -> AppUpdateError {
    match error {
        ureq::Error::RequireHttpsOnly(_) => AppUpdateError::InsecureRedirect,
        _ => AppUpdateError::NetworkUnavailable,
    }
}

fn parse_github_digest(value: &str) -> Result<String, AppUpdateError> {
    let digest = value
        .strip_prefix("sha256:")
        .ok_or(AppUpdateError::InvalidDigest)?;
    if is_sha256(digest) {
        Ok(digest.to_ascii_lowercase())
    } else {
        Err(AppUpdateError::InvalidDigest)
    }
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn verify_digest(bytes: &[u8], expected: &str) -> Result<(), AppUpdateError> {
    if !is_sha256(expected) {
        return Err(AppUpdateError::InvalidDigest);
    }
    let actual = format!("{:x}", Sha256::digest(bytes));
    if actual.eq_ignore_ascii_case(expected) {
        Ok(())
    } else {
        Err(AppUpdateError::DigestMismatch)
    }
}

fn verified_file(path: &Path, expected_size: u64, expected_digest: &str) -> bool {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return false;
    };
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() != expected_size {
        return false;
    }
    let Ok(mut file) = fs::File::open(path) else {
        return false;
    };
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 16 * 1024];
    loop {
        let Ok(count) = file.read(&mut buffer) else {
            return false;
        };
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    format!("{:x}", hasher.finalize()).eq_ignore_ascii_case(expected_digest)
}

pub(super) fn unique_sibling(path: &Path, purpose: &str) -> PathBuf {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("manis");
    let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    parent.join(format!(
        ".{name}.{purpose}.{}.{}",
        std::process::id(),
        counter
    ))
}

fn remove_file_if_exists(path: &Path) {
    if path.is_file() {
        let _ = fs::remove_file(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(version: &str, architecture: &str, name: &str) -> Vec<u8> {
        format!(
            r#"{{"schema_version":1,"version":"{version}","commit":"0123456789abcdef0123456789abcdef01234567","assets":[{{"platform":"{}","architecture":"{architecture}","name":"{name}","sha256":"{}","size":42}}]}}"#,
            std::env::consts::OS,
            "a".repeat(64)
        )
        .into_bytes()
    }

    #[test]
    fn numeric_versions_do_not_compare_lexically() {
        assert!(Version::parse("0.1.101").unwrap() > Version::parse("0.1.99").unwrap());
        assert!(Version::parse("0.2.0").unwrap() > Version::parse("0.1.999").unwrap());
    }

    #[test]
    fn current_or_older_release_is_ignored() {
        let bytes = manifest("0.1.100", std::env::consts::ARCH, "unused");
        assert_eq!(
            select_available_update(&bytes, "0.1.100", None).unwrap(),
            None
        );
        assert_eq!(
            select_available_update(&bytes, "0.1.101", None).unwrap(),
            None
        );
    }

    #[test]
    fn manifest_must_belong_to_the_release_commit() {
        let version = "0.1.101";
        let name = match (std::env::consts::OS, std::env::consts::ARCH) {
            ("macos", "aarch64") => format!("Manis-{version}-macos-arm64-unsigned.zip"),
            ("macos", "x86_64") => format!("Manis-{version}-macos-x86_64-unsigned.zip"),
            ("linux", "x86_64") => format!("manis-{version}-1-x86_64.pkg.tar.zst"),
            _ => return,
        };
        let bytes = manifest(version, std::env::consts::ARCH, &name);
        assert_eq!(
            select_available_update(
                &bytes,
                "0.1.100",
                Some("ffffffffffffffffffffffffffffffffffffffff")
            ),
            Err(AppUpdateError::InvalidMetadata)
        );
    }

    #[test]
    fn release_asset_must_have_the_exact_platform_filename() {
        let version = "0.1.101";
        let name = match (std::env::consts::OS, std::env::consts::ARCH) {
            ("macos", "aarch64") => format!("Manis-{version}-macos-arm64-unsigned.zip"),
            ("macos", "x86_64") => format!("Manis-{version}-macos-x86_64-unsigned.zip"),
            ("linux", "x86_64") => format!("manis-{version}-1-x86_64.pkg.tar.zst"),
            _ => return,
        };
        let bytes = manifest(version, std::env::consts::ARCH, &name);
        assert!(
            select_available_update(&bytes, "0.1.100", None)
                .unwrap()
                .is_some()
        );

        let unsafe_bytes = manifest(version, std::env::consts::ARCH, "../Manis.zip");
        assert_eq!(
            select_available_update(&unsafe_bytes, "0.1.100", None),
            Err(AppUpdateError::InvalidMetadata)
        );
    }
}
