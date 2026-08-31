use crate::localization::{CountNoun, Language};

pub(crate) fn summary(language: Language, count: usize, dropped: u64) -> String {
    let count = language.count(CountNoun::Log, count);
    match language {
        Language::English => {
            let dropped = if dropped == 1 {
                "1 log".to_owned()
            } else {
                format!("{dropped} logs")
            };
            format!("{count} · {dropped} dropped under load · sensitive data hidden")
        }
        Language::SimplifiedChinese => {
            format!("{count} · 高负载时丢弃 {dropped} 条日志 · 敏感信息已隐藏")
        }
    }
}
