//! Read-only compatibility parser for the legacy direct-rule store.
//!
//! New versions migrate these entries into unified manual routing rules. This module deliberately
//! has no runtime compiler or writer so the old and new rule systems cannot diverge.

use std::fmt;
use std::path::Path;

const DIRECT_RULES_FILE: &str = "direct-rules.state";
const DIRECT_RULES_VERSION: &str = "manis.direct-rules.v1";
/// A domain label stays well under this; the cap only stops a pathological file.
const MAX_DOMAIN_BYTES: usize = 253;
const MAX_DIRECT_RULES_FILE_BYTES: u64 = 64 * 1024;

/// One entry decoded from the legacy store.
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
            Self::Unavailable => formatter.write_str("direct rule store is unavailable"),
            Self::Corrupt => formatter.write_str("direct rule file is corrupt"),
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
            && label.len() <= 63
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    })
}

/// The entry old versions applied when no legacy file existed.
pub(crate) fn default_direct_rules() -> Vec<DirectRule> {
    vec![DirectRule::Port(22)]
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

/// Reads the stored rules, seeding SSH the first time the file does not exist yet.
///
/// An existing but empty file means the user cleared the list, which is preserved.
///
/// # Errors
/// Returns [`DirectRuleStoreError::Corrupt`] when the file cannot be trusted.
pub(crate) fn load_direct_rules_in(
    directory: &Path,
) -> Result<Vec<DirectRule>, DirectRuleStoreError> {
    let Some(contents) =
        crate::config_toml::read_entry(directory, DIRECT_RULES_FILE, MAX_DIRECT_RULES_FILE_BYTES)
            .map_err(|error| match error {
            crate::config_toml::ConfigTomlError::Unavailable => DirectRuleStoreError::Unavailable,
            crate::config_toml::ConfigTomlError::UnsafePath
            | crate::config_toml::ConfigTomlError::InvalidFormat
            | crate::config_toml::ConfigTomlError::Oversized => DirectRuleStoreError::Corrupt,
        })?
    else {
        return Ok(default_direct_rules());
    };
    decode_direct_rules(&contents)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{DirectRule, DirectRuleError, default_direct_rules, load_direct_rules_in};

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
    fn domain_labels_respect_the_dns_length_limit() {
        let valid = format!("{}.example", "a".repeat(63));
        let invalid = format!("{}.example", "a".repeat(64));

        assert!(DirectRule::parse(&valid).is_ok());
        assert_eq!(
            DirectRule::parse(&invalid),
            Err(DirectRuleError::InvalidDomain)
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

        // An explicitly empty legacy file means the user cleared the old list.
        let path = store.join(super::DIRECT_RULES_FILE);
        fs::write(&path, super::DIRECT_RULES_VERSION)?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
        assert_eq!(load_direct_rules_in(&store)?, Vec::new());

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
