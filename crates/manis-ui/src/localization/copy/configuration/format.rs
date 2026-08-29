use crate::{
    localization::{CountNoun, Language},
    mihomo::RuntimeProfileSource,
};

pub(crate) fn updated_minutes_ago(language: Language, minutes: u64) -> String {
    match language {
        Language::English => format!("Updated {minutes} min ago"),
        Language::SimplifiedChinese => format!("{minutes} 分钟前更新"),
    }
}

pub(crate) fn updated_hours_ago(language: Language, hours: u64) -> String {
    match language {
        Language::English => format!("Updated {hours} hr ago"),
        Language::SimplifiedChinese => format!("{hours} 小时前更新"),
    }
}

pub(crate) fn updated_days_ago(language: Language, days: u64) -> String {
    match language {
        Language::English => format!("Updated {days} d ago"),
        Language::SimplifiedChinese => format!("{days} 天前更新"),
    }
}

pub(crate) fn profile_source_detail(
    language: Language,
    source: RuntimeProfileSource,
) -> &'static str {
    match (language, source) {
        #[cfg(any(test, feature = "snapshot-fixtures"))]
        (Language::English, RuntimeProfileSource::FixtureController) => "Test snapshot only",
        (Language::English, RuntimeProfileSource::SavedSources) => {
            "Compiled from private local sources"
        }
        (Language::English, RuntimeProfileSource::Invalid) => "Check local startup arguments",
        #[cfg(any(test, feature = "snapshot-fixtures"))]
        (Language::SimplifiedChinese, RuntimeProfileSource::FixtureController) => "仅用于测试快照",
        (Language::SimplifiedChinese, RuntimeProfileSource::SavedSources) => "从本机私有来源编译",
        (Language::SimplifiedChinese, RuntimeProfileSource::Invalid) => "请检查来源设置",
    }
}

pub(crate) fn managed_core_version(language: Language, version: &str) -> String {
    match language {
        Language::English => format!("Manis-managed stable core · {version}"),
        Language::SimplifiedChinese => format!("Manis 托管稳定版内核 · {version}"),
    }
}

pub(crate) fn single_node_saved(language: Language, suffix: &str) -> String {
    match language {
        Language::English => format!("Single-node source saved · Added to Saved group{suffix}"),
        Language::SimplifiedChinese => format!("单节点来源已保存 · 已加入“已保存”分组{suffix}"),
    }
}

pub(crate) fn source_nodes(language: Language, kind: &str, count: usize) -> String {
    match language {
        Language::English => format!("{kind} · {count} nodes"),
        Language::SimplifiedChinese => format!("{kind} · {count} 个节点"),
    }
}

pub(crate) fn numbered_rule_source(language: Language, index: usize) -> String {
    match language {
        Language::English => format!("Rule source {index}"),
        Language::SimplifiedChinese => format!("规则源 {index}"),
    }
}

pub(crate) fn rule_source_counts(language: Language, rules: usize, skipped: usize) -> String {
    match language {
        Language::English => format!("{rules} rules · {skipped} skipped"),
        Language::SimplifiedChinese => format!("{rules} 条 · 跳过 {skipped} 条"),
    }
}

pub(crate) fn condition_title(language: Language, index: usize) -> String {
    match language {
        Language::English => format!("AND · Condition {index}"),
        Language::SimplifiedChinese => format!("并且 · 条件 {index}"),
    }
}

pub(crate) fn manual_rule_accessibility(language: Language, order: usize) -> String {
    match language {
        Language::English => {
            format!("Manual rule {order}. Enter edits, Space toggles, Delete removes the rule")
        }
        Language::SimplifiedChinese => {
            format!("第 {order} 条手动规则。回车编辑，空格启用或禁用，Delete 删除")
        }
    }
}

pub(crate) fn move_rule_group(language: Language, group: &str, up: bool) -> String {
    match (language, up) {
        (Language::English, true) => format!("Move {group} up"),
        (Language::English, false) => format!("Move {group} down"),
        (Language::SimplifiedChinese, true) => format!("上移{group}"),
        (Language::SimplifiedChinese, false) => format!("下移{group}"),
    }
}

pub(crate) fn active_rule_summary(language: Language, active: usize, disabled: usize) -> String {
    if disabled == 0 {
        return language.count(CountNoun::Rule, active);
    }
    match language {
        Language::English => format!("{active} active · {disabled} disabled"),
        Language::SimplifiedChinese => format!("{active} 条生效 · {disabled} 条已禁用"),
    }
}

pub(crate) fn manual_group_detail(language: Language, total: usize, disabled: usize) -> String {
    let count = language.count(CountNoun::Rule, total);
    match (language, disabled) {
        (Language::English, 0) => format!("{count} · Saved locally"),
        (Language::English, _) => format!("{count} · {disabled} disabled · Saved locally"),
        (Language::SimplifiedChinese, 0) => format!("{count} · 本地保存"),
        (Language::SimplifiedChinese, _) => format!("{count} · {disabled} 条已禁用 · 本地保存"),
    }
}

pub(crate) fn remote_group_detail(
    language: Language,
    rules: usize,
    enabled: bool,
    target: &str,
    update: &str,
) -> String {
    match (language, enabled) {
        (Language::English, false) => format!("{rules} rules · Disabled"),
        (Language::SimplifiedChinese, false) => format!("{rules} 条规则 · 已停用"),
        (Language::English, true) => format!("{rules} rules · Target {target} · {update}"),
        (Language::SimplifiedChinese, true) => {
            format!("{rules} 条规则 · 目标 {target} · {update}")
        }
    }
}

pub(crate) fn imported_rules(language: Language, rules: usize, skipped: usize) -> String {
    match (language, skipped) {
        (Language::English, 0) => format!("Imported {rules} rules"),
        (Language::English, _) => {
            format!("Imported {rules} rules · Skipped {skipped} invalid lines")
        }
        (Language::SimplifiedChinese, 0) => format!("已导入 {rules} 条规则"),
        (Language::SimplifiedChinese, _) => {
            format!("已导入 {rules} 条规则 · 跳过 {skipped} 条无效行")
        }
    }
}

pub(crate) fn duplicate_rule_source(language: Language, rules: usize, target: &str) -> String {
    match language {
        Language::English => format!(
            "This rule source already exists · {rules} rules · Target {target}. Manage or update the highlighted source below."
        ),
        Language::SimplifiedChinese => format!(
            "该规则源已存在 · {rules} 条规则 · 目标 {target}。请在下方管理或更新已标出的规则源。"
        ),
    }
}

pub(crate) fn qx_rules_applied(
    language: Language,
    action: QxRuleAction,
    rules: usize,
    suffix: &str,
) -> String {
    match (language, action) {
        (Language::English, QxRuleAction::Imported) => {
            format!("QX rules imported · {rules} active rules{suffix}")
        }
        (Language::English, QxRuleAction::Updated) => {
            format!("QX rules updated · {rules} active rules{suffix}")
        }
        (Language::SimplifiedChinese, QxRuleAction::Imported) => {
            format!("QX 规则已导入 · {rules} 条生效{suffix}")
        }
        (Language::SimplifiedChinese, QxRuleAction::Updated) => {
            format!("QX 规则更新完成 · {rules} 条生效{suffix}")
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) enum QxRuleAction {
    Imported,
    Updated,
}

pub(crate) fn qx_rules_removed(language: Language, suffix: &str) -> String {
    match language {
        Language::English => format!("Remote QX rules removed{suffix}"),
        Language::SimplifiedChinese => format!("远程 QX 规则已移除{suffix}"),
    }
}
