#[cfg(windows)]
use super::load_imported_subscription_in;
#[cfg(not(windows))]
use super::{
    IMPORTED_SUBSCRIPTION_FILE, LEGACY_MANIS_STORED_SUBSCRIPTION_VERSION,
    LEGACY_MANIS_STORED_SUBSCRIPTION_VERSION_V2, LEGACY_RELAY_STORED_SUBSCRIPTION_VERSION,
    MAX_STORED_SUBSCRIPTION_FILE_BYTES, MAX_SUBSCRIPTION_PROXY_DNS_SERVERS,
    STORED_SUBSCRIPTION_PREFIX, STORED_SUBSCRIPTION_SUFFIX, STORED_SUBSCRIPTION_VERSION, SecretUrl,
    current_unix_secs, decode_hex, encode_hex, next_stored_source_id, private_store_entries,
    read_private_source_allow_empty_max, remove_private_source, require_clean_absolute_store,
    valid_stored_id, write_private_atomic,
};
use super::{
    MAX_SUBSCRIPTION_SOURCE_NAME_BYTES, Path, PathBuf, ProxyDnsServer, RemoteSourceRefreshInterval,
    StoredSubscription, SubscriptionStoreError, brand,
};
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

#[cfg(not(windows))]
pub(super) struct DecodedSubscriptionSource {
    pub(super) stored: StoredSubscription,
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

pub(crate) fn validate_subscription_source_name(
    name: &str,
) -> Result<String, SubscriptionStoreError> {
    let name = name.trim();
    if name.is_empty()
        || name.len() > MAX_SUBSCRIPTION_SOURCE_NAME_BYTES
        || name.chars().any(char::is_control)
    {
        return Err(SubscriptionStoreError::InvalidSource);
    }
    Ok(name.to_owned())
}

pub(crate) fn normalize_qx_rule_source_name(
    name: &str,
) -> Result<Option<String>, SubscriptionStoreError> {
    let name = name.trim();
    if name.is_empty() {
        return Ok(None);
    }
    validate_subscription_source_name(name).map(Some)
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
pub(super) fn decode_subscription_source(
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
