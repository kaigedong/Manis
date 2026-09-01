use ureq::{Agent, ResponseExt as _};

use super::{CoreUpdateError, CoreUpdateFailureKind, DOWNLOAD_TIMEOUT, MAX_REDIRECTS};

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
