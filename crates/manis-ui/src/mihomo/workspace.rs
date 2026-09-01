#[cfg(not(windows))]
use super::fs;
use super::{
    Error, IMPORTED_SUBSCRIPTION_FILE, LEGACY_GENERATED_PROXY_GROUP_NAME,
    LEGACY_MANIS_QX_RULE_SOURCE_VERSION, LEGACY_MANIS_QX_RULE_SOURCE_VERSION_V2,
    LEGACY_MANIS_STORED_SUBSCRIPTION_VERSION, LEGACY_MANIS_STORED_SUBSCRIPTION_VERSION_V2,
    LEGACY_RELAY_QX_RULE_SOURCE_VERSION, LEGACY_RELAY_STORED_SUBSCRIPTION_VERSION,
    LEGACY_SAVED_SINGLE_NODE_VERSION, LoadError, MANIS_GLOBAL_GROUP_NAME,
    MAX_QX_RULE_SOURCE_CONTENT_BYTES, MAX_QX_RULE_SOURCE_FILE_BYTES,
    MAX_STORED_SUBSCRIPTION_FILE_BYTES, MAX_SUBSCRIPTION_FILE_BYTES,
    MAX_SUBSCRIPTION_PROXY_DNS_SERVERS, MAX_SUBSCRIPTION_SOURCE_NAME_BYTES, NEXT_STORED_SOURCE,
    Name, Ordering, Path, PathBuf, PolicyRef, Profile, ProfileMode, ProxyDnsServer,
    QX_RULE_SOURCE_PREFIX, QX_RULE_SOURCE_SUFFIX, QX_RULE_SOURCE_VERSION, QxRuleList,
    ROUTING_MODE_FILE, RoutingMode, Rule, SAVED_SINGLE_NODE_PREFIX, SAVED_SINGLE_NODE_SUFFIX,
    SAVED_SINGLE_NODE_VERSION, STORED_SUBSCRIPTION_PREFIX, STORED_SUBSCRIPTION_SUFFIX,
    STORED_SUBSCRIPTION_VERSION, SecretUrl, SingleNodeSource, SystemTime, UNIX_EPOCH,
    WORKSPACE_STATE_FILE, brand, fmt, has_only_clean_components, write_private_atomic,
};
#[cfg(not(windows))]
use std::io::Read;

#[cfg(any(windows, test))]
mod imported_subscription;

mod qx_rule_sources;
mod single_node_sources;
mod subscription_sources;
mod workspace_preferences;

pub(crate) use qx_rule_sources::apply_qx_rule_sources;
#[cfg(all(not(windows), test))]
pub(crate) use qx_rule_sources::save_qx_rule_source_in;
#[cfg(all(windows, test))]
pub(crate) use qx_rule_sources::save_qx_rule_source_in;
#[cfg(not(windows))]
pub(crate) use qx_rule_sources::{
    load_qx_rule_sources_in, remove_qx_rule_source_in, replace_qx_rule_source_content_in,
    replace_qx_rule_source_definition_in, save_named_qx_rule_source_in,
    update_qx_rule_source_enabled_in, update_qx_rule_source_name_in,
    update_qx_rule_source_refresh_interval_in, update_qx_rule_source_target_in,
};
#[cfg(windows)]
pub(crate) use qx_rule_sources::{
    load_qx_rule_sources_in, remove_qx_rule_source_in, replace_qx_rule_source_content_in,
    replace_qx_rule_source_definition_in, save_named_qx_rule_source_in,
    update_qx_rule_source_enabled_in, update_qx_rule_source_name_in,
    update_qx_rule_source_refresh_interval_in, update_qx_rule_source_target_in,
};
#[cfg(all(not(windows), test))]
pub(crate) use single_node_sources::save_single_node_source_in;
#[cfg(windows)]
pub(crate) use single_node_sources::{
    load_single_node_sources_in, remove_single_node_source_in, save_single_node_source_in,
    save_single_node_source_with_options_in, update_single_node_source_enabled_in,
    update_single_node_source_in,
};
#[cfg(not(windows))]
pub(crate) use single_node_sources::{
    load_single_node_sources_in, remove_single_node_source_in,
    save_single_node_source_with_options_in, update_single_node_source_enabled_in,
    update_single_node_source_in,
};
#[cfg(all(windows, test))]
pub(crate) use subscription_sources::save_subscription_source_in;
#[cfg(all(windows, test))]
pub(crate) use subscription_sources::update_subscription_source_refresh_interval_in;
#[cfg(windows)]
pub(crate) use subscription_sources::{
    imported_subscription_store_dir, load_subscription_sources_in,
    mark_subscription_source_update_success_in, remove_subscription_source_in,
    save_subscription_source_with_options_in, update_subscription_source_enabled_in,
    update_subscription_source_in, update_subscription_source_proxy_nameservers_in,
};
#[cfg(not(windows))]
pub(crate) use subscription_sources::{
    imported_subscription_store_dir, load_subscription_sources_in,
    mark_subscription_source_update_success_in, remove_subscription_source_in,
    save_subscription_source_with_options_in, update_subscription_source_enabled_in,
    update_subscription_source_in, update_subscription_source_proxy_nameservers_in,
};
pub(super) use subscription_sources::{
    normalize_qx_rule_source_name, validate_subscription_source_name,
};
#[cfg(all(not(windows), test))]
pub(crate) use subscription_sources::{
    save_subscription_source_in, update_subscription_source_refresh_interval_in,
};
pub(crate) use workspace_preferences::{
    load_collapsed_groups_in, load_routing_mode_in, profile_mode, save_collapsed_groups_in,
    save_routing_mode_in,
};

#[cfg(any(windows, test))]
pub(crate) use imported_subscription::load_imported_subscription_in;
#[cfg(test)]
pub(crate) use imported_subscription::{
    remove_imported_subscription_in, save_imported_subscription_in,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum RemoteSourceRefreshInterval {
    #[default]
    Manual,
    Hourly,
    SixHours,
    TwelveHours,
    Daily,
}

impl RemoteSourceRefreshInterval {
    fn key(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::Hourly => "1h",
            Self::SixHours => "6h",
            Self::TwelveHours => "12h",
            Self::Daily => "24h",
        }
    }

    fn parse_key(input: &str) -> Option<Self> {
        match input {
            "manual" => Some(Self::Manual),
            "1h" => Some(Self::Hourly),
            "6h" => Some(Self::SixHours),
            "12h" => Some(Self::TwelveHours),
            "24h" => Some(Self::Daily),
            _ => None,
        }
    }

    pub(crate) fn interval_secs(self) -> Option<u64> {
        match self {
            Self::Manual => None,
            Self::Hourly => Some(60 * 60),
            Self::SixHours => Some(6 * 60 * 60),
            Self::TwelveHours => Some(12 * 60 * 60),
            Self::Daily => Some(24 * 60 * 60),
        }
    }

    pub(crate) fn is_due(self, last_successful_update_unix_secs: u64, now_unix_secs: u64) -> bool {
        let Some(interval_secs) = self.interval_secs() else {
            return false;
        };
        last_successful_update_unix_secs == 0
            || now_unix_secs.saturating_sub(last_successful_update_unix_secs) >= interval_secs
    }
}

#[derive(Clone, Eq, PartialEq)]
pub(crate) struct StoredSubscription {
    pub id: String,
    pub name: String,
    pub source: SecretUrl,
    pub enabled: bool,
    pub refresh_interval: RemoteSourceRefreshInterval,
    pub last_successful_update_unix_secs: u64,
    pub proxy_server_nameservers: Vec<ProxyDnsServer>,
}

impl fmt::Debug for StoredSubscription {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StoredSubscription")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("source", &"<redacted>")
            .field("enabled", &self.enabled)
            .field("refresh_interval", &self.refresh_interval)
            .field(
                "last_successful_update_unix_secs",
                &self.last_successful_update_unix_secs,
            )
            .field(
                "proxy_server_nameservers",
                &format_args!("<{} redacted>", self.proxy_server_nameservers.len()),
            )
            .finish()
    }
}

#[derive(Clone, Eq, PartialEq)]
pub(crate) struct StoredSingleNode {
    pub id: String,
    pub name: String,
    pub source: SingleNodeSource,
    pub enabled: bool,
}

impl fmt::Debug for StoredSingleNode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StoredSingleNode")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("source", &"<redacted>")
            .field("enabled", &self.enabled)
            .finish()
    }
}

#[derive(Clone, Eq, PartialEq)]
pub(crate) struct StoredQxRuleSource {
    pub id: String,
    pub name: Option<String>,
    pub source: SecretUrl,
    pub enabled: bool,
    pub target_policy: Name,
    pub content: String,
    pub rule_count: usize,
    pub diagnostic_count: usize,
    pub refresh_interval: RemoteSourceRefreshInterval,
    pub last_successful_update_unix_secs: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum SaveQxRuleSourceOutcome {
    Created(StoredQxRuleSource),
    Existing(StoredQxRuleSource),
}

impl SaveQxRuleSourceOutcome {
    #[cfg(test)]
    pub(crate) fn into_source(self) -> StoredQxRuleSource {
        match self {
            Self::Created(source) | Self::Existing(source) => source,
        }
    }
}

impl fmt::Debug for StoredQxRuleSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StoredQxRuleSource")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("source", &"<redacted>")
            .field("enabled", &self.enabled)
            .field("target_policy", &self.target_policy)
            .field("content", &"<redacted>")
            .field("rule_count", &self.rule_count)
            .field("diagnostic_count", &self.diagnostic_count)
            .field("refresh_interval", &self.refresh_interval)
            .field(
                "last_successful_update_unix_secs",
                &self.last_successful_update_unix_secs,
            )
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SubscriptionPreviewError {
    UnsupportedPlatform,
    BinaryUnavailable,
    InvalidSource,
    WorkspaceUnavailable,
    ProfileUnavailable,
    EngineUnavailable,
    ProviderUnavailable,
    EmptyProvider,
}

impl fmt::Display for SubscriptionPreviewError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::UnsupportedPlatform => "isolated Mihomo preview is unsupported on this platform",
            Self::BinaryUnavailable => "the Manis-managed Mihomo binary is unavailable",
            Self::InvalidSource => "subscription source is invalid",
            Self::WorkspaceUnavailable => "private preview workspace is unavailable",
            Self::ProfileUnavailable => "secure subscription preview profile is unavailable",
            Self::EngineUnavailable => "Mihomo preview process could not start",
            Self::ProviderUnavailable => "Mihomo could not download or parse the subscription",
            Self::EmptyProvider => "subscription contains no proxy nodes",
        })
    }
}

impl Error for SubscriptionPreviewError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SubscriptionStoreError {
    DataDirectoryUnavailable,
    InvalidSource,
    StoreUnavailable,
    StoredSourceUnavailable,
}

impl fmt::Display for SubscriptionStoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::DataDirectoryUnavailable => "Manis user data directory is unavailable",
            Self::InvalidSource => "subscription source is invalid",
            Self::StoreUnavailable => "subscription store is unavailable",
            Self::StoredSourceUnavailable => "stored subscription could not be read safely",
        })
    }
}

impl Error for SubscriptionStoreError {}

pub(super) fn storage_version_supported(
    actual: Option<&str>,
    current: &str,
    legacy_relay: &str,
) -> bool {
    actual.is_some_and(|actual| actual == current || actual == legacy_relay)
}

pub(super) fn encode_hex(value: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(value.len() * 2);
    for byte in value.bytes() {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

pub(super) fn decode_hex(value: &str) -> Result<String, SubscriptionStoreError> {
    if !value.len().is_multiple_of(2) {
        return Err(SubscriptionStoreError::StoredSourceUnavailable);
    }
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len() / 2);
    for index in (0..bytes.len()).step_by(2) {
        let high = decode_hex_digit(bytes[index])?;
        let low = decode_hex_digit(bytes[index + 1])?;
        decoded.push((high << 4) | low);
    }
    String::from_utf8(decoded).map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)
}

fn decode_hex_digit(value: u8) -> Result<u8, SubscriptionStoreError> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        _ => Err(SubscriptionStoreError::StoredSourceUnavailable),
    }
}

pub(super) fn next_stored_source_id(prefix: &str) -> String {
    let timestamp = current_unix_nanos();
    let sequence = NEXT_STORED_SOURCE.fetch_add(1, Ordering::Relaxed);
    format!("{prefix}{timestamp:x}-{sequence:x}")
}

pub(crate) fn current_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub(super) fn current_unix_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

pub(super) fn valid_stored_id(id: &str, prefix: &str) -> bool {
    id.strip_prefix(prefix).is_some_and(|suffix| {
        !suffix.is_empty()
            && suffix
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() || byte == b'-')
    })
}

fn valid_workspace_group_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 160
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b':' | b'-' | b'_'))
}

#[cfg(not(windows))]
pub(super) fn private_store_entries(
    directory: &Path,
) -> Result<Option<Vec<PathBuf>>, SubscriptionStoreError> {
    require_clean_absolute_store(directory)?;
    let iterator = match fs::read_dir(directory) {
        Ok(iterator) => iterator,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_error) => return Err(SubscriptionStoreError::StoredSourceUnavailable),
    };
    let mut paths = Vec::new();
    for entry in iterator {
        paths.push(
            entry
                .map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)?
                .path(),
        );
    }
    Ok(Some(paths))
}

#[cfg(not(windows))]
pub(super) fn read_private_source_allow_empty(
    path: &Path,
) -> Result<String, SubscriptionStoreError> {
    read_private_source_allow_empty_max(path, MAX_SUBSCRIPTION_FILE_BYTES)
}

#[cfg(not(windows))]
pub(super) fn read_private_source_allow_empty_max(
    path: &Path,
    max_bytes: u64,
) -> Result<String, SubscriptionStoreError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(SubscriptionStoreError::StoredSourceUnavailable);
    }
    let file =
        fs::File::open(path).map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)?;
    let opened_metadata = file
        .metadata()
        .map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)?;
    if opened_metadata.len() > max_bytes {
        return Err(SubscriptionStoreError::StoredSourceUnavailable);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};

        if metadata.dev() != opened_metadata.dev()
            || metadata.ino() != opened_metadata.ino()
            || opened_metadata.permissions().mode() & 0o077 != 0
        {
            return Err(SubscriptionStoreError::StoredSourceUnavailable);
        }
    }
    let mut contents = String::new();
    file.take(max_bytes + 1)
        .read_to_string(&mut contents)
        .map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)?;
    if contents.len() as u64 > max_bytes {
        return Err(SubscriptionStoreError::StoredSourceUnavailable);
    }
    Ok(contents)
}

#[cfg(not(windows))]
pub(super) fn remove_private_source(path: &Path) -> Result<(), SubscriptionStoreError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            Err(SubscriptionStoreError::StoredSourceUnavailable)
        }
        Ok(_) => fs::remove_file(path).map_err(|_error| SubscriptionStoreError::StoreUnavailable),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_error) => Err(SubscriptionStoreError::StoreUnavailable),
    }
}

#[cfg(not(windows))]
pub(super) fn require_clean_absolute_store(directory: &Path) -> Result<(), SubscriptionStoreError> {
    if !directory.is_absolute() || !has_only_clean_components(directory) {
        return Err(SubscriptionStoreError::StoreUnavailable);
    }
    match fs::symlink_metadata(directory) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            Err(SubscriptionStoreError::StoreUnavailable)
        }
        Ok(metadata) => {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if metadata.permissions().mode() & 0o077 != 0 {
                    return Err(SubscriptionStoreError::StoredSourceUnavailable);
                }
            }
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_error) => Err(SubscriptionStoreError::StoreUnavailable),
    }
}
