//! User-authored routing rules with QX-compatible match types.

use std::fmt;
use std::net::IpAddr;

use manis_core::KernelKind;

mod compiler;
mod store;

mod model;

pub(super) use model::LEGACY_GENERATED_PROXY_GROUP_NAME;
pub(crate) use model::{
    MAX_CONDITIONS, ManualRule, ManualRuleCompileError, ManualRuleCondition, ManualRuleEditError,
    ManualRuleError, ManualRuleKind, ManualRuleStoreError, replace_manual_rule,
};

pub(crate) use compiler::append_manual_rules;
#[cfg(all(not(windows), test))]
pub(super) use store::decode_manual_rules;
pub(crate) use store::{load_manual_rules_in, save_manual_rules_in};

const MANUAL_RULES_FILE: &str = "manual-routing-rules.state";
const MANUAL_RULES_VERSION_V1: &str = "manis.manual-routing-rules.v1";
const MANUAL_RULES_VERSION_V2: &str = "manis.manual-routing-rules.v2";
const MANUAL_RULES_VERSION_V3: &str = "manis.manual-routing-rules.v3";
const MANUAL_RULES_VERSION_V4: &str = "manis.manual-routing-rules.v4";
const MAX_MANUAL_RULES_FILE_BYTES: u64 = 256 * 1024;
#[cfg(test)]
mod tests {
    use std::fs;

    use super::{
        ManualRule, ManualRuleCompileError, ManualRuleEditError, ManualRuleError, ManualRuleKind,
        append_manual_rules, load_manual_rules_in, replace_manual_rule, save_manual_rules_in,
    };

    #[test]
    fn qx_parameter_shapes_are_validated_and_normalized() {
        let cases = [
            (ManualRuleKind::Host, "EXAMPLE.com", "example.com"),
            (ManualRuleKind::HostSuffix, "example.com", "example.com"),
            (
                ManualRuleKind::HostWildcard,
                "*.Example.?om",
                "*.example.?om",
            ),
            (ManualRuleKind::HostKeyword, "google", "google"),
            (ManualRuleKind::UserAgent, "*abc?", "*abc?"),
            (ManualRuleKind::IpCidr, "192.168.0.1/24", "192.168.0.1/24"),
            (
                ManualRuleKind::Ip6Cidr,
                "2001:4860:4860::8888/32",
                "2001:4860:4860::8888/32",
            ),
            (ManualRuleKind::GeoIp, "us", "US"),
            (ManualRuleKind::IpAsn, "06185", "6185"),
            (ManualRuleKind::DstPort, "022", "22"),
        ];
        for (kind, input, expected) in cases {
            let rule = ManualRule::parse(kind, input, "Proxy").expect("valid QX parameter");
            assert_eq!(rule.conditions()[0].parameter(), expected);
        }
    }

    #[test]
    fn domain_rule_labels_match_imported_rule_terminology() {
        assert_eq!(ManualRuleKind::Host.display_label(), "DOMAIN");
        assert_eq!(ManualRuleKind::HostSuffix.display_label(), "DOMAIN-SUFFIX");
        assert_eq!(
            ManualRuleKind::HostWildcard.display_label(),
            "DOMAIN-WILDCARD"
        );
        assert_eq!(
            ManualRuleKind::HostKeyword.display_label(),
            "DOMAIN-KEYWORD"
        );
        assert_eq!(ManualRuleKind::Final.display_label(), "FINAL");
    }

    #[test]
    fn final_is_parameterless_and_cannot_be_combined() {
        let final_rule =
            ManualRule::parse(ManualRuleKind::Final, "", "DIRECT").expect("parameterless FINAL");

        assert!(final_rule.is_final());
        assert!(final_rule.conditions().is_empty());
        assert_eq!(final_rule.target(), "DIRECT");
        assert_eq!(
            ManualRule::parse(ManualRuleKind::Final, "unused", "DIRECT"),
            Err(ManualRuleError::FinalHasNoParameter)
        );
        assert_eq!(
            ManualRule::parse_conditions(
                vec![
                    (ManualRuleKind::HostSuffix, "example.com".to_owned()),
                    (ManualRuleKind::Final, String::new()),
                ],
                "DIRECT",
            ),
            Err(ManualRuleError::FinalMustStandAlone)
        );
    }

    #[test]
    fn address_families_and_unsafe_values_are_rejected() {
        assert_eq!(
            ManualRule::parse(ManualRuleKind::IpCidr, "2001:db8::/32", "Proxy"),
            Err(ManualRuleError::InvalidIpv4Cidr)
        );
        assert_eq!(
            ManualRule::parse(ManualRuleKind::Ip6Cidr, "192.0.2.0/24", "Proxy"),
            Err(ManualRuleError::InvalidIpv6Cidr)
        );
        assert_eq!(
            ManualRule::parse(ManualRuleKind::Host, "https://example.com", "Proxy"),
            Err(ManualRuleError::InvalidDomain)
        );
        assert_eq!(
            ManualRule::parse(ManualRuleKind::HostKeyword, "a,b", "Proxy"),
            Err(ManualRuleError::InvalidKeyword)
        );
        assert_eq!(
            ManualRule::parse(ManualRuleKind::DstPort, "0", "DIRECT"),
            Err(ManualRuleError::InvalidDestinationPort)
        );
    }

    #[test]
    fn manual_rules_append_in_source_order_without_generated_fallbacks() {
        let mut profile = manis_profile::Profile::qx_default(
            manis_profile::SecretUrl::parse_https("https://example.invalid/subscription")
                .expect("fixture URL"),
        )
        .expect("fixture profile");
        let rules = vec![
            ManualRule::parse(ManualRuleKind::Host, "example.com", "DIRECT").expect("rule"),
            ManualRule::parse(ManualRuleKind::IpAsn, "13335", "Proxy").expect("rule"),
        ];
        append_manual_rules(&mut profile, &rules, manis_core::KernelKind::Mihomo)
            .expect("supported rules");
        let yaml = manis_profile::render_mihomo_yaml(&profile).expect("rendered profile");
        assert!(
            yaml.find("DOMAIN,example.com,DIRECT") < yaml.find("IP-ASN,13335,__MANIS_GLOBAL__")
        );
        assert!(!yaml.contains("GEOIP,CN,DIRECT"));
        assert!(!yaml.contains("MATCH,__MANIS_GLOBAL__"));
    }

    #[test]
    fn geoip_rules_resolve_domain_only_connections() {
        let mut profile = manis_profile::Profile::qx_default(
            manis_profile::SecretUrl::parse_https("https://example.invalid/subscription")
                .expect("fixture URL"),
        )
        .expect("fixture profile");
        let rule = ManualRule::parse(ManualRuleKind::GeoIp, "CN", "DIRECT").expect("rule");

        append_manual_rules(&mut profile, &[rule], manis_core::KernelKind::Mihomo)
            .expect("supported rule");
        let yaml = manis_profile::render_mihomo_yaml(&profile).expect("rendered profile");

        assert!(yaml.contains("GEOIP,CN,DIRECT"));
        assert!(!yaml.contains("GEOIP,CN,DIRECT,no-resolve"));
    }

    #[test]
    fn final_compiles_after_every_specific_rule_regardless_of_saved_order() {
        let mut profile = manis_profile::Profile::qx_default(
            manis_profile::SecretUrl::parse_https("https://example.invalid/subscription")
                .expect("fixture URL"),
        )
        .expect("fixture profile");
        let rules = vec![
            ManualRule::final_rule("DIRECT").expect("FINAL"),
            ManualRule::parse(ManualRuleKind::HostSuffix, "example.com", "Proxy")
                .expect("specific rule"),
        ];

        append_manual_rules(&mut profile, &rules, manis_core::KernelKind::Mihomo)
            .expect("supported rules");
        let yaml = manis_profile::render_mihomo_yaml(&profile).expect("rendered profile");

        assert!(
            yaml.find("DOMAIN-SUFFIX,example.com,__MANIS_GLOBAL__") < yaml.find("MATCH,DIRECT")
        );
        assert!(yaml.trim_end().ends_with("\"MATCH,DIRECT\""));
    }

    #[test]
    fn replacing_with_a_second_final_is_rejected_even_when_targets_differ() {
        let first = ManualRule::parse(ManualRuleKind::Host, "example.com", "DIRECT")
            .expect("specific rule");
        let final_rule = ManualRule::final_rule("DIRECT").expect("FINAL");
        let mut rules = vec![first, final_rule];

        assert_eq!(
            replace_manual_rule(
                &mut rules,
                0,
                ManualRule::final_rule("REJECT").expect("replacement FINAL"),
            ),
            Err(ManualRuleEditError::FinalAlreadyExists)
        );
    }

    #[test]
    fn multiple_final_rules_leave_profile_unchanged() {
        let mut profile = manis_profile::Profile::qx_default(
            manis_profile::SecretUrl::parse_https("https://example.invalid/subscription")
                .expect("fixture URL"),
        )
        .expect("fixture profile");
        let before = profile.clone();
        let rules = vec![
            ManualRule::parse(ManualRuleKind::HostSuffix, "example.com", "Proxy")
                .expect("specific rule"),
            ManualRule::final_rule("DIRECT").expect("first FINAL"),
            ManualRule::final_rule("Proxy").expect("second FINAL"),
        ];

        assert_eq!(
            append_manual_rules(&mut profile, &rules, manis_core::KernelKind::Mihomo),
            Err(ManualRuleCompileError::MultipleFinalRules)
        );
        assert_eq!(profile, before);
    }

    #[test]
    fn disabled_manual_rules_are_not_compiled() {
        let mut profile = manis_profile::Profile::qx_default(
            manis_profile::SecretUrl::parse_https("https://example.invalid/subscription")
                .expect("fixture URL"),
        )
        .expect("fixture profile");
        let enabled =
            ManualRule::parse(ManualRuleKind::Host, "enabled.example", "DIRECT").expect("rule");
        let mut disabled =
            ManualRule::parse(ManualRuleKind::Host, "disabled.example", "DIRECT").expect("rule");
        disabled.set_enabled(false);

        append_manual_rules(
            &mut profile,
            &[enabled, disabled],
            manis_core::KernelKind::Mihomo,
        )
        .expect("supported rules");
        let yaml = manis_profile::render_mihomo_yaml(&profile).expect("rendered profile");
        assert!(yaml.contains("DOMAIN,enabled.example,DIRECT"));
        assert!(!yaml.contains("disabled.example"));
    }

    #[test]
    fn compound_domain_and_port_rule_compiles_as_an_exact_and_match() {
        let mut profile = manis_profile::Profile::qx_default(
            manis_profile::SecretUrl::parse_https("https://example.invalid/subscription")
                .expect("fixture URL"),
        )
        .expect("fixture profile");
        let rule = ManualRule::parse_conditions(
            vec![
                (ManualRuleKind::HostSuffix, "github.com".to_owned()),
                (ManualRuleKind::DstPort, "22".to_owned()),
            ],
            "DIRECT",
        )
        .expect("compound rule");
        append_manual_rules(&mut profile, &[rule], manis_core::KernelKind::Mihomo)
            .expect("supported rule");

        let yaml = manis_profile::render_mihomo_yaml(&profile).expect("rendered profile");
        assert!(yaml.contains("AND,((DOMAIN-SUFFIX,github.com),(DST-PORT,22)),DIRECT"));
    }

    #[cfg(not(windows))]
    #[test]
    fn rules_round_trip_through_private_storage() -> Result<(), Box<dyn std::error::Error>> {
        let root = std::env::temp_dir().join(format!("manis-manual-rules-{}", std::process::id()));
        if root.exists() {
            fs::remove_dir_all(&root)?;
        }
        fs::create_dir_all(&root)?;
        let mut rules = vec![
            ManualRule::final_rule("DIRECT")?,
            ManualRule::parse_conditions(
                vec![
                    (ManualRuleKind::HostSuffix, "example.com".to_owned()),
                    (ManualRuleKind::DstPort, "22".to_owned()),
                ],
                "Proxy",
            )?,
            ManualRule::parse(ManualRuleKind::GeoIp, "US", "DIRECT")?,
        ];
        rules[2].set_enabled(false);
        save_manual_rules_in(&root, &rules)?;
        let loaded = load_manual_rules_in(&root)?;
        assert_eq!(loaded.len(), 3);
        assert!(loaded.last().is_some_and(ManualRule::is_final));
        assert!(!loaded[1].is_enabled());
        let stored = crate::config_toml::read_entry(
            &root,
            super::MANUAL_RULES_FILE,
            super::MAX_MANUAL_RULES_FILE_BYTES,
        )?
        .expect("stored rules");
        assert!(stored.starts_with(super::MANUAL_RULES_VERSION_V4));
        assert!(
            stored
                .lines()
                .last()
                .is_some_and(|line| line == "final\t1\tDIRECT")
        );
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[cfg(not(windows))]
    #[test]
    fn storage_rejects_more_than_one_final_rule() -> Result<(), Box<dyn std::error::Error>> {
        let root = std::env::temp_dir().join(format!("manis-manual-final-{}", std::process::id()));
        fs::create_dir_all(&root)?;
        let rules = [
            ManualRule::final_rule("DIRECT")?,
            ManualRule::final_rule("REJECT")?,
        ];

        assert_eq!(
            save_manual_rules_in(&root, &rules),
            Err(super::ManualRuleStoreError::Corrupt)
        );
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[cfg(not(windows))]
    #[test]
    fn version_two_rules_upgrade_as_enabled() {
        let contents = format!(
            "{}\nlegacy-direct-rules-migrated\t1\nrule\tDIRECT\thost\texample.com",
            super::MANUAL_RULES_VERSION_V2
        );

        let (rules, migrated) = super::decode_manual_rules(&contents).expect("legacy v2 rules");

        assert!(migrated);
        assert_eq!(rules.len(), 1);
        assert!(rules[0].is_enabled());
    }

    #[test]
    fn replacing_a_manual_rule_preserves_its_enabled_state() {
        let mut existing = ManualRule::parse(ManualRuleKind::HostSuffix, "old.example", "DIRECT")
            .expect("existing rule");
        existing.set_enabled(false);
        let replacement =
            ManualRule::parse(ManualRuleKind::HostSuffix, "replacement.example", "DIRECT")
                .expect("replacement rule");
        let mut rules = vec![existing];

        replace_manual_rule(&mut rules, 0, replacement).expect("replace rule");

        assert!(!rules[0].is_enabled());
        assert_eq!(rules[0].conditions()[0].parameter(), "replacement.example");
    }

    #[test]
    fn replacing_a_manual_rule_preserves_order_and_ignores_itself_for_duplicates() {
        let first = ManualRule::parse(ManualRuleKind::HostSuffix, "example.com", "DIRECT")
            .expect("first rule");
        let second =
            ManualRule::parse(ManualRuleKind::DstPort, "22", "DIRECT").expect("second rule");
        let replacement = ManualRule::parse(ManualRuleKind::HostSuffix, "github.com", "DIRECT")
            .expect("replacement rule");
        let mut rules = vec![first.clone(), second.clone()];

        let previous = replace_manual_rule(&mut rules, 0, replacement.clone())
            .expect("distinct replacement should succeed");

        assert_eq!(previous, first);
        assert_eq!(rules, vec![replacement.clone(), second.clone()]);
        assert_eq!(
            replace_manual_rule(&mut rules, 0, replacement),
            Ok(rules[0].clone()),
            "saving an unchanged rule must not be treated as a duplicate"
        );
        assert_eq!(
            replace_manual_rule(&mut rules, 0, second),
            Err(ManualRuleEditError::Duplicate)
        );
        assert_eq!(
            replace_manual_rule(&mut rules, 9, first),
            Err(ManualRuleEditError::Missing)
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn legacy_direct_rules_migrate_once_into_manual_rules() -> Result<(), Box<dyn std::error::Error>>
    {
        use std::os::unix::fs::PermissionsExt;

        let root =
            std::env::temp_dir().join(format!("manis-manual-migration-{}", std::process::id()));
        if root.exists() {
            fs::remove_dir_all(&root)?;
        }
        fs::create_dir_all(&root)?;
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700))?;
        let legacy_path = root.join("direct-rules.state");
        fs::write(
            &legacy_path,
            "manis.direct-rules.v1\nport\t22\ndomain-suffix\tgithub.com",
        )?;
        fs::set_permissions(&legacy_path, fs::Permissions::from_mode(0o600))?;

        let migrated = load_manual_rules_in(&root)?;
        assert_eq!(migrated.len(), 2);
        assert_eq!(migrated[0].conditions()[0].kind(), ManualRuleKind::DstPort);
        assert_eq!(migrated[0].target(), "DIRECT");
        assert_eq!(
            migrated[1].conditions()[0].kind(),
            ManualRuleKind::HostSuffix
        );

        save_manual_rules_in(&root, &migrated[1..])?;
        let reloaded = load_manual_rules_in(&root)?;
        assert_eq!(reloaded, migrated[1..]);

        fs::remove_dir_all(root)?;
        Ok(())
    }

    impl std::fmt::Display for ManualRuleError {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(formatter, "{self:?}")
        }
    }

    impl std::error::Error for ManualRuleError {}
}
