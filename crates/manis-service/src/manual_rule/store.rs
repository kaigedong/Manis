use super::{
    MANUAL_RULES_FILE, MANUAL_RULES_VERSION_V1, MANUAL_RULES_VERSION_V2, MANUAL_RULES_VERSION_V3,
    MANUAL_RULES_VERSION_V4, MAX_MANUAL_RULES_FILE_BYTES,
};
use super::{ManualRule, ManualRuleKind, ManualRuleStoreError};
use std::path::Path;

fn encode_manual_rules(rules: &[ManualRule]) -> Result<String, ManualRuleStoreError> {
    if rules.iter().filter(|rule| rule.is_final()).count() > 1 {
        return Err(ManualRuleStoreError::Corrupt);
    }
    let mut contents = String::from(MANUAL_RULES_VERSION_V4);
    contents.push_str("\nlegacy-direct-rules-migrated\t1");
    for rule in rules.iter().filter(|rule| !rule.is_final()) {
        contents.push_str("\nrule\t");
        contents.push_str(if rule.enabled { "1" } else { "0" });
        contents.push('\t');
        contents.push_str(rule.target.as_str());
        for condition in rule.conditions() {
            contents.push('\t');
            contents.push_str(condition.kind.storage_key());
            contents.push('\t');
            contents.push_str(&condition.parameter);
        }
    }
    if let Some(rule) = rules.iter().find(|rule| rule.is_final()) {
        contents.push_str("\nfinal\t");
        contents.push_str(if rule.enabled { "1" } else { "0" });
        contents.push('\t');
        contents.push_str(rule.target.as_str());
    }
    Ok(contents)
}

fn normalize_loaded_rule_order(
    mut rules: Vec<ManualRule>,
) -> Result<Vec<ManualRule>, ManualRuleStoreError> {
    if rules.iter().filter(|rule| rule.is_final()).count() > 1 {
        return Err(ManualRuleStoreError::Corrupt);
    }
    if let Some(index) = rules.iter().position(ManualRule::is_final) {
        let final_rule = rules.remove(index);
        rules.push(final_rule);
    }
    Ok(rules)
}

fn decode_v4_manual_rules<'a>(
    mut lines: impl Iterator<Item = &'a str>,
) -> Result<Vec<ManualRule>, ManualRuleStoreError> {
    if lines.next() != Some("legacy-direct-rules-migrated\t1") {
        return Err(ManualRuleStoreError::Corrupt);
    }
    let rules = lines
        .filter(|line| !line.is_empty())
        .map(|line| {
            let fields = line.split('\t').collect::<Vec<_>>();
            match fields.as_slice() {
                ["final", enabled @ ("0" | "1"), target] => {
                    let mut rule = ManualRule::final_rule(target)
                        .map_err(|_error| ManualRuleStoreError::Corrupt)?;
                    rule.enabled = *enabled == "1";
                    Ok(rule)
                }
                _ if fields.first() == Some(&"rule")
                    && matches!(fields.get(1), Some(&"0" | &"1"))
                    && fields.len() >= 5
                    && (fields.len() - 3).is_multiple_of(2) =>
                {
                    let enabled = fields[1] == "1";
                    let target = fields[2];
                    let conditions = fields[3..]
                        .as_chunks::<2>()
                        .0
                        .iter()
                        .map(|pair| {
                            ManualRuleKind::from_storage_key(pair[0])
                                .filter(|kind| *kind != ManualRuleKind::Final)
                                .map(|kind| (kind, pair[1].to_owned()))
                                .ok_or(ManualRuleStoreError::Corrupt)
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    let mut rule = ManualRule::parse_conditions(conditions, target)
                        .map_err(|_error| ManualRuleStoreError::Corrupt)?;
                    rule.enabled = enabled;
                    Ok(rule)
                }
                _ => Err(ManualRuleStoreError::Corrupt),
            }
        })
        .collect::<Result<Vec<_>, _>>()?;
    normalize_loaded_rule_order(rules)
}

fn decode_v3_manual_rules<'a>(
    mut lines: impl Iterator<Item = &'a str>,
) -> Result<Vec<ManualRule>, ManualRuleStoreError> {
    if lines.next() != Some("legacy-direct-rules-migrated\t1") {
        return Err(ManualRuleStoreError::Corrupt);
    }
    lines
        .filter(|line| !line.is_empty())
        .map(|line| {
            let fields = line.split('\t').collect::<Vec<_>>();
            if fields.first() != Some(&"rule")
                || !matches!(fields.get(1), Some(&"0" | &"1"))
                || fields.len() < 5
                || !(fields.len() - 3).is_multiple_of(2)
            {
                return Err(ManualRuleStoreError::Corrupt);
            }
            let enabled = fields[1] == "1";
            let target = fields[2];
            let conditions = fields[3..]
                .as_chunks::<2>()
                .0
                .iter()
                .map(|pair| {
                    ManualRuleKind::from_storage_key(pair[0])
                        .filter(|kind| *kind != ManualRuleKind::Final)
                        .map(|kind| (kind, pair[1].to_owned()))
                        .ok_or(ManualRuleStoreError::Corrupt)
                })
                .collect::<Result<Vec<_>, _>>()?;
            let mut rule = ManualRule::parse_conditions(conditions, target)
                .map_err(|_error| ManualRuleStoreError::Corrupt)?;
            rule.enabled = enabled;
            Ok(rule)
        })
        .collect()
}

fn decode_v1_manual_rules<'a>(
    lines: impl Iterator<Item = &'a str>,
) -> Result<Vec<ManualRule>, ManualRuleStoreError> {
    lines
        .filter(|line| !line.is_empty())
        .map(|line| {
            let mut fields = line.split('\t');
            let kind = fields
                .next()
                .and_then(ManualRuleKind::from_storage_key)
                .filter(|kind| *kind != ManualRuleKind::Final)
                .ok_or(ManualRuleStoreError::Corrupt)?;
            let parameter = fields.next().ok_or(ManualRuleStoreError::Corrupt)?;
            let target = fields.next().ok_or(ManualRuleStoreError::Corrupt)?;
            if fields.next().is_some() {
                return Err(ManualRuleStoreError::Corrupt);
            }
            ManualRule::parse(kind, parameter, target)
                .map_err(|_error| ManualRuleStoreError::Corrupt)
        })
        .collect()
}

fn decode_v2_manual_rules<'a>(
    mut lines: impl Iterator<Item = &'a str>,
) -> Result<Vec<ManualRule>, ManualRuleStoreError> {
    if lines.next() != Some("legacy-direct-rules-migrated\t1") {
        return Err(ManualRuleStoreError::Corrupt);
    }
    lines
        .filter(|line| !line.is_empty())
        .map(|line| {
            let fields = line.split('\t').collect::<Vec<_>>();
            if fields.first() != Some(&"rule")
                || fields.len() < 4
                || !(fields.len() - 2).is_multiple_of(2)
            {
                return Err(ManualRuleStoreError::Corrupt);
            }
            let target = fields[1];
            let conditions = fields[2..]
                .as_chunks::<2>()
                .0
                .iter()
                .map(|pair| {
                    ManualRuleKind::from_storage_key(pair[0])
                        .filter(|kind| *kind != ManualRuleKind::Final)
                        .map(|kind| (kind, pair[1].to_owned()))
                        .ok_or(ManualRuleStoreError::Corrupt)
                })
                .collect::<Result<Vec<_>, _>>()?;
            ManualRule::parse_conditions(conditions, target)
                .map_err(|_error| ManualRuleStoreError::Corrupt)
        })
        .collect()
}

pub(crate) fn decode_manual_rules(
    contents: &str,
) -> Result<(Vec<ManualRule>, bool), ManualRuleStoreError> {
    let mut lines = contents.lines();
    match lines.next() {
        Some(MANUAL_RULES_VERSION_V1) => decode_v1_manual_rules(lines).map(|rules| (rules, false)),
        Some(MANUAL_RULES_VERSION_V2) => decode_v2_manual_rules(lines).map(|rules| (rules, true)),
        Some(MANUAL_RULES_VERSION_V3) => decode_v3_manual_rules(lines).map(|rules| (rules, true)),
        Some(MANUAL_RULES_VERSION_V4) => decode_v4_manual_rules(lines).map(|rules| (rules, true)),
        _ => Err(ManualRuleStoreError::Corrupt),
    }
}

fn convert_legacy_direct_rule(
    rule: crate::direct_rule::DirectRule,
) -> Result<ManualRule, ManualRuleStoreError> {
    let (kind, parameter) = match rule {
        crate::direct_rule::DirectRule::Port(port) => (ManualRuleKind::DstPort, port.to_string()),
        crate::direct_rule::DirectRule::DomainSuffix(domain) => {
            (ManualRuleKind::HostSuffix, domain)
        }
    };
    ManualRule::parse(kind, &parameter, "DIRECT").map_err(|_error| ManualRuleStoreError::Corrupt)
}

fn merge_legacy_direct_rules(
    rules: Vec<ManualRule>,
    legacy: Vec<crate::direct_rule::DirectRule>,
) -> Result<Vec<ManualRule>, ManualRuleStoreError> {
    let mut merged = legacy
        .into_iter()
        .map(convert_legacy_direct_rule)
        .collect::<Result<Vec<_>, _>>()?;
    for rule in rules {
        if !merged.contains(&rule) {
            merged.push(rule);
        }
    }
    Ok(merged)
}

fn read_manual_rules_document(
    directory: &Path,
) -> Result<(Vec<ManualRule>, bool), ManualRuleStoreError> {
    let Some(contents) =
        crate::config_toml::read_entry(directory, MANUAL_RULES_FILE, MAX_MANUAL_RULES_FILE_BYTES)
            .map_err(|error| match error {
            crate::config_toml::ConfigTomlError::Unavailable => ManualRuleStoreError::Unavailable,
            crate::config_toml::ConfigTomlError::UnsafePath
            | crate::config_toml::ConfigTomlError::InvalidFormat
            | crate::config_toml::ConfigTomlError::Oversized => ManualRuleStoreError::Corrupt,
        })?
    else {
        return Ok((Vec::new(), false));
    };
    decode_manual_rules(&contents)
}

fn map_legacy_store_error(error: crate::direct_rule::DirectRuleStoreError) -> ManualRuleStoreError {
    match error {
        crate::direct_rule::DirectRuleStoreError::Unavailable => ManualRuleStoreError::Unavailable,
        crate::direct_rule::DirectRuleStoreError::Corrupt => ManualRuleStoreError::Corrupt,
    }
}

fn migrate_legacy_direct_rules_in(
    directory: &Path,
    rules: Vec<ManualRule>,
) -> Result<Vec<ManualRule>, ManualRuleStoreError> {
    let legacy =
        crate::direct_rule::load_direct_rules_in(directory).map_err(map_legacy_store_error)?;
    let merged = merge_legacy_direct_rules(rules, legacy)?;
    save_manual_rules_in(directory, &merged)?;
    Ok(merged)
}

pub(crate) fn save_manual_rules_in(
    directory: &Path,
    rules: &[ManualRule],
) -> Result<(), ManualRuleStoreError> {
    let contents = encode_manual_rules(rules)?;
    crate::config_toml::write_entry(directory, MANUAL_RULES_FILE, &contents)
        .map(|_path| ())
        .map_err(|_error| ManualRuleStoreError::Unavailable)
}

pub(crate) fn load_manual_rules_in(
    directory: &Path,
) -> Result<Vec<ManualRule>, ManualRuleStoreError> {
    let (rules, legacy_migrated) = read_manual_rules_document(directory)?;
    if legacy_migrated {
        return Ok(rules);
    }
    migrate_legacy_direct_rules_in(directory, rules)
}
