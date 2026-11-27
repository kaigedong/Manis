use super::{
    HashMap, LoadError, LogLevel, MANUAL_ROUTING_RULE_GROUP_ID, MAX_SUBSCRIPTION_PROXY_DNS_SERVERS,
    Path, Profile, ProxyDnsServer, Rule, SecretUrl, StoredSingleNode, VlessProxy,
    apply_qx_rule_sources, compile_managed_policy_groups, configured_mixed_port,
    load_custom_proxy_dns_entries_in, load_local_routing_rule_groups_in,
    load_managed_policy_groups_in, load_qx_rule_sources_in, load_routing_mode_in,
    load_routing_rule_group_order_in, load_single_node_sources_in, load_subscription_sources_in,
    normalized_routing_rule_group_order, profile_mode, record_event,
};

pub(super) fn compile_saved_profile(
    store_dir: &Path,
    base_subscription: Option<SecretUrl>,
) -> Result<Profile, LoadError> {
    let (subscriptions, mut provider_indexes, subscription_nameservers) =
        load_subscription_inputs(store_dir, base_subscription)?;
    let custom_entries = load_custom_proxy_dns_entries_in(store_dir).map_err(|error| {
        LoadError::Runtime(format!(
            "custom proxy DNS servers could not be read: {error}"
        ))
    })?;
    let stored_single_nodes = load_enabled_single_nodes(store_dir)?;
    let (local_provider_paths, vless_nodes) = compile_single_node_inputs(
        &stored_single_nodes,
        subscriptions.len(),
        &mut provider_indexes,
    );
    let (mut profile, bootstrap_fallback) = build_base_profile(
        store_dir,
        subscriptions,
        local_provider_paths,
        vless_nodes,
        &stored_single_nodes,
        &provider_indexes,
    )?;
    let effective_nameservers = if custom_entries.is_empty() {
        subscription_nameservers
    } else {
        custom_entries
            .into_iter()
            .filter(|entry| entry.enabled)
            .map(|entry| entry.server)
            .collect()
    };
    if !effective_nameservers.is_empty() {
        profile.set_proxy_server_nameservers(effective_nameservers);
    }
    apply_saved_routing(store_dir, &mut profile, bootstrap_fallback)?;
    Ok(profile)
}

type SubscriptionInputs = (Vec<SecretUrl>, HashMap<String, usize>, Vec<ProxyDnsServer>);

fn load_subscription_inputs(
    store_dir: &Path,
    base_subscription: Option<SecretUrl>,
) -> Result<SubscriptionInputs, LoadError> {
    let mut subscriptions = base_subscription.into_iter().collect::<Vec<_>>();
    let stored = load_subscription_sources_in(store_dir)
        .map_err(|error| {
            LoadError::Runtime(format!(
                "saved subscription sources could not be read: {error}"
            ))
        })?
        .into_iter()
        .filter(|stored| stored.enabled)
        .collect::<Vec<_>>();
    let mut indexes = HashMap::new();
    let mut nameservers = Vec::new();
    for source in stored {
        let index = subscriptions
            .iter()
            .position(|candidate| candidate == &source.source)
            .unwrap_or_else(|| {
                subscriptions.push(source.source);
                subscriptions.len() - 1
            });
        indexes.insert(source.id, index);
        for nameserver in source.proxy_server_nameservers {
            if !nameservers.contains(&nameserver)
                && nameservers.len() < MAX_SUBSCRIPTION_PROXY_DNS_SERVERS
            {
                nameservers.push(nameserver);
            }
        }
    }
    Ok((subscriptions, indexes, nameservers))
}

fn load_enabled_single_nodes(store_dir: &Path) -> Result<Vec<StoredSingleNode>, LoadError> {
    load_single_node_sources_in(store_dir)
        .map_err(|error| {
            LoadError::Runtime(format!(
                "saved single-node sources could not be read: {error}"
            ))
        })
        .map(|nodes| nodes.into_iter().filter(|stored| stored.enabled).collect())
}

fn compile_single_node_inputs(
    nodes: &[StoredSingleNode],
    subscription_count: usize,
    provider_indexes: &mut HashMap<String, usize>,
) -> (Vec<String>, Vec<VlessProxy>) {
    let paths = nodes
        .iter()
        .enumerate()
        .map(|(offset, stored)| {
            provider_indexes.insert(stored.id.clone(), subscription_count + offset);
            format!("./single_nodes/{}.txt", stored.id)
        })
        .collect();
    (paths, Vec::new())
}

fn build_base_profile(
    store_dir: &Path,
    subscriptions: Vec<SecretUrl>,
    local_provider_paths: Vec<String>,
    vless_nodes: Vec<VlessProxy>,
    stored_single_nodes: &[StoredSingleNode],
    provider_indexes: &HashMap<String, usize>,
) -> Result<(Profile, Option<Rule>), LoadError> {
    let mixed_port = configured_mixed_port().map_err(LoadError::Runtime)?;
    let policy_groups = load_managed_policy_groups_in(store_dir)
        .map_err(|error| LoadError::Runtime(format!("policy groups could not be read: {error}")))?;
    let user_groups = compile_managed_policy_groups(
        &policy_groups,
        provider_indexes,
        stored_single_nodes,
        &vless_nodes,
        subscriptions.len() + local_provider_paths.len(),
    )?;
    let bootstrap =
        subscriptions.is_empty() && local_provider_paths.is_empty() && vless_nodes.is_empty();
    if bootstrap && !user_groups.is_empty() {
        return Err(LoadError::Runtime(
            "policy groups cannot be generated without a node source".to_owned(),
        ));
    }
    let mut profile = if bootstrap {
        Profile::managed_empty(mixed_port)
    } else {
        Profile::qx_sources_with_groups_and_local_providers(
            subscriptions,
            local_provider_paths,
            vless_nodes,
            user_groups,
            mixed_port,
        )
    }
    .map_err(|error| LoadError::Runtime(error.to_string()))?;
    let fallback = if bootstrap { profile.rules.pop() } else { None };
    Ok((profile, fallback))
}

fn apply_saved_routing(
    store_dir: &Path,
    profile: &mut Profile,
    bootstrap_fallback: Option<Rule>,
) -> Result<(), LoadError> {
    let routing_mode = load_routing_mode_in(store_dir).map_err(|error| {
        LoadError::Runtime(format!("saved routing mode could not be read: {error}"))
    })?;
    profile.set_mode(profile_mode(routing_mode));
    let sources = load_qx_rule_sources_in(store_dir).map_err(|error| {
        LoadError::Runtime(format!("QX rule sources could not be read: {error}"))
    })?;
    let local_groups = load_local_routing_rule_groups_in(store_dir).map_err(|error| {
        LoadError::Runtime(format!(
            "local routing rule groups could not be read: {error}"
        ))
    })?;
    let manual_rules = crate::manual_rule::load_manual_rules_in(store_dir)
        .map_err(|error| LoadError::Runtime(error.to_string()))?;
    let stored_order = load_routing_rule_group_order_in(store_dir).map_err(|error| {
        LoadError::Runtime(format!(
            "saved routing rule group order could not be read: {error}"
        ))
    })?;
    let has_regular_manual_rules = manual_rules.iter().any(|rule| !rule.is_final());
    let order = normalized_routing_rule_group_order(
        &stored_order,
        has_regular_manual_rules,
        &sources,
        &local_groups,
    );
    for group_id in order {
        if group_id == MANUAL_ROUTING_RULE_GROUP_ID {
            crate::manual_rule::append_manual_non_final_rules(
                profile,
                &manual_rules,
                manis_core::KernelKind::Mihomo,
            )
            .map_err(|error| LoadError::Runtime(error.to_string()))?;
        } else if let Some(source) = sources.iter().find(|source| source.id == group_id) {
            apply_qx_rule_sources(profile, std::slice::from_ref(source))?;
        } else if let Some(group) = local_groups
            .iter()
            .find(|group| group.id == group_id && group.enabled)
        {
            let skipped = crate::manual_rule::append_local_routing_group_rules(
                profile,
                &group.rules,
                &group.target_policy,
                manis_core::KernelKind::Mihomo,
            )
            .map_err(|error| LoadError::Runtime(error.to_string()))?;
            // FINAL is configured on the pipeline fallback node, not inside local groups.
            if skipped > 0 {
                record_event(
                    LogLevel::Warn,
                    "routing.local_group.final_skipped",
                    format!("group={} skipped={skipped}", group.id),
                );
            }
        }
    }
    crate::manual_rule::apply_manual_final_rule(profile, &manual_rules)
        .map_err(|error| LoadError::Runtime(error.to_string()))?;
    if !profile
        .rules
        .iter()
        .any(|rule| matches!(rule, Rule::Match { .. }))
    {
        if let Some(fallback) = bootstrap_fallback {
            profile.rules.push(fallback);
        } else {
            profile
                .ensure_global_match()
                .map_err(|error| LoadError::Runtime(error.to_string()))?;
        }
    }
    // One gate for every arm: a local group's target can outlive the policy it names, exactly like
    // a source's can, and a dangling policy must fail here instead of reaching the core.
    profile
        .validate()
        .map_err(|error| LoadError::Runtime(error.to_string()))?;
    Ok(())
}
