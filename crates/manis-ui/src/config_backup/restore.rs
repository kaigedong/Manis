use super::{
    BTreeMap, BackupError, BackupSummary, ImportError, ImportResult, LanguagePreference, Path,
    PathBuf, PreparedBackup, backup_current_store, create_backup_dir, mihomo,
    require_clean_absolute_path,
};

pub(crate) fn restore(
    directory: &Path,
    prepared: &PreparedBackup,
) -> Result<ImportResult, ImportError> {
    require_clean_absolute_path(directory).map_err(ImportError::new)?;
    let previous = crate::config_toml::read_source(directory)
        .map_err(|error| ImportError::new(BackupError::from(error)))?;
    let backup_dir = create_backup_dir(directory).map_err(ImportError::new)?;
    backup_current_store(directory, &backup_dir)
        .map_err(|error| ImportError::with_backup(error, backup_dir.clone(), false))?;

    let restore_result = (|| -> Result<(), BackupError> {
        crate::config_toml::replace_source(directory, &prepared.source)?;
        validate_store(directory, &prepared.files)?;
        Ok(())
    })();

    if let Err(error) = restore_result {
        let rollback_failed = crate::config_toml::replace_source(directory, &previous).is_err();
        return Err(ImportError::with_backup(error, backup_dir, rollback_failed));
    }

    Ok(ImportResult { backup_dir })
}

pub(crate) fn backup_root(directory: &Path) -> Result<PathBuf, BackupError> {
    require_clean_absolute_path(directory)?;
    Ok(super::backup_storage_root(directory))
}

pub(super) fn validate_store(
    directory: &Path,
    files: &BTreeMap<String, String>,
) -> Result<BackupSummary, BackupError> {
    let subscriptions = mihomo::load_subscription_sources_in(directory)?.len();
    let single_nodes = mihomo::load_single_node_sources_in(directory)?.len();
    let rule_sources = mihomo::load_qx_rule_sources_in(directory)?.len();
    let policies = mihomo::load_managed_policy_groups_in(directory)?;
    mihomo::validate_managed_policy_references(&policies)
        .map_err(|_| BackupError::InvalidConfiguration)?;
    mihomo::load_routing_rule_group_order_in(directory)?;
    mihomo::load_collapsed_groups_in(directory)?;
    mihomo::load_node_selection_preferences_in(directory)?;
    mihomo::load_routing_mode_in(directory)?;
    crate::localization::load_language_preference_in(directory)
        .map(|_: LanguagePreference| ())
        .map_err(|_| BackupError::InvalidConfiguration)?;
    let manual_rules = if files.contains_key("manual-routing-rules.state")
        || files.contains_key("direct-rules.state")
    {
        crate::manual_rule::load_manual_rules_in(directory)
            .map_err(|_| BackupError::InvalidConfiguration)?
            .len()
    } else {
        0
    };
    Ok(BackupSummary {
        subscriptions,
        single_nodes,
        policy_groups: policies.len(),
        rule_sources,
        manual_rules,
    })
}
