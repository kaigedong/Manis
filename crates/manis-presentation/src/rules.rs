//! Rules page shared by Android and the desktop shells.

use serde::{Deserialize, Serialize};

/// Rule kinds the editor offers.
pub const RULE_KINDS: &[&str] = &[
    "DOMAIN-SUFFIX",
    "DOMAIN",
    "DOMAIN-KEYWORD",
    "DOMAIN-WILDCARD",
    "IP-CIDR",
    "IP6-CIDR",
    "GEOIP",
    "IP-ASN",
    "DST-PORT",
    "FINAL",
];

/// One editable or runtime rule before display formatting.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleRowInput {
    pub kind: String,
    pub value: String,
    pub policy: String,
    pub enabled: bool,
    #[serde(default)]
    pub source: Option<String>,
}

/// Raw pieces for the rules page.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RulesPageInput {
    pub rules: Vec<RuleRowInput>,
    #[serde(default)]
    pub policies: Vec<String>,
    #[serde(default)]
    pub warnings: Vec<String>,
    pub summary: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleRowView {
    pub index: usize,
    pub kind: String,
    pub value: String,
    pub policy: String,
    pub policy_label: String,
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RulesPageView {
    pub rules: Vec<RuleRowView>,
    pub kinds: Vec<String>,
    pub policies: Vec<String>,
    pub warnings: Vec<String>,
    pub summary: String,
}

impl RulesPageView {
    /// Formats rules, policies and warnings for the page.
    #[must_use]
    pub fn build(input: RulesPageInput) -> Self {
        let rules = input
            .rules
            .into_iter()
            .enumerate()
            .map(|(index, row)| RuleRowView {
                index,
                policy_label: policy_label(&row.policy),
                kind: row.kind,
                value: row.value,
                policy: row.policy,
                enabled: row.enabled,
                source: row.source,
            })
            .collect();
        let mut policies = vec![
            "DIRECT".to_owned(),
            "REJECT".to_owned(),
            "__MANIS_GLOBAL__".to_owned(),
        ];
        for policy in input.policies {
            if !policies.contains(&policy) {
                policies.push(policy);
            }
        }
        Self {
            rules,
            kinds: RULE_KINDS.iter().map(|kind| (*kind).to_owned()).collect(),
            policies,
            warnings: input.warnings.into_iter().take(8).collect(),
            summary: input.summary,
        }
    }

    /// Parses a JSON request and formats the page.
    ///
    /// # Errors
    /// Returns a JSON error when the input does not match the rules shape.
    pub fn from_json(request: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str::<RulesPageInput>(request).map(Self::build)
    }
}

fn policy_label(policy: &str) -> String {
    if policy == "__MANIS_GLOBAL__" {
        "全局".to_owned()
    } else {
        policy.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_policies_and_limits_warnings() {
        let mut warnings = Vec::new();
        for index in 0..10 {
            warnings.push(format!("w{index}"));
        }
        let view = RulesPageView::from_json(&format!(
            r#"{{
                "rules": [{{"kind": "DOMAIN", "value": "example.test", "policy": "__MANIS_GLOBAL__", "enabled": true}}],
                "policies": ["HK", "DIRECT"],
                "warnings": {warnings:?},
                "summary": "1 条"
            }}"#
        ))
        .expect("rules page");
        assert_eq!(view.rules[0].policy_label, "全局");
        assert_eq!(
            view.policies,
            ["DIRECT", "REJECT", "__MANIS_GLOBAL__", "HK"]
        );
        assert_eq!(view.warnings.len(), 8);
        assert_eq!(view.kinds.len(), RULE_KINDS.len());
    }
}
