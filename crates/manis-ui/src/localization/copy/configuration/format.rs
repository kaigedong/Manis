use crate::{
    localization::{CountNoun, Language, LanguagePreferenceError},
    manual_rule::ManualRuleStoreError,
    mihomo::{RuntimeProfileSource, SubscriptionPreviewError, SubscriptionStoreError},
    rule_source::RuleDownloadError,
    subscription::{SourceNodeDetail, SourceNodeSecurity, SubscriptionInputError},
};

pub(crate) const fn language_preference_error(
    language: Language,
    error: &LanguagePreferenceError,
) -> &'static str {
    match (language, error) {
        (Language::English, LanguagePreferenceError::Unavailable { .. }) => {
            "The language preference could not be read or saved"
        }
        (Language::English, LanguagePreferenceError::UnsafeFile) => {
            "The language preference is not stored in a safe regular file"
        }
        (Language::English, LanguagePreferenceError::InvalidValue) => {
            "The saved language preference is not recognized"
        }
        (Language::SimplifiedChinese, LanguagePreferenceError::Unavailable { .. }) => {
            "无法读取或保存语言偏好"
        }
        (Language::SimplifiedChinese, LanguagePreferenceError::UnsafeFile) => {
            "语言偏好文件不是安全的常规文件"
        }
        (Language::SimplifiedChinese, LanguagePreferenceError::InvalidValue) => {
            "保存的语言偏好值无法识别"
        }
    }
}

pub(crate) fn source_node_detail(language: Language, detail: SourceNodeDetail) -> String {
    match detail {
        SourceNodeDetail::SingleNode => match language {
            Language::English => "Single-node source".to_owned(),
            Language::SimplifiedChinese => "单节点来源".to_owned(),
        },
        SourceNodeDetail::Vless {
            security,
            transport,
        } => {
            let security = match (language, security) {
                (_, SourceNodeSecurity::Tls) => "TLS",
                (_, SourceNodeSecurity::Reality) => "REALITY",
                (Language::English, SourceNodeSecurity::Unspecified) => {
                    "Security layer not specified"
                }
                (Language::English, SourceNodeSecurity::None) => "No TLS",
                (Language::English, SourceNodeSecurity::Custom) => "Custom security layer",
                (Language::SimplifiedChinese, SourceNodeSecurity::Unspecified) => "未声明安全层",
                (Language::SimplifiedChinese, SourceNodeSecurity::None) => "无 TLS",
                (Language::SimplifiedChinese, SourceNodeSecurity::Custom) => "自定义安全层",
            };
            transport.map_or_else(
                || security.to_owned(),
                |transport| format!("{security} · {transport}"),
            )
        }
    }
}

pub(crate) const fn subscription_input_error(
    language: Language,
    error: SubscriptionInputError,
) -> &'static str {
    match (language, error) {
        (Language::English, SubscriptionInputError::Empty) => {
            "Enter a subscription URL or single-node share link"
        }
        (Language::English, SubscriptionInputError::UnsupportedSource) => {
            "Use an HTTP/HTTPS subscription or a supported single-node share link"
        }
        (Language::English, SubscriptionInputError::TooLong) => {
            "The source URL is too long; make sure the complete URL was copied"
        }
        (Language::English, SubscriptionInputError::InvalidPreset) => {
            "The subscription URL is valid, but its default profile could not be created"
        }
        (
            Language::English,
            SubscriptionInputError::InvalidVless | SubscriptionInputError::InvalidSingleNode,
        ) => "The single-node link is invalid; check its protocol, server, and parameters",
        (Language::SimplifiedChinese, SubscriptionInputError::Empty) => {
            "请输入订阅链接或单节点分享链接"
        }
        (Language::SimplifiedChinese, SubscriptionInputError::UnsupportedSource) => {
            "请输入 HTTP/HTTPS 订阅或受支持的单节点分享链接"
        }
        (Language::SimplifiedChinese, SubscriptionInputError::TooLong) => {
            "来源地址过长，请确认复制的是完整地址"
        }
        (Language::SimplifiedChinese, SubscriptionInputError::InvalidPreset) => {
            "订阅地址有效，但无法生成默认策略"
        }
        (
            Language::SimplifiedChinese,
            SubscriptionInputError::InvalidVless | SubscriptionInputError::InvalidSingleNode,
        ) => "单节点链接无效，请检查协议、服务器和参数",
    }
}

pub(crate) const fn rule_download_error(
    language: Language,
    error: RuleDownloadError,
) -> &'static str {
    match (language, error) {
        (Language::English, RuleDownloadError::InvalidHttpsUrl) => {
            "Enter a complete HTTPS rule source URL"
        }
        (Language::English, RuleDownloadError::NetworkUnavailable) => {
            "The rule source could not be downloaded; check the network and try again"
        }
        (Language::English, RuleDownloadError::RequestRejected) => {
            "The rule source rejected the request or returned an unexpected status"
        }
        (Language::English, RuleDownloadError::InsecureRedirect) => {
            "The rule source redirected to a non-HTTPS page, so the import was stopped"
        }
        (Language::English, RuleDownloadError::DocumentTooLarge) => {
            "The rule source exceeds 1 MiB and was not imported"
        }
        (Language::English, RuleDownloadError::InvalidText) => {
            "The rule source is not valid UTF-8 text"
        }
        (Language::SimplifiedChinese, RuleDownloadError::InvalidHttpsUrl) => {
            "请输入完整的 HTTPS 规则地址"
        }
        (Language::SimplifiedChinese, RuleDownloadError::NetworkUnavailable) => {
            "规则下载失败，请检查网络后重试"
        }
        (Language::SimplifiedChinese, RuleDownloadError::RequestRejected) => {
            "规则源拒绝了请求或返回了异常状态"
        }
        (Language::SimplifiedChinese, RuleDownloadError::InsecureRedirect) => {
            "规则地址跳转到了非 HTTPS 页面，已停止导入"
        }
        (Language::SimplifiedChinese, RuleDownloadError::DocumentTooLarge) => {
            "规则文件超过 1 MiB，未执行导入"
        }
        (Language::SimplifiedChinese, RuleDownloadError::InvalidText) => {
            "规则文件不是有效的 UTF-8 文本"
        }
    }
}

pub(crate) const fn subscription_preview_error(
    language: Language,
    error: SubscriptionPreviewError,
) -> &'static str {
    match (language, error) {
        (Language::English, SubscriptionPreviewError::UnsupportedPlatform) => {
            "This platform cannot start an isolated Mihomo preview process"
        }
        (Language::English, SubscriptionPreviewError::BinaryUnavailable) => {
            "The Manis-managed Mihomo core is not installed; download it in Settings and try again"
        }
        (Language::English, SubscriptionPreviewError::InvalidSource) => {
            "The subscription URL is invalid; check it and try again"
        }
        (Language::English, SubscriptionPreviewError::WorkspaceUnavailable) => {
            "A private preview workspace could not be created; check temporary-directory permissions"
        }
        (Language::English, SubscriptionPreviewError::ProfileUnavailable) => {
            "A secure subscription preview profile could not be created"
        }
        (Language::English, SubscriptionPreviewError::EngineUnavailable) => {
            "The Mihomo preview process could not start"
        }
        (Language::English, SubscriptionPreviewError::ProviderUnavailable) => {
            "Mihomo could not download or parse this subscription; check the network and subscription status"
        }
        (Language::English, SubscriptionPreviewError::EmptyProvider) => {
            "The subscription is reachable, but it contains no proxy nodes"
        }
        (Language::SimplifiedChinese, SubscriptionPreviewError::UnsupportedPlatform) => {
            "当前平台尚不能启动隔离的 Mihomo 预览进程"
        }
        (Language::SimplifiedChinese, SubscriptionPreviewError::BinaryUnavailable) => {
            "找不到 Manis 管理的 Mihomo 内核，请在设置中下载后重试"
        }
        (Language::SimplifiedChinese, SubscriptionPreviewError::InvalidSource) => {
            "订阅地址无效，请检查后重试"
        }
        (Language::SimplifiedChinese, SubscriptionPreviewError::WorkspaceUnavailable) => {
            "无法创建私有预览空间，请检查临时目录权限"
        }
        (Language::SimplifiedChinese, SubscriptionPreviewError::ProfileUnavailable) => {
            "无法生成安全的订阅预览配置"
        }
        (Language::SimplifiedChinese, SubscriptionPreviewError::EngineUnavailable) => {
            "Mihomo 预览进程启动失败"
        }
        (Language::SimplifiedChinese, SubscriptionPreviewError::ProviderUnavailable) => {
            "Mihomo 无法下载或解析这份订阅，请检查网络和订阅状态"
        }
        (Language::SimplifiedChinese, SubscriptionPreviewError::EmptyProvider) => {
            "订阅可以访问，但没有解析出任何代理节点"
        }
    }
}

pub(crate) const fn subscription_store_error(
    language: Language,
    error: SubscriptionStoreError,
) -> &'static str {
    match (language, error) {
        (Language::English, SubscriptionStoreError::DataDirectoryUnavailable) => {
            "The Manis user data directory is unavailable"
        }
        (Language::English, SubscriptionStoreError::InvalidSource) => {
            "The subscription source is invalid, so it was not imported"
        }
        (Language::English, SubscriptionStoreError::StoreUnavailable) => {
            "The subscription could not be saved safely; check user data directory permissions"
        }
        (Language::English, SubscriptionStoreError::StoredSourceUnavailable) => {
            "The saved subscription could not be read safely and must be imported again"
        }
        (Language::SimplifiedChinese, SubscriptionStoreError::DataDirectoryUnavailable) => {
            "无法确定 Manis 的用户数据目录"
        }
        (Language::SimplifiedChinese, SubscriptionStoreError::InvalidSource) => {
            "订阅地址无效，未执行导入"
        }
        (Language::SimplifiedChinese, SubscriptionStoreError::StoreUnavailable) => {
            "无法安全保存订阅，请检查用户数据目录权限"
        }
        (Language::SimplifiedChinese, SubscriptionStoreError::StoredSourceUnavailable) => {
            "已保存的订阅无法安全读取，需要重新导入"
        }
    }
}

pub(crate) const fn manual_rule_store_error(
    language: Language,
    error: ManualRuleStoreError,
) -> &'static str {
    match (language, error) {
        (Language::English, ManualRuleStoreError::Unavailable) => {
            "The manual routing rule store is unavailable"
        }
        (Language::English, ManualRuleStoreError::Corrupt) => {
            "The manual routing rule file is corrupt"
        }
        (Language::SimplifiedChinese, ManualRuleStoreError::Unavailable) => {
            "手动分流规则存储不可用"
        }
        (Language::SimplifiedChinese, ManualRuleStoreError::Corrupt) => "手动分流规则文件已损坏",
    }
}

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
