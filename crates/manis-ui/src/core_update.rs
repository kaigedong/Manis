use std::error::Error;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use serde::Deserialize;

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

mod archive;
mod download;
mod install;
mod release;
mod version_probe;

use archive::{parse_sha256_digest, unpack_core_binary, verify_asset_digest};
use download::{download_bytes, download_text};
#[cfg(target_os = "linux")]
pub(crate) use install::bundled_seed_path;
#[cfg(not(test))]
pub(crate) use install::install_bundled_seed_if_missing;
#[cfg(test)]
use install::{
    core_binary_name, install_asset_package, install_seed_if_missing_from, managed_core_path_in,
    publish_staged_core, rename_replace, require_managed_core_file, write_staged_core,
};
pub(crate) use install::{install_latest_core_update, managed_core_binary_path};
use install::{remove_file_if_exists, unique_sibling_path, zip_entry_is_mihomo_binary};
#[cfg(test)]
use release::select_release_asset;
pub(crate) use release::{InstalledCore, SeedInstallOutcome};
use release::{download_asset_package, fetch_current_release_asset};
use version_probe::validate_binary_version;

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
pub(crate) enum CoreUpdateFailureKind {
    NetworkUnavailable,
    InvalidReleaseMetadata,
    InvalidArchive,
    Io,
    VersionMismatch,
    PublishFailed,
}

#[derive(Debug)]
pub(crate) enum CoreUpdateError {
    UnsupportedPlatform,
    DataDirUnavailable,
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
    Caused {
        kind: CoreUpdateFailureKind,
        operation: &'static str,
        source: Box<dyn Error + Send + Sync>,
    },
}

impl PartialEq for CoreUpdateError {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (
                Self::Caused {
                    kind: left_kind,
                    operation: left_operation,
                    source: left_source,
                },
                Self::Caused {
                    kind: right_kind,
                    operation: right_operation,
                    source: right_source,
                },
            ) => {
                left_kind == right_kind
                    && left_operation == right_operation
                    && left_source.to_string() == right_source.to_string()
            }
            (Self::Caused { .. }, _) | (_, Self::Caused { .. }) => false,
            _ => std::mem::discriminant(self) == std::mem::discriminant(other),
        }
    }
}

impl Eq for CoreUpdateError {}

impl fmt::Display for CoreUpdateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Caused {
                operation, source, ..
            } => write!(
                formatter,
                "{} ({operation}: {source})",
                self.localized_message(Language::English)
            ),
            _ => formatter.write_str(self.localized_message(Language::English)),
        }
    }
}

impl CoreUpdateError {
    fn caused(
        kind: CoreUpdateFailureKind,
        operation: &'static str,
        source: impl Error + Send + Sync + 'static,
    ) -> Self {
        Self::Caused {
            kind,
            operation,
            source: Box::new(source),
        }
    }

    pub(crate) const fn localized_message(&self, language: Language) -> &'static str {
        language.localized(self.message())
    }

    const fn message(&self) -> LocalizedText {
        match self {
            Self::UnsupportedPlatform => copy::core_update::UNSUPPORTED_PLATFORM,
            Self::DataDirUnavailable => copy::core_update::DATA_DIR_UNAVAILABLE,
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
            Self::Caused { kind, .. } => match kind {
                CoreUpdateFailureKind::NetworkUnavailable => copy::core_update::NETWORK_UNAVAILABLE,
                CoreUpdateFailureKind::InvalidReleaseMetadata => {
                    copy::core_update::INVALID_RELEASE_METADATA
                }
                CoreUpdateFailureKind::InvalidArchive => copy::core_update::INVALID_ARCHIVE,
                CoreUpdateFailureKind::Io => copy::core_update::IO,
                CoreUpdateFailureKind::VersionMismatch => copy::core_update::VERSION_MISMATCH,
                CoreUpdateFailureKind::PublishFailed => copy::core_update::PUBLISH_FAILED,
            },
        }
    }
}

impl Error for CoreUpdateError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Caused { source, .. } => Some(source.as_ref()),
            _ => None,
        }
    }
}

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

#[cfg(test)]
mod tests {
    use super::download::enforce_body_limit;
    use super::version_probe::{parse_reported_version, reported_binary_version_with_timeout};
    use super::*;

    #[test]
    fn core_update_errors_have_language_specific_user_messages() {
        assert_eq!(
            CoreUpdateError::caused(
                CoreUpdateFailureKind::NetworkUnavailable,
                "download release metadata",
                std::io::Error::other("offline"),
            )
            .localized_message(Language::English),
            "The Mihomo release download failed; check the network and try again"
        );
        assert_eq!(
            CoreUpdateError::caused(
                CoreUpdateFailureKind::NetworkUnavailable,
                "download release metadata",
                std::io::Error::other("offline"),
            )
            .localized_message(Language::SimplifiedChinese),
            "Mihomo release 下载失败，请检查网络后重试"
        );
        assert_eq!(
            CoreUpdateError::PublishFailed.to_string(),
            "The Mihomo core update could not be published"
        );
    }

    #[test]
    fn caused_update_errors_keep_their_source_and_user_category() {
        let error = CoreUpdateError::caused(
            CoreUpdateFailureKind::Io,
            "write staged core",
            std::io::Error::other("disk unavailable"),
        );

        assert_eq!(
            error.localized_message(Language::English),
            CoreUpdateError::Io.localized_message(Language::English)
        );
        assert!(std::error::Error::source(&error).is_some());
        assert!(error.to_string().contains("disk unavailable"));
    }
    use flate2::{Compression, write::GzEncoder};
    use sha2::Digest as _;
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
    fn downloaded_bodies_are_bounded() {
        assert_eq!(
            enforce_body_limit(vec![7_u8; 5], 4),
            Err(CoreUpdateError::PackageTooLarge)
        );
        assert_eq!(enforce_body_limit(vec![7_u8; 4], 4), Ok(vec![7_u8; 4]));
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
            reported_binary_version_with_timeout(&binary, Duration::from_millis(50)),
            Err(CoreUpdateError::VersionMismatch)
        );

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn parses_reported_mihomo_version() {
        assert_eq!(
            parse_reported_version("Mihomo Meta v1.19.30 linux amd64"),
            Some("v1.19.30".to_owned())
        );
        assert_eq!(parse_reported_version("mihomo development build"), None);
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
    fn replace_failure_keeps_existing_core() {
        let root = test_temp_dir("manis-core-replace-failure");
        let target = root.join("mihomo");
        let staged = root.join("missing-stage");
        fs::write(&target, b"old core").expect("write target");

        assert!(rename_replace(&staged, &target).is_err());
        assert_eq!(fs::read(&target).expect("read target"), b"old core");

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
