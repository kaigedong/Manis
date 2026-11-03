use super::{
    LEGACY_GENERATED_PROXY_GROUP_NAME, LoadError, MANIS_GLOBAL_GROUP_NAME,
    MAX_QX_RULE_SOURCE_CONTENT_BYTES, Name, Path, PolicyRef, Profile, QxRuleList,
    RemoteSourceRefreshInterval, Rule, SaveQxRuleSourceOutcome, StoredQxRuleSource,
    SubscriptionStoreError, normalize_qx_rule_source_name, validate_subscription_source_name,
};
#[cfg(not(windows))]
use super::{
    LEGACY_MANIS_QX_RULE_SOURCE_VERSION, LEGACY_MANIS_QX_RULE_SOURCE_VERSION_V2,
    LEGACY_RELAY_QX_RULE_SOURCE_VERSION, MAX_QX_RULE_SOURCE_FILE_BYTES, QX_RULE_SOURCE_PREFIX,
    QX_RULE_SOURCE_SUFFIX, QX_RULE_SOURCE_VERSION, SecretUrl, current_unix_secs, decode_hex,
    encode_hex, next_stored_source_id, private_store_entries, read_private_source_allow_empty_max,
    remove_private_source, require_clean_absolute_store, valid_stored_id, write_private_atomic,
};

#[cfg(all(not(windows), test))]
pub(crate) fn save_qx_rule_source_in(
    directory: &Path,
    url_input: &str,
    target_policy: &str,
    content: &str,
) -> Result<SaveQxRuleSourceOutcome, SubscriptionStoreError> {
    save_named_qx_rule_source_in(directory, url_input, "", target_policy, content)
}

#[cfg(not(windows))]
pub(crate) fn save_named_qx_rule_source_in(
    directory: &Path,
    url_input: &str,
    name: &str,
    target_policy: &str,
    content: &str,
) -> Result<SaveQxRuleSourceOutcome, SubscriptionStoreError> {
    let source = SecretUrl::parse_https(url_input)
        .map_err(|_error| SubscriptionStoreError::InvalidSource)?;
    let name = normalize_qx_rule_source_name(name)?;
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
    let contents = encode_qx_rule_source(QxRuleSourceEncoding {
        id: &id,
        url_input,
        name: name.as_deref(),
        target_policy: &target_policy,
        content,
        enabled: true,
        refresh_interval: RemoteSourceRefreshInterval::Manual,
        last_successful_update_unix_secs,
    })?;
    write_private_atomic(directory, &file_name, contents.as_bytes())
        .map_err(|_error| SubscriptionStoreError::StoreUnavailable)?;
    Ok(SaveQxRuleSourceOutcome::Created(StoredQxRuleSource {
        id,
        name,
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
pub(crate) fn save_named_qx_rule_source_in(
    _directory: &Path,
    _url_input: &str,
    _name: &str,
    _target_policy: &str,
    _content: &str,
) -> Result<SaveQxRuleSourceOutcome, SubscriptionStoreError> {
    Err(SubscriptionStoreError::StoreUnavailable)
}

#[cfg(all(windows, test))]
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
            name: decoded.stored.name.as_deref(),
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
pub(crate) fn update_qx_rule_source_name_in(
    directory: &Path,
    id: &str,
    name: &str,
) -> Result<StoredQxRuleSource, SubscriptionStoreError> {
    let name = normalize_qx_rule_source_name(name)?;
    let decoded = read_qx_rule_source_by_id_in(directory, id)?;
    write_qx_rule_source_in(
        directory,
        QxRuleSourceWrite {
            id,
            url_input: &decoded.url_input,
            name: name.as_deref(),
            target_policy: decoded.stored.target_policy.as_str(),
            content: &decoded.stored.content,
            enabled: decoded.stored.enabled,
            refresh_interval: decoded.stored.refresh_interval,
            last_successful_update_unix_secs: decoded.stored.last_successful_update_unix_secs,
        },
    )
}

#[cfg(windows)]
pub(crate) fn update_qx_rule_source_name_in(
    _directory: &Path,
    _id: &str,
    _name: &str,
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
            name: decoded.stored.name.as_deref(),
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
#[allow(clippy::too_many_arguments)]
pub(crate) fn replace_qx_rule_source_definition_in(
    directory: &Path,
    id: &str,
    name: &str,
    url_input: &str,
    target_policy: &str,
    content: &str,
    refresh_interval: RemoteSourceRefreshInterval,
    last_successful_update_unix_secs: u64,
) -> Result<StoredQxRuleSource, SubscriptionStoreError> {
    let decoded = read_qx_rule_source_by_id_in(directory, id)?;
    let name = normalize_qx_rule_source_name(name)?;
    write_qx_rule_source_in(
        directory,
        QxRuleSourceWrite {
            id,
            url_input,
            name: name.as_deref(),
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
            name: decoded.stored.name.as_deref(),
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
#[allow(clippy::too_many_arguments)]
pub(crate) fn replace_qx_rule_source_definition_in(
    _directory: &Path,
    _id: &str,
    _name: &str,
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
            name: decoded.stored.name.as_deref(),
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

pub(crate) fn apply_qx_rule_sources(
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
    name: Option<&'a str>,
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
    let name = match write.name {
        Some(name) => normalize_qx_rule_source_name(name)?,
        None => None,
    };
    let target_policy =
        Name::parse(write.target_policy).map_err(|_error| SubscriptionStoreError::InvalidSource)?;
    let (rule_count, diagnostic_count) = validate_qx_rule_source_content(write.content)?;
    let contents = encode_qx_rule_source(QxRuleSourceEncoding {
        id: write.id,
        url_input: write.url_input,
        name: name.as_deref(),
        target_policy: &target_policy,
        content: write.content,
        enabled: write.enabled,
        refresh_interval: write.refresh_interval,
        last_successful_update_unix_secs: write.last_successful_update_unix_secs,
    })?;
    let file_name = qx_rule_source_file_name(write.id)?;
    write_private_atomic(directory, &file_name, contents.as_bytes())
        .map_err(|_error| SubscriptionStoreError::StoreUnavailable)?;
    Ok(StoredQxRuleSource {
        id: write.id.to_owned(),
        name,
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

#[derive(Clone, Copy)]
#[cfg(not(windows))]
struct QxRuleSourceEncoding<'a> {
    id: &'a str,
    url_input: &'a str,
    name: Option<&'a str>,
    target_policy: &'a Name,
    content: &'a str,
    enabled: bool,
    refresh_interval: RemoteSourceRefreshInterval,
    last_successful_update_unix_secs: u64,
}

#[cfg(not(windows))]
fn encode_qx_rule_source(
    write: QxRuleSourceEncoding<'_>,
) -> Result<String, SubscriptionStoreError> {
    let QxRuleSourceEncoding {
        id,
        url_input,
        name,
        target_policy,
        content,
        enabled,
        refresh_interval,
        last_successful_update_unix_secs,
    } = write;
    if !valid_stored_id(id, QX_RULE_SOURCE_PREFIX) {
        return Err(SubscriptionStoreError::InvalidSource);
    }
    SecretUrl::parse_https(url_input).map_err(|_error| SubscriptionStoreError::InvalidSource)?;
    let name = match name {
        Some(name) => normalize_qx_rule_source_name(name)?,
        None => None,
    };
    validate_qx_rule_source_content(content)?;
    let mut lines = vec![
        QX_RULE_SOURCE_VERSION.to_owned(),
        format!("id\t{id}"),
        format!("url\t{}", encode_hex(url_input)),
    ];
    if let Some(name) = name.as_ref() {
        lines.push(format!("name\t{}", encode_hex(name)));
    }
    lines.extend([
        format!("target\t{}", encode_hex(target_policy.as_str())),
        format!("content\t{}", encode_hex(content)),
        format!("enabled\t{}", u8::from(enabled)),
        format!("refresh\t{}", refresh_interval.key()),
        format!("last-success\t{last_successful_update_unix_secs}"),
    ]);
    Ok(lines.join("\n"))
}

#[cfg(not(windows))]
fn decode_qx_rule_source(
    contents: &str,
    expected_id: &str,
) -> Result<StoredQxRuleSource, SubscriptionStoreError> {
    decode_qx_rule_source_with_url(contents, expected_id).map(|decoded| decoded.stored)
}

#[cfg(not(windows))]
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
                | LEGACY_MANIS_QX_RULE_SOURCE_VERSION_V2
                | LEGACY_MANIS_QX_RULE_SOURCE_VERSION
                | LEGACY_RELAY_QX_RULE_SOURCE_VERSION
        )
    ) {
        return Err(SubscriptionStoreError::StoredSourceUnavailable);
    }
    let mut id = None;
    let mut url = None;
    let mut name = None;
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
            ["name", value] if version == Some(QX_RULE_SOURCE_VERSION) && name.is_none() => {
                name = Some(validate_subscription_source_name(&decode_hex(value)?)?);
            }
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
            name,
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
