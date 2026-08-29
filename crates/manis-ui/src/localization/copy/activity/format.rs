use crate::localization::Language;

pub(crate) fn numbered_rule_source(language: Language, index: usize) -> String {
    match language {
        Language::English => format!("Rule source {index}"),
        Language::SimplifiedChinese => format!("规则源 {index}"),
    }
}

pub(crate) fn unknown_target_port(language: Language, port: &str) -> String {
    match language {
        Language::English => format!("Unknown target · port {port}"),
        Language::SimplifiedChinese => format!("未知目标 · 端口 {port}"),
    }
}
