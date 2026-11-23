use manis_core::KernelKind;
use manis_profile::{Name, PolicyRef, Profile, Rule};

use super::{
    LEGACY_GENERATED_PROXY_GROUP_NAME, ManualRule, ManualRuleCompileError, ManualRuleCondition,
    ManualRuleKind,
};

impl ManualRuleCondition {
    fn to_profile_condition(&self) -> Result<manis_profile::RuleCondition, ManualRuleCompileError> {
        Ok(match self.kind {
            ManualRuleKind::Host => manis_profile::RuleCondition::Domain(self.parameter.clone()),
            ManualRuleKind::HostSuffix => {
                manis_profile::RuleCondition::DomainSuffix(self.parameter.clone())
            }
            ManualRuleKind::HostWildcard => {
                manis_profile::RuleCondition::DomainWildcard(self.parameter.clone())
            }
            ManualRuleKind::HostKeyword => {
                manis_profile::RuleCondition::DomainKeyword(self.parameter.clone())
            }
            ManualRuleKind::UserAgent => {
                return Err(ManualRuleCompileError::UnsupportedType(self.kind));
            }
            ManualRuleKind::IpCidr | ManualRuleKind::Ip6Cidr => {
                manis_profile::RuleCondition::IpCidr {
                    value: self.parameter.clone(),
                    no_resolve: true,
                }
            }
            ManualRuleKind::GeoIp => manis_profile::RuleCondition::GeoIp {
                country: self.parameter.clone(),
                no_resolve: false,
            },
            ManualRuleKind::IpAsn => manis_profile::RuleCondition::IpAsn {
                asn: self
                    .parameter
                    .parse()
                    .map_err(|_error| ManualRuleCompileError::CorruptValue)?,
                no_resolve: true,
            },
            ManualRuleKind::DstPort => manis_profile::RuleCondition::DstPort(
                self.parameter
                    .parse()
                    .map_err(|_error| ManualRuleCompileError::CorruptValue)?,
            ),
            ManualRuleKind::Final => return Err(ManualRuleCompileError::CorruptValue),
        })
    }
}

impl ManualRule {
    fn to_profile_rule(
        &self,
        legacy_proxy_target: Option<&PolicyRef>,
    ) -> Result<Rule, ManualRuleCompileError> {
        self.to_profile_rule_for(policy_ref(&self.target, legacy_proxy_target))
    }

    /// Compiles with a caller-chosen policy, so a local group decides where its rules go.
    fn to_profile_rule_for(&self, policy: PolicyRef) -> Result<Rule, ManualRuleCompileError> {
        if self.is_final() {
            return Ok(Rule::Match { policy });
        }
        let conditions = self.conditions();
        if conditions.len() > 1 {
            return Ok(Rule::All {
                conditions: conditions
                    .iter()
                    .map(ManualRuleCondition::to_profile_condition)
                    .collect::<Result<Vec<_>, _>>()?,
                policy,
            });
        }
        let condition = conditions[0].to_profile_condition()?;
        Ok(match condition {
            manis_profile::RuleCondition::Domain(value) => Rule::Domain { value, policy },
            manis_profile::RuleCondition::DomainKeyword(value) => {
                Rule::DomainKeyword { value, policy }
            }
            manis_profile::RuleCondition::DomainSuffix(value) => {
                Rule::DomainSuffix { value, policy }
            }
            manis_profile::RuleCondition::DomainWildcard(value) => {
                Rule::DomainWildcard { value, policy }
            }
            manis_profile::RuleCondition::IpCidr { value, no_resolve } => Rule::IpCidr {
                value,
                policy,
                no_resolve,
            },
            manis_profile::RuleCondition::IpAsn { asn, no_resolve } => Rule::IpAsn {
                asn,
                policy,
                no_resolve,
            },
            manis_profile::RuleCondition::GeoIp {
                country,
                no_resolve,
            } => Rule::GeoIp {
                country,
                policy,
                no_resolve,
            },
            manis_profile::RuleCondition::DstPort(port) => Rule::DstPort { port, policy },
        })
    }
}

fn policy_ref(target: &Name, legacy_proxy_target: Option<&PolicyRef>) -> PolicyRef {
    match target.as_str() {
        "DIRECT" => PolicyRef::Direct,
        "REJECT" => PolicyRef::Reject,
        LEGACY_GENERATED_PROXY_GROUP_NAME => legacy_proxy_target
            .cloned()
            .unwrap_or_else(|| PolicyRef::Group(target.clone())),
        _ => PolicyRef::Group(target.clone()),
    }
}

fn legacy_proxy_target(profile: &Profile) -> Option<PolicyRef> {
    let has_user_named_proxy = profile
        .groups
        .iter()
        .any(|group| group.name.as_str() == LEGACY_GENERATED_PROXY_GROUP_NAME);
    (!has_user_named_proxy)
        .then(|| {
            profile
                .groups
                .iter()
                .find(|group| group.name.as_str() != manis_profile::MANIS_GLOBAL_GROUP_NAME)
                .or_else(|| profile.groups.first())
                .map(|group| PolicyRef::Group(group.name.clone()))
        })
        .flatten()
}

fn reject_unsupported_types(
    rules: &[ManualRule],
    kernel: KernelKind,
) -> Result<(), ManualRuleCompileError> {
    for rule in rules.iter().filter(|rule| rule.enabled && !rule.is_final()) {
        if let Some(condition) = rule
            .conditions()
            .iter()
            .find(|condition| !condition.kind.supported_by(kernel))
        {
            return Err(ManualRuleCompileError::UnsupportedType(condition.kind));
        }
    }
    Ok(())
}

fn insert_before_terminal(profile: &mut Profile, compiled: Vec<Rule>) {
    let insert_at = profile
        .rules
        .iter()
        .position(|rule| matches!(rule, Rule::Match { .. }))
        .unwrap_or(profile.rules.len());
    profile.rules.splice(insert_at..insert_at, compiled);
}

pub(crate) fn append_manual_rules(
    profile: &mut Profile,
    rules: &[ManualRule],
    kernel: KernelKind,
) -> Result<(), ManualRuleCompileError> {
    reject_unsupported_types(rules, kernel)?;
    let legacy_proxy_target = legacy_proxy_target(profile);
    let compiled = rules
        .iter()
        .filter(|rule| rule.enabled && !rule.is_final())
        .map(|rule| rule.to_profile_rule(legacy_proxy_target.as_ref()))
        .collect::<Result<Vec<_>, _>>()?;

    let mut final_rules = rules.iter().filter(|rule| rule.enabled && rule.is_final());
    let terminal = final_rules.next();
    if final_rules.next().is_some() {
        return Err(ManualRuleCompileError::MultipleFinalRules);
    }
    let terminal = terminal
        .map(|terminal| terminal.to_profile_rule(legacy_proxy_target.as_ref()))
        .transpose()?;

    insert_before_terminal(profile, compiled);

    if let Some(terminal) = terminal {
        profile
            .rules
            .retain(|rule| !matches!(rule, Rule::Match { .. }));
        profile.rules.push(terminal);
    }
    Ok(())
}

/// Appends one local group's rules with the group's target policy pinned for every rule.
///
/// FINAL is manual-only, so a stored FINAL rule inside a local group is skipped instead of
/// compiled. Returns how many rules were skipped that way.
pub(crate) fn append_local_routing_group_rules(
    profile: &mut Profile,
    rules: &[ManualRule],
    target_policy: &Name,
    kernel: KernelKind,
) -> Result<usize, ManualRuleCompileError> {
    reject_unsupported_types(rules, kernel)?;
    let policy = policy_ref(target_policy, legacy_proxy_target(profile).as_ref());
    let mut skipped_final = 0;
    let compiled = rules
        .iter()
        .filter(|rule| rule.enabled)
        .filter_map(|rule| {
            if rule.is_final() {
                skipped_final += 1;
                return None;
            }
            Some(rule.to_profile_rule_for(policy.clone()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    insert_before_terminal(profile, compiled);
    Ok(skipped_final)
}
