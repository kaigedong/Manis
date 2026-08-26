//! User-managed rules that keep chosen traffic off the proxy.
//!
//! TUN mode captures every connection, which breaks protocols the proxy egress cannot carry —
//! SSH being the one that shows up as a failing `git push`. These rules are compiled ahead of
//! every other rule so the listed ports and domains always go direct.

use std::fmt;
#[cfg(not(windows))]
use std::fs;
use std::path::Path;

#[cfg(not(windows))]
use manis_profile::write_private_atomic;
use manis_profile::{PolicyRef, Profile, Rule};

const DIRECT_RULES_FILE: &str = "direct-rules.state";
const DIRECT_RULES_VERSION: &str = "manis.direct-rules.v1";
/// A domain label stays well under this; the cap only stops a pathological file.
const MAX_DOMAIN_BYTES: usize = 253;
#[cfg(not(windows))]
const MAX_DIRECT_RULES_FILE_BYTES: u64 = 64 * 1024;

/// One user-managed exemption from the proxy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum DirectRule {
    Port(u16),
    DomainSuffix(String),
}

/// Why a typed entry could not become a rule.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DirectRuleError {
    Empty,
    PortOutOfRange,
    InvalidDomain,
}

/// Why stored rules could not be read or written.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DirectRuleStoreError {
    Unavailable,
    Corrupt,
}

impl fmt::Display for DirectRuleStoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable => formatter.write_str("直连规则存储不可用"),
            Self::Corrupt => formatter.write_str("直连规则文件已损坏"),
        }
    }
}

impl std::error::Error for DirectRuleStoreError {}

impl DirectRule {
    /// Reads one entry the way the user typed it.
    ///
    /// A bare number is a destination port; anything else is a domain suffix. Everything is
    /// validated here so a malformed entry can never reach a generated kernel config.
    ///
    /// # Errors
    /// Returns the specific reason the entry is not usable.
    pub(crate) fn parse(input: &str) -> Result<Self, DirectRuleError> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err(DirectRuleError::Empty);
        }
        if trimmed.bytes().all(|byte| byte.is_ascii_digit()) {
            return trimmed
                .parse::<u16>()
                .ok()
                .filter(|port| *port > 0)
                .map(Self::Port)
                .ok_or(DirectRuleError::PortOutOfRange);
        }
        let domain = trimmed.to_ascii_lowercase();
        if is_domain_suffix(&domain) {
            Ok(Self::DomainSuffix(domain))
        } else {
            Err(DirectRuleError::InvalidDomain)
        }
    }

    /// The text shown in the list and written to storage.
    pub(crate) fn label(&self) -> String {
        match self {
            Self::Port(port) => port.to_string(),
            Self::DomainSuffix(domain) => domain.clone(),
        }
    }
}

/// Accepts only what both kernels treat as a plain domain suffix.
///
/// Rejecting schemes, paths, whitespace and empty labels here keeps the generated YAML and JSON
/// free of values that would need escaping.
fn is_domain_suffix(value: &str) -> bool {
    if value.is_empty() || value.len() > MAX_DOMAIN_BYTES || !value.contains('.') {
        return false;
    }
    value.split('.').all(|label| {
        !label.is_empty()
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    })
}

/// The rules a fresh install starts with.
///
/// SSH is the case that sends people to the tray to turn the proxy off, so it ships enabled.
pub(crate) fn default_direct_rules() -> Vec<DirectRule> {
    vec![DirectRule::Port(22)]
}

/// Compiles the entries into kernel rules that all resolve to a direct connection.
pub(crate) fn to_profile_rules(rules: &[DirectRule]) -> Vec<Rule> {
    rules
        .iter()
        .map(|rule| match rule {
            DirectRule::Port(port) => Rule::DstPort {
                port: *port,
                policy: PolicyRef::Direct,
            },
            DirectRule::DomainSuffix(value) => Rule::DomainSuffix {
                value: value.clone(),
                policy: PolicyRef::Direct,
            },
        })
        .collect()
}

/// Puts the user's exemptions ahead of every rule the profile already carries.
///
/// Order within the list is preserved so the page reads the same way the kernel matches.
pub(crate) fn prepend_direct_rules(profile: &mut Profile, rules: &[DirectRule]) {
    for (index, rule) in to_profile_rules(rules).into_iter().enumerate() {
        profile.rules.insert(index, rule);
    }
}

fn encode_direct_rules(rules: &[DirectRule]) -> String {
    let mut contents = String::from(DIRECT_RULES_VERSION);
    for rule in rules {
        let kind = match rule {
            DirectRule::Port(_) => "port",
            DirectRule::DomainSuffix(_) => "domain-suffix",
        };
        contents.push('\n');
        contents.push_str(kind);
        contents.push('\t');
        contents.push_str(&rule.label());
    }
    contents
}

fn decode_direct_rules(contents: &str) -> Result<Vec<DirectRule>, DirectRuleStoreError> {
    let mut lines = contents.lines();
    if lines.next() != Some(DIRECT_RULES_VERSION) {
        return Err(DirectRuleStoreError::Corrupt);
    }
    let mut rules = Vec::new();
    for line in lines.filter(|line| !line.is_empty()) {
        let (kind, value) = line.split_once('\t').ok_or(DirectRuleStoreError::Corrupt)?;
        // Re-validating on read means a hand-edited file cannot widen what reaches the kernel.
        let rule = match kind {
            "port" => match DirectRule::parse(value) {
                Ok(rule @ DirectRule::Port(_)) => rule,
                _ => return Err(DirectRuleStoreError::Corrupt),
            },
            "domain-suffix" => match DirectRule::parse(value) {
                Ok(rule @ DirectRule::DomainSuffix(_)) => rule,
                _ => return Err(DirectRuleStoreError::Corrupt),
            },
            _ => return Err(DirectRuleStoreError::Corrupt),
        };
        rules.push(rule);
    }
    Ok(rules)
}

/// Writes the rules to a private `0600` file, replacing it atomically.
///
/// # Errors
/// Returns [`DirectRuleStoreError::Unavailable`] when the store cannot be written.
#[cfg(not(windows))]
pub(crate) fn save_direct_rules_in(
    directory: &Path,
    rules: &[DirectRule],
) -> Result<(), DirectRuleStoreError> {
    write_private_atomic(
        directory,
        DIRECT_RULES_FILE,
        encode_direct_rules(rules).as_bytes(),
    )
    .map(|_path| ())
    .map_err(|_error| DirectRuleStoreError::Unavailable)
}

#[cfg(windows)]
pub(crate) fn save_direct_rules_in(
    _directory: &Path,
    _rules: &[DirectRule],
) -> Result<(), DirectRuleStoreError> {
    Err(DirectRuleStoreError::Unavailable)
}

/// Reads the stored rules, seeding SSH the first time the file does not exist yet.
///
/// An existing but empty file means the user cleared the list, which is preserved.
///
/// # Errors
/// Returns [`DirectRuleStoreError::Corrupt`] when the file cannot be trusted.
#[cfg(not(windows))]
pub(crate) fn load_direct_rules_in(
    directory: &Path,
) -> Result<Vec<DirectRule>, DirectRuleStoreError> {
    let path = directory.join(DIRECT_RULES_FILE);
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(default_direct_rules());
        }
        Err(_error) => return Err(DirectRuleStoreError::Unavailable),
    };
    // A symlink here would let another writer redirect the read outside the private store.
    if !metadata.is_file() || metadata.len() > MAX_DIRECT_RULES_FILE_BYTES {
        return Err(DirectRuleStoreError::Corrupt);
    }
    let contents = fs::read_to_string(&path).map_err(|_error| DirectRuleStoreError::Corrupt)?;
    decode_direct_rules(&contents)
}

#[cfg(windows)]
pub(crate) fn load_direct_rules_in(
    _directory: &Path,
) -> Result<Vec<DirectRule>, DirectRuleStoreError> {
    Ok(default_direct_rules())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{
        DirectRule, DirectRuleError, default_direct_rules, load_direct_rules_in,
        save_direct_rules_in, to_profile_rules,
    };

    fn test_dir(name: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("manis-{name}-{}", std::process::id()));
        if root.exists() {
            fs::remove_dir_all(&root).expect("remove stale fixture");
        }
        root
    }

    #[test]
    fn a_bare_number_parses_as_a_destination_port() {
        assert_eq!(DirectRule::parse("22"), Ok(DirectRule::Port(22)));
        assert_eq!(DirectRule::parse(" 443 "), Ok(DirectRule::Port(443)));
        assert_eq!(DirectRule::parse("65535"), Ok(DirectRule::Port(65535)));
    }

    #[test]
    fn anything_else_parses_as_a_domain_suffix() {
        assert_eq!(
            DirectRule::parse("github.com"),
            Ok(DirectRule::DomainSuffix("github.com".to_owned()))
        );
        assert_eq!(
            DirectRule::parse("  SSH.GitHub.com "),
            Ok(DirectRule::DomainSuffix("ssh.github.com".to_owned()))
        );
    }

    #[test]
    fn ports_outside_the_usable_range_are_rejected() {
        assert_eq!(DirectRule::parse("0"), Err(DirectRuleError::PortOutOfRange));
        assert_eq!(
            DirectRule::parse("65536"),
            Err(DirectRuleError::PortOutOfRange)
        );
        assert_eq!(
            DirectRule::parse("999999999999"),
            Err(DirectRuleError::PortOutOfRange)
        );
    }

    #[test]
    fn malformed_domains_are_rejected_before_reaching_the_kernel() {
        assert_eq!(DirectRule::parse(""), Err(DirectRuleError::Empty));
        assert_eq!(DirectRule::parse("   "), Err(DirectRuleError::Empty));
        for invalid in [
            "https://github.com",
            "github.com/path",
            "git hub.com",
            "github..com",
            ".github.com",
            "github.com.",
            "-github.com",
            "gith\tub.com",
        ] {
            assert_eq!(
                DirectRule::parse(invalid),
                Err(DirectRuleError::InvalidDomain),
                "{invalid} must be rejected"
            );
        }
    }

    #[test]
    fn direct_rules_are_matched_before_every_inherited_rule() {
        let mut profile = manis_profile::Profile::qx_default(
            manis_profile::SecretUrl::parse_https(
                "https://subscription.example.invalid/client?token=fixture",
            )
            .expect("fixture url is valid"),
        )
        .expect("default profile is valid");

        super::prepend_direct_rules(
            &mut profile,
            &[
                DirectRule::Port(22),
                DirectRule::DomainSuffix("github.com".to_owned()),
            ],
        );

        let rendered = manis_profile::render_mihomo_yaml(&profile).expect("profile should render");
        let ssh = rendered.find("DST-PORT,22,DIRECT").expect("port rule");
        let domain = rendered
            .find("DOMAIN-SUFFIX,github.com,DIRECT")
            .expect("domain rule");
        let geoip = rendered.find("GEOIP,CN,DIRECT").expect("inherited rule");
        let terminal = rendered.find("MATCH,Proxy").expect("terminal rule");

        assert!(ssh < domain, "entries keep the order the user set");
        assert!(
            domain < geoip,
            "direct rules outrank the inherited GEOIP rule"
        );
        assert!(geoip < terminal);
        assert!(profile.validate().is_ok());
    }

    #[test]
    fn rules_compile_to_direct_kernel_rules() {
        let rules = to_profile_rules(&[
            DirectRule::Port(22),
            DirectRule::DomainSuffix("github.com".to_owned()),
        ]);

        assert_eq!(
            rules,
            vec![
                manis_profile::Rule::DstPort {
                    port: 22,
                    policy: manis_profile::PolicyRef::Direct,
                },
                manis_profile::Rule::DomainSuffix {
                    value: "github.com".to_owned(),
                    policy: manis_profile::PolicyRef::Direct,
                },
            ]
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn a_missing_file_seeds_ssh_but_an_empty_file_stays_empty()
    -> Result<(), Box<dyn std::error::Error>> {
        use std::os::unix::fs::PermissionsExt;

        let root = test_dir("direct-rules-seed");
        let store = root.join("subscriptions");
        fs::create_dir_all(&store)?;
        fs::set_permissions(&store, fs::Permissions::from_mode(0o700))?;

        // Nothing saved yet: SSH is seeded so the common case works out of the box.
        assert_eq!(load_direct_rules_in(&store)?, default_direct_rules());
        assert_eq!(default_direct_rules(), vec![DirectRule::Port(22)]);

        // The user cleared the list; that decision must survive a restart.
        save_direct_rules_in(&store, &[])?;
        assert_eq!(load_direct_rules_in(&store)?, Vec::new());

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[cfg(not(windows))]
    #[test]
    fn saved_rules_round_trip_in_a_private_file() -> Result<(), Box<dyn std::error::Error>> {
        use std::os::unix::fs::PermissionsExt;

        let root = test_dir("direct-rules-roundtrip");
        let store = root.join("subscriptions");
        fs::create_dir_all(&store)?;
        fs::set_permissions(&store, fs::Permissions::from_mode(0o700))?;

        let rules = vec![
            DirectRule::Port(22),
            DirectRule::DomainSuffix("github.com".to_owned()),
        ];
        save_direct_rules_in(&store, &rules)?;

        assert_eq!(load_direct_rules_in(&store)?, rules);

        let path = store.join(super::DIRECT_RULES_FILE);
        let mode = fs::metadata(&path)?.permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[cfg(not(windows))]
    #[test]
    fn a_corrupt_file_is_reported_rather_than_silently_dropped()
    -> Result<(), Box<dyn std::error::Error>> {
        use std::os::unix::fs::PermissionsExt;

        let root = test_dir("direct-rules-corrupt");
        let store = root.join("subscriptions");
        fs::create_dir_all(&store)?;
        fs::set_permissions(&store, fs::Permissions::from_mode(0o700))?;
        let path = store.join(super::DIRECT_RULES_FILE);

        for corrupt in [
            "manis.direct-rules.v9\nport\t22",
            "manis.direct-rules.v1\nport\t70000",
            "manis.direct-rules.v1\nport\tnot-a-number",
            "manis.direct-rules.v1\nunknown-kind\t22",
            "manis.direct-rules.v1\ndomain-suffix\thttps://github.com",
        ] {
            fs::write(&path, corrupt)?;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
            assert!(
                load_direct_rules_in(&store).is_err(),
                "{corrupt} must be rejected"
            );
        }

        fs::remove_dir_all(root)?;
        Ok(())
    }
}
