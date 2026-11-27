use super::{
    CUSTOM_PROXY_DNS_FILE, CUSTOM_PROXY_DNS_VERSION, MAX_CUSTOM_PROXY_DNS_FILE_BYTES,
    MAX_SUBSCRIPTION_PROXY_DNS_SERVERS, Path, ProfileMode, ProxyDnsServer, ROUTING_MODE_FILE,
    RoutingMode, SubscriptionStoreError, WORKSPACE_STATE_FILE, decode_hex, encode_hex,
    valid_workspace_group_id,
};
#[cfg(not(windows))]
use super::{require_clean_absolute_store, write_private_atomic};

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
    let contents = crate::config_toml::read_entry(
        directory,
        WORKSPACE_STATE_FILE,
        super::MAX_SUBSCRIPTION_FILE_BYTES,
    )
    .map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)?
    .unwrap_or_default();
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
    let Some(contents) = crate::config_toml::read_entry(
        directory,
        ROUTING_MODE_FILE,
        super::MAX_SUBSCRIPTION_FILE_BYTES,
    )
    .map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)?
    else {
        return Ok(RoutingMode::Rule);
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

pub(crate) fn profile_mode(mode: RoutingMode) -> ProfileMode {
    match mode {
        RoutingMode::Direct => ProfileMode::Direct,
        RoutingMode::Global => ProfileMode::Global,
        RoutingMode::Rule => ProfileMode::Rule,
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct StoredProxyDnsEntry {
    pub(crate) server: ProxyDnsServer,
    pub(crate) enabled: bool,
}

#[cfg(not(windows))]
pub(crate) fn save_custom_proxy_dns_entries_in(
    directory: &Path,
    entries: &[StoredProxyDnsEntry],
) -> Result<(), SubscriptionStoreError> {
    require_clean_absolute_store(directory)?;
    if entries.len() > MAX_SUBSCRIPTION_PROXY_DNS_SERVERS {
        return Err(SubscriptionStoreError::InvalidSource);
    }
    let mut deduplicated: Vec<&StoredProxyDnsEntry> = Vec::with_capacity(entries.len());
    for entry in entries {
        if !deduplicated
            .iter()
            .any(|existing| existing.server == entry.server)
        {
            deduplicated.push(entry);
        }
    }
    if deduplicated.is_empty() {
        return crate::config_toml::remove_entry(directory, CUSTOM_PROXY_DNS_FILE)
            .map_err(|_error| SubscriptionStoreError::StoreUnavailable);
    }
    let mut lines = Vec::with_capacity(deduplicated.len() + 1);
    lines.push(CUSTOM_PROXY_DNS_VERSION.to_owned());
    for entry in deduplicated {
        let key = if entry.enabled {
            "proxy-dns"
        } else {
            "proxy-dns-disabled"
        };
        lines.push(format!("{key}\t{}", encode_hex(entry.server.as_str())));
    }
    let contents = format!("{}\n", lines.join("\n"));
    write_private_atomic(directory, CUSTOM_PROXY_DNS_FILE, contents.as_bytes())
        .map(|_path| ())
        .map_err(|_error| SubscriptionStoreError::StoreUnavailable)
}

#[cfg(windows)]
pub(crate) fn save_custom_proxy_dns_entries_in(
    _directory: &Path,
    _entries: &[StoredProxyDnsEntry],
) -> Result<(), SubscriptionStoreError> {
    Err(SubscriptionStoreError::StoreUnavailable)
}

#[cfg(not(windows))]
pub(crate) fn load_custom_proxy_dns_entries_in(
    directory: &Path,
) -> Result<Vec<StoredProxyDnsEntry>, SubscriptionStoreError> {
    require_clean_absolute_store(directory)?;
    let Some(contents) = crate::config_toml::read_entry(
        directory,
        CUSTOM_PROXY_DNS_FILE,
        MAX_CUSTOM_PROXY_DNS_FILE_BYTES,
    )
    .map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)?
    else {
        return Ok(Vec::new());
    };
    let mut lines = contents.lines().filter(|line| !line.trim().is_empty());
    let Some(version) = lines.next() else {
        return Ok(Vec::new());
    };
    if version != CUSTOM_PROXY_DNS_VERSION {
        return Err(SubscriptionStoreError::StoredSourceUnavailable);
    }
    let mut entries: Vec<StoredProxyDnsEntry> = Vec::new();
    for line in lines {
        let Some((key, encoded)) = line.split_once('\t') else {
            return Err(SubscriptionStoreError::StoredSourceUnavailable);
        };
        let enabled = match key {
            "proxy-dns" => true,
            "proxy-dns-disabled" => false,
            _ => return Err(SubscriptionStoreError::StoredSourceUnavailable),
        };
        let decoded = decode_hex(encoded)?;
        let server = ProxyDnsServer::parse_https(&decoded)
            .map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)?;
        if entries.len() >= MAX_SUBSCRIPTION_PROXY_DNS_SERVERS {
            return Err(SubscriptionStoreError::StoredSourceUnavailable);
        }
        if !entries.iter().any(|existing| existing.server == server) {
            entries.push(StoredProxyDnsEntry { server, enabled });
        }
    }
    Ok(entries)
}

#[cfg(windows)]
pub(crate) fn load_custom_proxy_dns_entries_in(
    _directory: &Path,
) -> Result<Vec<StoredProxyDnsEntry>, SubscriptionStoreError> {
    Ok(Vec::new())
}

#[cfg(test)]
pub(crate) fn save_custom_proxy_dns_servers_in(
    directory: &Path,
    servers: &[ProxyDnsServer],
) -> Result<(), SubscriptionStoreError> {
    let entries: Vec<_> = servers
        .iter()
        .cloned()
        .map(|server| StoredProxyDnsEntry {
            server,
            enabled: true,
        })
        .collect();
    save_custom_proxy_dns_entries_in(directory, &entries)
}

pub(crate) fn load_custom_proxy_dns_servers_in(
    directory: &Path,
) -> Result<Vec<ProxyDnsServer>, SubscriptionStoreError> {
    Ok(load_custom_proxy_dns_entries_in(directory)?
        .into_iter()
        .filter(|entry| entry.enabled)
        .map(|entry| entry.server)
        .collect())
}
