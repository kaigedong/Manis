use std::io::{Cursor, Read};
use std::path::Path;

use flate2::read::GzDecoder;
use sha2::Digest as _;
use sha2::Sha256;
use zip::ZipArchive;

use super::{
    ArchiveKind, CoreUpdateError, CoreUpdateFailureKind, MAX_CORE_BINARY_BYTES, ReleaseAsset,
    zip_entry_is_mihomo_binary,
};

impl ArchiveKind {
    pub(super) fn from_asset_name(name: &str) -> Option<Self> {
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

pub(super) fn parse_sha256_digest(value: &str) -> Result<String, CoreUpdateError> {
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

pub(super) fn is_valid_sha256_hex(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

pub(super) fn unpack_zip_core_binary(package: &[u8]) -> Result<Vec<u8>, CoreUpdateError> {
    let reader = Cursor::new(package);
    let mut archive = ZipArchive::new(reader).map_err(|error| {
        CoreUpdateError::caused(
            CoreUpdateFailureKind::InvalidArchive,
            "open core archive",
            error,
        )
    })?;
    for index in 0..archive.len() {
        let mut file = archive.by_index(index).map_err(|error| {
            CoreUpdateError::caused(
                CoreUpdateFailureKind::InvalidArchive,
                "read core archive entry",
                error,
            )
        })?;
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

pub(super) fn read_bounded_archive_entry(
    reader: &mut impl Read,
) -> Result<Vec<u8>, CoreUpdateError> {
    let mut binary = Vec::new();
    reader
        .take(MAX_CORE_BINARY_BYTES + 1)
        .read_to_end(&mut binary)
        .map_err(|error| {
            CoreUpdateError::caused(
                CoreUpdateFailureKind::InvalidArchive,
                "decompress core archive entry",
                error,
            )
        })?;
    if binary.len() as u64 > MAX_CORE_BINARY_BYTES {
        Err(CoreUpdateError::PackageTooLarge)
    } else if binary.is_empty() {
        Err(CoreUpdateError::InvalidArchive)
    } else {
        Ok(binary)
    }
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
