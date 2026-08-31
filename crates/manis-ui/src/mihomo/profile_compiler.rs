use super::{
    HashMap, KernelKind, LoadError, MANUAL_ROUTING_RULE_GROUP_ID,
    MAX_SUBSCRIPTION_PROXY_DNS_SERVERS, Path, Profile, ProxyDnsServer, Rule, SecretUrl,
    StoredSingleNode, VlessProxy, apply_qx_rule_sources, compile_managed_policy_groups,
    configured_mixed_port, load_managed_policy_groups_in, load_qx_rule_sources_in,
    load_routing_mode_in, load_routing_rule_group_order_in, load_single_node_sources_in,
    load_subscription_sources_in, normalized_routing_rule_group_order, profile_mode,
};

pub(super) fn compile_saved_profile(
    store_dir: &Path,
    base_subscription: Option<SecretUrl>,
    kernel: KernelKind,
) -> Result<Profile, LoadError> {
    let (subscriptions, mut provider_indexes, nameservers) =
        load_subscription_inputs(store_dir, base_subscription, kernel)?;
    let stored_single_nodes = load_enabled_single_nodes(store_dir)?;
    let (local_provider_paths, vless_nodes) = compile_single_node_inputs(
        kernel,
        &stored_single_nodes,
        subscriptions.len(),
        &mut provider_indexes,
    )?;
    let (mut profile, bootstrap_fallback) = build_base_profile(
        store_dir,
        subscriptions,
        local_provider_paths,
        vless_nodes,
        &stored_single_nodes,
        &provider_indexes,
    )?;
    if !nameservers.is_empty() {
        profile.set_proxy_server_nameservers(nameservers);
    }
    apply_saved_routing(store_dir, kernel, &mut profile, bootstrap_fallback)?;
    Ok(profile)
}

type SubscriptionInputs = (Vec<SecretUrl>, HashMap<String, usize>, Vec<ProxyDnsServer>);

fn load_subscription_inputs(
    store_dir: &Path,
    base_subscription: Option<SecretUrl>,
    kernel: KernelKind,
) -> Result<SubscriptionInputs, LoadError> {
    let mut subscriptions = base_subscription.into_iter().collect::<Vec<_>>();
    let stored = load_subscription_sources_in(store_dir)
        .map_err(|_error| {
            LoadError::Runtime("saved subscription sources could not be read".to_owned())
        })?
        .into_iter()
        .filter(|stored| stored.enabled)
        .collect::<Vec<_>>();
    if kernel == KernelKind::SingBox && !stored.is_empty() {
        return Err(LoadError::Runtime(
            "sing-box cannot read Clash subscriptions yet; use manual VLESS nodes instead"
                .to_owned(),
        ));
    }
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
        .map_err(|_error| {
            LoadError::Runtime("saved single-node sources could not be read".to_owned())
        })
        .map(|nodes| nodes.into_iter().filter(|stored| stored.enabled).collect())
}

fn compile_single_node_inputs(
    kernel: KernelKind,
    nodes: &[StoredSingleNode],
    subscription_count: usize,
    provider_indexes: &mut HashMap<String, usize>,
) -> Result<(Vec<String>, Vec<VlessProxy>), LoadError> {
    if kernel == KernelKind::Mihomo {
        let paths = nodes
            .iter()
            .enumerate()
            .map(|(offset, stored)| {
                provider_indexes.insert(stored.id.clone(), subscription_count + offset);
                format!("./single_nodes/{}.txt", stored.id)
            })
            .collect();
        return Ok((paths, Vec::new()));
    }
    let nodes = nodes
        .iter()
        .map(|stored| {
            stored
                .source
                .expose_to(VlessProxy::parse_share_link)
                .map_err(|_error| {
                    LoadError::Runtime(
                        "sing-box currently supports only manual VLESS single-node sources"
                            .to_owned(),
                    )
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok((Vec::new(), nodes))
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
        .map_err(|_error| LoadError::Runtime("policy groups could not be read".to_owned()))?;
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
    kernel: KernelKind,
    profile: &mut Profile,
    bootstrap_fallback: Option<Rule>,
) -> Result<(), LoadError> {
    let routing_mode = load_routing_mode_in(store_dir)
        .map_err(|_error| LoadError::Runtime("saved routing mode could not be read".to_owned()))?;
    profile.set_mode(profile_mode(routing_mode));
    let sources = load_qx_rule_sources_in(store_dir)
        .map_err(|_error| LoadError::Runtime("QX rule sources could not be read".to_owned()))?;
    let manual_rules = crate::manual_rule::load_manual_rules_in(store_dir)
        .map_err(|error| LoadError::Runtime(error.to_string()))?;
    let stored_order = load_routing_rule_group_order_in(store_dir).map_err(|_error| {
        LoadError::Runtime("saved routing rule group order could not be read".to_owned())
    })?;
    let order =
        normalized_routing_rule_group_order(&stored_order, !manual_rules.is_empty(), &sources);
    for group_id in order {
        if group_id == MANUAL_ROUTING_RULE_GROUP_ID {
            crate::manual_rule::append_manual_rules(profile, &manual_rules, kernel)
                .map_err(|error| LoadError::Runtime(error.to_string()))?;
        } else if let Some(source) = sources.iter().find(|source| source.id == group_id) {
            apply_qx_rule_sources(profile, std::slice::from_ref(source))?;
        }
    }
    if let Some(fallback) = bootstrap_fallback
        && !profile
            .rules
            .iter()
            .any(|rule| matches!(rule, Rule::Match { .. }))
    {
        profile.rules.push(fallback);
    }
    Ok(())
}
