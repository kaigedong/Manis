use super::super::{
    BTreeMap, LEGACY_RELAY_NODE_SELECTION_PREFERENCES_VERSION, MAX_NODE_SELECTION_FILE_BYTES,
    MAX_NODE_SELECTION_POLICY_TARGETS, NODE_SELECTION_PREFERENCES_FILE,
    NODE_SELECTION_PREFERENCES_VERSION, Name, NodeIdentity, Path, SubscriptionStoreError,
    decode_hex, encode_hex, storage_version_supported,
};
#[cfg(not(windows))]
use super::super::{require_clean_absolute_store, write_private_atomic};

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
        if !self.policy_targets.contains_key(policy)
            && self.policy_targets.len() >= MAX_NODE_SELECTION_POLICY_TARGETS
        {
            return Err(SubscriptionStoreError::InvalidSource);
        }
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
    let Some(contents) = crate::config_toml::read_entry(
        directory,
        NODE_SELECTION_PREFERENCES_FILE,
        MAX_NODE_SELECTION_FILE_BYTES,
    )
    .map_err(|_error| SubscriptionStoreError::StoredSourceUnavailable)?
    else {
        return Ok(NodeSelectionPreferences::default());
    };
    decode_node_selection_preferences(&contents)
}

#[cfg(windows)]
pub(crate) fn load_node_selection_preferences_in(
    _directory: &Path,
) -> Result<NodeSelectionPreferences, SubscriptionStoreError> {
    Ok(NodeSelectionPreferences::default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_selection_preferences_enforce_policy_target_capacity_on_insert() {
        let mut preferences = NodeSelectionPreferences::default();
        for index in 0..MAX_NODE_SELECTION_POLICY_TARGETS {
            preferences
                .set_policy_target(format!("Policy {index}"), "Tokyo")
                .expect("target under capacity should be accepted");
        }

        preferences
            .set_policy_target("Policy 0", "Osaka")
            .expect("updating an existing target should stay valid");
        assert_eq!(preferences.policy_target("Policy 0"), Some("Osaka"));
        assert_eq!(
            preferences.set_policy_target("Overflow", "Tokyo"),
            Err(SubscriptionStoreError::InvalidSource)
        );
        assert_eq!(
            preferences.policy_targets.len(),
            MAX_NODE_SELECTION_POLICY_TARGETS
        );
    }
}
