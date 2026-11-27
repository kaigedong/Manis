use crate::app_update::DownloadProgress;

use super::{
    ArchiveKind, CoreUpdateError, CoreUpdateFailureKind, Deserialize, LATEST_STABLE_RELEASE_API,
    MAX_CORE_PACKAGE_BYTES, MAX_RELEASE_METADATA_BYTES, PathBuf, Platform, ReleaseAsset,
    download_bytes_with_progress, download_text, parse_sha256_digest, verify_asset_digest,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum CoreUpdateCheck {
    Current { version: String },
    Available(ReleaseAsset),
}

pub(crate) fn normalize_core_version(version: &str) -> String {
    let trimmed = version.trim();
    if trimmed.is_empty() {
        String::new()
    } else if trimmed.starts_with('v') {
        trimmed.to_owned()
    } else {
        format!("v{trimmed}")
    }
}

pub(crate) fn is_newer_core_version(current: Option<&str>, candidate: &str) -> bool {
    let Some(current) = current.map(str::trim).filter(|value| !value.is_empty()) else {
        return true;
    };
    let candidate_trimmed = candidate.trim();
    if candidate_trimmed.is_empty() {
        return false;
    }
    match (
        parse_numeric_triplet(current),
        parse_numeric_triplet(candidate_trimmed),
    ) {
        (Some(current_ver), Some(candidate_ver)) => candidate_ver > current_ver,
        _ => normalize_core_version(current) != normalize_core_version(candidate_trimmed),
    }
}

fn parse_numeric_triplet(version: &str) -> Option<(u64, u64, u64)> {
    let stripped = version.trim().strip_prefix('v').unwrap_or(version.trim());
    let core = stripped.split('-').next()?;
    let mut parts = core.split('.');
    let major = parts.next()?.parse::<u64>().ok()?;
    let minor = parts.next()?.parse::<u64>().ok()?;
    let patch = parts.next()?.parse::<u64>().ok()?;
    Some((major, minor, patch))
}

pub(crate) fn select_release_asset(
    release_json: &str,
    platform: Platform,
) -> Result<ReleaseAsset, CoreUpdateError> {
    let release: GithubRelease = serde_json::from_str(release_json).map_err(|error| {
        CoreUpdateError::caused(
            CoreUpdateFailureKind::InvalidReleaseMetadata,
            "parse release metadata",
            error,
        )
    })?;
    if release.prerelease {
        return Err(CoreUpdateError::MissingAsset);
    }

    let selected = release
        .assets
        .into_iter()
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
        name: selected.name,
        download_url: selected.browser_download_url,
        archive,
        sha256: digest,
        size: selected.size.filter(|size| *size > 0),
    })
}

pub(super) fn asset_priority(name: &str, platform: Platform) -> Option<usize> {
    let name = name.to_ascii_lowercase();
    if name.contains("alpha")
        || name.contains("compatible")
        || name.contains("-go")
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
            Self::MacosArm64 => &["mihomo-darwin-arm64"],
            Self::MacosX64 => &["mihomo-darwin-amd64-v2"],
            Self::LinuxX64 => &["mihomo-linux-amd64-v2", "mihomo-linux-amd64"],
            Self::WindowsX64 => &["mihomo-windows-amd64-v2", "mihomo-windows-amd64"],
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
    #[serde(default)]
    size: Option<u64>,
}

pub(crate) fn fetch_current_release_asset() -> Result<ReleaseAsset, CoreUpdateError> {
    fetch_release_asset_for_platform(Platform::current()?)
}

pub(crate) fn check_for_core_update(
    current_version: Option<&str>,
) -> Result<CoreUpdateCheck, CoreUpdateError> {
    let asset = fetch_current_release_asset()?;
    if is_newer_core_version(current_version, &asset.version) {
        Ok(CoreUpdateCheck::Available(asset))
    } else {
        Ok(CoreUpdateCheck::Current {
            version: asset.version,
        })
    }
}

pub(crate) fn fetch_release_asset_for_platform(
    platform: Platform,
) -> Result<ReleaseAsset, CoreUpdateError> {
    let metadata = download_text(LATEST_STABLE_RELEASE_API, MAX_RELEASE_METADATA_BYTES)?;
    select_release_asset(&metadata, platform)
}

pub(crate) fn download_asset_package_with_progress(
    asset: &ReleaseAsset,
    on_progress: impl FnMut(DownloadProgress),
) -> Result<Vec<u8>, CoreUpdateError> {
    download_bytes_with_progress(
        &asset.download_url,
        asset.size,
        MAX_CORE_PACKAGE_BYTES,
        on_progress,
    )
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
