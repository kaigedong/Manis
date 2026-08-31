use crate::localization::{CountNoun, Language};

pub(crate) fn summary(language: Language, count: usize) -> String {
    let count = language.count(CountNoun::Log, count);
    match language {
        Language::English => format!("{count} · sensitive data hidden"),
        Language::SimplifiedChinese => format!("{count} · 敏感信息已隐藏"),
    }
}
