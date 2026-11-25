use crate::localization::Language;

/// The privacy notice beside the log toolbar. The record count lives in the command bar.
pub(crate) fn summary(language: Language) -> String {
    match language {
        Language::English => "Sensitive data hidden".to_owned(),
        Language::SimplifiedChinese => "敏感信息已隐藏".to_owned(),
    }
}
