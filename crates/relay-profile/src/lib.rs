#![forbid(unsafe_code)]

use std::collections::HashSet;
use std::fmt::{self, Write as _};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write as _};
use std::path::{Component, Path, PathBuf};

const MAX_SECRET_URL_BYTES: usize = 16 * 1024;

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

    fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Profile {
    pub mixed_port: u16,
    pub log_level: LogLevel,
    pub store_selected: bool,
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
            mixed_port: 7890,
            log_level: LogLevel::Warning,
            store_selected: true,
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
                    kind: PolicyGroupKind::UrlTest {
                        use_providers: vec![provider_name.clone()],
                        url: "https://www.gstatic.com/generate_204".to_owned(),
                        interval_secs: 600,
                    },
                },
                PolicyGroup {
                    name: proxy_name.clone(),
                    kind: PolicyGroupKind::Select {
                        proxies: vec![PolicyRef::Group(automatic_name), PolicyRef::Direct],
                        use_providers: vec![provider_name],
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

    /// Validates names, references, paths, intervals, and rule termination.
    ///
    /// # Errors
    /// Returns a stable error category that never embeds subscription data.
    pub fn validate(&self) -> Result<(), ProfileError> {
        if self.mixed_port == 0 {
            return Err(ProfileError::InvalidValue("mixed port"));
        }

        let mut provider_names = HashSet::new();
        for provider in &self.providers {
            if !provider_names.insert(provider.name.clone()) {
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
                || !group_names.insert(group.name.clone())
            {
                return Err(ProfileError::DuplicateName);
            }
        }

        for group in &self.groups {
            match &group.kind {
                PolicyGroupKind::Select {
                    proxies,
                    use_providers,
                } => {
                    if proxies.is_empty() && use_providers.is_empty() {
                        return Err(ProfileError::InvalidValue("select group"));
                    }
                    validate_policy_refs(proxies, &group_names)?;
                    validate_provider_refs(use_providers, &provider_names)?;
                }
                PolicyGroupKind::UrlTest {
                    use_providers,
                    url,
                    interval_secs,
                } => {
                    if use_providers.is_empty() || *interval_secs == 0 || !is_https_url(url) {
                        return Err(ProfileError::InvalidValue("url-test group"));
                    }
                    validate_provider_refs(use_providers, &provider_names)?;
                }
            }
        }

        if !matches!(self.rules.last(), Some(Rule::Match { .. })) {
            return Err(ProfileError::MissingTerminalMatch);
        }
        for (index, rule) in self.rules.iter().enumerate() {
            if matches!(rule, Rule::Match { .. }) && index + 1 != self.rules.len() {
                return Err(ProfileError::MissingTerminalMatch);
            }
            match rule {
                Rule::Domain { value, policy } | Rule::DomainSuffix { value, policy } => {
                    if !is_rule_value(value) {
                        return Err(ProfileError::InvalidValue("domain rule"));
                    }
                    validate_policy_ref(policy, &group_names)?;
                }
                Rule::GeoIp {
                    country, policy, ..
                } => {
                    if !is_rule_value(country) {
                        return Err(ProfileError::InvalidValue("GEOIP rule"));
                    }
                    validate_policy_ref(policy, &group_names)?;
                }
                Rule::Match { policy } => validate_policy_ref(policy, &group_names)?,
            }
        }
        Ok(())
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PolicyGroup {
    pub name: Name,
    pub kind: PolicyGroupKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PolicyGroupKind {
    Select {
        proxies: Vec<PolicyRef>,
        use_providers: Vec<Name>,
    },
    UrlTest {
        use_providers: Vec<Name>,
        url: String,
        interval_secs: u32,
    },
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum PolicyRef {
    Direct,
    Reject,
    Group(Name),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Rule {
    Domain {
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProfileError {
    InvalidUrl,
    InvalidName,
    InvalidValue(&'static str),
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
    yaml.push_str("mode: \"rule\"\nallow-lan: false\nbind-address: \"127.0.0.1\"\n");
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
        writeln!(yaml, "  - name: {}", quoted(group.name.as_str()))
            .expect("String write cannot fail");
        match &group.kind {
            PolicyGroupKind::Select {
                proxies,
                use_providers,
            } => {
                yaml.push_str("    type: \"select\"\n");
                if !proxies.is_empty() {
                    yaml.push_str("    proxies:\n");
                    for policy in proxies {
                        writeln!(yaml, "      - {}", quoted(policy_name(policy)))
                            .expect("String write cannot fail");
                    }
                }
                render_provider_use(&mut yaml, use_providers);
            }
            PolicyGroupKind::UrlTest {
                use_providers,
                url,
                interval_secs,
            } => {
                yaml.push_str("    type: \"url-test\"\n");
                render_provider_use(&mut yaml, use_providers);
                writeln!(yaml, "    url: {}", quoted(url)).expect("String write cannot fail");
                writeln!(yaml, "    interval: {interval_secs}").expect("String write cannot fail");
                yaml.push_str("    lazy: true\n");
            }
        }
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

fn validate_policy_refs(
    policies: &[PolicyRef],
    groups: &HashSet<Name>,
) -> Result<(), ProfileError> {
    for policy in policies {
        validate_policy_ref(policy, groups)?;
    }
    Ok(())
}

fn validate_policy_ref(policy: &PolicyRef, groups: &HashSet<Name>) -> Result<(), ProfileError> {
    if let PolicyRef::Group(name) = policy
        && !groups.contains(name)
    {
        return Err(ProfileError::DanglingReference);
    }
    Ok(())
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

fn is_rule_value(value: &str) -> bool {
    is_plain_value(value, 1024) && !value.contains(',')
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

fn policy_name(policy: &PolicyRef) -> &str {
    match policy {
        PolicyRef::Direct => "DIRECT",
        PolicyRef::Reject => "REJECT",
        PolicyRef::Group(name) => name.as_str(),
    }
}

fn render_rule(rule: &Rule) -> String {
    match rule {
        Rule::Domain { value, policy } => format!("DOMAIN,{value},{}", policy_name(policy)),
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
