use manis_core::{ManagedPolicyGroup, ManagedPolicyIcon, ManagedPolicyStrategy};

use super::{
    BTreeMap, BTreeSet, HashMap, LEGACY_MANAGED_POLICY_PREFIX, LEGACY_MANAGED_POLICY_SUFFIX,
    LEGACY_MANIS_MANAGED_POLICY_VERSION, LEGACY_RELAY_MANAGED_POLICY_VERSION,
    LEGACY_RELAY_NODE_SELECTION_PREFERENCES_VERSION, LoadError, MANAGED_POLICY_PREFIX,
    MANAGED_POLICY_SUFFIX, MANAGED_POLICY_VERSION, MAX_MANAGED_POLICIES,
    MAX_NODE_SELECTION_FILE_BYTES, MAX_NODE_SELECTION_POLICY_TARGETS, MAX_SUBSCRIPTION_FILE_BYTES,
    NODE_SELECTION_PREFERENCES_FILE, NODE_SELECTION_PREFERENCES_VERSION, Name, NodeIdentity, Path,
    PolicyCandidateMatcher, PolicyRef, StoredSingleNode, SubscriptionStoreError, UserPolicyGroup,
    UserPolicyGroupKind, VlessProxy, decode_hex, encode_hex, next_stored_source_id,
    storage_version_supported, valid_stored_id,
};
#[cfg(not(windows))]
use super::{
    fs, private_store_entries, read_private_source_allow_empty,
    read_private_source_allow_empty_max, remove_private_source, require_clean_absolute_store,
    write_private_atomic,
};

pub(crate) fn new_managed_policy_id() -> String {
    next_stored_source_id(MANAGED_POLICY_PREFIX)
}

fn direct_policy_for_member(
    member: &NodeIdentity,
    current_group_id: &str,
    groups: &[ManagedPolicyGroup],
) -> Result<Option<PolicyRef>, LoadError> {
    if member.source_id == "builtin" {
        return Ok(match member.node_name.as_str() {
            "DIRECT" => Some(PolicyRef::Direct),
            "REJECT" => Some(PolicyRef::Reject),
            _ => None,
        });
    }
    let Some(policy_id) = member.source_id.strip_prefix("policy:") else {
        return Ok(None);
    };
    if policy_id == current_group_id {
        return Ok(None);
    }
    groups
        .iter()
        .find(|candidate| candidate.id == policy_id)
        .map(|candidate| {
            Name::parse(&candidate.name)
                .map(PolicyRef::Group)
                .map_err(|error| LoadError::Runtime(error.to_string()))
        })
        .transpose()
}

pub(super) fn compile_managed_policy_groups(
    groups: &[ManagedPolicyGroup],
    stored_provider_indexes: &HashMap<String, usize>,
    stored_single_nodes: &[StoredSingleNode],
    vless_nodes: &[VlessProxy],
    provider_count: usize,
) -> Result<Vec<UserPolicyGroup>, LoadError> {
    validate_managed_policy_references(groups)?;
    let context = ManagedPolicyCompileContext {
        groups,
        stored_provider_indexes,
        stored_single_nodes,
        vless_nodes,
        provider_count,
    };
    groups
        .iter()
        .map(|group| compile_managed_policy_group(group, &context))
        .collect()
}

struct ManagedPolicyCompileContext<'a> {
    groups: &'a [ManagedPolicyGroup],
    stored_provider_indexes: &'a HashMap<String, usize>,
    stored_single_nodes: &'a [StoredSingleNode],
    vless_nodes: &'a [VlessProxy],
    provider_count: usize,
}

fn compile_managed_policy_group(
    group: &ManagedPolicyGroup,
    context: &ManagedPolicyCompileContext<'_>,
) -> Result<UserPolicyGroup, LoadError> {
    let (provider_indexes, direct_proxies, direct_policies, filter) =
        compile_policy_candidates(group, context)?;
    if provider_indexes.is_empty() && direct_proxies.is_empty() && direct_policies.is_empty() {
        return Err(LoadError::Runtime(format!(
            "policy group '{}' matched no available nodes",
            group.name
        )));
    }
    let kind = match group.strategy {
        ManagedPolicyStrategy::Manual => UserPolicyGroupKind::Select,
        ManagedPolicyStrategy::LowestLatency => UserPolicyGroupKind::UrlTest {
            tolerance: 50,
            interval_secs: group.test_interval_secs,
        },
    };
    Ok(UserPolicyGroup {
        name: Name::parse(&group.name).map_err(|error| LoadError::Runtime(error.to_string()))?,
        icon: None,
        kind,
        provider_indexes,
        direct_proxies,
        direct_policies,
        filter,
    })
}

type CompiledPolicyCandidates = (Vec<usize>, Vec<Name>, Vec<PolicyRef>, Option<String>);

fn compile_policy_candidates(
    group: &ManagedPolicyGroup,
    context: &ManagedPolicyCompileContext<'_>,
) -> Result<CompiledPolicyCandidates, LoadError> {
    let mut provider_indexes = Vec::new();
    let mut direct_proxies = Vec::new();
    let mut direct_policies = Vec::new();
    let filter = match &group.matcher {
        PolicyCandidateMatcher::All => {
            provider_indexes.extend(0..context.provider_count);
            direct_proxies.extend(context.vless_nodes.iter().map(|proxy| proxy.name().clone()));
            None
        }
        PolicyCandidateMatcher::NameContains(fragment) => {
            provider_indexes.extend(0..context.provider_count);
            let lowercase = fragment.to_lowercase();
            direct_proxies.extend(
                context
                    .vless_nodes
                    .iter()
                    .filter(|proxy| proxy.name().as_str().to_lowercase().contains(&lowercase))
                    .map(|proxy| proxy.name().clone()),
            );
            Some(format!("(?i){}", escape_regex(fragment)))
        }
        PolicyCandidateMatcher::Explicit(members) => compile_explicit_policy_candidates(
            group,
            members,
            context,
            &mut provider_indexes,
            &mut direct_proxies,
            &mut direct_policies,
        )?,
    };
    Ok((provider_indexes, direct_proxies, direct_policies, filter))
}

fn compile_explicit_policy_candidates(
    group: &ManagedPolicyGroup,
    members: &BTreeSet<NodeIdentity>,
    context: &ManagedPolicyCompileContext<'_>,
    provider_indexes: &mut Vec<usize>,
    direct_proxies: &mut Vec<Name>,
    direct_policies: &mut Vec<PolicyRef>,
) -> Result<Option<String>, LoadError> {
    let mut provider_names = Vec::new();
    for member in members {
        if member.source_id == "builtin" || member.source_id.starts_with("policy:") {
            if let Some(policy) = direct_policy_for_member(member, &group.id, context.groups)? {
                direct_policies.push(policy);
            }
            continue;
        }
        if member.source_id == "saved" {
            if let Some(stored) = context
                .stored_single_nodes
                .iter()
                .find(|stored| stored.source.preview().name == member.node_name)
                && let Some(index) = context
                    .stored_provider_indexes
                    .get(stored.id.as_str())
                    .copied()
            {
                if !provider_indexes.contains(&index) {
                    provider_indexes.push(index);
                }
            } else if let Some(proxy) = context
                .vless_nodes
                .iter()
                .find(|proxy| proxy.name().as_str() == member.node_name)
            {
                direct_proxies.push(proxy.name().clone());
            }
            continue;
        }
        let Some(stored_id) = member.source_id.strip_prefix("subscription:") else {
            continue;
        };
        let Some(index) = context.stored_provider_indexes.get(stored_id).copied() else {
            continue;
        };
        if !provider_indexes.contains(&index) {
            provider_indexes.push(index);
        }
        provider_names.push(member.node_name.as_str());
    }
    Ok((!provider_names.is_empty()).then(|| {
        format!(
            "^(?:{})$",
            provider_names
                .into_iter()
                .map(escape_regex)
                .collect::<Vec<_>>()
                .join("|")
        )
    }))
}

pub(crate) fn validate_managed_policy_references(
    groups: &[ManagedPolicyGroup],
) -> Result<(), LoadError> {
    fn visit(
        id: &str,
        groups: &[ManagedPolicyGroup],
        visiting: &mut BTreeSet<String>,
        visited: &mut BTreeSet<String>,
    ) -> Result<(), LoadError> {
        if visited.contains(id) {
            return Ok(());
        }
        if !visiting.insert(id.to_owned()) {
            return Err(LoadError::Runtime(
                "policy groups cannot contain cyclic references".to_owned(),
            ));
        }
        let group = groups.iter().find(|group| group.id == id).ok_or_else(|| {
            LoadError::Runtime("referenced policy group does not exist".to_owned())
        })?;
        if let PolicyCandidateMatcher::Explicit(members) = &group.matcher {
            for member in members {
                if let Some(candidate_id) = member.source_id.strip_prefix("policy:") {
                    if candidate_id == id {
                        return Err(LoadError::Runtime(
                            "a policy group cannot reference itself".to_owned(),
                        ));
                    }
                    visit(candidate_id, groups, visiting, visited)?;
                }
            }
        }
        visiting.remove(id);
        visited.insert(id.to_owned());
        Ok(())
    }

    let mut visiting = BTreeSet::new();
    let mut visited = BTreeSet::new();
    for group in groups {
        visit(&group.id, groups, &mut visiting, &mut visited)?;
    }
    Ok(())
}

fn escape_regex(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        if matches!(
            character,
            '\\' | '.' | '+' | '*' | '?' | '(' | ')' | '|' | '[' | ']' | '{' | '}' | '^' | '$'
        ) {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    escaped
}

#[cfg(not(windows))]
pub(crate) fn save_managed_policy_in(
    directory: &Path,
    group: &ManagedPolicyGroup,
) -> Result<(), SubscriptionStoreError> {
    group
        .validate()
        .map_err(|_error| SubscriptionStoreError::InvalidSource)?;
    if !valid_managed_policy_id(&group.id) {
        return Err(SubscriptionStoreError::InvalidSource);
    }
    let contents = encode_managed_policy(group)?;
    let file_name = format!("{}{MANAGED_POLICY_SUFFIX}", group.id);
    write_private_atomic(directory, &file_name, contents.as_bytes())
        .map(|_path| ())
        .map_err(|_error| SubscriptionStoreError::StoreUnavailable)
}

#[cfg(windows)]
pub(crate) fn save_managed_policy_in(
    _directory: &Path,
    _group: &ManagedPolicyGroup,
) -> Result<(), SubscriptionStoreError> {
    Err(SubscriptionStoreError::StoreUnavailable)
}

#[cfg(not(windows))]
pub(crate) fn load_managed_policy_groups_in(
    directory: &Path,
) -> Result<Vec<ManagedPolicyGroup>, SubscriptionStoreError> {
    let mut groups = BTreeMap::new();
    let Some(entries) = private_store_entries(directory)? else {
        return Ok(Vec::new());
    };
    for path in &entries {
        let Some(file_name) = path.file_name().and_then(std::ffi::OsStr::to_str) else {
            continue;
        };
        let Some(id) = file_name.strip_suffix(MANAGED_POLICY_SUFFIX) else {
            continue;
        };
        if !valid_managed_policy_id(id) {
            continue;
        }
        let contents = read_private_source_allow_empty(path)?;
        let group = decode_managed_policy(&contents, id)?;
        groups.insert(group.id.clone(), group);
        if groups.len() > MAX_MANAGED_POLICIES {
            return Err(SubscriptionStoreError::StoredSourceUnavailable);
        }
    }
    for path in entries {
        let Some(file_name) = path.file_name().and_then(std::ffi::OsStr::to_str) else {
            continue;
        };
        let Some(id) = file_name.strip_suffix(LEGACY_MANAGED_POLICY_SUFFIX) else {
            continue;
        };
        if !valid_stored_id(id, LEGACY_MANAGED_POLICY_PREFIX) {
            continue;
        }
        if !groups.contains_key(id) {
            let contents = read_private_source_allow_empty(&path)?;
            let group = decode_managed_policy(&contents, id)?;
            save_managed_policy_in(directory, &group)?;
            groups.insert(group.id.clone(), group);
        }
        remove_private_source(&path)?;
        if groups.len() > MAX_MANAGED_POLICIES {
            return Err(SubscriptionStoreError::StoredSourceUnavailable);
        }
    }
    Ok(groups.into_values().collect())
}

#[cfg(windows)]
pub(crate) fn load_managed_policy_groups_in(
    _directory: &Path,
) -> Result<Vec<ManagedPolicyGroup>, SubscriptionStoreError> {
    Ok(Vec::new())
}

#[cfg(not(windows))]
pub(crate) fn remove_managed_policy_in(
    directory: &Path,
    id: &str,
) -> Result<(), SubscriptionStoreError> {
    require_clean_absolute_store(directory)?;
    if !valid_managed_policy_id(id) {
        return Err(SubscriptionStoreError::StoreUnavailable);
    }
    remove_private_source(&directory.join(format!("{id}{MANAGED_POLICY_SUFFIX}")))?;
    remove_private_source(&directory.join(format!("{id}{LEGACY_MANAGED_POLICY_SUFFIX}")))
}

#[cfg(windows)]
pub(crate) fn remove_managed_policy_in(
    _directory: &Path,
    _id: &str,
) -> Result<(), SubscriptionStoreError> {
    Err(SubscriptionStoreError::StoreUnavailable)
}

fn encode_managed_policy(group: &ManagedPolicyGroup) -> Result<String, SubscriptionStoreError> {
    group
        .validate()
        .map_err(|_error| SubscriptionStoreError::InvalidSource)?;
    let (matcher_key, filter, members): (&str, &str, Vec<&NodeIdentity>) = match &group.matcher {
        PolicyCandidateMatcher::All => ("all", "", Vec::new()),
        PolicyCandidateMatcher::NameContains(value) => ("name", value, Vec::new()),
        PolicyCandidateMatcher::Explicit(members) => ("explicit", "", members.iter().collect()),
    };
    let mut lines = vec![
        MANAGED_POLICY_VERSION.to_owned(),
        format!("id\t{}", group.id),
        format!("name\t{}", encode_hex(&group.name)),
        format!("icon\t{}", group.icon.key()),
        format!("strategy\t{}", group.strategy.key()),
        format!("interval\t{}", group.test_interval_secs),
        format!("matcher\t{matcher_key}"),
        format!("filter\t{}", encode_hex(filter)),
    ];
    lines.extend(members.into_iter().map(|member| {
        format!(
            "member\t{}\t{}",
            encode_hex(&member.source_id),
            encode_hex(&member.node_name)
        )
    }));
    let contents = lines.join("\n");
    if contents.len() as u64 > MAX_SUBSCRIPTION_FILE_BYTES {
        return Err(SubscriptionStoreError::InvalidSource);
    }
    Ok(contents)
}

fn decode_managed_policy(
    contents: &str,
    expected_id: &str,
) -> Result<ManagedPolicyGroup, SubscriptionStoreError> {
    let mut lines = contents.lines();
    if !lines.next().is_some_and(|version| {
        matches!(
            version,
            MANAGED_POLICY_VERSION
                | LEGACY_MANIS_MANAGED_POLICY_VERSION
                | LEGACY_RELAY_MANAGED_POLICY_VERSION
        )
    }) {
        return Err(SubscriptionStoreError::StoredSourceUnavailable);
    }
    let mut id = None;
    let mut name = None;
    let mut icon = None;
    let mut strategy = None;
    let mut interval = None;
    let mut matcher = None;
    let mut filter = None;
    let mut members = BTreeSet::new();
    for line in lines {
        let fields = line.split('\t').collect::<Vec<_>>();
        match fields.as_slice() {
            ["id", value] if id.is_none() => id = Some((*value).to_owned()),
            ["name", value] if name.is_none() => name = Some(decode_hex(value)?),
            ["icon", value] if icon.is_none() => {
                icon = Some(
                    ManagedPolicyIcon::parse_key(value)
                        .map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)?,
                );
            }
            ["strategy", value] if strategy.is_none() => {
                strategy = Some(
                    ManagedPolicyStrategy::parse_key(value)
                        .map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)?,
                );
            }
            ["interval", value] if interval.is_none() => {
                interval = Some(
                    value
                        .parse::<u32>()
                        .map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)?,
                );
            }
            ["matcher", value] if matcher.is_none() => matcher = Some((*value).to_owned()),
            ["filter", value] if filter.is_none() => filter = Some(decode_hex(value)?),
            ["member", source, node] => {
                members.insert(
                    NodeIdentity::new(&decode_hex(source)?, &decode_hex(node)?)
                        .map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)?,
                );
            }
            _ => return Err(SubscriptionStoreError::StoredSourceUnavailable),
        }
    }
    let id = id.ok_or(SubscriptionStoreError::StoredSourceUnavailable)?;
    if id != expected_id {
        return Err(SubscriptionStoreError::StoredSourceUnavailable);
    }
    let mut group = ManagedPolicyGroup::new(
        &id,
        &name.ok_or(SubscriptionStoreError::StoredSourceUnavailable)?,
    )
    .map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)?;
    group.icon = icon.ok_or(SubscriptionStoreError::StoredSourceUnavailable)?;
    group.strategy = strategy.ok_or(SubscriptionStoreError::StoredSourceUnavailable)?;
    group
        .set_test_interval_secs(interval.unwrap_or(600))
        .map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)?;
    let filter = filter.ok_or(SubscriptionStoreError::StoredSourceUnavailable)?;
    let parsed_matcher = match matcher.as_deref() {
        Some("all") if filter.is_empty() && members.is_empty() => PolicyCandidateMatcher::All,
        Some("name") if !filter.is_empty() && members.is_empty() => {
            PolicyCandidateMatcher::name_contains(&filter)
                .map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)?
        }
        Some("explicit") if filter.is_empty() => PolicyCandidateMatcher::Explicit(members),
        _ => return Err(SubscriptionStoreError::StoredSourceUnavailable),
    };
    group
        .set_matcher(parsed_matcher)
        .map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)?;
    group
        .validate()
        .map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)?;
    Ok(group)
}

fn valid_managed_policy_id(id: &str) -> bool {
    valid_stored_id(id, MANAGED_POLICY_PREFIX) || valid_stored_id(id, LEGACY_MANAGED_POLICY_PREFIX)
}

fn encode_node_selection_preferences(
    preferences: &NodeSelectionPreferences,
) -> Result<String, SubscriptionStoreError> {
    preferences.validate()?;
    let mut lines = vec![NODE_SELECTION_PREFERENCES_VERSION.to_owned()];
    if let Some(global) = preferences.global() {
        let checked = NodeIdentity::new(&global.source_id, &global.node_name)
            .map_err(|_error| SubscriptionStoreError::InvalidSource)?;
        lines.push(format!(
            "global\t{}\t{}",
            encode_hex(&checked.source_id),
            encode_hex(&checked.node_name)
        ));
    }
    lines.extend(
        preferences.iter_policy_targets().map(|(policy, target)| {
            format!("policy\t{}\t{}", encode_hex(policy), encode_hex(target))
        }),
    );
    let contents = lines.join("\n");
    if contents.len() as u64 > MAX_NODE_SELECTION_FILE_BYTES {
        return Err(SubscriptionStoreError::InvalidSource);
    }
    Ok(contents)
}

fn decode_node_selection_preferences(
    contents: &str,
) -> Result<NodeSelectionPreferences, SubscriptionStoreError> {
    let mut lines = contents.lines();
    if !storage_version_supported(
        lines.next(),
        NODE_SELECTION_PREFERENCES_VERSION,
        LEGACY_RELAY_NODE_SELECTION_PREFERENCES_VERSION,
    ) {
        return Err(SubscriptionStoreError::StoredSourceUnavailable);
    }
    let mut preferences = NodeSelectionPreferences::default();
    let mut seen_global = false;
    for line in lines {
        let fields = line.split('\t').collect::<Vec<_>>();
        match fields.as_slice() {
            ["global", source, node] if !seen_global => {
                seen_global = true;
                let source = decode_hex(source)?;
                let node = decode_hex(node)?;
                preferences.global = Some(
                    NodeIdentity::new(&source, &node)
                        .map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)?,
                );
            }
            ["policy", policy, target] => {
                let policy = decode_hex(policy)?;
                let target = decode_hex(target)?;
                validate_node_selection_policy(&policy)
                    .map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)?;
                validate_node_selection_target(&target)
                    .map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)?;
                if preferences.policy_targets.insert(policy, target).is_some()
                    || preferences.policy_targets.len() > MAX_NODE_SELECTION_POLICY_TARGETS
                {
                    return Err(SubscriptionStoreError::StoredSourceUnavailable);
                }
            }
            _ => return Err(SubscriptionStoreError::StoredSourceUnavailable),
        }
    }
    Ok(preferences)
}

pub(super) fn validate_node_selection_policy(policy: &str) -> Result<(), SubscriptionStoreError> {
    Name::parse(policy)
        .map(|_name| ())
        .map_err(|_error| SubscriptionStoreError::InvalidSource)
}

pub(super) fn validate_node_selection_target(target: &str) -> Result<(), SubscriptionStoreError> {
    if valid_node_selection_target(target) {
        Ok(())
    } else {
        Err(SubscriptionStoreError::InvalidSource)
    }
}

fn valid_node_selection_target(target: &str) -> bool {
    !target.is_empty()
        && target.len() <= 512
        && target.trim() == target
        && !target.chars().any(char::is_control)
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct NodeSelectionPreferences {
    global: Option<NodeIdentity>,
    policy_targets: BTreeMap<String, String>,
}

impl NodeSelectionPreferences {
    pub(crate) fn global(&self) -> Option<&NodeIdentity> {
        self.global.as_ref()
    }

    pub(crate) fn set_global(&mut self, global: NodeIdentity) {
        self.global = Some(global);
    }

    pub(crate) fn policy_target(&self, policy: &str) -> Option<&str> {
        self.policy_targets.get(policy).map(String::as_str)
    }

    pub(crate) fn set_policy_target(
        &mut self,
        policy: impl AsRef<str>,
        target: impl AsRef<str>,
    ) -> Result<(), SubscriptionStoreError> {
        let policy = policy.as_ref();
        let target = target.as_ref();
        validate_node_selection_policy(policy)?;
        validate_node_selection_target(target)?;
        self.policy_targets
            .insert(policy.to_owned(), target.to_owned());
        Ok(())
    }

    pub(crate) fn iter_policy_targets(&self) -> impl Iterator<Item = (&str, &str)> {
        self.policy_targets
            .iter()
            .map(|(policy, target)| (policy.as_str(), target.as_str()))
    }

    fn validate(&self) -> Result<(), SubscriptionStoreError> {
        if self.policy_targets.len() > MAX_NODE_SELECTION_POLICY_TARGETS {
            return Err(SubscriptionStoreError::InvalidSource);
        }
        for (policy, target) in &self.policy_targets {
            validate_node_selection_policy(policy)?;
            validate_node_selection_target(target)?;
        }
        Ok(())
    }
}

#[cfg(not(windows))]
pub(crate) fn save_node_selection_preferences_in(
    directory: &Path,
    preferences: &NodeSelectionPreferences,
) -> Result<(), SubscriptionStoreError> {
    preferences.validate()?;
    let contents = encode_node_selection_preferences(preferences)?;
    write_private_atomic(
        directory,
        NODE_SELECTION_PREFERENCES_FILE,
        contents.as_bytes(),
    )
    .map(|_path| ())
    .map_err(|_error| SubscriptionStoreError::StoreUnavailable)
}

#[cfg(windows)]
pub(crate) fn save_node_selection_preferences_in(
    _directory: &Path,
    _preferences: &NodeSelectionPreferences,
) -> Result<(), SubscriptionStoreError> {
    Err(SubscriptionStoreError::StoreUnavailable)
}

#[cfg(not(windows))]
pub(crate) fn load_node_selection_preferences_in(
    directory: &Path,
) -> Result<NodeSelectionPreferences, SubscriptionStoreError> {
    require_clean_absolute_store(directory)?;
    let path = directory.join(NODE_SELECTION_PREFERENCES_FILE);
    let contents = match fs::symlink_metadata(&path) {
        Ok(_) => read_private_source_allow_empty_max(&path, MAX_NODE_SELECTION_FILE_BYTES)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(NodeSelectionPreferences::default());
        }
        Err(_error) => return Err(SubscriptionStoreError::StoredSourceUnavailable),
    };
    decode_node_selection_preferences(&contents)
}

#[cfg(windows)]
pub(crate) fn load_node_selection_preferences_in(
    _directory: &Path,
) -> Result<NodeSelectionPreferences, SubscriptionStoreError> {
    Ok(NodeSelectionPreferences::default())
}
