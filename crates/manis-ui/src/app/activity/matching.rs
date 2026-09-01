use manis_mihomo::Connection;
use manis_profile::{QxRule, QxRuleKind, QxRuleList};

use crate::{
    localization::{Language, copy},
    manual_rule::ManualRule,
    mihomo::StoredQxRuleSource,
};

enum ActivityRuleGroup<'a> {
    Manual {
        label: String,
        rules: &'a [ManualRule],
    },
    Remote {
        label: String,
        rules: Vec<QxRule>,
    },
}

pub(super) struct ActivityRuleMatcher<'a> {
    groups: Vec<ActivityRuleGroup<'a>>,
}

impl<'a> ActivityRuleMatcher<'a> {
    pub(super) fn new(
        group_order: &[String],
        manual_rules: &'a [ManualRule],
        sources: &[StoredQxRuleSource],
        language: Language,
    ) -> Self {
        let groups = group_order
            .iter()
            .filter_map(|group_id| {
                if group_id == crate::mihomo::MANUAL_ROUTING_RULE_GROUP_ID {
                    return Some(ActivityRuleGroup::Manual {
                        label: language.localized(copy::common::MANUAL_RULES).to_owned(),
                        rules: manual_rules,
                    });
                }
                sources
                    .iter()
                    .enumerate()
                    .find(|(_, source)| source.enabled && source.id == *group_id)
                    .map(|(index, source)| ActivityRuleGroup::Remote {
                        label: source
                            .name
                            .clone()
                            .or_else(|| source.source.subscription_name())
                            .unwrap_or_else(|| {
                                copy::common::numbered_rule_source(language, index + 1)
                            }),
                        rules: QxRuleList::parse(&source.content).rules,
                    })
            })
            .collect();
        Self { groups }
    }

    /// Mihomo's connection chain omits the source rule group. The configured order mirrors profile
    /// compilation order, so the first matching group recovers the same first-match semantics.
    pub(super) fn matching_group(&self, connection: &Connection) -> Option<&str> {
        self.groups.iter().find_map(|group| match group {
            ActivityRuleGroup::Manual { label, rules }
                if rules
                    .iter()
                    .any(|rule| manual_rule_matches_connection(rule, connection)) =>
            {
                Some(label.as_str())
            }
            ActivityRuleGroup::Remote { label, rules }
                if qx_rules_match_connection(rules, connection) =>
            {
                Some(label.as_str())
            }
            ActivityRuleGroup::Manual { .. } | ActivityRuleGroup::Remote { .. } => None,
        })
    }
}

fn normalized_rule_kind(value: &str) -> String {
    value
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .flat_map(char::to_lowercase)
        .collect()
}

fn manual_rule_matches_connection(rule: &ManualRule, connection: &Connection) -> bool {
    if !rule.is_enabled() {
        return false;
    }
    let Some(kind) = connection.rule.as_deref().map(str::trim) else {
        return false;
    };
    if rule.is_final() {
        return kind.eq_ignore_ascii_case("Match");
    }
    let [condition] = rule.conditions() else {
        return false;
    };
    normalized_rule_kind(kind) == normalized_rule_kind(condition.kind().display_label())
        && connection.rule_payload.as_deref().map(str::trim) == Some(condition.parameter())
}

fn qx_rules_match_connection(rules: &[QxRule], connection: &Connection) -> bool {
    let Some(kind) = connection.rule.as_deref().map(normalized_rule_kind) else {
        return false;
    };
    let Some(payload) = connection.rule_payload.as_deref().map(str::trim) else {
        return false;
    };
    rules.iter().any(|rule| {
        let source_kind = match rule.kind {
            QxRuleKind::Domain => "domain",
            QxRuleKind::DomainKeyword => "domainkeyword",
            QxRuleKind::DomainSuffix => "domainsuffix",
        };
        kind == source_kind && rule.value == payload
    })
}

#[cfg(test)]
mod tests {
    use manis_mihomo::{Connection, ConnectionMetadata};
    use manis_profile::QxRuleList;

    use crate::manual_rule::{ManualRule, ManualRuleKind};

    fn connection(kind: &str, payload: &str) -> Connection {
        Connection {
            id: None,
            metadata: ConnectionMetadata::default(),
            upload: 0,
            download: 0,
            start: None,
            chains: Vec::new(),
            provider_chains: Vec::new(),
            rule: Some(kind.to_owned()),
            rule_payload: Some(payload.to_owned()),
        }
    }

    #[test]
    fn manual_matching_respects_enabled_final_and_single_condition_rules() {
        let host = connection("DomainSuffix", "example.com");
        let mut rule = ManualRule::parse(ManualRuleKind::HostSuffix, "example.com", "DIRECT")
            .expect("valid manual rule");
        assert!(super::manual_rule_matches_connection(&rule, &host));
        rule.set_enabled(false);
        assert!(!super::manual_rule_matches_connection(&rule, &host));

        let final_rule = ManualRule::final_rule("DIRECT").expect("valid final rule");
        assert!(super::manual_rule_matches_connection(
            &final_rule,
            &connection("Match", "")
        ));
        assert!(!super::manual_rule_matches_connection(&final_rule, &host));
    }

    #[test]
    fn parsed_qx_rules_match_normalized_kinds_and_exact_payloads() {
        let rules = QxRuleList::parse("DOMAIN-SUFFIX,example.com,DIRECT\n").rules;

        assert!(super::qx_rules_match_connection(
            &rules,
            &connection("DomainSuffix", "example.com")
        ));
        assert!(!super::qx_rules_match_connection(
            &rules,
            &connection("DomainSuffix", "other.example")
        ));
    }
}
