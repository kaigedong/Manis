#![forbid(unsafe_code)]

use std::collections::{HashMap, HashSet};
use std::fmt::{self, Write as _};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write as _};
use std::path::{Component, Path, PathBuf};

const MAX_SECRET_URL_BYTES: usize = 16 * 1024;
const MAX_SUBSCRIPTION_NAME_BYTES: usize = 96;
const MAX_VLESS_FIELD_BYTES: usize = 1024;
const GROUP_TEST_URL: &str = "https://www.gstatic.com/generate_204";

#[derive(Clone, Eq, PartialEq)]
pub struct SecretUrl(String);

impl SecretUrl {
    /// Parses an HTTPS subscription URL while keeping its value out of diagnostics.
    ///
    /// # Errors
    /// Returns [`ProfileError::InvalidUrl`] without including any part of `input`.
    pub fn parse_https(input: &str) -> Result<Self, ProfileError> {
        if !is_https_url(input) || input.len() > MAX_SECRET_URL_BYTES {
            return Err(ProfileError::InvalidUrl);
        }
        Ok(Self(input.to_owned()))
    }

    /// Parses an HTTP or HTTPS Mihomo proxy-provider URL while keeping its value out of
    /// diagnostics.
    ///
    /// # Errors
    /// Returns [`ProfileError::InvalidUrl`] without including any part of `input`.
    pub fn parse_subscription(input: &str) -> Result<Self, ProfileError> {
        if !is_subscription_url(input) || input.len() > MAX_SECRET_URL_BYTES {
            return Err(ProfileError::InvalidUrl);
        }
        Ok(Self(input.to_owned()))
    }

    /// Reports whether this subscription uses encrypted HTTPS transport without exposing it.
    #[must_use]
    pub fn is_https(&self) -> bool {
        self.0.starts_with("https://")
    }

    /// Exposes the URL only for the lifetime of a caller-owned closure.
    ///
    /// This keeps the secret out of `Display`, `Debug`, and ordinary return values while allowing
    /// storage and network boundaries to consume it without cloning it into UI state.
    pub fn expose_to<T>(&self, use_secret: impl FnOnce(&str) -> T) -> T {
        use_secret(&self.0)
    }

    /// Returns a bounded user-facing subscription label from an explicit `name` query field.
    ///
    /// Other query values, including bearer tokens, are never used as display fallbacks.
    #[must_use]
    pub fn subscription_name(&self) -> Option<String> {
        let query = self.0.split_once('?')?.1.split('#').next()?;
        query.split('&').find_map(|field| {
            let (key, value) = field.split_once('=')?;
            if !key.eq_ignore_ascii_case("name") {
                return None;
            }
            let decoded = decode_query_value(value)?;
            let name = decoded.trim();
            is_plain_value(name, MAX_SUBSCRIPTION_NAME_BYTES).then(|| name.to_owned())
        })
    }
}

impl fmt::Debug for SecretUrl {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SecretUrl(<redacted>)")
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Name(String);

impl Name {
    /// Creates a provider or policy-group name safe for Mihomo rule scalars.
    ///
    /// # Errors
    /// Returns [`ProfileError::InvalidName`] for an unsafe value.
    pub fn parse(input: &str) -> Result<Self, ProfileError> {
        if !is_plain_value(input, 128) || input.contains(',') {
            return Err(ProfileError::InvalidName);
        }
        Ok(Self(input.to_owned()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Profile {
    pub mode: ProfileMode,
    pub mixed_port: u16,
    pub log_level: LogLevel,
    pub store_selected: bool,
    pub proxies: Vec<OutboundProxy>,
    pub providers: Vec<ProxyProvider>,
    pub groups: Vec<PolicyGroup>,
    pub rules: Vec<Rule>,
}

impl Profile {
    /// Builds the first QX-style preset with manual and automatic policy groups.
    ///
    /// # Errors
    /// Returns a redacted validation error if the preset is inconsistent.
    pub fn qx_default(subscription: SecretUrl) -> Result<Self, ProfileError> {
        let provider_name = Name::parse("subscription")?;
        let automatic_name = Name::parse("Auto")?;
        let proxy_name = Name::parse("Proxy")?;
        let profile = Self {
            mode: ProfileMode::Rule,
            mixed_port: 7890,
            log_level: LogLevel::Warning,
            store_selected: true,
            proxies: Vec::new(),
            providers: vec![ProxyProvider {
                name: provider_name.clone(),
                url: subscription,
                interval_secs: 86_400,
                path: "./proxy_providers/subscription.yaml".to_owned(),
                health_check: HealthCheck {
                    enabled: true,
                    interval_secs: 600,
                    url: "https://www.gstatic.com/generate_204".to_owned(),
                },
            }],
            groups: vec![
                PolicyGroup {
                    name: automatic_name.clone(),
                    icon: None,
                    kind: PolicyGroupKind::UrlTest {
                        proxies: Vec::new(),
                        use_providers: vec![provider_name.clone()],
                        filter: None,
                        url: GROUP_TEST_URL.to_owned(),
                        interval_secs: 600,
                        tolerance: None,
                    },
                },
                PolicyGroup {
                    name: proxy_name.clone(),
                    icon: None,
                    kind: PolicyGroupKind::Select {
                        proxies: vec![PolicyRef::Group(automatic_name), PolicyRef::Direct],
                        use_providers: vec![provider_name],
                        filter: None,
                    },
                },
            ],
            rules: vec![
                Rule::GeoIp {
                    country: "CN".to_owned(),
                    policy: PolicyRef::Direct,
                    no_resolve: true,
                },
                Rule::Match {
                    policy: PolicyRef::Group(proxy_name),
                },
            ],
        };
        profile.validate()?;
        Ok(profile)
    }

    pub fn set_mode(&mut self, mode: ProfileMode) {
        self.mode = mode;
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

    /// Builds a QX-style policy profile from persisted sources and user-defined node groups.
    ///
    /// User groups are compiled before the generated `Auto` and `Proxy` groups. The final `Proxy`
    /// group references every user group so rule matching can route into those policies.
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
        if subscriptions.is_empty() && vless_nodes.is_empty() {
            return Err(ProfileError::InvalidValue("profile sources"));
        }
        let automatic_name = Name::parse("Auto")?;
        let proxy_name = Name::parse("Proxy")?;
        let providers = subscriptions
            .into_iter()
            .enumerate()
            .map(|(index, url)| {
                let display_index = index + 1;
                Ok(ProxyProvider {
                    name: Name::parse(&format!("Subscription {display_index}"))?,
                    url,
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
        let mut select_refs = compiled_user_groups
            .iter()
            .map(|group| PolicyRef::Group(group.name.clone()))
            .collect::<Vec<_>>();
        select_refs.push(PolicyRef::Group(automatic_name.clone()));
        select_refs.push(PolicyRef::Direct);
        select_refs.extend(direct_refs.iter().cloned());
        let mut groups = Vec::with_capacity(compiled_user_groups.len() + 2);
        groups.append(&mut compiled_user_groups);
        groups.extend([
            PolicyGroup {
                name: automatic_name.clone(),
                icon: None,
                kind: PolicyGroupKind::UrlTest {
                    proxies: direct_refs,
                    use_providers: provider_names.clone(),
                    filter: None,
                    url: GROUP_TEST_URL.to_owned(),
                    interval_secs: 600,
                    tolerance: None,
                },
            },
            PolicyGroup {
                name: proxy_name.clone(),
                icon: None,
                kind: PolicyGroupKind::Select {
                    proxies: select_refs,
                    use_providers: provider_names,
                    filter: None,
                },
            },
        ]);
        let profile = Self {
            mode: ProfileMode::Rule,
            mixed_port,
            log_level: LogLevel::Warning,
            store_selected: true,
            proxies: vless_nodes.into_iter().map(OutboundProxy::Vless).collect(),
            providers,
            groups,
            rules: vec![
                Rule::GeoIp {
                    country: "CN".to_owned(),
                    policy: PolicyRef::Direct,
                    no_resolve: true,
                },
                Rule::Match {
                    policy: PolicyRef::Group(proxy_name),
                },
            ],
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
            proxies: Vec::new(),
            providers: vec![ProxyProvider {
                name: provider_name.clone(),
                url: subscription,
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

        let mut all_names = HashSet::new();
        let mut proxy_names = HashSet::new();
        for proxy in &self.proxies {
            let name = proxy.name();
            if matches!(name.as_str(), "DIRECT" | "REJECT")
                || !all_names.insert(name.clone())
                || !proxy_names.insert(name.clone())
            {
                return Err(ProfileError::DuplicateName);
            }
            proxy.validate()?;
        }

        let mut provider_names = HashSet::new();
        for provider in &self.providers {
            if !all_names.insert(provider.name.clone())
                || !provider_names.insert(provider.name.clone())
            {
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
                || !all_names.insert(group.name.clone())
                || !group_names.insert(group.name.clone())
            {
                return Err(ProfileError::DuplicateName);
            }
        }

        validate_groups(&self.groups, &group_names, &proxy_names, &provider_names)?;

        if !matches!(self.rules.last(), Some(Rule::Match { .. })) {
            return Err(ProfileError::MissingTerminalMatch);
        }
        for (index, rule) in self.rules.iter().enumerate() {
            if matches!(rule, Rule::Match { .. }) && index + 1 != self.rules.len() {
                return Err(ProfileError::MissingTerminalMatch);
            }
            match rule {
                Rule::Domain { value, policy }
                | Rule::DomainKeyword { value, policy }
                | Rule::DomainSuffix { value, policy } => {
                    if !is_rule_value(value) {
                        return Err(ProfileError::InvalidValue("domain rule"));
                    }
                    validate_policy_ref(policy, &group_names, &proxy_names)?;
                }
                Rule::GeoIp {
                    country, policy, ..
                } => {
                    if !is_rule_value(country) {
                        return Err(ProfileError::InvalidValue("GEOIP rule"));
                    }
                    validate_policy_ref(policy, &group_names, &proxy_names)?;
                }
                Rule::Match { policy } => {
                    validate_policy_ref(policy, &group_names, &proxy_names)?;
                }
            }
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
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProxyProvider {
    pub name: Name,
    pub url: SecretUrl,
    pub interval_secs: u32,
    pub path: String,
    pub health_check: HealthCheck,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HealthCheck {
    pub enabled: bool,
    pub interval_secs: u32,
    pub url: String,
}

#[derive(Clone, Eq, PartialEq)]
pub enum OutboundProxy {
    Vless(VlessProxy),
}

impl OutboundProxy {
    #[must_use]
    pub fn name(&self) -> &Name {
        match self {
            Self::Vless(proxy) => proxy.name(),
        }
    }

    fn validate(&self) -> Result<(), ProfileError> {
        match self {
            Self::Vless(proxy) => proxy.validate(),
        }
    }
}

impl fmt::Debug for OutboundProxy {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Vless(_) => formatter.write_str("OutboundProxy::Vless(<redacted>)"),
        }
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct VlessProxy {
    name: Name,
    server: String,
    port: u16,
    uuid: String,
    flow: Option<String>,
    packet_encoding: Option<String>,
    security: VlessSecurity,
    servername: Option<String>,
    alpn: Vec<String>,
    client_fingerprint: Option<String>,
    skip_cert_verify: bool,
    reality_public_key: Option<String>,
    reality_short_id: Option<String>,
    transport: VlessTransport,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum VlessSecurity {
    None,
    Tls,
    Reality,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum VlessTransport {
    Tcp,
    Ws {
        path: Option<String>,
        host: Option<String>,
    },
    Http {
        path: Option<String>,
        host: Option<String>,
    },
    H2 {
        path: Option<String>,
        host: Option<String>,
    },
    Grpc {
        service_name: Option<String>,
    },
    Xhttp {
        path: Option<String>,
        host: Option<String>,
        mode: Option<String>,
    },
}

struct VlessSecurityOptions {
    security: VlessSecurity,
    servername: Option<String>,
    alpn: Vec<String>,
    client_fingerprint: Option<String>,
    skip_cert_verify: bool,
    reality_public_key: Option<String>,
    reality_short_id: Option<String>,
}

impl VlessProxy {
    /// Parses the explicitly supported subset of a VLESS share link.
    ///
    /// Unknown or duplicate query keys are rejected instead of being silently ignored. Errors
    /// never contain source material.
    ///
    /// # Errors
    /// Returns a fixed-category [`ProfileError`] for malformed or unsupported links.
    pub fn parse_share_link(input: &str) -> Result<Self, ProfileError> {
        if input.len() > MAX_SECRET_URL_BYTES
            || input.trim() != input
            || input.chars().any(char::is_control)
        {
            return Err(ProfileError::InvalidVless);
        }
        let remainder = input
            .strip_prefix("vless://")
            .ok_or(ProfileError::InvalidVless)?;
        let (without_fragment, fragment) = remainder
            .split_once('#')
            .map_or((remainder, None), |(value, name)| (value, Some(name)));
        let (authority, query) = without_fragment
            .split_once('?')
            .map_or((without_fragment, ""), |(value, query)| (value, query));
        let (uuid, server_port) = authority
            .split_once('@')
            .ok_or(ProfileError::InvalidVless)?;
        if !is_uuid(uuid) {
            return Err(ProfileError::InvalidVless);
        }
        let (server, port) = parse_vless_server(server_port)?;
        let fields = parse_vless_query(query)?;
        require_vless_encryption(fields.get("encryption"))?;

        let name = match fragment {
            Some(value) => decode_query_value(value)
                .ok_or(ProfileError::InvalidVless)?
                .trim()
                .to_owned(),
            None => String::new(),
        };
        let name = if name.is_empty() {
            format!("VLESS · {server}")
        } else {
            name
        };
        let name = Name::parse(&name).map_err(|_error| ProfileError::InvalidVless)?;
        let flow = optional_vless_value(&fields, "flow")?;
        if flow
            .as_deref()
            .is_some_and(|value| value != "xtls-rprx-vision")
        {
            return Err(ProfileError::UnsupportedVless);
        }
        let packet_encoding = optional_vless_value(&fields, "packetencoding")?;
        if packet_encoding
            .as_deref()
            .is_some_and(|value| !matches!(value, "xudp" | "packetaddr"))
        {
            return Err(ProfileError::UnsupportedVless);
        }
        let security = parse_vless_security(&fields)?;
        let transport = parse_vless_transport(&fields)?;
        let proxy = Self {
            name,
            server,
            port,
            uuid: uuid.to_ascii_lowercase(),
            flow,
            packet_encoding,
            security: security.security,
            servername: security.servername,
            alpn: security.alpn,
            client_fingerprint: security.client_fingerprint,
            skip_cert_verify: security.skip_cert_verify,
            reality_public_key: security.reality_public_key,
            reality_short_id: security.reality_short_id,
            transport,
        };
        proxy.validate()?;
        Ok(proxy)
    }

    #[must_use]
    pub fn name(&self) -> &Name {
        &self.name
    }

    fn validate(&self) -> Result<(), ProfileError> {
        if self.port == 0
            || !is_vless_host(&self.server)
            || !is_uuid(&self.uuid)
            || self
                .reality_public_key
                .as_deref()
                .is_some_and(|value| !is_plain_value(value, MAX_VLESS_FIELD_BYTES))
            || self
                .reality_short_id
                .as_deref()
                .is_some_and(|value| !is_plain_value(value, 64))
        {
            return Err(ProfileError::InvalidVless);
        }
        Ok(())
    }
}

impl fmt::Debug for VlessProxy {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("VlessProxy(<redacted>)")
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PolicyGroup {
    pub name: Name,
    pub icon: Option<String>,
    pub kind: PolicyGroupKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PolicyGroupKind {
    Select {
        proxies: Vec<PolicyRef>,
        use_providers: Vec<Name>,
        filter: Option<String>,
    },
    UrlTest {
        proxies: Vec<PolicyRef>,
        use_providers: Vec<Name>,
        filter: Option<String>,
        url: String,
        interval_secs: u32,
        tolerance: Option<u16>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UserPolicyGroup {
    pub name: Name,
    pub icon: Option<String>,
    pub kind: UserPolicyGroupKind,
    pub provider_indexes: Vec<usize>,
    pub direct_proxies: Vec<Name>,
    pub filter: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UserPolicyGroupKind {
    Select,
    UrlTest { tolerance: u16, interval_secs: u32 },
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum PolicyRef {
    Direct,
    Reject,
    Group(Name),
    Proxy(Name),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Rule {
    Domain {
        value: String,
        policy: PolicyRef,
    },
    DomainKeyword {
        value: String,
        policy: PolicyRef,
    },
    DomainSuffix {
        value: String,
        policy: PolicyRef,
    },
    GeoIp {
        country: String,
        policy: PolicyRef,
        no_resolve: bool,
    },
    Match {
        policy: PolicyRef,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QxRuleKind {
    Domain,
    DomainKeyword,
    DomainSuffix,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QxRule {
    pub kind: QxRuleKind,
    pub value: String,
    pub source_policy: Name,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QxRuleList {
    pub rules: Vec<QxRule>,
    pub diagnostics: Vec<QxRuleDiagnostic>,
}

impl QxRuleList {
    /// Parses a Quantumult X rule-list document into valid rules plus line diagnostics.
    #[must_use]
    pub fn parse(input: &str) -> Self {
        let mut rules = Vec::new();
        let mut diagnostics = Vec::new();
        for (index, raw_line) in input.lines().enumerate() {
            let line_number = index + 1;
            let line = raw_line.trim();
            if line.is_empty()
                || line.starts_with('#')
                || line.starts_with("//")
                || line.starts_with(';')
            {
                continue;
            }
            let fields = line.split(',').map(str::trim).collect::<Vec<_>>();
            if fields.len() != 3 {
                diagnostics.push(QxRuleDiagnostic {
                    line_number,
                    kind: QxRuleDiagnosticKind::InvalidFieldCount,
                    detail: "expected rule type, value, and policy".to_owned(),
                });
                continue;
            }
            let Some(kind) = parse_qx_rule_kind(fields[0]) else {
                diagnostics.push(QxRuleDiagnostic {
                    line_number,
                    kind: QxRuleDiagnosticKind::UnsupportedRuleType,
                    detail: "unsupported rule type".to_owned(),
                });
                continue;
            };
            if !is_rule_value(fields[1]) {
                diagnostics.push(QxRuleDiagnostic {
                    line_number,
                    kind: QxRuleDiagnosticKind::InvalidValue,
                    detail: "rule value is empty or contains unsafe characters".to_owned(),
                });
                continue;
            }
            let Ok(source_policy) = Name::parse(fields[2]) else {
                diagnostics.push(QxRuleDiagnostic {
                    line_number,
                    kind: QxRuleDiagnosticKind::InvalidPolicy,
                    detail: "policy name is empty or contains unsafe characters".to_owned(),
                });
                continue;
            };
            rules.push(QxRule {
                kind,
                value: fields[1].to_owned(),
                source_policy,
            });
        }
        Self { rules, diagnostics }
    }

    /// Returns unique source policy labels in first-seen order.
    #[must_use]
    pub fn source_policies(&self) -> Vec<Name> {
        let mut seen = HashSet::new();
        let mut policies = Vec::new();
        for rule in &self.rules {
            if seen.insert(rule.source_policy.as_str().to_ascii_lowercase()) {
                policies.push(rule.source_policy.clone());
            }
        }
        policies
    }

    /// Converts parsed QX rules with caller-provided policy mapping.
    ///
    /// # Errors
    /// Returns the first source policy that the caller did not map.
    pub fn to_profile_rules(
        &self,
        mut map_policy: impl FnMut(&Name) -> Option<PolicyRef>,
    ) -> Result<Vec<Rule>, QxRuleImportError> {
        self.rules
            .iter()
            .map(|rule| {
                let policy = map_policy(&rule.source_policy).ok_or_else(|| {
                    QxRuleImportError::MissingPolicyMapping {
                        source_policy: rule.source_policy.clone(),
                    }
                })?;
                Ok(match rule.kind {
                    QxRuleKind::Domain => Rule::Domain {
                        value: rule.value.clone(),
                        policy,
                    },
                    QxRuleKind::DomainKeyword => Rule::DomainKeyword {
                        value: rule.value.clone(),
                        policy,
                    },
                    QxRuleKind::DomainSuffix => Rule::DomainSuffix {
                        value: rule.value.clone(),
                        policy,
                    },
                })
            })
            .collect()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QxRuleDiagnostic {
    pub line_number: usize,
    pub kind: QxRuleDiagnosticKind,
    pub detail: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QxRuleDiagnosticKind {
    InvalidFieldCount,
    UnsupportedRuleType,
    InvalidValue,
    InvalidPolicy,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum QxRuleImportError {
    MissingPolicyMapping { source_policy: Name },
}

impl QxRuleImportError {
    #[must_use]
    pub fn source_policy(&self) -> &Name {
        match self {
            Self::MissingPolicyMapping { source_policy } => source_policy,
        }
    }
}

impl fmt::Display for QxRuleImportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingPolicyMapping { source_policy } => {
                write!(
                    formatter,
                    "QX source policy `{}` has no local policy mapping",
                    source_policy.as_str()
                )
            }
        }
    }
}

impl std::error::Error for QxRuleImportError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProfileError {
    InvalidUrl,
    InvalidName,
    InvalidValue(&'static str),
    InvalidVless,
    UnsupportedVless,
    DuplicateName,
    DanglingReference,
    MissingTerminalMatch,
}

impl fmt::Display for ProfileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidUrl => formatter.write_str("secret URL is invalid"),
            Self::InvalidName => formatter.write_str("profile name is invalid"),
            Self::InvalidValue(label) => write!(formatter, "profile {label} is invalid"),
            Self::InvalidVless => formatter.write_str("VLESS source is invalid"),
            Self::UnsupportedVless => formatter.write_str("VLESS option is not supported"),
            Self::DuplicateName => formatter.write_str("profile names must be unique"),
            Self::DanglingReference => formatter.write_str("profile contains a dangling reference"),
            Self::MissingTerminalMatch => {
                formatter.write_str("profile rules must end with exactly one MATCH")
            }
        }
    }
}

impl std::error::Error for ProfileError {}

#[derive(Debug)]
pub enum WriteError {
    InvalidFileName,
    InvalidRuntimePath,
    RuntimeDirSymlink,
    RuntimeDirNotDirectory,
    FinalPathSymlink,
    FinalPathNotFile,
    Io(io::Error),
}

impl fmt::Display for WriteError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidFileName => formatter.write_str("generated profile file name is invalid"),
            Self::InvalidRuntimePath => {
                formatter.write_str("profile runtime directory path is invalid")
            }
            Self::RuntimeDirSymlink => {
                formatter.write_str("profile runtime directory cannot be a symlink")
            }
            Self::RuntimeDirNotDirectory => {
                formatter.write_str("profile runtime path must be a directory")
            }
            Self::FinalPathSymlink => {
                formatter.write_str("generated profile path cannot be a symlink")
            }
            Self::FinalPathNotFile => {
                formatter.write_str("generated profile path must be a regular file")
            }
            Self::Io(source) => write!(formatter, "private profile write failed: {source}"),
        }
    }
}

impl std::error::Error for WriteError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(source) => Some(source),
            _ => None,
        }
    }
}

/// Renders the small Relay profile schema as deterministic Mihomo YAML.
///
/// # Errors
/// Returns a redacted validation error. A successful result contains the subscription URL and
/// must itself be treated as secret material.
pub fn render_mihomo_yaml(profile: &Profile) -> Result<String, ProfileError> {
    profile.validate()?;
    let mut yaml = String::new();
    writeln!(yaml, "mode: {}", quoted(profile.mode.as_mihomo_mode()))
        .expect("String write cannot fail");
    yaml.push_str("allow-lan: false\nbind-address: \"127.0.0.1\"\n");
    writeln!(yaml, "mixed-port: {}", profile.mixed_port).expect("String write cannot fail");
    writeln!(
        yaml,
        "log-level: \"{}\"",
        match profile.log_level {
            LogLevel::Silent => "silent",
            LogLevel::Warning => "warning",
        }
    )
    .expect("String write cannot fail");
    yaml.push_str("profile:\n");
    writeln!(yaml, "  store-selected: {}", profile.store_selected)
        .expect("String write cannot fail");
    yaml.push_str("proxies:\n");
    for proxy in &profile.proxies {
        match proxy {
            OutboundProxy::Vless(proxy) => render_vless_proxy(&mut yaml, proxy),
        }
    }
    yaml.push_str("proxy-providers:\n");
    for provider in &profile.providers {
        writeln!(yaml, "  {}:", quoted(provider.name.as_str())).expect("String write cannot fail");
        yaml.push_str("    type: \"http\"\n");
        writeln!(yaml, "    url: {}", quoted(&provider.url.0)).expect("String write cannot fail");
        writeln!(yaml, "    path: {}", quoted(&provider.path)).expect("String write cannot fail");
        writeln!(yaml, "    interval: {}", provider.interval_secs)
            .expect("String write cannot fail");
        yaml.push_str("    proxy: \"DIRECT\"\n    health-check:\n");
        writeln!(yaml, "      enable: {}", provider.health_check.enabled)
            .expect("String write cannot fail");
        writeln!(yaml, "      url: {}", quoted(&provider.health_check.url))
            .expect("String write cannot fail");
        writeln!(
            yaml,
            "      interval: {}",
            provider.health_check.interval_secs
        )
        .expect("String write cannot fail");
        yaml.push_str("      timeout: 5000\n      lazy: true\n      expected-status: 204\n");
    }
    yaml.push_str("proxy-groups:\n");
    for group in &profile.groups {
        render_proxy_group(&mut yaml, group);
    }
    yaml.push_str("rules:\n");
    for rule in &profile.rules {
        writeln!(yaml, "  - {}", quoted(&render_rule(rule))).expect("String write cannot fail");
    }
    Ok(yaml)
}

/// Writes secret bytes with private permissions using a same-directory temporary file.
///
/// # Errors
/// Returns a path-safety or I/O error without including `bytes`.
pub fn write_private_atomic(
    runtime_dir: &Path,
    file_name: &str,
    bytes: &[u8],
) -> Result<PathBuf, WriteError> {
    if !runtime_dir.is_absolute()
        || !runtime_dir.components().all(|component| {
            matches!(
                component,
                Component::Prefix(_) | Component::RootDir | Component::Normal(_)
            )
        })
    {
        return Err(WriteError::InvalidRuntimePath);
    }
    if Path::new(file_name).components().count() != 1
        || !matches!(
            Path::new(file_name).components().next(),
            Some(Component::Normal(_))
        )
    {
        return Err(WriteError::InvalidFileName);
    }
    prepare_runtime_dir(runtime_dir)?;
    let final_path = runtime_dir.join(file_name);
    if let Ok(metadata) = fs::symlink_metadata(&final_path) {
        if metadata.file_type().is_symlink() {
            return Err(WriteError::FinalPathSymlink);
        }
        if !metadata.is_file() {
            return Err(WriteError::FinalPathNotFile);
        }
    }

    let (temp_path, mut temp_file) = create_private_temp(runtime_dir, file_name)?;
    let write_result = (|| -> Result<(), WriteError> {
        temp_file.write_all(bytes).map_err(WriteError::Io)?;
        temp_file.sync_all().map_err(WriteError::Io)?;
        drop(temp_file);
        replace_file(&temp_path, &final_path)?;
        harden_file(&final_path)?;
        sync_runtime_dir(runtime_dir)?;
        Ok(())
    })();
    if write_result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    write_result?;
    Ok(final_path)
}

fn compile_user_groups(
    user_groups: Vec<UserPolicyGroup>,
    provider_names: &[Name],
    proxy_names: &HashSet<Name>,
    test_url: &str,
) -> Result<Vec<PolicyGroup>, ProfileError> {
    let mut group_names = HashSet::new();
    user_groups
        .into_iter()
        .map(|group| {
            if !group_names.insert(group.name.clone()) {
                return Err(ProfileError::DuplicateName);
            }
            if group.provider_indexes.is_empty() && group.direct_proxies.is_empty() {
                return Err(ProfileError::InvalidValue("user proxy group"));
            }
            if group
                .icon
                .as_deref()
                .is_some_and(|value| !is_group_metadata(value))
                || group
                    .filter
                    .as_deref()
                    .is_some_and(|value| !is_group_metadata(value))
            {
                return Err(ProfileError::InvalidValue("user proxy group"));
            }

            let mut seen_providers = HashSet::new();
            let mut use_providers = Vec::with_capacity(group.provider_indexes.len());
            for index in group.provider_indexes {
                let Some(provider) = provider_names.get(index) else {
                    return Err(ProfileError::DanglingReference);
                };
                if !seen_providers.insert(index) {
                    return Err(ProfileError::DuplicateName);
                }
                use_providers.push(provider.clone());
            }

            let mut seen_proxies = HashSet::new();
            let mut proxies = Vec::with_capacity(group.direct_proxies.len());
            for name in group.direct_proxies {
                if !proxy_names.contains(&name) {
                    return Err(ProfileError::DanglingReference);
                }
                if !seen_proxies.insert(name.clone()) {
                    return Err(ProfileError::DuplicateName);
                }
                proxies.push(PolicyRef::Proxy(name));
            }

            let kind = match group.kind {
                UserPolicyGroupKind::Select => PolicyGroupKind::Select {
                    proxies,
                    use_providers,
                    filter: group.filter,
                },
                UserPolicyGroupKind::UrlTest {
                    tolerance,
                    interval_secs,
                } => PolicyGroupKind::UrlTest {
                    proxies,
                    use_providers,
                    filter: group.filter,
                    url: test_url.to_owned(),
                    interval_secs,
                    tolerance: Some(tolerance),
                },
            };
            Ok(PolicyGroup {
                name: group.name,
                icon: group.icon,
                kind,
            })
        })
        .collect()
}

fn validate_policy_refs(
    policies: &[PolicyRef],
    groups: &HashSet<Name>,
    proxies: &HashSet<Name>,
) -> Result<(), ProfileError> {
    for policy in policies {
        validate_policy_ref(policy, groups, proxies)?;
    }
    Ok(())
}

fn validate_groups(
    groups: &[PolicyGroup],
    group_names: &HashSet<Name>,
    proxy_names: &HashSet<Name>,
    provider_names: &HashSet<Name>,
) -> Result<(), ProfileError> {
    for group in groups {
        if group
            .icon
            .as_deref()
            .is_some_and(|value| !is_group_metadata(value))
        {
            return Err(ProfileError::InvalidValue("proxy group icon"));
        }
        match &group.kind {
            PolicyGroupKind::Select {
                proxies,
                use_providers,
                filter,
            } => {
                if proxies.is_empty() && use_providers.is_empty() {
                    return Err(ProfileError::InvalidValue("select group"));
                }
                validate_group_filter(filter.as_ref())?;
                validate_policy_refs(proxies, group_names, proxy_names)?;
                validate_provider_refs(use_providers, provider_names)?;
            }
            PolicyGroupKind::UrlTest {
                proxies,
                use_providers,
                filter,
                url,
                interval_secs,
                tolerance: _,
            } => {
                if (proxies.is_empty() && use_providers.is_empty())
                    || *interval_secs == 0
                    || !is_https_url(url)
                {
                    return Err(ProfileError::InvalidValue("url-test group"));
                }
                validate_group_filter(filter.as_ref())?;
                validate_policy_refs(proxies, group_names, proxy_names)?;
                validate_provider_refs(use_providers, provider_names)?;
            }
        }
    }
    Ok(())
}

fn validate_group_filter(filter: Option<&String>) -> Result<(), ProfileError> {
    if filter
        .map(String::as_str)
        .is_some_and(|value| !is_group_metadata(value))
    {
        Err(ProfileError::InvalidValue("proxy group filter"))
    } else {
        Ok(())
    }
}

fn validate_policy_ref(
    policy: &PolicyRef,
    groups: &HashSet<Name>,
    proxies: &HashSet<Name>,
) -> Result<(), ProfileError> {
    match policy {
        PolicyRef::Group(name) if !groups.contains(name) => Err(ProfileError::DanglingReference),
        PolicyRef::Proxy(name) if !proxies.contains(name) => Err(ProfileError::DanglingReference),
        _ => Ok(()),
    }
}

fn validate_provider_refs(providers: &[Name], known: &HashSet<Name>) -> Result<(), ProfileError> {
    if providers.iter().all(|name| known.contains(name)) {
        Ok(())
    } else {
        Err(ProfileError::DanglingReference)
    }
}

fn is_https_url(input: &str) -> bool {
    is_url_with_scheme(input, "https://")
}

fn is_subscription_url(input: &str) -> bool {
    is_url_with_scheme(input, "https://") || is_url_with_scheme(input, "http://")
}

fn parse_vless_query(query: &str) -> Result<HashMap<String, String>, ProfileError> {
    const SUPPORTED: [&str; 16] = [
        "encryption",
        "flow",
        "packetencoding",
        "security",
        "sni",
        "alpn",
        "fp",
        "allowinsecure",
        "pbk",
        "sid",
        "type",
        "path",
        "host",
        "servicename",
        "mode",
        "headertype",
    ];
    let mut fields = HashMap::new();
    if query.is_empty() {
        return Ok(fields);
    }
    for pair in query.split('&') {
        let (key, raw_value) = pair.split_once('=').ok_or(ProfileError::InvalidVless)?;
        let key = key.to_ascii_lowercase();
        if !SUPPORTED.contains(&key.as_str()) || fields.contains_key(&key) {
            return Err(ProfileError::UnsupportedVless);
        }
        let value = decode_query_value(raw_value).ok_or(ProfileError::InvalidVless)?;
        if value.len() > MAX_VLESS_FIELD_BYTES || value.chars().any(char::is_control) {
            return Err(ProfileError::InvalidVless);
        }
        fields.insert(key, value);
    }
    Ok(fields)
}

fn optional_vless_value(
    fields: &HashMap<String, String>,
    key: &str,
) -> Result<Option<String>, ProfileError> {
    fields.get(key).map_or(Ok(None), |value| {
        if value.is_empty() {
            return Ok(None);
        }
        is_plain_value(value, MAX_VLESS_FIELD_BYTES)
            .then(|| value.clone())
            .map(Some)
            .ok_or(ProfileError::InvalidVless)
    })
}

fn parse_vless_security(
    fields: &HashMap<String, String>,
) -> Result<VlessSecurityOptions, ProfileError> {
    let security = match fields.get("security").map_or("none", String::as_str) {
        "none" => VlessSecurity::None,
        "tls" => VlessSecurity::Tls,
        "reality" => VlessSecurity::Reality,
        _ => return Err(ProfileError::UnsupportedVless),
    };
    let servername = optional_vless_value(fields, "sni")?;
    let alpn = optional_vless_value(fields, "alpn")?
        .map(|value| value.split(',').map(str::to_owned).collect::<Vec<String>>())
        .unwrap_or_default();
    if alpn.iter().any(|value| !is_plain_value(value, 64)) {
        return Err(ProfileError::InvalidVless);
    }
    let client_fingerprint = optional_vless_value(fields, "fp")?;
    let skip_cert_verify = parse_vless_bool(fields.get("allowinsecure"))?;
    let reality_public_key = optional_vless_value(fields, "pbk")?;
    let reality_short_id = optional_vless_value(fields, "sid")?;
    if security == VlessSecurity::None
        && (servername.is_some()
            || !alpn.is_empty()
            || client_fingerprint.is_some()
            || skip_cert_verify)
    {
        return Err(ProfileError::UnsupportedVless);
    }
    if security == VlessSecurity::Reality && reality_public_key.is_none() {
        return Err(ProfileError::InvalidVless);
    }
    if security != VlessSecurity::Reality
        && (reality_public_key.is_some() || reality_short_id.is_some())
    {
        return Err(ProfileError::UnsupportedVless);
    }
    Ok(VlessSecurityOptions {
        security,
        servername,
        alpn,
        client_fingerprint,
        skip_cert_verify,
        reality_public_key,
        reality_short_id,
    })
}

fn parse_vless_transport(fields: &HashMap<String, String>) -> Result<VlessTransport, ProfileError> {
    let path = optional_vless_value(fields, "path")?;
    let host = optional_vless_value(fields, "host")?;
    let service_name = optional_vless_value(fields, "servicename")?;
    let mode = optional_vless_value(fields, "mode")?;
    let header_type = optional_vless_value(fields, "headertype")?;
    if header_type.as_deref().is_some_and(|value| value != "none") {
        return Err(ProfileError::UnsupportedVless);
    }
    match fields.get("type").map_or("tcp", String::as_str) {
        "tcp" if path.is_none() && host.is_none() && service_name.is_none() && mode.is_none() => {
            Ok(VlessTransport::Tcp)
        }
        "ws" if service_name.is_none() && mode.is_none() => Ok(VlessTransport::Ws { path, host }),
        "http" if service_name.is_none() && mode.is_none() => {
            Ok(VlessTransport::Http { path, host })
        }
        "h2" if service_name.is_none() && mode.is_none() => Ok(VlessTransport::H2 { path, host }),
        "grpc" if path.is_none() && host.is_none() && mode.is_none() => {
            Ok(VlessTransport::Grpc { service_name })
        }
        "xhttp" if service_name.is_none() => Ok(VlessTransport::Xhttp { path, host, mode }),
        _ => Err(ProfileError::UnsupportedVless),
    }
}

fn require_vless_encryption(value: Option<&String>) -> Result<(), ProfileError> {
    match value.map_or("none", String::as_str) {
        "none" => Ok(()),
        _ => Err(ProfileError::UnsupportedVless),
    }
}

fn parse_vless_bool(value: Option<&String>) -> Result<bool, ProfileError> {
    match value.map_or("false", String::as_str) {
        "false" | "0" => Ok(false),
        "true" | "1" => Ok(true),
        _ => Err(ProfileError::InvalidVless),
    }
}

fn parse_vless_server(value: &str) -> Result<(String, u16), ProfileError> {
    let (host, port) = if let Some(ipv6) = value.strip_prefix('[') {
        let (host, port) = ipv6.split_once("]:").ok_or(ProfileError::InvalidVless)?;
        (host, port)
    } else {
        value.rsplit_once(':').ok_or(ProfileError::InvalidVless)?
    };
    let port = port
        .parse::<u16>()
        .ok()
        .filter(|port| *port > 0)
        .ok_or(ProfileError::InvalidVless)?;
    if !is_vless_host(host) {
        return Err(ProfileError::InvalidVless);
    }
    Ok((host.to_owned(), port))
}

fn is_vless_host(value: &str) -> bool {
    is_plain_value(value, 253)
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_' | b':'))
}

fn is_uuid(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(index, byte)| {
            if matches!(index, 8 | 13 | 18 | 23) {
                byte == b'-'
            } else {
                byte.is_ascii_hexdigit()
            }
        })
}

fn decode_query_value(value: &str) -> Option<String> {
    let input = value.as_bytes();
    let mut output = Vec::with_capacity(input.len());
    let mut index = 0;
    while index < input.len() {
        match input[index] {
            b'%' => {
                let high = hex_value(*input.get(index + 1)?)?;
                let low = hex_value(*input.get(index + 2)?)?;
                output.push((high << 4) | low);
                index += 3;
            }
            b'+' => {
                output.push(b' ');
                index += 1;
            }
            byte => {
                output.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8(output).ok()
}

fn hex_value(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn is_url_with_scheme(input: &str, scheme: &str) -> bool {
    if input.is_empty()
        || input.trim() != input
        || input.chars().any(char::is_control)
        || !input.starts_with(scheme)
    {
        return false;
    }
    let remainder = &input[scheme.len()..];
    let authority = remainder.split(['/', '?', '#']).next().unwrap_or_default();
    !authority.is_empty() && !authority.contains('@') && !authority.starts_with(':')
}

fn is_plain_value(value: &str, max_bytes: usize) -> bool {
    !value.is_empty()
        && value.len() <= max_bytes
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

fn is_group_metadata(value: &str) -> bool {
    is_plain_value(value, 1024)
}

fn is_rule_value(value: &str) -> bool {
    is_plain_value(value, 1024) && !value.contains(',')
}

fn parse_qx_rule_kind(value: &str) -> Option<QxRuleKind> {
    if value.eq_ignore_ascii_case("DOMAIN") {
        Some(QxRuleKind::Domain)
    } else if value.eq_ignore_ascii_case("DOMAIN-KEYWORD") {
        Some(QxRuleKind::DomainKeyword)
    } else if value.eq_ignore_ascii_case("DOMAIN-SUFFIX") {
        Some(QxRuleKind::DomainSuffix)
    } else {
        None
    }
}

fn is_safe_relative_path(value: &str) -> bool {
    !value.is_empty()
        && !value.chars().any(char::is_control)
        && !Path::new(value).is_absolute()
        && Path::new(value)
            .components()
            .all(|component| matches!(component, Component::CurDir | Component::Normal(_)))
}

fn quoted(value: &str) -> String {
    let mut output = String::with_capacity(value.len() + 2);
    output.push('"');
    for character in value.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            '\u{08}' => output.push_str("\\b"),
            '\u{0C}' => output.push_str("\\f"),
            character if character.is_control() => {
                write!(output, "\\u{:04X}", u32::from(character))
                    .expect("String write cannot fail");
            }
            character => output.push(character),
        }
    }
    output.push('"');
    output
}

fn render_provider_use(yaml: &mut String, providers: &[Name]) {
    if providers.is_empty() {
        return;
    }
    yaml.push_str("    use:\n");
    for provider in providers {
        writeln!(yaml, "      - {}", quoted(provider.as_str())).expect("String write cannot fail");
    }
}

fn render_proxy_group(yaml: &mut String, group: &PolicyGroup) {
    writeln!(yaml, "  - name: {}", quoted(group.name.as_str())).expect("String write cannot fail");
    if let Some(icon) = &group.icon {
        writeln!(yaml, "    icon: {}", quoted(icon)).expect("String write cannot fail");
    }
    match &group.kind {
        PolicyGroupKind::Select {
            proxies,
            use_providers,
            filter,
        } => {
            yaml.push_str("    type: \"select\"\n");
            render_policy_refs(yaml, proxies);
            render_group_filter(yaml, filter.as_ref());
            render_provider_use(yaml, use_providers);
        }
        PolicyGroupKind::UrlTest {
            proxies,
            use_providers,
            filter,
            url,
            interval_secs,
            tolerance,
        } => {
            yaml.push_str("    type: \"url-test\"\n");
            render_policy_refs(yaml, proxies);
            render_group_filter(yaml, filter.as_ref());
            render_provider_use(yaml, use_providers);
            writeln!(yaml, "    url: {}", quoted(url)).expect("String write cannot fail");
            writeln!(yaml, "    interval: {interval_secs}").expect("String write cannot fail");
            if let Some(tolerance) = tolerance {
                writeln!(yaml, "    tolerance: {tolerance}").expect("String write cannot fail");
            }
            yaml.push_str("    lazy: true\n");
        }
    }
}

fn render_policy_refs(yaml: &mut String, proxies: &[PolicyRef]) {
    if proxies.is_empty() {
        return;
    }
    yaml.push_str("    proxies:\n");
    for policy in proxies {
        writeln!(yaml, "      - {}", quoted(policy_name(policy)))
            .expect("String write cannot fail");
    }
}

fn render_group_filter(yaml: &mut String, filter: Option<&String>) {
    if let Some(filter) = filter {
        writeln!(yaml, "    filter: {}", quoted(filter)).expect("String write cannot fail");
    }
}

fn policy_name(policy: &PolicyRef) -> &str {
    match policy {
        PolicyRef::Direct => "DIRECT",
        PolicyRef::Reject => "REJECT",
        PolicyRef::Group(name) | PolicyRef::Proxy(name) => name.as_str(),
    }
}

fn render_vless_proxy(yaml: &mut String, proxy: &VlessProxy) {
    writeln!(yaml, "  - name: {}", quoted(proxy.name.as_str())).expect("String write cannot fail");
    yaml.push_str("    type: \"vless\"\n");
    writeln!(yaml, "    server: {}", quoted(&proxy.server)).expect("String write cannot fail");
    writeln!(yaml, "    port: {}", proxy.port).expect("String write cannot fail");
    writeln!(yaml, "    uuid: {}", quoted(&proxy.uuid)).expect("String write cannot fail");
    yaml.push_str("    udp: true\n");
    if let Some(flow) = &proxy.flow {
        writeln!(yaml, "    flow: {}", quoted(flow)).expect("String write cannot fail");
    }
    if let Some(packet_encoding) = &proxy.packet_encoding {
        writeln!(yaml, "    packet-encoding: {}", quoted(packet_encoding))
            .expect("String write cannot fail");
    }
    let network = match proxy.transport {
        VlessTransport::Tcp => "tcp",
        VlessTransport::Ws { .. } => "ws",
        VlessTransport::Http { .. } => "http",
        VlessTransport::H2 { .. } => "h2",
        VlessTransport::Grpc { .. } => "grpc",
        VlessTransport::Xhttp { .. } => "xhttp",
    };
    writeln!(yaml, "    network: {}", quoted(network)).expect("String write cannot fail");
    let tls = proxy.security != VlessSecurity::None;
    writeln!(yaml, "    tls: {tls}").expect("String write cannot fail");
    if let Some(servername) = &proxy.servername {
        writeln!(yaml, "    servername: {}", quoted(servername)).expect("String write cannot fail");
    }
    if !proxy.alpn.is_empty() {
        yaml.push_str("    alpn:\n");
        for value in &proxy.alpn {
            writeln!(yaml, "      - {}", quoted(value)).expect("String write cannot fail");
        }
    }
    if let Some(fingerprint) = &proxy.client_fingerprint {
        writeln!(yaml, "    client-fingerprint: {}", quoted(fingerprint))
            .expect("String write cannot fail");
    }
    if proxy.skip_cert_verify {
        yaml.push_str("    skip-cert-verify: true\n");
    }
    if proxy.security == VlessSecurity::Reality {
        yaml.push_str("    reality-opts:\n");
        if let Some(public_key) = &proxy.reality_public_key {
            writeln!(yaml, "      public-key: {}", quoted(public_key))
                .expect("String write cannot fail");
        }
        if let Some(short_id) = &proxy.reality_short_id {
            writeln!(yaml, "      short-id: {}", quoted(short_id))
                .expect("String write cannot fail");
        }
    }
    match &proxy.transport {
        VlessTransport::Tcp => {}
        VlessTransport::Ws { path, host } => {
            if path.is_some() || host.is_some() {
                yaml.push_str("    ws-opts:\n");
                render_transport_path_host(yaml, path.as_deref(), host.as_deref(), false);
            }
        }
        VlessTransport::Http { path, host } => {
            if path.is_some() || host.is_some() {
                yaml.push_str("    http-opts:\n");
                render_transport_path_host(yaml, path.as_deref(), host.as_deref(), true);
            }
        }
        VlessTransport::H2 { path, host } => {
            if path.is_some() || host.is_some() {
                yaml.push_str("    h2-opts:\n");
                render_transport_path_host(yaml, path.as_deref(), host.as_deref(), true);
            }
        }
        VlessTransport::Grpc { service_name } => {
            if let Some(service_name) = service_name {
                yaml.push_str("    grpc-opts:\n");
                writeln!(yaml, "      grpc-service-name: {}", quoted(service_name))
                    .expect("String write cannot fail");
            }
        }
        VlessTransport::Xhttp { path, host, mode } => {
            if path.is_some() || host.is_some() || mode.is_some() {
                yaml.push_str("    xhttp-opts:\n");
                render_transport_path_host(yaml, path.as_deref(), host.as_deref(), false);
                if let Some(mode) = mode {
                    writeln!(yaml, "      mode: {}", quoted(mode))
                        .expect("String write cannot fail");
                }
            }
        }
    }
}

fn render_transport_path_host(
    yaml: &mut String,
    path: Option<&str>,
    host: Option<&str>,
    list_values: bool,
) {
    if let Some(path) = path {
        if list_values {
            yaml.push_str("      path:\n");
            writeln!(yaml, "        - {}", quoted(path)).expect("String write cannot fail");
        } else {
            writeln!(yaml, "      path: {}", quoted(path)).expect("String write cannot fail");
        }
    }
    if let Some(host) = host {
        if list_values {
            yaml.push_str("      headers:\n        Host:\n");
            writeln!(yaml, "          - {}", quoted(host)).expect("String write cannot fail");
        } else {
            yaml.push_str("      headers:\n");
            writeln!(yaml, "        Host: {}", quoted(host)).expect("String write cannot fail");
        }
    }
}

fn render_rule(rule: &Rule) -> String {
    match rule {
        Rule::Domain { value, policy } => format!("DOMAIN,{value},{}", policy_name(policy)),
        Rule::DomainKeyword { value, policy } => {
            format!("DOMAIN-KEYWORD,{value},{}", policy_name(policy))
        }
        Rule::DomainSuffix { value, policy } => {
            format!("DOMAIN-SUFFIX,{value},{}", policy_name(policy))
        }
        Rule::GeoIp {
            country,
            policy,
            no_resolve,
        } => {
            let suffix = if *no_resolve { ",no-resolve" } else { "" };
            format!("GEOIP,{country},{}{suffix}", policy_name(policy))
        }
        Rule::Match { policy } => format!("MATCH,{}", policy_name(policy)),
    }
}

fn prepare_runtime_dir(path: &Path) -> Result<(), WriteError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(WriteError::RuntimeDirSymlink);
        }
        Ok(metadata) if !metadata.is_dir() => return Err(WriteError::RuntimeDirNotDirectory),
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            fs::create_dir_all(path).map_err(WriteError::Io)?;
        }
        Err(error) => return Err(WriteError::Io(error)),
    }
    let metadata = fs::symlink_metadata(path).map_err(WriteError::Io)?;
    if metadata.file_type().is_symlink() {
        return Err(WriteError::RuntimeDirSymlink);
    }
    if !metadata.is_dir() {
        return Err(WriteError::RuntimeDirNotDirectory);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(WriteError::Io)?;
    }
    Ok(())
}

fn create_private_temp(runtime_dir: &Path, file_name: &str) -> Result<(PathBuf, File), WriteError> {
    for sequence in 0..64_u8 {
        let temp_path = runtime_dir.join(format!(
            ".{file_name}.{}.{}.tmp",
            std::process::id(),
            sequence
        ));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&temp_path) {
            Ok(file) => return Ok((temp_path, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(WriteError::Io(error)),
        }
    }
    Err(WriteError::Io(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "no private temporary profile name was available",
    )))
}

fn harden_file(path: &Path) -> Result<(), WriteError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(WriteError::Io)?;
    }
    Ok(())
}

#[cfg(not(windows))]
fn replace_file(source: &Path, destination: &Path) -> Result<(), WriteError> {
    fs::rename(source, destination).map_err(WriteError::Io)
}

#[cfg(windows)]
fn replace_file(source: &Path, destination: &Path) -> Result<(), WriteError> {
    match fs::rename(source, destination) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            fs::remove_file(destination).map_err(WriteError::Io)?;
            fs::rename(source, destination).map_err(WriteError::Io)
        }
        Err(error) => Err(WriteError::Io(error)),
    }
}

#[cfg(unix)]
fn sync_runtime_dir(path: &Path) -> Result<(), WriteError> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(WriteError::Io)
}

#[cfg(not(unix))]
fn sync_runtime_dir(_path: &Path) -> Result<(), WriteError> {
    Ok(())
}
