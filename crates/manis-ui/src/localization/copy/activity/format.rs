use crate::localization::Language;

pub(crate) fn unknown_target_port(language: Language, port: &str) -> String {
    match language {
        Language::English => format!("Unknown target · port {port}"),
        Language::SimplifiedChinese => format!("未知目标 · 端口 {port}"),
    }
}
