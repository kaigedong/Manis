use std::{
    collections::{HashMap, HashSet},
    fmt,
};

use crate::{
    MAX_VLESS_FIELD_BYTES, Name, PolicyRef, ProfileError, Rule, VlessSecurity,
    VlessSecurityOptions, VlessTransport, is_rule_value,
};

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

pub(crate) fn is_https_url(input: &str) -> bool {
    is_url_with_scheme(input, "https://")
}

pub(crate) fn is_subscription_url(input: &str) -> bool {
    is_url_with_scheme(input, "https://") || is_url_with_scheme(input, "http://")
}

pub(crate) fn parse_vless_query(query: &str) -> Result<HashMap<String, String>, ProfileError> {
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

pub(crate) fn optional_vless_value(
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

pub(crate) fn parse_vless_security(
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
    let skip_cert_verify = parse_vless_bool(fields.get("allowinsecure").map(String::as_str))?;
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

pub(crate) fn parse_vless_transport(
    fields: &HashMap<String, String>,
) -> Result<VlessTransport, ProfileError> {
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

pub(crate) fn require_vless_encryption(value: Option<&str>) -> Result<(), ProfileError> {
    match value.unwrap_or("none") {
        "none" => Ok(()),
        _ => Err(ProfileError::UnsupportedVless),
    }
}

fn parse_vless_bool(value: Option<&str>) -> Result<bool, ProfileError> {
    match value.unwrap_or("false") {
        "false" | "0" => Ok(false),
        "true" | "1" => Ok(true),
        _ => Err(ProfileError::InvalidVless),
    }
}

pub(crate) fn parse_vless_server(value: &str) -> Result<(String, u16), ProfileError> {
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

pub(crate) fn is_vless_host(value: &str) -> bool {
    is_plain_value(value, 253)
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_' | b':'))
}

pub(crate) fn is_uuid(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(index, byte)| {
            if matches!(index, 8 | 13 | 18 | 23) {
                byte == b'-'
            } else {
                byte.is_ascii_hexdigit()
            }
        })
}

pub(crate) fn decode_query_value(value: &str) -> Option<String> {
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

pub(crate) fn is_plain_value(value: &str, max_bytes: usize) -> bool {
    !value.is_empty()
        && value.len() <= max_bytes
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

fn parse_qx_rule_kind(value: &str) -> Option<QxRuleKind> {
    if value.eq_ignore_ascii_case("DOMAIN") || value.eq_ignore_ascii_case("HOST") {
        Some(QxRuleKind::Domain)
    } else if value.eq_ignore_ascii_case("DOMAIN-KEYWORD")
        || value.eq_ignore_ascii_case("HOST-KEYWORD")
    {
        Some(QxRuleKind::DomainKeyword)
    } else if value.eq_ignore_ascii_case("DOMAIN-SUFFIX")
        || value.eq_ignore_ascii_case("HOST-SUFFIX")
    {
        Some(QxRuleKind::DomainSuffix)
    } else {
        None
    }
}
