use std::collections::HashSet;

use crate::{
    GROUP_TEST_URL, HealthCheck, MANIS_GLOBAL_GROUP_NAME, Name, OutboundProxy, PolicyGroup,
    PolicyGroupKind, PolicyRef, ProfileError, ProxyDnsServer, ProxyProvider, ProxyProviderSource,
    Rule, SecretUrl, UserPolicyGroup, VlessProxy, compile_user_groups, default_proxy_dns_servers,
    is_https_url, is_safe_relative_path, validate_groups, validate_rule,
};

const MAX_PROXY_DNS_SERVERS: usize = 8;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Profile {
    pub mode: ProfileMode,
    pub mixed_port: u16,
    pub log_level: LogLevel,
    pub store_selected: bool,
    pub proxy_server_nameservers: Vec<ProxyDnsServer>,
    pub proxies: Vec<OutboundProxy>,
    pub providers: Vec<ProxyProvider>,
    pub groups: Vec<PolicyGroup>,
    pub rules: Vec<Rule>,
}

impl Profile {
    /// Builds the direct-only bootstrap profile used before the first node source is added.
    ///
    /// # Errors
    /// Returns a redacted validation error when the managed listener configuration is invalid.
    pub fn managed_empty(mixed_port: u16) -> Result<Self, ProfileError> {
        let profile = Self {
            mode: ProfileMode::Rule,
            mixed_port,
            log_level: LogLevel::Info,
            store_selected: true,
            proxy_server_nameservers: default_proxy_dns_servers(),
            proxies: Vec::new(),
            providers: Vec::new(),
            groups: Vec::new(),
            rules: vec![Rule::Match {
                policy: PolicyRef::Direct,
            }],
        };
        profile.validate()?;
        Ok(profile)
    }

    /// Builds a minimal QX-style profile with one hidden global-exit selector.
    ///
    /// # Errors
    /// Returns a redacted validation error if the preset is inconsistent.
    pub fn qx_default(subscription: SecretUrl) -> Result<Self, ProfileError> {
        Self::qx_sources(vec![subscription], Vec::new(), 7890)
    }

    pub fn set_mode(&mut self, mode: ProfileMode) {
        self.mode = mode;
    }

    pub fn set_proxy_server_nameservers(&mut self, nameservers: Vec<ProxyDnsServer>) {
        self.proxy_server_nameservers = nameservers;
    }

    #[must_use]
    pub fn with_mode(mut self, mode: ProfileMode) -> Self {
        self.set_mode(mode);
        self
    }

    /// Builds a QX-style policy profile from persisted subscriptions and manual VLESS nodes.
    ///
    /// # Errors
    /// Returns a redacted validation error if a source set is empty or contains ambiguous names.
    pub fn qx_sources(
        subscriptions: Vec<SecretUrl>,
        vless_nodes: Vec<VlessProxy>,
        mixed_port: u16,
    ) -> Result<Self, ProfileError> {
        Self::qx_sources_with_groups(subscriptions, vless_nodes, Vec::new(), mixed_port)
    }

    /// Builds a QX-style policy profile from persisted sources and user-defined policy groups.
    ///
    /// User groups are compiled as-is. Manis adds only the hidden global-exit selector required by
    /// global routing mode; it does not create visible policy groups on the user's behalf.
    ///
    /// # Errors
    /// Returns a redacted validation error if a source set is empty, names collide, or a user
    /// group references an unknown provider index or direct proxy name.
    pub fn qx_sources_with_groups(
        subscriptions: Vec<SecretUrl>,
        vless_nodes: Vec<VlessProxy>,
        user_groups: Vec<UserPolicyGroup>,
        mixed_port: u16,
    ) -> Result<Self, ProfileError> {
        Self::qx_sources_with_groups_and_local_providers(
            subscriptions,
            Vec::new(),
            vless_nodes,
            user_groups,
            mixed_port,
        )
    }

    /// Builds a QX-style profile with remote subscriptions and Mihomo file providers.
    ///
    /// Each local path points to a private file containing one proxy share link. This lets Mihomo
    /// parse all single-node protocols it supports without duplicating those protocol parsers here.
    ///
    /// # Errors
    /// Returns a redacted validation error if the source set is empty, names collide, a local
    /// provider path is invalid, or a user group references an unknown source or policy.
    pub fn qx_sources_with_groups_and_local_providers(
        subscriptions: Vec<SecretUrl>,
        local_provider_paths: Vec<String>,
        vless_nodes: Vec<VlessProxy>,
        user_groups: Vec<UserPolicyGroup>,
        mixed_port: u16,
    ) -> Result<Self, ProfileError> {
        if subscriptions.is_empty() && local_provider_paths.is_empty() && vless_nodes.is_empty() {
            return Err(ProfileError::InvalidValue("profile sources"));
        }
        if vless_nodes.iter().any(|proxy| {
            proxy.name().as_str().eq_ignore_ascii_case("GLOBAL")
                || proxy.name().as_str() == MANIS_GLOBAL_GROUP_NAME
        }) {
            return Err(ProfileError::InvalidValue("reserved proxy name"));
        }
        let global_exit_name = Name::parse(MANIS_GLOBAL_GROUP_NAME)?;
        let providers = subscriptions
            .into_iter()
            .enumerate()
            .map(|(index, url)| {
                let display_index = index + 1;
                Ok(ProxyProvider {
                    name: Name::parse(&format!("Subscription {display_index}"))?,
                    source: ProxyProviderSource::Http(url),
                    interval_secs: 86_400,
                    path: format!("./proxy_providers/subscription-{display_index}.yaml"),
                    health_check: HealthCheck {
                        enabled: true,
                        interval_secs: 600,
                        url: GROUP_TEST_URL.to_owned(),
                    },
                })
            })
            .collect::<Result<Vec<_>, ProfileError>>()?;
        let mut providers = providers;
        for (index, path) in local_provider_paths.into_iter().enumerate() {
            let display_index = index + 1;
            providers.push(ProxyProvider {
                name: Name::parse(&format!("Single node {display_index}"))?,
                source: ProxyProviderSource::File,
                interval_secs: 86_400,
                path,
                health_check: HealthCheck {
                    enabled: true,
                    interval_secs: 600,
                    url: GROUP_TEST_URL.to_owned(),
                },
            });
        }
        let provider_names = providers
            .iter()
            .map(|provider| provider.name.clone())
            .collect::<Vec<_>>();
        let direct_refs = vless_nodes
            .iter()
            .map(|proxy| PolicyRef::Proxy(proxy.name().clone()))
            .collect::<Vec<_>>();
        let proxy_names = vless_nodes
            .iter()
            .map(|proxy| proxy.name().clone())
            .collect::<HashSet<_>>();
        let mut compiled_user_groups =
            compile_user_groups(user_groups, &provider_names, &proxy_names, GROUP_TEST_URL)?;
        let mut groups = Vec::with_capacity(compiled_user_groups.len() + 1);
        groups.append(&mut compiled_user_groups);
        groups.push(PolicyGroup {
            name: global_exit_name.clone(),
            icon: None,
            kind: PolicyGroupKind::Select {
                proxies: direct_refs,
                use_providers: provider_names,
                filter: None,
            },
        });
        let profile = Self {
            mode: ProfileMode::Rule,
            mixed_port,
            log_level: LogLevel::Info,
            store_selected: true,
            proxy_server_nameservers: default_proxy_dns_servers(),
            proxies: vless_nodes.into_iter().map(OutboundProxy::Vless).collect(),
            providers,
            groups,
            rules: Vec::new(),
        };
        profile.validate()?;
        Ok(profile)
    }

    /// Builds a minimal isolated profile used only to let Mihomo fetch and parse one provider.
    ///
    /// # Errors
    /// Returns a redacted validation error when the port or generated references are invalid.
    pub fn subscription_preview(
        subscription: SecretUrl,
        mixed_port: u16,
    ) -> Result<Self, ProfileError> {
        let provider_name = Name::parse("subscription")?;
        let preview_name = Name::parse("Preview")?;
        let profile = Self {
            mode: ProfileMode::Rule,
            mixed_port,
            log_level: LogLevel::Silent,
            store_selected: false,
            proxy_server_nameservers: default_proxy_dns_servers(),
            proxies: Vec::new(),
            providers: vec![ProxyProvider {
                name: provider_name.clone(),
                source: ProxyProviderSource::Http(subscription),
                interval_secs: 86_400,
                path: "./proxy_providers/subscription.yaml".to_owned(),
                health_check: HealthCheck {
                    enabled: false,
                    interval_secs: 600,
                    url: GROUP_TEST_URL.to_owned(),
                },
            }],
            groups: vec![PolicyGroup {
                name: preview_name.clone(),
                icon: None,
                kind: PolicyGroupKind::Select {
                    proxies: Vec::new(),
                    use_providers: vec![provider_name],
                    filter: None,
                },
            }],
            rules: vec![Rule::Match {
                policy: PolicyRef::Group(preview_name),
            }],
        };
        profile.validate()?;
        Ok(profile)
    }

    /// Validates names, references, paths, intervals, and rule termination.
    ///
    /// # Errors
    /// Returns a stable error category that never embeds subscription data.
    pub fn validate(&self) -> Result<(), ProfileError> {
        if self.mixed_port == 0 {
            return Err(ProfileError::InvalidValue("mixed port"));
        }
        if self.proxy_server_nameservers.is_empty()
            || self.proxy_server_nameservers.len() > MAX_PROXY_DNS_SERVERS
        {
            return Err(ProfileError::InvalidValue("proxy DNS servers"));
        }

        let mut all_names = HashSet::new();
        let mut proxy_names = HashSet::new();
        for proxy in &self.proxies {
            let name = proxy.name();
            if matches!(name.as_str(), "DIRECT" | "REJECT")
                || !all_names.insert(name)
                || !proxy_names.insert(name)
            {
                return Err(ProfileError::DuplicateName);
            }
            proxy.validate()?;
        }

        let mut provider_names = HashSet::new();
        for provider in &self.providers {
            if !all_names.insert(&provider.name) || !provider_names.insert(&provider.name) {
                return Err(ProfileError::DuplicateName);
            }
            if provider.interval_secs == 0
                || (provider.health_check.enabled && provider.health_check.interval_secs == 0)
                || !is_safe_relative_path(&provider.path)
                || !is_https_url(&provider.health_check.url)
            {
                return Err(ProfileError::InvalidValue("proxy provider"));
            }
        }

        let mut group_names = HashSet::new();
        for group in &self.groups {
            if matches!(group.name.as_str(), "DIRECT" | "REJECT")
                || !all_names.insert(&group.name)
                || !group_names.insert(&group.name)
            {
                return Err(ProfileError::DuplicateName);
            }
        }

        validate_groups(&self.groups, &group_names, &proxy_names, &provider_names)?;

        for (index, rule) in self.rules.iter().enumerate() {
            if matches!(rule, Rule::Match { .. }) && index + 1 != self.rules.len() {
                return Err(ProfileError::MissingTerminalMatch);
            }
            validate_rule(rule, &group_names, &proxy_names)?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ProfileMode {
    Direct,
    Global,
    #[default]
    Rule,
}

impl ProfileMode {
    #[must_use]
    pub const fn as_mihomo_mode(self) -> &'static str {
        match self {
            Self::Direct => "direct",
            Self::Global => "global",
            Self::Rule => "rule",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LogLevel {
    Silent,
    Warning,
    Info,
}
