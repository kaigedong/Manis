use crate::localization::{Language, LocalizedText};

pub(crate) const COLLAPSE: LocalizedText = LocalizedText::new("Collapse", "收起");
pub(crate) const DIRECT: LocalizedText = LocalizedText::new("Direct", "直连");
pub(crate) const EDIT_POLICY_GROUP: LocalizedText =
    LocalizedText::new("Edit policy group", "编辑策略组");
pub(crate) const EXPAND: LocalizedText = LocalizedText::new("Expand", "展开");
pub(crate) const FOR_EXAMPLE_HONG_KONG: LocalizedText =
    LocalizedText::new("For example: Hong Kong", "例如：Hong Kong");
pub(crate) const FOR_EXAMPLE_HONG_KONG_AUTO: LocalizedText =
    LocalizedText::new("For example: Hong Kong Auto", "例如：香港自动优选");
pub(crate) const FOR_EXAMPLE_MY_SUBSCRIPTION: LocalizedText =
    LocalizedText::new("For example: My subscription", "例如：我的订阅");
pub(crate) const HTTPS_SUBSCRIPTION: LocalizedText =
    LocalizedText::new("HTTPS subscription", "HTTPS 订阅");
pub(crate) const HTTP_SUBSCRIPTION: LocalizedText =
    LocalizedText::new("HTTP subscription", "HTTP 订阅");
pub(crate) const LATENCY: LocalizedText = LocalizedText::new("Latency", "延迟");
pub(crate) const MANUAL_RULES: LocalizedText = LocalizedText::new("Manual rules", "手动规则");
pub(crate) const MIHOMO_DID_NOT_RETURN_A_RESULT: LocalizedText =
    LocalizedText::new("Mihomo did not return a result", "Mihomo 未返回结果");
pub(crate) const OFF: LocalizedText = LocalizedText::new("Off", "关闭代理");
pub(crate) const SAVED: LocalizedText = LocalizedText::new("Saved", "已保存");
pub(crate) const SYSTEM_PROXY: LocalizedText = LocalizedText::new("System proxy", "系统代理");
pub(crate) const TUN_PROXY: LocalizedText = LocalizedText::new("TUN proxy", "TUN 代理");

pub(crate) fn numbered_rule_source(language: Language, index: usize) -> String {
    match language {
        Language::English => format!("Rule source {index}"),
        Language::SimplifiedChinese => format!("规则源 {index}"),
    }
}
