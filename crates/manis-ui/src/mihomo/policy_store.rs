mod managed_policy;
mod node_selection;

pub(super) use managed_policy::compile_managed_policy_groups;
pub(crate) use managed_policy::{
    load_managed_policy_groups_in, new_managed_policy_id, remove_managed_policy_in,
    save_managed_policy_in, validate_managed_policy_references,
};
pub(crate) use node_selection::{
    NodeSelectionPreferences, load_node_selection_preferences_in,
    save_node_selection_preferences_in,
};

use super::{
    BTreeMap, BTreeSet, HashMap, LEGACY_MANAGED_POLICY_PREFIX, LEGACY_MANAGED_POLICY_SUFFIX,
    LEGACY_MANIS_MANAGED_POLICY_VERSION, LEGACY_RELAY_MANAGED_POLICY_VERSION, LoadError,
    MANAGED_POLICY_PREFIX, MANAGED_POLICY_SUFFIX, MANAGED_POLICY_VERSION, MANIS_GLOBAL_GROUP_NAME,
    MAX_MANAGED_POLICIES, MAX_SUBSCRIPTION_FILE_BYTES, Name, NodeIdentity, Path,
    PolicyCandidateMatcher, PolicyRef, StoredSingleNode, SubscriptionStoreError, UserPolicyGroup,
    UserPolicyGroupKind, VlessProxy, decode_hex, encode_hex, next_stored_source_id,
    valid_stored_id,
};
#[cfg(not(windows))]
use super::{
    private_store_entries, read_private_source_allow_empty, remove_private_source,
    require_clean_absolute_store,
};
use manis_core::{ManagedPolicyGroup, ManagedPolicyIcon, ManagedPolicyStrategy};

#[cfg(not(windows))]
fn write_private_atomic(
    directory: &Path,
    file_name: &str,
    bytes: &[u8],
) -> Result<std::path::PathBuf, crate::config_toml::ConfigTomlError> {
    let contents = std::str::from_utf8(bytes)
        .map_err(|_error| crate::config_toml::ConfigTomlError::InvalidFormat)?;
    crate::config_toml::write_entry(directory, file_name, contents)
}
