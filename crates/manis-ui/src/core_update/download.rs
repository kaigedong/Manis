use std::io::Read;
use std::time::{Duration, Instant};

use ureq::{Agent, ResponseExt as _};

use crate::app_update::{DownloadProgress, compute_transfer_speed};

use super::{CoreUpdateError, CoreUpdateFailureKind, DOWNLOAD_TIMEOUT, MAX_REDIRECTS};

const DOWNLOAD_CHUNK_BYTES: usize = 32 * 1024;
const PROGRESS_EMIT_INTERVAL: Duration = Duration::from_millis(100);

pub(super) fn download_text(url: &str, max_bytes: u64) -> Result<String, CoreUpdateError> {
    let bytes = download_bytes(url, max_bytes)?;
    String::from_utf8(bytes).map_err(|error| {
        CoreUpdateError::caused(
            CoreUpdateFailureKind::InvalidReleaseMetadata,
            "decode release metadata",
            error,
        )
    })
}

pub(super) fn download_bytes(url: &str, max_bytes: u64) -> Result<Vec<u8>, CoreUpdateError> {
    let config = Agent::config_builder()
        .https_only(true)
        .max_redirects(MAX_REDIRECTS)
        .timeout_global(Some(DOWNLOAD_TIMEOUT))
        .user_agent("Manis/0.1 Mihomo-Core-Updater")
        .build();
    let agent: Agent = config.into();
    let mut response = agent.get(url).call().map_err(map_request_error)?;
    if response.get_uri().scheme_str() != Some("https") {
        return Err(CoreUpdateError::InsecureRedirect);
    }
    response
        .body_mut()
        .with_config()
        .limit(max_bytes + 1)
        .read_to_vec()
        .map_err(map_body_error)
        .and_then(|bytes| enforce_body_limit(bytes, max_bytes))
}

pub(super) fn download_bytes_with_progress(
    url: &str,
    fallback_total_bytes: Option<u64>,
    max_bytes: u64,
    on_progress: impl FnMut(DownloadProgress),
) -> Result<Vec<u8>, CoreUpdateError> {
    let config = Agent::config_builder()
        .https_only(true)
        .max_redirects(MAX_REDIRECTS)
        .timeout_global(Some(DOWNLOAD_TIMEOUT))
        .user_agent("Manis/0.1 Mihomo-Core-Updater")
        .build();
    let agent: Agent = config.into();
    let mut response = agent.get(url).call().map_err(map_request_error)?;
    if response.get_uri().scheme_str() != Some("https") {
        return Err(CoreUpdateError::InsecureRedirect);
    }
    let header_total_bytes = response
        .headers()
        .get("content-length")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse::<u64>().ok())
        .filter(|total| *total > 0);
    let total_bytes =
        header_total_bytes.or_else(|| fallback_total_bytes.filter(|total| *total > 0));
    let mut reader = response.body_mut().as_reader();
    read_bytes_stream(&mut reader, total_bytes, max_bytes, on_progress)
}

pub(super) fn read_bytes_stream(
    reader: &mut impl Read,
    total_bytes: Option<u64>,
    max_bytes: u64,
    mut on_progress: impl FnMut(DownloadProgress),
) -> Result<Vec<u8>, CoreUpdateError> {
    if total_bytes.is_some_and(|total| total > max_bytes) {
        return Err(CoreUpdateError::PackageTooLarge);
    }
    let initial_capacity = total_bytes
        .and_then(|total| usize::try_from(total.min(max_bytes)).ok())
        .unwrap_or(0);
    let mut bytes = Vec::with_capacity(initial_capacity);
    let mut buffer = vec![0_u8; DOWNLOAD_CHUNK_BYTES];
    let started_at = Instant::now();
    let mut last_emit = started_at;
    let mut last_sample_at = started_at;
    let mut last_sample_bytes = 0_u64;
    let mut smoothed_speed = 0_u64;

    on_progress(DownloadProgress {
        downloaded_bytes: 0,
        total_bytes,
        bytes_per_second: 0,
    });

    loop {
        let read = reader.read(&mut buffer).map_err(|error| {
            CoreUpdateError::caused(
                CoreUpdateFailureKind::NetworkUnavailable,
                "read core update stream",
                error,
            )
        })?;
        if read == 0 {
            break;
        }
        let next_len = bytes
            .len()
            .checked_add(read)
            .ok_or(CoreUpdateError::PackageTooLarge)?;
        if next_len as u64 > max_bytes {
            return Err(CoreUpdateError::PackageTooLarge);
        }
        bytes.extend_from_slice(&buffer[..read]);
        let downloaded = bytes.len() as u64;
        let now = Instant::now();
        if now.duration_since(last_emit) >= PROGRESS_EMIT_INTERVAL {
            smoothed_speed = compute_transfer_speed(
                downloaded,
                last_sample_bytes,
                now.duration_since(last_sample_at),
                now.duration_since(started_at),
                smoothed_speed,
            );
            last_sample_bytes = downloaded;
            last_sample_at = now;
            last_emit = now;
            on_progress(DownloadProgress {
                downloaded_bytes: downloaded,
                total_bytes,
                bytes_per_second: smoothed_speed,
            });
        }
    }

    let downloaded = bytes.len() as u64;
    let now = Instant::now();
    let final_speed = compute_transfer_speed(
        downloaded,
        last_sample_bytes,
        now.duration_since(last_sample_at),
        now.duration_since(started_at),
        smoothed_speed,
    );
    on_progress(DownloadProgress {
        downloaded_bytes: downloaded,
        total_bytes: total_bytes.or(Some(downloaded)),
        bytes_per_second: final_speed,
    });
    Ok(bytes)
}

pub(super) fn enforce_body_limit(
    bytes: Vec<u8>,
    max_bytes: u64,
) -> Result<Vec<u8>, CoreUpdateError> {
    if bytes.len() as u64 > max_bytes {
        Err(CoreUpdateError::PackageTooLarge)
    } else {
        Ok(bytes)
    }
}

pub(super) fn map_request_error(error: ureq::Error) -> CoreUpdateError {
    if matches!(&error, ureq::Error::RequireHttpsOnly(_)) {
        CoreUpdateError::InsecureRedirect
    } else {
        CoreUpdateError::caused(
            CoreUpdateFailureKind::NetworkUnavailable,
            "request core update",
            error,
        )
    }
}

pub(super) fn map_body_error(error: ureq::Error) -> CoreUpdateError {
    if matches!(&error, ureq::Error::BodyExceedsLimit(_)) {
        CoreUpdateError::PackageTooLarge
    } else {
        CoreUpdateError::caused(
            CoreUpdateFailureKind::NetworkUnavailable,
            "read core update response",
            error,
        )
    }
}
