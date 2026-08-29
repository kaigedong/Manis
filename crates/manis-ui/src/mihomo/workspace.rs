use super::{
    Error, IMPORTED_SUBSCRIPTION_FILE, LEGACY_GENERATED_PROXY_GROUP_NAME,
    LEGACY_MANIS_QX_RULE_SOURCE_VERSION, LEGACY_MANIS_STORED_SUBSCRIPTION_VERSION,
    LEGACY_MANIS_STORED_SUBSCRIPTION_VERSION_V2, LEGACY_RELAY_QX_RULE_SOURCE_VERSION,
    LEGACY_RELAY_STORED_SUBSCRIPTION_VERSION, LEGACY_SAVED_SINGLE_NODE_VERSION, LoadError,
    MANIS_GLOBAL_GROUP_NAME, MAX_QX_RULE_SOURCE_CONTENT_BYTES, MAX_QX_RULE_SOURCE_FILE_BYTES,
    MAX_STORED_SUBSCRIPTION_FILE_BYTES, MAX_SUBSCRIPTION_FILE_BYTES,
    MAX_SUBSCRIPTION_PROXY_DNS_SERVERS, MAX_SUBSCRIPTION_SOURCE_NAME_BYTES, NEXT_STORED_SOURCE,
    Name, Ordering, Path, PathBuf, PolicyRef, Profile, ProfileMode, ProxyDnsServer,
    QX_RULE_SOURCE_PREFIX, QX_RULE_SOURCE_SUFFIX, QX_RULE_SOURCE_VERSION, QxRuleList,
    ROUTING_MODE_FILE, Read, RoutingMode, Rule, SAVED_SINGLE_NODE_PREFIX, SAVED_SINGLE_NODE_SUFFIX,
    SAVED_SINGLE_NODE_VERSION, STORED_SUBSCRIPTION_PREFIX, STORED_SUBSCRIPTION_SUFFIX,
    STORED_SUBSCRIPTION_VERSION, SecretUrl, SingleNodeSource, SystemTime, UNIX_EPOCH,
    WORKSPACE_STATE_FILE, brand, fmt, fs, has_only_clean_components, write_private_atomic,
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

pub(crate) fn imported_subscription_store_dir() -> Result<PathBuf, SubscriptionStoreError> {
    brand::data_dir()
        .map(|directory| directory.join("subscriptions"))
        .ok_or(SubscriptionStoreError::DataDirectoryUnavailable)
}

#[cfg(all(not(windows), test))]
pub(crate) fn save_subscription_source_in(
    directory: &Path,
    input: &str,
) -> Result<StoredSubscription, SubscriptionStoreError> {
    let source = SecretUrl::parse_subscription(input)
        .map_err(|_error| SubscriptionStoreError::InvalidSource)?;
    let name = source
        .subscription_name()
        .unwrap_or_else(|| "Subscription".to_owned());
    save_subscription_source_with_options_in(
        directory,
        input,
        &name,
        RemoteSourceRefreshInterval::Manual,
        true,
    )
}

#[cfg(not(windows))]
pub(crate) fn save_subscription_source_with_options_in(
    directory: &Path,
    input: &str,
    name: &str,
    refresh_interval: RemoteSourceRefreshInterval,
    enabled: bool,
) -> Result<StoredSubscription, SubscriptionStoreError> {
    let source = SecretUrl::parse_subscription(input)
        .map_err(|_error| SubscriptionStoreError::InvalidSource)?;
    let name = validate_subscription_source_name(name)?;
    if let Some(existing) = load_subscription_sources_in(directory)?
        .into_iter()
        .find(|stored| stored.source == source)
    {
        return write_subscription_source_in(
            directory,
            SubscriptionSourceWrite {
                id: &existing.id,
                url_input: input,
                name: &name,
                enabled,
                refresh_interval,
                last_successful_update_unix_secs: existing.last_successful_update_unix_secs,
                proxy_server_nameservers: &existing.proxy_server_nameservers,
            },
        );
    }
    let id = next_stored_source_id(STORED_SUBSCRIPTION_PREFIX);
    let file_name = format!("{id}{STORED_SUBSCRIPTION_SUFFIX}");
    let last_successful_update_unix_secs = current_unix_secs();
    let contents = encode_subscription_source(
        &id,
        input,
        &name,
        enabled,
        refresh_interval,
        last_successful_update_unix_secs,
        &[],
    )?;
    write_private_atomic(directory, &file_name, contents.as_bytes())
        .map_err(|_error| SubscriptionStoreError::StoreUnavailable)?;
    Ok(StoredSubscription {
        id,
        name,
        source,
        enabled,
        refresh_interval,
        last_successful_update_unix_secs,
        proxy_server_nameservers: Vec::new(),
    })
}

#[cfg(all(windows, test))]
pub(crate) fn save_subscription_source_in(
    _directory: &Path,
    _input: &str,
) -> Result<StoredSubscription, SubscriptionStoreError> {
    Err(SubscriptionStoreError::StoreUnavailable)
}

#[cfg(windows)]
pub(crate) fn save_subscription_source_with_options_in(
    _directory: &Path,
    _input: &str,
    _name: &str,
    _refresh_interval: RemoteSourceRefreshInterval,
    _enabled: bool,
) -> Result<StoredSubscription, SubscriptionStoreError> {
    Err(SubscriptionStoreError::StoreUnavailable)
}

#[cfg(not(windows))]
pub(crate) fn update_subscription_source_in(
    directory: &Path,
    id: &str,
    input: &str,
    name: &str,
    refresh_interval: RemoteSourceRefreshInterval,
    enabled: bool,
) -> Result<StoredSubscription, SubscriptionStoreError> {
    let decoded = read_subscription_source_by_id_in(directory, id)?;
    let source = SecretUrl::parse_subscription(input)
        .map_err(|_error| SubscriptionStoreError::InvalidSource)?;
    if load_subscription_sources_in(directory)?
        .iter()
        .any(|stored| stored.id != id && stored.source == source)
    {
        return Err(SubscriptionStoreError::InvalidSource);
    }
    let source_changed = decoded.stored.source != source;
    let proxy_server_nameservers: &[ProxyDnsServer] = if source_changed {
        &[]
    } else {
        &decoded.stored.proxy_server_nameservers
    };
    let validated_name = validate_subscription_source_name(name)?;
    write_subscription_source_in(
        directory,
        SubscriptionSourceWrite {
            id,
            url_input: input,
            name: &validated_name,
            enabled,
            refresh_interval,
            last_successful_update_unix_secs: if source_changed {
                current_unix_secs()
            } else {
                decoded.stored.last_successful_update_unix_secs
            },
            proxy_server_nameservers,
        },
    )
}

#[cfg(not(windows))]
pub(crate) fn update_subscription_source_enabled_in(
    directory: &Path,
    id: &str,
    enabled: bool,
) -> Result<StoredSubscription, SubscriptionStoreError> {
    let decoded = read_subscription_source_by_id_in(directory, id)?;
    write_subscription_source_in(
        directory,
        SubscriptionSourceWrite {
            id,
            url_input: &decoded.url_input,
            name: &decoded.stored.name,
            enabled,
            refresh_interval: decoded.stored.refresh_interval,
            last_successful_update_unix_secs: decoded.stored.last_successful_update_unix_secs,
            proxy_server_nameservers: &decoded.stored.proxy_server_nameservers,
        },
    )
}

#[cfg(windows)]
pub(crate) fn update_subscription_source_enabled_in(
    _directory: &Path,
    _id: &str,
    _enabled: bool,
) -> Result<StoredSubscription, SubscriptionStoreError> {
    Err(SubscriptionStoreError::StoreUnavailable)
}

#[cfg(windows)]
pub(crate) fn update_subscription_source_in(
    _directory: &Path,
    _id: &str,
    _input: &str,
    _name: &str,
    _refresh_interval: RemoteSourceRefreshInterval,
    _enabled: bool,
) -> Result<StoredSubscription, SubscriptionStoreError> {
    Err(SubscriptionStoreError::StoreUnavailable)
}

#[cfg(not(windows))]
pub(crate) fn load_subscription_sources_in(
    directory: &Path,
) -> Result<Vec<StoredSubscription>, SubscriptionStoreError> {
    let mut sources = Vec::new();
    let Some(entries) = private_store_entries(directory)? else {
        return Ok(sources);
    };
    for path in entries {
        let Some(file_name) = path.file_name().and_then(std::ffi::OsStr::to_str) else {
            continue;
        };
        let id = if file_name == IMPORTED_SUBSCRIPTION_FILE {
            "subscription:legacy".to_owned()
        } else if let Some(id) = file_name.strip_suffix(STORED_SUBSCRIPTION_SUFFIX)
            && valid_stored_id(id, STORED_SUBSCRIPTION_PREFIX)
        {
            id.to_owned()
        } else {
            continue;
        };
        let contents =
            read_private_source_allow_empty_max(&path, MAX_STORED_SUBSCRIPTION_FILE_BYTES)?;
        let decoded = decode_subscription_source(&contents, &id)?;
        if !sources
            .iter()
            .any(|stored: &StoredSubscription| stored.source == decoded.stored.source)
        {
            sources.push(decoded.stored);
        }
    }
    sources.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(sources)
}

#[cfg(windows)]
pub(crate) fn load_subscription_sources_in(
    directory: &Path,
) -> Result<Vec<StoredSubscription>, SubscriptionStoreError> {
    load_imported_subscription_in(directory).map(|source| {
        source
            .map(|source| {
                vec![StoredSubscription {
                    id: "subscription:legacy".to_owned(),
                    name: source
                        .subscription_name()
                        .unwrap_or_else(|| "Subscription".to_owned()),
                    source,
                    enabled: true,
                    refresh_interval: RemoteSourceRefreshInterval::Manual,
                    last_successful_update_unix_secs: 0,
                    proxy_server_nameservers: Vec::new(),
                }]
            })
            .unwrap_or_default()
    })
}

#[cfg(all(not(windows), test))]
pub(crate) fn update_subscription_source_refresh_interval_in(
    directory: &Path,
    id: &str,
    refresh_interval: RemoteSourceRefreshInterval,
) -> Result<StoredSubscription, SubscriptionStoreError> {
    let decoded = read_subscription_source_by_id_in(directory, id)?;
    write_subscription_source_in(
        directory,
        SubscriptionSourceWrite {
            id,
            url_input: &decoded.url_input,
            name: &decoded.stored.name,
            enabled: decoded.stored.enabled,
            refresh_interval,
            last_successful_update_unix_secs: decoded.stored.last_successful_update_unix_secs,
            proxy_server_nameservers: &decoded.stored.proxy_server_nameservers,
        },
    )
}

#[cfg(all(windows, test))]
pub(crate) fn update_subscription_source_refresh_interval_in(
    _directory: &Path,
    _id: &str,
    _refresh_interval: RemoteSourceRefreshInterval,
) -> Result<StoredSubscription, SubscriptionStoreError> {
    Err(SubscriptionStoreError::StoreUnavailable)
}

#[cfg(not(windows))]
pub(crate) fn mark_subscription_source_update_success_in(
    directory: &Path,
    id: &str,
    last_successful_update_unix_secs: u64,
) -> Result<StoredSubscription, SubscriptionStoreError> {
    let decoded = read_subscription_source_by_id_in(directory, id)?;
    write_subscription_source_in(
        directory,
        SubscriptionSourceWrite {
            id,
            url_input: &decoded.url_input,
            name: &decoded.stored.name,
            enabled: decoded.stored.enabled,
            refresh_interval: decoded.stored.refresh_interval,
            last_successful_update_unix_secs,
            proxy_server_nameservers: &decoded.stored.proxy_server_nameservers,
        },
    )
}

#[cfg(not(windows))]
pub(crate) fn update_subscription_source_proxy_nameservers_in(
    directory: &Path,
    id: &str,
    proxy_server_nameservers: &[ProxyDnsServer],
) -> Result<StoredSubscription, SubscriptionStoreError> {
    if proxy_server_nameservers.is_empty()
        || proxy_server_nameservers.len() > MAX_SUBSCRIPTION_PROXY_DNS_SERVERS
    {
        return Err(SubscriptionStoreError::InvalidSource);
    }
    let decoded = read_subscription_source_by_id_in(directory, id)?;
    write_subscription_source_in(
        directory,
        SubscriptionSourceWrite {
            id,
            url_input: &decoded.url_input,
            name: &decoded.stored.name,
            enabled: decoded.stored.enabled,
            refresh_interval: decoded.stored.refresh_interval,
            last_successful_update_unix_secs: decoded.stored.last_successful_update_unix_secs,
            proxy_server_nameservers,
        },
    )
}

#[cfg(windows)]
pub(crate) fn update_subscription_source_proxy_nameservers_in(
    _directory: &Path,
    _id: &str,
    _proxy_server_nameservers: &[ProxyDnsServer],
) -> Result<StoredSubscription, SubscriptionStoreError> {
    Err(SubscriptionStoreError::StoreUnavailable)
}

#[cfg(windows)]
pub(crate) fn mark_subscription_source_update_success_in(
    _directory: &Path,
    _id: &str,
    _last_successful_update_unix_secs: u64,
) -> Result<StoredSubscription, SubscriptionStoreError> {
    Err(SubscriptionStoreError::StoreUnavailable)
}

#[cfg(not(windows))]
pub(crate) fn remove_subscription_source_in(
    directory: &Path,
    id: &str,
) -> Result<(), SubscriptionStoreError> {
    require_clean_absolute_store(directory)?;
    let file_name = if id == "subscription:legacy" {
        IMPORTED_SUBSCRIPTION_FILE.to_owned()
    } else if valid_stored_id(id, STORED_SUBSCRIPTION_PREFIX) {
        format!("{id}{STORED_SUBSCRIPTION_SUFFIX}")
    } else {
        return Err(SubscriptionStoreError::StoreUnavailable);
    };
    remove_private_source(&directory.join(file_name))
}

#[cfg(windows)]
pub(crate) fn remove_subscription_source_in(
    _directory: &Path,
    _id: &str,
) -> Result<(), SubscriptionStoreError> {
    Err(SubscriptionStoreError::StoreUnavailable)
}

#[cfg(all(not(windows), test))]
pub(crate) fn save_single_node_source_in(
    directory: &Path,
    input: &str,
) -> Result<StoredSingleNode, SubscriptionStoreError> {
    let source =
        SingleNodeSource::parse(input).map_err(|_error| SubscriptionStoreError::InvalidSource)?;
    let name = source.preview().name.clone();
    save_single_node_source_with_options_in(directory, input, &name, true)
}

#[cfg(not(windows))]
pub(crate) fn save_single_node_source_with_options_in(
    directory: &Path,
    input: &str,
    name: &str,
    enabled: bool,
) -> Result<StoredSingleNode, SubscriptionStoreError> {
    let source =
        SingleNodeSource::parse(input).map_err(|_error| SubscriptionStoreError::InvalidSource)?;
    if let Some(existing) = load_single_node_sources_in(directory)?
        .into_iter()
        .find(|stored| stored.source == source)
    {
        if existing.enabled == enabled && existing.name == name.trim() {
            return Ok(existing);
        }
        return update_single_node_source_in(directory, &existing.id, input, name, enabled);
    }
    let id = next_stored_source_id(SAVED_SINGLE_NODE_PREFIX);
    let file_name = format!("{id}{SAVED_SINGLE_NODE_SUFFIX}");
    let name = validate_subscription_source_name(name)?;
    let encoded = encode_single_node_source(&id, input, &name, enabled)?;
    write_private_atomic(directory, &file_name, encoded.as_bytes())
        .map_err(|_error| SubscriptionStoreError::StoreUnavailable)?;
    Ok(StoredSingleNode {
        id,
        name,
        source,
        enabled,
    })
}

#[cfg(windows)]
pub(crate) fn save_single_node_source_in(
    _directory: &Path,
    _input: &str,
) -> Result<StoredSingleNode, SubscriptionStoreError> {
    Err(SubscriptionStoreError::StoreUnavailable)
}

#[cfg(windows)]
pub(crate) fn save_single_node_source_with_options_in(
    _directory: &Path,
    _input: &str,
    _name: &str,
    _enabled: bool,
) -> Result<StoredSingleNode, SubscriptionStoreError> {
    Err(SubscriptionStoreError::StoreUnavailable)
}

#[cfg(not(windows))]
pub(crate) fn update_single_node_source_in(
    directory: &Path,
    id: &str,
    input: &str,
    name: &str,
    enabled: bool,
) -> Result<StoredSingleNode, SubscriptionStoreError> {
    require_clean_absolute_store(directory)?;
    if !valid_stored_id(id, SAVED_SINGLE_NODE_PREFIX) {
        return Err(SubscriptionStoreError::InvalidSource);
    }
    let source =
        SingleNodeSource::parse(input).map_err(|_error| SubscriptionStoreError::InvalidSource)?;
    if load_single_node_sources_in(directory)?
        .into_iter()
        .any(|stored| stored.id != id && stored.source == source)
    {
        return Err(SubscriptionStoreError::InvalidSource);
    }
    let name = validate_subscription_source_name(name)?;
    let encoded = encode_single_node_source(id, input, &name, enabled)?;
    write_private_atomic(
        directory,
        &format!("{id}{SAVED_SINGLE_NODE_SUFFIX}"),
        encoded.as_bytes(),
    )
    .map_err(|_error| SubscriptionStoreError::StoreUnavailable)?;
    Ok(StoredSingleNode {
        id: id.to_owned(),
        name,
        source,
        enabled,
    })
}

#[cfg(windows)]
pub(crate) fn update_single_node_source_in(
    _directory: &Path,
    _id: &str,
    _input: &str,
    _name: &str,
    _enabled: bool,
) -> Result<StoredSingleNode, SubscriptionStoreError> {
    Err(SubscriptionStoreError::StoreUnavailable)
}

pub(crate) fn update_single_node_source_enabled_in(
    directory: &Path,
    id: &str,
    enabled: bool,
) -> Result<StoredSingleNode, SubscriptionStoreError> {
    let stored = load_single_node_sources_in(directory)?
        .into_iter()
        .find(|stored| stored.id == id)
        .ok_or(SubscriptionStoreError::StoredSourceUnavailable)?;
    let input = stored.source.expose_to(str::to_owned);
    update_single_node_source_in(directory, id, &input, &stored.name, enabled)
}

#[cfg(not(windows))]
pub(crate) fn load_single_node_sources_in(
    directory: &Path,
) -> Result<Vec<StoredSingleNode>, SubscriptionStoreError> {
    let mut nodes = Vec::new();
    let Some(entries) = private_store_entries(directory)? else {
        return Ok(nodes);
    };
    for path in entries {
        let Some(file_name) = path.file_name().and_then(std::ffi::OsStr::to_str) else {
            continue;
        };
        let Some(id) = file_name.strip_suffix(SAVED_SINGLE_NODE_SUFFIX) else {
            continue;
        };
        if !valid_stored_id(id, SAVED_SINGLE_NODE_PREFIX) {
            continue;
        }
        let contents =
            read_private_source_allow_empty_max(&path, MAX_STORED_SUBSCRIPTION_FILE_BYTES)?;
        nodes.push(decode_single_node_source(&contents, id)?);
    }
    nodes.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(nodes)
}

#[cfg(windows)]
pub(crate) fn load_single_node_sources_in(
    _directory: &Path,
) -> Result<Vec<StoredSingleNode>, SubscriptionStoreError> {
    Ok(Vec::new())
}

fn encode_single_node_source(
    id: &str,
    input: &str,
    name: &str,
    enabled: bool,
) -> Result<String, SubscriptionStoreError> {
    if !valid_stored_id(id, SAVED_SINGLE_NODE_PREFIX)
        || input.len() > crate::subscription::MAX_SUBSCRIPTION_BYTES
    {
        return Err(SubscriptionStoreError::InvalidSource);
    }
    SingleNodeSource::parse(input).map_err(|_error| SubscriptionStoreError::InvalidSource)?;
    let name = validate_subscription_source_name(name)?;
    Ok([
        SAVED_SINGLE_NODE_VERSION.to_owned(),
        format!("id\t{id}"),
        format!("name\t{}", encode_hex(&name)),
        format!("enabled\t{}", if enabled { "true" } else { "false" }),
        format!("url\t{}", encode_hex(input)),
    ]
    .join("\n"))
}

fn decode_single_node_source(
    contents: &str,
    expected_id: &str,
) -> Result<StoredSingleNode, SubscriptionStoreError> {
    if !matches!(
        contents.lines().next(),
        Some(SAVED_SINGLE_NODE_VERSION | LEGACY_SAVED_SINGLE_NODE_VERSION)
    ) {
        let source = SingleNodeSource::parse(contents)
            .map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)?;
        return Ok(StoredSingleNode {
            id: expected_id.to_owned(),
            name: source.preview().name.clone(),
            source,
            enabled: true,
        });
    }
    let mut id = None;
    let mut name = None;
    let mut enabled = None;
    let mut url = None;
    for line in contents.lines().skip(1) {
        let fields = line.split('\t').collect::<Vec<_>>();
        match fields.as_slice() {
            ["id", value] if id.is_none() => id = Some(*value),
            ["name", value] if name.is_none() => {
                name = Some(validate_subscription_source_name(&decode_hex(value)?)?);
            }
            ["enabled", value] if enabled.is_none() => {
                enabled = Some(match *value {
                    "true" => true,
                    "false" => false,
                    _ => return Err(SubscriptionStoreError::StoredSourceUnavailable),
                });
            }
            ["url", value] if url.is_none() => url = Some(decode_hex(value)?),
            _ => return Err(SubscriptionStoreError::StoredSourceUnavailable),
        }
    }
    if id != Some(expected_id) || !valid_stored_id(expected_id, SAVED_SINGLE_NODE_PREFIX) {
        return Err(SubscriptionStoreError::StoredSourceUnavailable);
    }
    let source = SingleNodeSource::parse(
        url.as_deref()
            .ok_or(SubscriptionStoreError::StoredSourceUnavailable)?,
    )
    .map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)?;
    Ok(StoredSingleNode {
        id: expected_id.to_owned(),
        name: name.unwrap_or_else(|| source.preview().name.clone()),
        source,
        enabled: enabled.ok_or(SubscriptionStoreError::StoredSourceUnavailable)?,
    })
}

#[cfg(not(windows))]
pub(crate) fn remove_single_node_source_in(
    directory: &Path,
    id: &str,
) -> Result<(), SubscriptionStoreError> {
    require_clean_absolute_store(directory)?;
    if !valid_stored_id(id, SAVED_SINGLE_NODE_PREFIX) {
        return Err(SubscriptionStoreError::StoreUnavailable);
    }
    remove_private_source(&directory.join(format!("{id}{SAVED_SINGLE_NODE_SUFFIX}")))
}

#[cfg(windows)]
pub(crate) fn remove_single_node_source_in(
    _directory: &Path,
    _id: &str,
) -> Result<(), SubscriptionStoreError> {
    Err(SubscriptionStoreError::StoreUnavailable)
}

#[cfg(not(windows))]
pub(crate) fn save_qx_rule_source_in(
    directory: &Path,
    url_input: &str,
    target_policy: &str,
    content: &str,
) -> Result<SaveQxRuleSourceOutcome, SubscriptionStoreError> {
    let source = SecretUrl::parse_https(url_input)
        .map_err(|_error| SubscriptionStoreError::InvalidSource)?;
    let target_policy =
        Name::parse(target_policy).map_err(|_error| SubscriptionStoreError::InvalidSource)?;
    let (rule_count, diagnostic_count) = validate_qx_rule_source_content(content)?;
    if let Some(existing) = load_qx_rule_sources_in(directory)?
        .into_iter()
        .find(|stored| stored.source == source)
    {
        return Ok(SaveQxRuleSourceOutcome::Existing(existing));
    }
    let id = next_stored_source_id(QX_RULE_SOURCE_PREFIX);
    let file_name = format!("{id}{QX_RULE_SOURCE_SUFFIX}");
    let last_successful_update_unix_secs = current_unix_secs();
    let contents = encode_qx_rule_source(
        &id,
        url_input,
        &target_policy,
        content,
        true,
        RemoteSourceRefreshInterval::Manual,
        last_successful_update_unix_secs,
    )?;
    write_private_atomic(directory, &file_name, contents.as_bytes())
        .map_err(|_error| SubscriptionStoreError::StoreUnavailable)?;
    Ok(SaveQxRuleSourceOutcome::Created(StoredQxRuleSource {
        id,
        source,
        enabled: true,
        target_policy,
        content: content.to_owned(),
        rule_count,
        diagnostic_count,
        refresh_interval: RemoteSourceRefreshInterval::Manual,
        last_successful_update_unix_secs,
    }))
}

#[cfg(windows)]
pub(crate) fn save_qx_rule_source_in(
    _directory: &Path,
    _url_input: &str,
    _target_policy: &str,
    _content: &str,
) -> Result<SaveQxRuleSourceOutcome, SubscriptionStoreError> {
    Err(SubscriptionStoreError::StoreUnavailable)
}

#[cfg(not(windows))]
pub(crate) fn load_qx_rule_sources_in(
    directory: &Path,
) -> Result<Vec<StoredQxRuleSource>, SubscriptionStoreError> {
    let mut sources = Vec::new();
    let Some(entries) = private_store_entries(directory)? else {
        return Ok(sources);
    };
    for path in entries {
        let Some(file_name) = path.file_name().and_then(std::ffi::OsStr::to_str) else {
            continue;
        };
        let Some(id) = file_name.strip_suffix(QX_RULE_SOURCE_SUFFIX) else {
            continue;
        };
        if !valid_stored_id(id, QX_RULE_SOURCE_PREFIX) {
            continue;
        }
        let contents = read_private_source_allow_empty_max(&path, MAX_QX_RULE_SOURCE_FILE_BYTES)?;
        sources.push(decode_qx_rule_source(&contents, id)?);
    }
    sources.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(sources)
}

#[cfg(windows)]
pub(crate) fn load_qx_rule_sources_in(
    _directory: &Path,
) -> Result<Vec<StoredQxRuleSource>, SubscriptionStoreError> {
    Ok(Vec::new())
}

#[cfg(not(windows))]
pub(crate) fn remove_qx_rule_source_in(
    directory: &Path,
    id: &str,
) -> Result<(), SubscriptionStoreError> {
    require_clean_absolute_store(directory)?;
    if !valid_stored_id(id, QX_RULE_SOURCE_PREFIX) {
        return Err(SubscriptionStoreError::StoreUnavailable);
    }
    remove_private_source(&directory.join(format!("{id}{QX_RULE_SOURCE_SUFFIX}")))
}

#[cfg(windows)]
pub(crate) fn remove_qx_rule_source_in(
    _directory: &Path,
    _id: &str,
) -> Result<(), SubscriptionStoreError> {
    Err(SubscriptionStoreError::StoreUnavailable)
}

#[cfg(not(windows))]
pub(crate) fn update_qx_rule_source_refresh_interval_in(
    directory: &Path,
    id: &str,
    refresh_interval: RemoteSourceRefreshInterval,
) -> Result<StoredQxRuleSource, SubscriptionStoreError> {
    let decoded = read_qx_rule_source_by_id_in(directory, id)?;
    write_qx_rule_source_in(
        directory,
        QxRuleSourceWrite {
            id,
            url_input: &decoded.url_input,
            target_policy: decoded.stored.target_policy.as_str(),
            content: &decoded.stored.content,
            enabled: decoded.stored.enabled,
            refresh_interval,
            last_successful_update_unix_secs: decoded.stored.last_successful_update_unix_secs,
        },
    )
}

#[cfg(windows)]
pub(crate) fn update_qx_rule_source_refresh_interval_in(
    _directory: &Path,
    _id: &str,
    _refresh_interval: RemoteSourceRefreshInterval,
) -> Result<StoredQxRuleSource, SubscriptionStoreError> {
    Err(SubscriptionStoreError::StoreUnavailable)
}

#[cfg(not(windows))]
pub(crate) fn update_qx_rule_source_target_in(
    directory: &Path,
    id: &str,
    target_policy: &str,
) -> Result<StoredQxRuleSource, SubscriptionStoreError> {
    let decoded = read_qx_rule_source_by_id_in(directory, id)?;
    write_qx_rule_source_in(
        directory,
        QxRuleSourceWrite {
            id,
            url_input: &decoded.url_input,
            target_policy,
            content: &decoded.stored.content,
            enabled: decoded.stored.enabled,
            refresh_interval: decoded.stored.refresh_interval,
            last_successful_update_unix_secs: decoded.stored.last_successful_update_unix_secs,
        },
    )
}

#[cfg(windows)]
pub(crate) fn update_qx_rule_source_target_in(
    _directory: &Path,
    _id: &str,
    _target_policy: &str,
) -> Result<StoredQxRuleSource, SubscriptionStoreError> {
    Err(SubscriptionStoreError::StoreUnavailable)
}

#[cfg(not(windows))]
pub(crate) fn replace_qx_rule_source_definition_in(
    directory: &Path,
    id: &str,
    url_input: &str,
    target_policy: &str,
    content: &str,
    refresh_interval: RemoteSourceRefreshInterval,
    last_successful_update_unix_secs: u64,
) -> Result<StoredQxRuleSource, SubscriptionStoreError> {
    let decoded = read_qx_rule_source_by_id_in(directory, id)?;
    write_qx_rule_source_in(
        directory,
        QxRuleSourceWrite {
            id,
            url_input,
            target_policy,
            content,
            enabled: decoded.stored.enabled,
            refresh_interval,
            last_successful_update_unix_secs,
        },
    )
}

#[cfg(not(windows))]
pub(crate) fn update_qx_rule_source_enabled_in(
    directory: &Path,
    id: &str,
    enabled: bool,
) -> Result<StoredQxRuleSource, SubscriptionStoreError> {
    let decoded = read_qx_rule_source_by_id_in(directory, id)?;
    write_qx_rule_source_in(
        directory,
        QxRuleSourceWrite {
            id,
            url_input: &decoded.url_input,
            target_policy: decoded.stored.target_policy.as_str(),
            content: &decoded.stored.content,
            enabled,
            refresh_interval: decoded.stored.refresh_interval,
            last_successful_update_unix_secs: decoded.stored.last_successful_update_unix_secs,
        },
    )
}

#[cfg(windows)]
pub(crate) fn update_qx_rule_source_enabled_in(
    _directory: &Path,
    _id: &str,
    _enabled: bool,
) -> Result<StoredQxRuleSource, SubscriptionStoreError> {
    Err(SubscriptionStoreError::StoreUnavailable)
}

#[cfg(windows)]
pub(crate) fn replace_qx_rule_source_definition_in(
    _directory: &Path,
    _id: &str,
    _url_input: &str,
    _target_policy: &str,
    _content: &str,
    _refresh_interval: RemoteSourceRefreshInterval,
    _last_successful_update_unix_secs: u64,
) -> Result<StoredQxRuleSource, SubscriptionStoreError> {
    Err(SubscriptionStoreError::StoreUnavailable)
}

#[cfg(not(windows))]
pub(crate) fn replace_qx_rule_source_content_in(
    directory: &Path,
    id: &str,
    content: &str,
    last_successful_update_unix_secs: u64,
) -> Result<StoredQxRuleSource, SubscriptionStoreError> {
    validate_qx_rule_source_content(content)?;
    let decoded = read_qx_rule_source_by_id_in(directory, id)?;
    write_qx_rule_source_in(
        directory,
        QxRuleSourceWrite {
            id,
            url_input: &decoded.url_input,
            target_policy: decoded.stored.target_policy.as_str(),
            content,
            enabled: decoded.stored.enabled,
            refresh_interval: decoded.stored.refresh_interval,
            last_successful_update_unix_secs,
        },
    )
}

#[cfg(windows)]
pub(crate) fn replace_qx_rule_source_content_in(
    _directory: &Path,
    _id: &str,
    _content: &str,
    _last_successful_update_unix_secs: u64,
) -> Result<StoredQxRuleSource, SubscriptionStoreError> {
    Err(SubscriptionStoreError::StoreUnavailable)
}

pub(super) fn apply_qx_rule_sources(
    profile: &mut Profile,
    sources: &[StoredQxRuleSource],
) -> Result<(), LoadError> {
    let has_user_named_proxy = profile
        .groups
        .iter()
        .any(|group| group.name.as_str() == LEGACY_GENERATED_PROXY_GROUP_NAME);
    let legacy_proxy_target = (!has_user_named_proxy)
        .then(|| {
            profile
                .groups
                .iter()
                .find(|group| group.name.as_str() != MANIS_GLOBAL_GROUP_NAME)
                .or_else(|| profile.groups.first())
                .map(|group| PolicyRef::Group(group.name.clone()))
        })
        .flatten();
    let mut imported_rules = Vec::new();
    for source in sources {
        if !source.enabled {
            continue;
        }
        let target_policy =
            qx_rule_target_policy(&source.target_policy, legacy_proxy_target.as_ref());
        let parsed = QxRuleList::parse(&source.content);
        if parsed.rules.is_empty() {
            return Err(LoadError::Runtime(
                "已保存的 QX 规则源没有可导入规则".to_owned(),
            ));
        }
        let rules = parsed
            .to_profile_rules(|_source_policy| Some(target_policy.clone()))
            .map_err(|_error| LoadError::Runtime("无法映射 QX 规则策略".to_owned()))?;
        imported_rules.extend(rules);
    }
    if imported_rules.is_empty() {
        return Ok(());
    }
    let insert_at = profile
        .rules
        .iter()
        .position(|rule| matches!(rule, Rule::Match { .. }))
        .unwrap_or(profile.rules.len());
    profile.rules.splice(insert_at..insert_at, imported_rules);
    profile
        .validate()
        .map_err(|error| LoadError::Runtime(error.to_string()))
}

fn qx_rule_target_policy(
    target_policy: &Name,
    legacy_proxy_target: Option<&PolicyRef>,
) -> PolicyRef {
    match target_policy.as_str() {
        "DIRECT" => PolicyRef::Direct,
        "REJECT" => PolicyRef::Reject,
        LEGACY_GENERATED_PROXY_GROUP_NAME => legacy_proxy_target
            .cloned()
            .unwrap_or_else(|| PolicyRef::Group(target_policy.clone())),
        _ => PolicyRef::Group(target_policy.clone()),
    }
}

#[cfg(not(windows))]
pub(crate) fn save_collapsed_groups_in<'a>(
    directory: &Path,
    group_ids: impl IntoIterator<Item = &'a str>,
) -> Result<(), SubscriptionStoreError> {
    let mut ids: Vec<_> = group_ids
        .into_iter()
        .filter(|id| valid_workspace_group_id(id))
        .collect();
    ids.sort_unstable();
    ids.dedup();
    let contents = ids.join("\n");
    write_private_atomic(directory, WORKSPACE_STATE_FILE, contents.as_bytes())
        .map(|_path| ())
        .map_err(|_error| SubscriptionStoreError::StoreUnavailable)
}

#[cfg(windows)]
pub(crate) fn save_collapsed_groups_in<'a>(
    _directory: &Path,
    _group_ids: impl IntoIterator<Item = &'a str>,
) -> Result<(), SubscriptionStoreError> {
    Err(SubscriptionStoreError::StoreUnavailable)
}

#[cfg(not(windows))]
pub(crate) fn load_collapsed_groups_in(
    directory: &Path,
) -> Result<Vec<String>, SubscriptionStoreError> {
    require_clean_absolute_store(directory)?;
    let path = directory.join(WORKSPACE_STATE_FILE);
    let contents = match fs::symlink_metadata(&path) {
        Ok(_) => read_private_source_allow_empty(&path)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(_error) => return Err(SubscriptionStoreError::StoredSourceUnavailable),
    };
    contents
        .lines()
        .map(str::to_owned)
        .map(|id| {
            valid_workspace_group_id(&id)
                .then_some(id)
                .ok_or(SubscriptionStoreError::StoredSourceUnavailable)
        })
        .collect()
}

#[cfg(windows)]
pub(crate) fn load_collapsed_groups_in(
    _directory: &Path,
) -> Result<Vec<String>, SubscriptionStoreError> {
    Ok(Vec::new())
}

#[cfg(not(windows))]
pub(crate) fn save_routing_mode_in(
    directory: &Path,
    mode: RoutingMode,
) -> Result<(), SubscriptionStoreError> {
    write_private_atomic(directory, ROUTING_MODE_FILE, mode.wire_value().as_bytes())
        .map(|_path| ())
        .map_err(|_error| SubscriptionStoreError::StoreUnavailable)
}

#[cfg(windows)]
pub(crate) fn save_routing_mode_in(
    _directory: &Path,
    _mode: RoutingMode,
) -> Result<(), SubscriptionStoreError> {
    Err(SubscriptionStoreError::StoreUnavailable)
}

#[cfg(not(windows))]
pub(crate) fn load_routing_mode_in(
    directory: &Path,
) -> Result<RoutingMode, SubscriptionStoreError> {
    require_clean_absolute_store(directory)?;
    let path = directory.join(ROUTING_MODE_FILE);
    let contents = match fs::symlink_metadata(&path) {
        Ok(_) => read_private_source_allow_empty(&path)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(RoutingMode::Rule);
        }
        Err(_error) => return Err(SubscriptionStoreError::StoredSourceUnavailable),
    };
    RoutingMode::parse_wire_value(contents.trim())
        .ok_or(SubscriptionStoreError::StoredSourceUnavailable)
}

#[cfg(windows)]
pub(crate) fn load_routing_mode_in(
    _directory: &Path,
) -> Result<RoutingMode, SubscriptionStoreError> {
    Ok(RoutingMode::Rule)
}

pub(super) fn profile_mode(mode: RoutingMode) -> ProfileMode {
    match mode {
        RoutingMode::Direct => ProfileMode::Direct,
        RoutingMode::Global => ProfileMode::Global,
        RoutingMode::Rule => ProfileMode::Rule,
    }
}

#[cfg(not(windows))]
struct DecodedSubscriptionSource {
    stored: StoredSubscription,
    url_input: String,
}

#[cfg(not(windows))]
fn read_subscription_source_by_id_in(
    directory: &Path,
    id: &str,
) -> Result<DecodedSubscriptionSource, SubscriptionStoreError> {
    require_clean_absolute_store(directory)?;
    let file_name = subscription_source_file_name(id)?;
    let contents = read_private_source_allow_empty_max(
        &directory.join(file_name),
        MAX_STORED_SUBSCRIPTION_FILE_BYTES,
    )?;
    decode_subscription_source(&contents, id)
}

#[cfg(not(windows))]
#[derive(Clone, Copy)]
struct SubscriptionSourceWrite<'a> {
    id: &'a str,
    url_input: &'a str,
    name: &'a str,
    enabled: bool,
    refresh_interval: RemoteSourceRefreshInterval,
    last_successful_update_unix_secs: u64,
    proxy_server_nameservers: &'a [ProxyDnsServer],
}

#[cfg(not(windows))]
fn write_subscription_source_in(
    directory: &Path,
    write: SubscriptionSourceWrite<'_>,
) -> Result<StoredSubscription, SubscriptionStoreError> {
    let source = SecretUrl::parse_subscription(write.url_input)
        .map_err(|_error| SubscriptionStoreError::InvalidSource)?;
    let name = validate_subscription_source_name(write.name)?;
    let contents = encode_subscription_source(
        write.id,
        write.url_input,
        &name,
        write.enabled,
        write.refresh_interval,
        write.last_successful_update_unix_secs,
        write.proxy_server_nameservers,
    )?;
    let file_name = subscription_source_file_name(write.id)?;
    write_private_atomic(directory, &file_name, contents.as_bytes())
        .map_err(|_error| SubscriptionStoreError::StoreUnavailable)?;
    Ok(StoredSubscription {
        id: write.id.to_owned(),
        name,
        source,
        enabled: write.enabled,
        refresh_interval: write.refresh_interval,
        last_successful_update_unix_secs: write.last_successful_update_unix_secs,
        proxy_server_nameservers: write.proxy_server_nameservers.to_vec(),
    })
}

fn validate_subscription_source_name(name: &str) -> Result<String, SubscriptionStoreError> {
    let name = name.trim();
    if name.is_empty()
        || name.len() > MAX_SUBSCRIPTION_SOURCE_NAME_BYTES
        || name.chars().any(char::is_control)
    {
        return Err(SubscriptionStoreError::InvalidSource);
    }
    Ok(name.to_owned())
}

#[cfg(not(windows))]
fn encode_subscription_source(
    id: &str,
    url_input: &str,
    name: &str,
    enabled: bool,
    refresh_interval: RemoteSourceRefreshInterval,
    last_successful_update_unix_secs: u64,
    proxy_server_nameservers: &[ProxyDnsServer],
) -> Result<String, SubscriptionStoreError> {
    if !valid_subscription_source_id(id) {
        return Err(SubscriptionStoreError::InvalidSource);
    }
    if url_input.len() > crate::subscription::MAX_SUBSCRIPTION_BYTES {
        return Err(SubscriptionStoreError::InvalidSource);
    }
    SecretUrl::parse_subscription(url_input)
        .map_err(|_error| SubscriptionStoreError::InvalidSource)?;
    let name = validate_subscription_source_name(name)?;
    if proxy_server_nameservers.len() > MAX_SUBSCRIPTION_PROXY_DNS_SERVERS {
        return Err(SubscriptionStoreError::InvalidSource);
    }
    let mut lines = vec![
        STORED_SUBSCRIPTION_VERSION.to_owned(),
        format!("id\t{id}"),
        format!("name\t{}", encode_hex(&name)),
        format!("enabled\t{}", if enabled { "true" } else { "false" }),
        format!("url\t{}", encode_hex(url_input)),
        format!("refresh\t{}", refresh_interval.key()),
        format!("last-success\t{last_successful_update_unix_secs}"),
    ];
    lines.extend(
        proxy_server_nameservers
            .iter()
            .map(|nameserver| format!("proxy-dns\t{}", encode_hex(nameserver.as_str()))),
    );
    Ok(lines.join("\n"))
}

#[cfg(not(windows))]
fn decode_subscription_source(
    contents: &str,
    expected_id: &str,
) -> Result<DecodedSubscriptionSource, SubscriptionStoreError> {
    if !valid_subscription_source_id(expected_id) {
        return Err(SubscriptionStoreError::StoredSourceUnavailable);
    }
    let version = contents.lines().next();
    let structured = version.is_some_and(|version| {
        matches!(
            version,
            STORED_SUBSCRIPTION_VERSION
                | LEGACY_MANIS_STORED_SUBSCRIPTION_VERSION_V2
                | LEGACY_MANIS_STORED_SUBSCRIPTION_VERSION
                | LEGACY_RELAY_STORED_SUBSCRIPTION_VERSION
        )
    });
    if !structured {
        if contents.is_empty() || contents.lines().count() != 1 || contents.trim() != contents {
            return Err(SubscriptionStoreError::StoredSourceUnavailable);
        }
        return decode_legacy_subscription_source(contents, expected_id);
    }
    let fields = parse_subscription_source_fields(contents, version)?;
    let id = fields
        .id
        .ok_or(SubscriptionStoreError::StoredSourceUnavailable)?;
    if id != expected_id || !valid_subscription_source_id(&id) {
        return Err(SubscriptionStoreError::StoredSourceUnavailable);
    }
    let url_input = fields
        .url
        .ok_or(SubscriptionStoreError::StoredSourceUnavailable)?;
    let source = SecretUrl::parse_subscription(&url_input)
        .map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)?;
    let name = fields.name.unwrap_or_else(|| {
        source
            .subscription_name()
            .unwrap_or_else(|| "Subscription".to_owned())
    });
    Ok(DecodedSubscriptionSource {
        stored: StoredSubscription {
            id,
            name,
            source,
            enabled: fields.enabled.unwrap_or(true),
            refresh_interval: fields.refresh_interval.unwrap_or_default(),
            last_successful_update_unix_secs: fields.last_success.unwrap_or_default(),
            proxy_server_nameservers: fields.proxy_dns,
        },
        url_input,
    })
}

#[cfg(not(windows))]
fn decode_legacy_subscription_source(
    contents: &str,
    expected_id: &str,
) -> Result<DecodedSubscriptionSource, SubscriptionStoreError> {
    let source = SecretUrl::parse_subscription(contents)
        .map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)?;
    Ok(DecodedSubscriptionSource {
        stored: StoredSubscription {
            id: expected_id.to_owned(),
            name: source
                .subscription_name()
                .unwrap_or_else(|| "Subscription".to_owned()),
            source,
            enabled: true,
            refresh_interval: RemoteSourceRefreshInterval::Manual,
            last_successful_update_unix_secs: 0,
            proxy_server_nameservers: Vec::new(),
        },
        url_input: contents.to_owned(),
    })
}

#[cfg(not(windows))]
#[derive(Default)]
struct SubscriptionSourceFields {
    id: Option<String>,
    name: Option<String>,
    enabled: Option<bool>,
    url: Option<String>,
    refresh_interval: Option<RemoteSourceRefreshInterval>,
    last_success: Option<u64>,
    proxy_dns: Vec<ProxyDnsServer>,
}

#[cfg(not(windows))]
fn parse_subscription_source_fields(
    contents: &str,
    version: Option<&str>,
) -> Result<SubscriptionSourceFields, SubscriptionStoreError> {
    let mut parsed = SubscriptionSourceFields::default();
    for line in contents.lines().skip(1) {
        let fields = line.split('\t').collect::<Vec<_>>();
        match fields.as_slice() {
            ["id", value] if parsed.id.is_none() => parsed.id = Some((*value).to_owned()),
            ["name", value]
                if version == Some(STORED_SUBSCRIPTION_VERSION) && parsed.name.is_none() =>
            {
                parsed.name = Some(validate_subscription_source_name(&decode_hex(value)?)?);
            }
            ["enabled", value]
                if version == Some(STORED_SUBSCRIPTION_VERSION) && parsed.enabled.is_none() =>
            {
                parsed.enabled = Some(match *value {
                    "true" => true,
                    "false" => false,
                    _ => return Err(SubscriptionStoreError::StoredSourceUnavailable),
                });
            }
            ["url", value] if parsed.url.is_none() => parsed.url = Some(decode_hex(value)?),
            ["refresh", value] if parsed.refresh_interval.is_none() => {
                parsed.refresh_interval = Some(
                    RemoteSourceRefreshInterval::parse_key(value)
                        .ok_or(SubscriptionStoreError::StoredSourceUnavailable)?,
                );
            }
            ["last-success", value] if parsed.last_success.is_none() => {
                parsed.last_success = Some(
                    value
                        .parse::<u64>()
                        .map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)?,
                );
            }
            ["proxy-dns", value]
                if matches!(
                    version,
                    Some(STORED_SUBSCRIPTION_VERSION | LEGACY_MANIS_STORED_SUBSCRIPTION_VERSION_V2)
                ) && parsed.proxy_dns.len() < MAX_SUBSCRIPTION_PROXY_DNS_SERVERS =>
            {
                let decoded = decode_hex(value)?;
                let nameserver = ProxyDnsServer::parse_https(&decoded)
                    .map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)?;
                if !parsed.proxy_dns.contains(&nameserver) {
                    parsed.proxy_dns.push(nameserver);
                }
            }
            _ => return Err(SubscriptionStoreError::StoredSourceUnavailable),
        }
    }
    Ok(parsed)
}

#[cfg(not(windows))]
fn subscription_source_file_name(id: &str) -> Result<String, SubscriptionStoreError> {
    if id == "subscription:legacy" {
        Ok(IMPORTED_SUBSCRIPTION_FILE.to_owned())
    } else if valid_stored_id(id, STORED_SUBSCRIPTION_PREFIX) {
        Ok(format!("{id}{STORED_SUBSCRIPTION_SUFFIX}"))
    } else {
        Err(SubscriptionStoreError::StoreUnavailable)
    }
}

#[cfg(not(windows))]
fn valid_subscription_source_id(id: &str) -> bool {
    id == "subscription:legacy" || valid_stored_id(id, STORED_SUBSCRIPTION_PREFIX)
}

struct DecodedQxRuleSource {
    stored: StoredQxRuleSource,
    url_input: String,
}

#[cfg(not(windows))]
fn read_qx_rule_source_by_id_in(
    directory: &Path,
    id: &str,
) -> Result<DecodedQxRuleSource, SubscriptionStoreError> {
    require_clean_absolute_store(directory)?;
    let file_name = qx_rule_source_file_name(id)?;
    let contents = read_private_source_allow_empty_max(
        &directory.join(file_name),
        MAX_QX_RULE_SOURCE_FILE_BYTES,
    )?;
    decode_qx_rule_source_with_url(&contents, id)
}

#[cfg(not(windows))]
#[derive(Clone, Copy)]
struct QxRuleSourceWrite<'a> {
    id: &'a str,
    url_input: &'a str,
    target_policy: &'a str,
    content: &'a str,
    enabled: bool,
    refresh_interval: RemoteSourceRefreshInterval,
    last_successful_update_unix_secs: u64,
}

#[cfg(not(windows))]
fn write_qx_rule_source_in(
    directory: &Path,
    write: QxRuleSourceWrite<'_>,
) -> Result<StoredQxRuleSource, SubscriptionStoreError> {
    let source = SecretUrl::parse_https(write.url_input)
        .map_err(|_error| SubscriptionStoreError::InvalidSource)?;
    let target_policy =
        Name::parse(write.target_policy).map_err(|_error| SubscriptionStoreError::InvalidSource)?;
    let (rule_count, diagnostic_count) = validate_qx_rule_source_content(write.content)?;
    let contents = encode_qx_rule_source(
        write.id,
        write.url_input,
        &target_policy,
        write.content,
        write.enabled,
        write.refresh_interval,
        write.last_successful_update_unix_secs,
    )?;
    let file_name = qx_rule_source_file_name(write.id)?;
    write_private_atomic(directory, &file_name, contents.as_bytes())
        .map_err(|_error| SubscriptionStoreError::StoreUnavailable)?;
    Ok(StoredQxRuleSource {
        id: write.id.to_owned(),
        source,
        enabled: write.enabled,
        target_policy,
        content: write.content.to_owned(),
        rule_count,
        diagnostic_count,
        refresh_interval: write.refresh_interval,
        last_successful_update_unix_secs: write.last_successful_update_unix_secs,
    })
}

#[cfg(not(windows))]
fn qx_rule_source_file_name(id: &str) -> Result<String, SubscriptionStoreError> {
    if valid_stored_id(id, QX_RULE_SOURCE_PREFIX) {
        Ok(format!("{id}{QX_RULE_SOURCE_SUFFIX}"))
    } else {
        Err(SubscriptionStoreError::StoreUnavailable)
    }
}

fn encode_qx_rule_source(
    id: &str,
    url_input: &str,
    target_policy: &Name,
    content: &str,
    enabled: bool,
    refresh_interval: RemoteSourceRefreshInterval,
    last_successful_update_unix_secs: u64,
) -> Result<String, SubscriptionStoreError> {
    if !valid_stored_id(id, QX_RULE_SOURCE_PREFIX) {
        return Err(SubscriptionStoreError::InvalidSource);
    }
    SecretUrl::parse_https(url_input).map_err(|_error| SubscriptionStoreError::InvalidSource)?;
    validate_qx_rule_source_content(content)?;
    Ok([
        QX_RULE_SOURCE_VERSION.to_owned(),
        format!("id\t{id}"),
        format!("url\t{}", encode_hex(url_input)),
        format!("target\t{}", encode_hex(target_policy.as_str())),
        format!("content\t{}", encode_hex(content)),
        format!("enabled\t{}", u8::from(enabled)),
        format!("refresh\t{}", refresh_interval.key()),
        format!("last-success\t{last_successful_update_unix_secs}"),
    ]
    .join("\n"))
}

fn decode_qx_rule_source(
    contents: &str,
    expected_id: &str,
) -> Result<StoredQxRuleSource, SubscriptionStoreError> {
    decode_qx_rule_source_with_url(contents, expected_id).map(|decoded| decoded.stored)
}

fn decode_qx_rule_source_with_url(
    contents: &str,
    expected_id: &str,
) -> Result<DecodedQxRuleSource, SubscriptionStoreError> {
    let mut lines = contents.lines();
    let version = lines.next();
    if !matches!(
        version,
        Some(
            QX_RULE_SOURCE_VERSION
                | LEGACY_MANIS_QX_RULE_SOURCE_VERSION
                | LEGACY_RELAY_QX_RULE_SOURCE_VERSION
        )
    ) {
        return Err(SubscriptionStoreError::StoredSourceUnavailable);
    }
    let mut id = None;
    let mut url = None;
    let mut target = None;
    let mut content = None;
    let mut enabled = None;
    let mut refresh_interval = None;
    let mut last_successful_update_unix_secs = None;
    for line in lines {
        let fields = line.split('\t').collect::<Vec<_>>();
        match fields.as_slice() {
            ["id", value] if id.is_none() => id = Some((*value).to_owned()),
            ["url", value] if url.is_none() => url = Some(decode_hex(value)?),
            ["target", value] if target.is_none() => target = Some(decode_hex(value)?),
            ["content", value] if content.is_none() => content = Some(decode_hex(value)?),
            ["enabled", value] if enabled.is_none() => {
                enabled = Some(match *value {
                    "0" => false,
                    "1" => true,
                    _ => return Err(SubscriptionStoreError::StoredSourceUnavailable),
                });
            }
            ["refresh", value] if refresh_interval.is_none() => {
                refresh_interval = Some(
                    RemoteSourceRefreshInterval::parse_key(value)
                        .ok_or(SubscriptionStoreError::StoredSourceUnavailable)?,
                );
            }
            ["last-success", value] if last_successful_update_unix_secs.is_none() => {
                last_successful_update_unix_secs = Some(
                    value
                        .parse::<u64>()
                        .map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)?,
                );
            }
            _ => return Err(SubscriptionStoreError::StoredSourceUnavailable),
        }
    }
    let id = id.ok_or(SubscriptionStoreError::StoredSourceUnavailable)?;
    if id != expected_id || !valid_stored_id(&id, QX_RULE_SOURCE_PREFIX) {
        return Err(SubscriptionStoreError::StoredSourceUnavailable);
    }
    let url_input = url.ok_or(SubscriptionStoreError::StoredSourceUnavailable)?;
    let source = SecretUrl::parse_https(&url_input)
        .map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)?;
    let target_policy =
        Name::parse(&target.ok_or(SubscriptionStoreError::StoredSourceUnavailable)?)
            .map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)?;
    let content = content.ok_or(SubscriptionStoreError::StoredSourceUnavailable)?;
    let (rule_count, diagnostic_count) = validate_qx_rule_source_content(&content)?;
    Ok(DecodedQxRuleSource {
        stored: StoredQxRuleSource {
            id,
            source,
            enabled: enabled.unwrap_or(true),
            target_policy,
            content,
            rule_count,
            diagnostic_count,
            refresh_interval: refresh_interval.unwrap_or_default(),
            last_successful_update_unix_secs: last_successful_update_unix_secs.unwrap_or_default(),
        },
        url_input,
    })
}

fn validate_qx_rule_source_content(
    content: &str,
) -> Result<(usize, usize), SubscriptionStoreError> {
    if content.is_empty() || content.len() > MAX_QX_RULE_SOURCE_CONTENT_BYTES {
        return Err(SubscriptionStoreError::InvalidSource);
    }
    let parsed = QxRuleList::parse(content);
    if parsed.rules.is_empty() {
        return Err(SubscriptionStoreError::InvalidSource);
    }
    Ok((parsed.rules.len(), parsed.diagnostics.len()))
}

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

#[cfg(all(not(windows), test))]
pub(crate) fn save_imported_subscription_in(
    directory: &Path,
    input: &str,
) -> Result<SecretUrl, SubscriptionStoreError> {
    let subscription = SecretUrl::parse_subscription(input)
        .map_err(|_error| SubscriptionStoreError::InvalidSource)?;
    write_private_atomic(directory, IMPORTED_SUBSCRIPTION_FILE, input.as_bytes())
        .map_err(|_error| SubscriptionStoreError::StoreUnavailable)?;
    Ok(subscription)
}

#[cfg(all(windows, test))]
pub(crate) fn save_imported_subscription_in(
    _directory: &Path,
    _input: &str,
) -> Result<SecretUrl, SubscriptionStoreError> {
    Err(SubscriptionStoreError::StoreUnavailable)
}

#[cfg(all(not(windows), test))]
pub(crate) fn load_imported_subscription_in(
    directory: &Path,
) -> Result<Option<SecretUrl>, SubscriptionStoreError> {
    require_clean_absolute_store(directory)?;
    let path = directory.join(IMPORTED_SUBSCRIPTION_FILE);
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_error) => return Err(SubscriptionStoreError::StoredSourceUnavailable),
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(SubscriptionStoreError::StoredSourceUnavailable);
    }
    let file =
        fs::File::open(&path).map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)?;
    let opened_metadata = file
        .metadata()
        .map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)?;
    if opened_metadata.len() > MAX_SUBSCRIPTION_FILE_BYTES {
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
    file.take(MAX_SUBSCRIPTION_FILE_BYTES + 1)
        .read_to_string(&mut contents)
        .map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)?;
    if contents.len() as u64 > MAX_SUBSCRIPTION_FILE_BYTES {
        return Err(SubscriptionStoreError::StoredSourceUnavailable);
    }
    decode_subscription_source(&contents, "subscription:legacy")
        .map(|decoded| Some(decoded.stored.source))
}

#[cfg(windows)]
pub(crate) fn load_imported_subscription_in(
    directory: &Path,
) -> Result<Option<SecretUrl>, SubscriptionStoreError> {
    if directory.join(IMPORTED_SUBSCRIPTION_FILE).exists() {
        Err(SubscriptionStoreError::StoredSourceUnavailable)
    } else {
        Ok(None)
    }
}

#[cfg(all(not(windows), test))]
pub(crate) fn remove_imported_subscription_in(
    directory: &Path,
) -> Result<(), SubscriptionStoreError> {
    require_clean_absolute_store(directory)?;
    let path = directory.join(IMPORTED_SUBSCRIPTION_FILE);
    match fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            Err(SubscriptionStoreError::StoredSourceUnavailable)
        }
        Ok(_) => fs::remove_file(path).map_err(|_error| SubscriptionStoreError::StoreUnavailable),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_error) => Err(SubscriptionStoreError::StoreUnavailable),
    }
}

#[cfg(all(windows, test))]
pub(crate) fn remove_imported_subscription_in(
    _directory: &Path,
) -> Result<(), SubscriptionStoreError> {
    Err(SubscriptionStoreError::StoreUnavailable)
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
