use manis_mihomo::Connection;
use manis_profile::{QxRule, QxRuleKind, QxRuleList};

use crate::{
    localization::{Language, copy},
    manual_rule::ManualRule,
    mihomo::{LocalRoutingRuleGroup, StoredQxRuleSource},
};

enum ActivityRuleGroup<'a> {
    Manual {
        label: String,
        rules: &'a [ManualRule],
    },
    Local {
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
    fallback_label: &'static str,
}

impl<'a> ActivityRuleMatcher<'a> {
    pub(super) fn new(
        group_order: &[String],
        manual_rules: &'a [ManualRule],
        local_groups: &'a [LocalRoutingRuleGroup],
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
                if let Some(group) = local_groups
                    .iter()
                    .find(|group| group.enabled && group.id == *group_id)
                {
                    return Some(ActivityRuleGroup::Local {
                        label: group.name.clone(),
                        rules: &group.rules,
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
        let fallback_label = match language {
            Language::English => "Default fallback",
            Language::SimplifiedChinese => "默认兜底",
        };
        Self {
            groups,
            fallback_label,
        }
    }

    /// Mihomo's connection chain omits the source rule group. The configured order mirrors profile
    /// compilation order, so the first matching group recovers the same first-match semantics.
    pub(super) fn matching_group(&self, connection: &Connection) -> Option<&str> {
        if let Some(matched) = self.groups.iter().find_map(|group| match group {
            ActivityRuleGroup::Manual { label, rules }
            | ActivityRuleGroup::Local { label, rules }
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
            ActivityRuleGroup::Manual { .. }
            | ActivityRuleGroup::Local { .. }
            | ActivityRuleGroup::Remote { .. } => None,
        }) {
            return Some(matched);
        }
        connection
            .rule
            .as_deref()
            .map(str::trim)
            .is_some_and(|kind| kind.eq_ignore_ascii_case("Match"))
            .then_some(self.fallback_label)
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
    if !rule.is_enabled() || rule.is_final() {
        return false;
    }
    let Some(kind) = connection.rule.as_deref().map(str::trim) else {
        return false;
    };
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
    fn manual_matching_excludes_final_and_attributes_match_to_default_fallback() {
        let host = connection("DomainSuffix", "example.com");
        let mut rule = ManualRule::parse(ManualRuleKind::HostSuffix, "example.com", "DIRECT")
            .expect("valid manual rule");
        assert!(super::manual_rule_matches_connection(&rule, &host));
        rule.set_enabled(false);
        assert!(!super::manual_rule_matches_connection(&rule, &host));

        let final_rule = ManualRule::final_rule("DIRECT").expect("valid final rule");
        assert!(
            !super::manual_rule_matches_connection(&final_rule, &connection("Match", "")),
            "FINAL must never be attributed to the Manual rules group"
        );
        assert!(!super::manual_rule_matches_connection(&final_rule, &host));

        let manual_rules = vec![
            ManualRule::parse(ManualRuleKind::HostSuffix, "example.com", "DIRECT")
                .expect("valid manual rule"),
            final_rule,
        ];
        let order = vec![crate::mihomo::MANUAL_ROUTING_RULE_GROUP_ID.to_owned()];
        let matcher_zh = super::ActivityRuleMatcher::new(
            &order,
            &manual_rules,
            &[],
            &[],
            crate::localization::Language::SimplifiedChinese,
        );
        assert_eq!(
            matcher_zh.matching_group(&connection("Match", "")),
            Some("默认兜底")
        );
        assert_eq!(matcher_zh.matching_group(&host), Some("手动规则"));
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
