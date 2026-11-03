use std::error::Error;
use std::fmt;
use std::time::Duration;

use serde::Deserialize;
use sha2::{Digest as _, Sha256};
use ureq::{Agent, ResponseExt as _};

const RELEASE_API: &str = "https://api.github.com/repos/kaigedong/Manis/releases/tags/latest";
pub(crate) const REPOSITORY_URL: &str = "https://github.com/kaigedong/Manis";
pub(crate) const RELEASES_URL: &str = "https://github.com/kaigedong/Manis/releases/tag/latest";
const MANIFEST_NAME: &str = "manis-update.json";
const MANIFEST_SCHEMA_VERSION: u32 = 1;
const MAX_RELEASE_METADATA_BYTES: u64 = 1024 * 1024;
const MAX_MANIFEST_BYTES: u64 = 256 * 1024;
const METADATA_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_PACKAGE_BYTES: u64 = 256 * 1024 * 1024;
const PACKAGE_TIMEOUT: Duration = Duration::from_secs(600);
const MAX_REDIRECTS: u32 = 5;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct Version([u64; 3]);

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AvailableUpdate {
    pub(crate) version: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AppUpdateError {
    NetworkUnavailable,
    InvalidMetadata,
    InsecureRedirect,
    MissingMetadata,
    InvalidDigest,
    DigestMismatch,
    MetadataTooLarge,
    MissingPackage,
    UnsupportedPlatform,
    PackageTooLarge,
}

impl fmt::Display for AppUpdateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::NetworkUnavailable => "the update service is unavailable",
            Self::InvalidMetadata => "the update metadata is invalid",
            Self::InsecureRedirect => "the update metadata left HTTPS",
            Self::MissingMetadata => "the update manifest is unavailable",
            Self::InvalidDigest => "the update metadata checksum is invalid",
            Self::DigestMismatch => "the update metadata failed verification",
            Self::MetadataTooLarge => "the update metadata exceeds the safety limit",
            Self::MissingPackage => "the release has no package for this platform",
            Self::UnsupportedPlatform => "this platform cannot install updates itself",
            Self::PackageTooLarge => "the update package exceeds the safety limit",
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
}

pub(crate) const fn current_version() -> &'static str {
    match option_env!("MANIS_BUILD_VERSION") {
        Some(version) => version,
        None => env!("CARGO_PKG_VERSION"),
    }
}

pub(crate) fn check_for_update() -> Result<Option<AvailableUpdate>, AppUpdateError> {
    let release_bytes = download_bytes(RELEASE_API, MAX_RELEASE_METADATA_BYTES)?;
    let release = parse_release(&release_bytes)?;
    let manifest_asset = release
        .assets
        .iter()
        .find(|asset| asset.name == MANIFEST_NAME)
        .ok_or(AppUpdateError::MissingMetadata)?;
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

fn parse_release(bytes: &[u8]) -> Result<GithubRelease, AppUpdateError> {
    let release: GithubRelease =
        serde_json::from_slice(bytes).map_err(|_error| AppUpdateError::InvalidMetadata)?;
    if release.draft
        || !release.prerelease
        || release.tag_name != "latest"
        || !is_git_commit(&release.target_commitish)
    {
        return Err(AppUpdateError::InvalidMetadata);
    }
    Ok(release)
}

fn select_available_update(
    manifest_bytes: &[u8],
    installed_version: &str,
    release_commit: Option<&str>,
) -> Result<Option<AvailableUpdate>, AppUpdateError> {
    let manifest: UpdateManifest =
        serde_json::from_slice(manifest_bytes).map_err(|_error| AppUpdateError::InvalidMetadata)?;
    if manifest.schema_version != MANIFEST_SCHEMA_VERSION
        || !is_git_commit(&manifest.commit)
        || release_commit.is_some_and(|commit| !commit.eq_ignore_ascii_case(&manifest.commit))
    {
        return Err(AppUpdateError::InvalidMetadata);
    }
    let fetched = Version::parse(&manifest.version)?;
    let installed = Version::parse(installed_version)?;
    if fetched <= installed {
        return Ok(None);
    }
    Ok(Some(AvailableUpdate {
        version: manifest.version,
    }))
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

fn download_bytes(url: &str, max_bytes: u64) -> Result<Vec<u8>, AppUpdateError> {
    let agent = https_agent();
    let mut response = agent
        .get(url)
        .call()
        .map_err(|error| map_request_error(&error))?;
    require_https(&response)?;
    read_limited_body(&mut response, max_bytes)
}

fn read_limited_body(
    response: &mut ureq::http::Response<ureq::Body>,
    max_bytes: u64,
) -> Result<Vec<u8>, AppUpdateError> {
    response
        .body_mut()
        .with_config()
        .limit(max_bytes + 1)
        .read_to_vec()
        .map_err(|error| match error {
            ureq::Error::BodyExceedsLimit(_) => AppUpdateError::MetadataTooLarge,
            _ => AppUpdateError::NetworkUnavailable,
        })
        .and_then(|bytes| enforce_body_limit(bytes, max_bytes))
}

fn enforce_body_limit(bytes: Vec<u8>, max_bytes: u64) -> Result<Vec<u8>, AppUpdateError> {
    if bytes.len() as u64 > max_bytes {
        Err(AppUpdateError::MetadataTooLarge)
    } else {
        Ok(bytes)
    }
}

fn https_agent() -> Agent {
    Agent::config_builder()
        .https_only(true)
        .max_redirects(MAX_REDIRECTS)
        .timeout_global(Some(METADATA_TIMEOUT))
        .user_agent(concat!(
            "Manis/",
            env!("CARGO_PKG_VERSION"),
            " App-Update-Check"
        ))
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
    let actual = hex::encode(Sha256::digest(bytes));
    if actual.eq_ignore_ascii_case(expected) {
        Ok(())
    } else {
        Err(AppUpdateError::DigestMismatch)
    }
}

/// Whether this build can download and install an update by itself.
pub(crate) const fn supports_self_install() -> bool {
    cfg!(target_os = "macos")
}

/// The release asset this build installs, e.g. `Manis-0.1.104-macos-arm64-unsigned.zip`.
pub(crate) fn platform_asset_name(version: &str) -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        let architecture = if cfg!(target_arch = "aarch64") {
            "arm64"
        } else {
            "x86_64"
        };
        Some(format!("Manis-{version}-macos-{architecture}-unsigned.zip"))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = version;
        None
    }
}

/// Downloads the package for this release and verifies it against the release digest.
pub(crate) fn download_package(
    update: &AvailableUpdate,
) -> Result<std::path::PathBuf, AppUpdateError> {
    let asset_name =
        platform_asset_name(&update.version).ok_or(AppUpdateError::UnsupportedPlatform)?;
    let release_bytes = download_bytes(RELEASE_API, MAX_RELEASE_METADATA_BYTES)?;
    let release = parse_release(&release_bytes)?;
    let asset = release
        .assets
        .iter()
        .find(|asset| asset.name == asset_name)
        .ok_or(AppUpdateError::MissingPackage)?;
    let digest = asset
        .digest
        .as_deref()
        .ok_or(AppUpdateError::InvalidDigest)
        .and_then(parse_github_digest)?;
    let bytes = download_package_bytes(&asset.browser_download_url)?;
    verify_digest(&bytes, &digest)?;

    let directory = std::env::temp_dir().join(format!("manis-app-update-{}", std::process::id()));
    std::fs::create_dir_all(&directory).map_err(|_error| AppUpdateError::NetworkUnavailable)?;
    let package = directory.join(&asset_name);
    std::fs::write(&package, bytes).map_err(|_error| AppUpdateError::NetworkUnavailable)?;
    Ok(package)
}

fn download_package_bytes(url: &str) -> Result<Vec<u8>, AppUpdateError> {
    let agent: Agent = Agent::config_builder()
        .https_only(true)
        .max_redirects(MAX_REDIRECTS)
        .timeout_global(Some(PACKAGE_TIMEOUT))
        .user_agent(concat!("Manis/", env!("CARGO_PKG_VERSION"), " App-Update"))
        .build()
        .into();
    let mut response = agent
        .get(url)
        .call()
        .map_err(|error| map_request_error(&error))?;
    require_https(&response)?;
    response
        .body_mut()
        .with_config()
        .limit(MAX_PACKAGE_BYTES + 1)
        .read_to_vec()
        .map_err(|error| match error {
            ureq::Error::BodyExceedsLimit(_) => AppUpdateError::PackageTooLarge,
            _ => AppUpdateError::NetworkUnavailable,
        })
        .and_then(|bytes| {
            if bytes.len() as u64 > MAX_PACKAGE_BYTES {
                Err(AppUpdateError::PackageTooLarge)
            } else {
                Ok(bytes)
            }
        })
}

/// Installs a downloaded package and restarts the app. Returns only after the swap is scheduled.
pub(crate) fn install_package(package: &std::path::Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        macos_install::install(package)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = package;
        Err("this platform cannot install updates itself".to_owned())
    }
}

// Compiled on every unix target so the package handling is type-checked and tested here;
// only macOS exposes the install entry point above.
#[cfg(unix)]
#[allow(dead_code)]
mod macos_install {
    use std::io::{Cursor, Read, Write};
    use std::path::{Path, PathBuf};

    const MAX_ENTRIES: usize = 20_000;
    const MAX_EXTRACTED_BYTES: u64 = 1_024 * 1_024 * 1_024;

    pub(super) fn install(package: &Path) -> Result<(), String> {
        use std::process::{Command, Stdio};
        let executable = std::env::current_exe().map_err(|error| error.to_string())?;
        let bundle = executable
            .parent()
            .and_then(Path::parent)
            .and_then(Path::parent)
            .ok_or_else(|| "cannot locate the application bundle".to_owned())?;
        if bundle
            .extension()
            .is_none_or(|extension| extension != "app")
        {
            return Err("the app is not running from an application bundle".to_owned());
        }

        let staging = package
            .parent()
            .ok_or_else(|| "invalid update path".to_owned())?
            .join("extract");
        if staging.exists() {
            std::fs::remove_dir_all(&staging).map_err(|error| error.to_string())?;
        }
        std::fs::create_dir_all(&staging).map_err(|error| error.to_string())?;

        let bytes = std::fs::read(package).map_err(|error| error.to_string())?;
        extract_zip(&bytes, &staging)?;

        let replacement = find_bundle(&staging)?;
        if !replacement.join("Contents/MacOS/Manis").is_file() {
            return Err("the update package does not contain a Manis app".to_owned());
        }

        let script = staging.join("install.sh");
        let script_body = swap_script(std::process::id(), bundle, &replacement);
        std::fs::write(&script, script_body).map_err(|error| error.to_string())?;
        {
            use std::os::unix::fs::PermissionsExt as _;
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755))
                .map_err(|error| error.to_string())?;
        }

        Command::new("/bin/sh")
            .arg(&script)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    /// The detached swap: wait for this process, move the old bundle aside, move the new one in
    /// and reopen it. Quoting keeps paths with spaces intact.
    pub(super) fn swap_script(pid: u32, bundle: &Path, replacement: &Path) -> String {
        format!(
            "#!/bin/sh\nwhile kill -0 {pid} 2>/dev/null; do sleep 0.2; done\nbackup=\"{bundle}.old.$$\"\nrm -rf \"$backup\"\nmv \"{bundle}\" \"$backup\" || exit 1\nmv \"{replacement}\" \"{bundle}\" || {{ mv \"$backup\" \"{bundle}\"; exit 1; }}\nopen -n \"{bundle}\"\nsleep 2\nrm -rf \"$backup\"\nrm -rf \"$(dirname \"{replacement}\")\"\n",
            pid = pid,
            bundle = bundle.display(),
            replacement = replacement.display(),
        )
    }

    pub(super) fn extract_zip(bytes: &[u8], destination: &Path) -> Result<(), String> {
        let mut archive =
            zip::ZipArchive::new(Cursor::new(bytes)).map_err(|error| error.to_string())?;
        if archive.len() > MAX_ENTRIES {
            return Err("the update package has too many files".to_owned());
        }
        let mut extracted = 0u64;
        for index in 0..archive.len() {
            let mut entry = archive.by_index(index).map_err(|error| error.to_string())?;
            let name = entry
                .enclosed_name()
                .ok_or_else(|| "the update package contains an unsafe path".to_owned())?;
            if name.as_os_str().is_empty() {
                continue;
            }
            let target = destination.join(&name);
            if entry.is_dir() {
                std::fs::create_dir_all(&target).map_err(|error| error.to_string())?;
                continue;
            }
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            let mut file = std::fs::File::create(&target).map_err(|error| error.to_string())?;
            let mut buffer = vec![0u8; 64 * 1024];
            loop {
                let read = entry.read(&mut buffer).map_err(|error| error.to_string())?;
                if read == 0 {
                    break;
                }
                extracted += read as u64;
                if extracted > MAX_EXTRACTED_BYTES {
                    return Err("the update package expands beyond the safety limit".to_owned());
                }
                file.write_all(&buffer[..read])
                    .map_err(|error| error.to_string())?;
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt as _;
                if let Some(mode) = entry.unix_mode() {
                    let _ =
                        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(mode));
                }
            }
        }
        Ok(())
    }

    pub(super) fn find_bundle(staging: &Path) -> Result<PathBuf, String> {
        let direct = staging.join("Manis.app");
        if direct.is_dir() {
            return Ok(direct);
        }
        for entry in std::fs::read_dir(staging).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            let path = entry.path();
            if path.is_dir() {
                if path.extension().is_some_and(|extension| extension == "app") {
                    return Ok(path);
                }
                let nested = path.join("Manis.app");
                if nested.is_dir() {
                    return Ok(nested);
                }
            }
        }
        Err("the update package does not contain an application bundle".to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_asset_name_matches_the_release_layout() {
        #[cfg(target_os = "macos")]
        {
            let name = platform_asset_name("0.1.104").expect("macOS has a package");
            assert!(name.starts_with("Manis-0.1.104-macos-"));
            assert!(name.ends_with("-unsigned.zip"));
        }
        #[cfg(not(target_os = "macos"))]
        {
            assert_eq!(platform_asset_name("0.1.104"), None);
            assert!(!supports_self_install());
        }
    }

    #[cfg(unix)]
    #[test]
    fn swap_script_waits_and_quotes_paths() {
        let script = super::macos_install::swap_script(
            4242,
            std::path::Path::new("/Applications/Manis App.app"),
            std::path::Path::new("/tmp/extract/Manis.app"),
        );
        assert!(script.contains("kill -0 4242"));
        assert!(script.contains("\"/Applications/Manis App.app\""));
        assert!(script.contains("open -n \"/Applications/Manis App.app\""));
    }

    #[cfg(unix)]
    #[test]
    fn extracts_a_package_bundle() {
        use std::io::Write as _;
        let mut buffer = Vec::new();
        {
            let mut writer = zip::ZipWriter::new(std::io::Cursor::new(&mut buffer));
            let options = zip::write::SimpleFileOptions::default();
            writer
                .start_file("Manis.app/Contents/Info.plist", options)
                .unwrap();
            writer.write_all(b"<plist/>").unwrap();
            writer
                .start_file("Manis.app/Contents/MacOS/Manis", options)
                .unwrap();
            writer.write_all(b"binary").unwrap();
            writer.finish().unwrap();
        }
        let directory = std::env::temp_dir().join(format!("manis-zip-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).unwrap();
        super::macos_install::extract_zip(&buffer, &directory).unwrap();
        let bundle = super::macos_install::find_bundle(&directory).unwrap();
        assert!(bundle.join("Contents/MacOS/Manis").is_file());
        std::fs::remove_dir_all(&directory).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn rejects_path_traversal_in_a_package() {
        use std::io::Write as _;
        let mut buffer = Vec::new();
        {
            let mut writer = zip::ZipWriter::new(std::io::Cursor::new(&mut buffer));
            let options = zip::write::SimpleFileOptions::default();
            writer.start_file("../escape", options).unwrap();
            writer.write_all(b"nope").unwrap();
            writer.finish().unwrap();
        }
        let directory =
            std::env::temp_dir().join(format!("manis-zip-escape-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).unwrap();
        assert!(super::macos_install::extract_zip(&buffer, &directory).is_err());
        assert!(!directory.parent().unwrap().join("escape").exists());
        std::fs::remove_dir_all(&directory).unwrap();
    }

    const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

    fn release(asset_name: &str, digest: Option<&str>) -> Vec<u8> {
        let digest = digest.map_or_else(|| "null".to_owned(), |digest| format!(r#""{digest}""#));
        format!(
            r#"{{"tag_name":"latest","target_commitish":"{COMMIT}","draft":false,"prerelease":true,"assets":[{{"name":"{asset_name}","browser_download_url":"https://example.test/{asset_name}","digest":{digest}}}]}}"#
        )
        .into_bytes()
    }

    fn manifest(version: &str) -> Vec<u8> {
        format!(r#"{{"schema_version":1,"version":"{version}","commit":"{COMMIT}"}}"#).into_bytes()
    }

    #[test]
    fn numeric_versions_do_not_compare_lexically() {
        assert!(Version::parse("0.1.101").unwrap() > Version::parse("0.1.99").unwrap());
        assert!(Version::parse("0.2.0").unwrap() > Version::parse("0.1.999").unwrap());
    }

    #[test]
    fn newer_manifest_reports_available_update_without_binary_asset() {
        assert_eq!(
            select_available_update(&manifest("0.1.101"), "0.1.100", Some(COMMIT)).unwrap(),
            Some(AvailableUpdate {
                version: "0.1.101".to_owned(),
            })
        );
    }

    #[test]
    fn current_or_older_manifest_is_ignored_on_every_platform() {
        assert_eq!(
            select_available_update(&manifest("0.1.100"), "0.1.100", Some(COMMIT)).unwrap(),
            None
        );
        assert_eq!(
            select_available_update(&manifest("0.1.99"), "0.1.100", Some(COMMIT)).unwrap(),
            None
        );
    }

    #[test]
    fn manifest_must_belong_to_the_release_commit() {
        assert_eq!(
            select_available_update(
                &manifest("0.1.101"),
                "0.1.100",
                Some("ffffffffffffffffffffffffffffffffffffffff"),
            ),
            Err(AppUpdateError::InvalidMetadata)
        );
    }

    #[test]
    fn release_metadata_must_describe_the_rolling_latest_prerelease() {
        assert!(
            parse_release(&release(
                MANIFEST_NAME,
                Some(&format!("sha256:{}", "a".repeat(64)))
            ))
            .is_ok()
        );

        let stable = br#"{"tag_name":"v0.1.101","target_commitish":"0123456789abcdef0123456789abcdef01234567","draft":false,"prerelease":false,"assets":[]}"#;
        assert!(matches!(
            parse_release(stable),
            Err(AppUpdateError::InvalidMetadata)
        ));
    }

    #[test]
    fn release_requires_a_manifest_asset_with_github_digest() {
        let parsed = parse_release(&release(
            "Manis.zip",
            Some(&format!("sha256:{}", "a".repeat(64))),
        ))
        .unwrap();
        assert!(
            parsed
                .assets
                .iter()
                .all(|asset| asset.name != MANIFEST_NAME)
        );

        let release = parse_release(&release(MANIFEST_NAME, None)).unwrap();
        let digest = release
            .assets
            .iter()
            .find(|asset| asset.name == MANIFEST_NAME)
            .and_then(|asset| asset.digest.as_deref())
            .ok_or(AppUpdateError::InvalidDigest)
            .and_then(parse_github_digest);
        assert_eq!(digest, Err(AppUpdateError::InvalidDigest));
    }

    #[test]
    fn rejects_invalid_metadata_and_versions() {
        assert_eq!(
            select_available_update(br#"{"schema_version":2,"version":"0.1.101","commit":"0123456789abcdef0123456789abcdef01234567"}"#, "0.1.100", Some(COMMIT)),
            Err(AppUpdateError::InvalidMetadata)
        );
        assert_eq!(
            select_available_update(
                br#"{"schema_version":2,"version":"0.1.100","commit":"0123456789abcdef0123456789abcdef01234567"}"#,
                "0.1.100",
                Some(COMMIT)
            ),
            Err(AppUpdateError::InvalidMetadata)
        );
        assert_eq!(
            select_available_update(
                br#"{"schema_version":2,"version":"0.1.99","commit":"0123456789abcdef0123456789abcdef01234567"}"#,
                "0.1.100",
                Some(COMMIT)
            ),
            Err(AppUpdateError::InvalidMetadata)
        );
        assert_eq!(
            select_available_update(&manifest("0.1.x"), "0.1.100", Some(COMMIT)),
            Err(AppUpdateError::InvalidMetadata)
        );
    }

    #[test]
    fn verifies_manifest_digest() {
        let bytes = manifest("0.1.101");
        let digest = hex::encode(Sha256::digest(&bytes));
        assert_eq!(verify_digest(&bytes, &digest), Ok(()));
        assert_eq!(
            verify_digest(&bytes, &"0".repeat(64)),
            Err(AppUpdateError::DigestMismatch)
        );
    }

    #[test]
    fn metadata_bodies_are_bounded() {
        assert_eq!(
            enforce_body_limit(b"abc".to_vec(), 2),
            Err(AppUpdateError::MetadataTooLarge)
        );
        assert_eq!(enforce_body_limit(b"ab".to_vec(), 2), Ok(b"ab".to_vec()));
    }
}
