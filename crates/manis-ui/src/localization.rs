use std::fs;
use std::path::{Path, PathBuf};
#[cfg(any(target_os = "macos", target_os = "windows"))]
use std::process::Command;

use manis_profile::write_private_atomic;

use crate::diagnostics::{LogLevel, record_event};

pub(crate) mod copy;

const LANGUAGE_PREFERENCE_FILE: &str = "language.preference";
const MAX_LANGUAGE_PREFERENCE_BYTES: u64 = 32;

/// Stable product vocabulary shared by navigation, page headings, and actions.
///
/// Protocol names and user-provided values intentionally stay outside this
/// enum; they are rendered verbatim inside otherwise localized messages.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Message {
    Nodes,
    PolicyGroup,
    PolicyGroups,
    RoutingRules,
    NetworkActivity,
    Logs,
    Configuration,
    Settings,
    AddPolicyGroup,
    RefreshData,
    AddRule,
    ImportSubscription,
    SaveChanges,
    Cancel,
    Delete,
    Enable,
    Disable,
    Retry,
    ConnectMihomo,
    NoPolicyGroups,
    NoNodes,
    NoActiveConnections,
    NoLogs,
    NoFilterMatches,
    ChangesApplied,
    ChangesAppliedAndRestarted,
    ChangesFailedAndRestored,
    SavedChangesFailed,
    TunRestoreFailed,
    ApplyingChanges,
    RoutingApplyBusy,
    ManualRulesLocationUnavailable,
    ManualRulesSaveFailed,
    StoreRollbackFailed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CountNoun {
    Node,
    PolicyGroup,
    Rule,
    Source,
    Connection,
    Log,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum LanguagePreference {
    #[default]
    FollowSystem,
    English,
    SimplifiedChinese,
}

impl LanguagePreference {
    #[must_use]
    pub(crate) const fn persistence_key(self) -> &'static str {
        match self {
            Self::FollowSystem => "system",
            Self::English => "en",
            Self::SimplifiedChinese => "zh-CN",
        }
    }

    #[must_use]
    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "system" => Some(Self::FollowSystem),
            "en" => Some(Self::English),
            "zh-CN" => Some(Self::SimplifiedChinese),
            _ => None,
        }
    }

    #[must_use]
    pub(crate) fn resolve(self, system_locale: Option<&str>) -> Language {
        match self {
            Self::English => Language::English,
            Self::SimplifiedChinese => Language::SimplifiedChinese,
            Self::FollowSystem => Language::from_locale(system_locale),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum Language {
    #[default]
    English,
    SimplifiedChinese,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct LocalizedText {
    english: &'static str,
    chinese: &'static str,
}

impl LocalizedText {
    #[must_use]
    pub(crate) const fn new(english: &'static str, chinese: &'static str) -> Self {
        Self { english, chinese }
    }
}

impl Language {
    #[must_use]
    pub(crate) fn from_locale(locale: Option<&str>) -> Self {
        let Some(locale) = locale else {
            return Self::English;
        };
        let language = locale
            .trim()
            .split(['-', '_', '.', '@'])
            .next()
            .unwrap_or_default();
        if language.eq_ignore_ascii_case("zh") {
            Self::SimplifiedChinese
        } else {
            Self::English
        }
    }

    #[must_use]
    const fn select(self, english: &'static str, chinese: &'static str) -> &'static str {
        match self {
            Self::English => english,
            Self::SimplifiedChinese => chinese,
        }
    }

    #[must_use]
    pub(crate) const fn localized(self, text: LocalizedText) -> &'static str {
        self.select(text.english, text.chinese)
    }

    #[must_use]
    pub(crate) const fn message(self, message: Message) -> &'static str {
        let (english, chinese) = match message {
            Message::Nodes => ("Nodes", "节点"),
            Message::PolicyGroup => ("Policy group", "策略组"),
            Message::PolicyGroups => ("Policy groups", "策略组"),
            Message::RoutingRules => ("Routing rules", "分流规则"),
            Message::NetworkActivity => ("Network activity", "网络活动"),
            Message::Logs => ("Logs", "日志"),
            Message::Configuration => ("Configuration", "配置"),
            Message::Settings => ("Settings", "设置"),
            Message::AddPolicyGroup => ("Add policy group", "添加策略组"),
            Message::RefreshData => ("Refresh", "刷新"),
            Message::AddRule => ("Add rule", "添加规则"),
            Message::ImportSubscription => ("Import subscription", "导入订阅"),
            Message::SaveChanges => ("Save changes", "保存修改"),
            Message::Cancel => ("Cancel", "取消"),
            Message::Delete => ("Delete", "删除"),
            Message::Enable => ("Enable", "启用"),
            Message::Disable => ("Disable", "禁用"),
            Message::Retry => ("Try again", "重试"),
            Message::ConnectMihomo => ("Connect Mihomo", "连接 Mihomo"),
            Message::NoPolicyGroups => ("No policy groups yet", "暂无策略组"),
            Message::NoNodes => ("No nodes yet", "暂无节点"),
            Message::NoActiveConnections => ("No active connections", "暂无活动连接"),
            Message::NoLogs => ("No logs yet", "暂无日志"),
            Message::NoFilterMatches => ("No results match this filter", "没有符合当前筛选的结果"),
            Message::ChangesApplied => (" · changes applied", " · 更改已生效"),
            Message::ChangesAppliedAndRestarted => (
                " · changes applied and Mihomo restarted",
                " · 更改已生效，Mihomo 已重新启动",
            ),
            Message::ChangesFailedAndRestored => (
                " · changes could not be applied; the previous rules were restored: ",
                " · 更改未能生效，已恢复先前的规则：",
            ),
            Message::SavedChangesFailed => (
                " · saved, but the changes could not be applied: ",
                " · 已保存，但更改未能生效：",
            ),
            Message::TunRestoreFailed => (
                " · kernel reloaded, but TUN could not be restored and was turned off: ",
                " · 内核已重载，但 TUN 恢复失败，已回退为关闭：",
            ),
            Message::ApplyingChanges => ("applying changes…", "正在应用更改…"),
            Message::RoutingApplyBusy => (
                "Wait for the current routing changes to finish applying",
                "请等待当前分流更改应用完成",
            ),
            Message::ManualRulesLocationUnavailable => (
                "Could not determine where to save manual rules",
                "无法确定手动分流规则的保存位置",
            ),
            Message::ManualRulesSaveFailed => {
                ("Could not save manual rules: ", "无法保存手动分流规则：")
            }
            Message::StoreRollbackFailed => (
                " · restoring the previous configuration also failed: ",
                " · 恢复先前配置时也发生失败：",
            ),
        };
        self.select(english, chinese)
    }

    #[must_use]
    pub(crate) fn count(self, noun: CountNoun, count: usize) -> String {
        match self {
            Self::English => {
                let (singular, plural) = match noun {
                    CountNoun::Node => ("node", "nodes"),
                    CountNoun::PolicyGroup => ("policy group", "policy groups"),
                    CountNoun::Rule => ("rule", "rules"),
                    CountNoun::Source => ("source", "sources"),
                    CountNoun::Connection => ("active connection", "active connections"),
                    CountNoun::Log => ("log", "logs"),
                };
                format!("{count} {}", if count == 1 { singular } else { plural })
            }
            Self::SimplifiedChinese => {
                let noun = match noun {
                    CountNoun::Node => "个节点",
                    CountNoun::PolicyGroup => "个策略组",
                    CountNoun::Rule => "条规则",
                    CountNoun::Source => "个来源",
                    CountNoun::Connection => "条活动连接",
                    CountNoun::Log => "条日志",
                };
                format!("{count} {noun}")
            }
        }
    }

    #[must_use]
    pub(crate) const fn display_name(self) -> &'static str {
        match self {
            Self::English => "English",
            Self::SimplifiedChinese => "中文",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Localizer {
    preference: LanguagePreference,
    system_locale: Option<String>,
}

impl Localizer {
    #[must_use]
    pub(crate) fn load(directory: Option<&Path>) -> Self {
        let preference = match directory {
            Some(directory) => match load_language_preference_in(directory) {
                Ok(preference) => preference,
                Err(error) => {
                    record_event(
                        LogLevel::Warn,
                        "localization.preference.load_failed",
                        error.to_string(),
                    );
                    LanguagePreference::default()
                }
            },
            None => LanguagePreference::default(),
        };
        Self {
            preference,
            system_locale: detect_system_locale(),
        }
    }

    #[must_use]
    pub(crate) const fn preference(&self) -> LanguagePreference {
        self.preference
    }

    #[must_use]
    pub(crate) fn language(&self) -> Language {
        self.preference.resolve(self.system_locale.as_deref())
    }

    pub(crate) fn set_preference(&mut self, preference: LanguagePreference) {
        self.preference = preference;
    }
}

#[must_use]
pub(crate) fn detect_system_locale() -> Option<String> {
    platform_locale().or_else(environment_locale)
}

fn environment_locale() -> Option<String> {
    ["LANGUAGE", "LC_ALL", "LC_MESSAGES", "LANG"]
        .into_iter()
        .filter_map(|key| std::env::var(key).ok())
        .find_map(|value| {
            value
                .split(':')
                .find(|locale| !locale.trim().is_empty() && !is_neutral_locale(locale))
                .map(str::to_owned)
        })
}

fn is_neutral_locale(locale: &str) -> bool {
    matches!(
        locale
            .trim()
            .split(['.', '@'])
            .next()
            .unwrap_or_default()
            .to_ascii_uppercase()
            .as_str(),
        "C" | "POSIX"
    )
}

#[cfg(target_os = "macos")]
fn platform_locale() -> Option<String> {
    let output = Command::new("/usr/bin/defaults")
        .args(["read", "-g", "AppleLanguages"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    parse_apple_languages(&String::from_utf8(output.stdout).ok()?)
}

#[cfg(target_os = "macos")]
fn parse_apple_languages(value: &str) -> Option<String> {
    value
        .split(|character: char| {
            character.is_whitespace() || matches!(character, '(' | ')' | ',' | '"' | '\'')
        })
        .map(str::trim)
        .find(|candidate| {
            !candidate.is_empty()
                && candidate.chars().any(char::is_alphabetic)
                && !matches!(*candidate, "(" | ")")
        })
        .map(str::to_owned)
}

#[cfg(target_os = "windows")]
fn platform_locale() -> Option<String> {
    use std::os::windows::process::CommandExt as _;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let output = Command::new("powershell.exe")
        .args(["-NoProfile", "-Command", "(Get-UICulture).Name"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout)
        .ok()
        .map(|locale| locale.trim().to_owned())
        .filter(|locale| !locale.is_empty())
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn platform_locale() -> Option<String> {
    None
}

pub(crate) fn load_language_preference_in(
    directory: &Path,
) -> Result<LanguagePreference, LanguagePreferenceError> {
    let path = directory.join(LANGUAGE_PREFERENCE_FILE);
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(LanguagePreference::FollowSystem);
        }
        Err(error) => {
            return Err(LanguagePreferenceError::unavailable(
                "inspect language preference",
                error,
            ));
        }
    };
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.len() > MAX_LANGUAGE_PREFERENCE_BYTES
    {
        return Err(LanguagePreferenceError::UnsafeFile);
    }
    let value = fs::read_to_string(path)
        .map_err(|error| LanguagePreferenceError::unavailable("read language preference", error))?;
    LanguagePreference::parse(value.trim_end_matches(['\r', '\n']))
        .ok_or(LanguagePreferenceError::InvalidValue)
}

pub(crate) fn save_language_preference_in(
    directory: &Path,
    preference: LanguagePreference,
) -> Result<PathBuf, LanguagePreferenceError> {
    let contents = format!("{}\n", preference.persistence_key());
    write_private_atomic(directory, LANGUAGE_PREFERENCE_FILE, contents.as_bytes())
        .map_err(|error| LanguagePreferenceError::unavailable("save language preference", error))
}

#[derive(Debug)]
pub(crate) enum LanguagePreferenceError {
    Unavailable {
        operation: &'static str,
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    UnsafeFile,
    InvalidValue,
}

impl LanguagePreferenceError {
    fn unavailable(
        operation: &'static str,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self::Unavailable {
            operation,
            source: Box::new(source),
        }
    }
}

impl std::fmt::Display for LanguagePreferenceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Unavailable { operation, source } => {
                return write!(formatter, "{operation} failed: {source}");
            }
            Self::UnsafeFile => "language preference is not a safe regular file",
            Self::InvalidValue => "language preference contains an unknown value",
        })
    }
}

impl std::error::Error for LanguagePreferenceError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Unavailable { source, .. } => Some(source.as_ref()),
            Self::UnsafeFile | Self::InvalidValue => None,
        }
    }
}

#[cfg(all(test, not(windows)))]
mod tests {
    use std::fs;

    use crate::{
        mihomo::{LiveStreamPhase, SubscriptionStoreError},
        subscription::{SourceNodeDetail, SourceNodeSecurity, SubscriptionInputError},
    };

    use super::{
        CountNoun, Language, LanguagePreference, Message, copy, load_language_preference_in,
        save_language_preference_in,
    };

    #[test]
    fn typed_source_errors_are_localized_at_the_presentation_boundary() {
        assert_eq!(
            copy::configuration::subscription_input_error(
                Language::English,
                SubscriptionInputError::InvalidVless,
            ),
            "The single-node link is invalid; check its protocol, server, and parameters"
        );
        assert_eq!(
            copy::configuration::subscription_input_error(
                Language::SimplifiedChinese,
                SubscriptionInputError::InvalidVless,
            ),
            "单节点链接无效，请检查协议、服务器和参数"
        );
        assert_eq!(
            copy::configuration::subscription_store_error(
                Language::SimplifiedChinese,
                SubscriptionStoreError::StoredSourceUnavailable,
            ),
            "已保存的订阅无法安全读取，需要重新导入"
        );
    }

    #[test]
    fn source_and_live_state_relocalize_without_reparsing() {
        let detail = SourceNodeDetail::Vless {
            security: SourceNodeSecurity::None,
            transport: Some("WebSocket"),
        };
        assert_eq!(
            copy::configuration::source_node_detail(Language::English, detail),
            "No TLS · WebSocket"
        );
        assert_eq!(
            copy::configuration::source_node_detail(Language::SimplifiedChinese, detail),
            "无 TLS · WebSocket"
        );

        let phase = LiveStreamPhase::Reconnecting(3);
        assert_eq!(
            copy::app::live_stream_phase(Language::English, &phase),
            "Reconnecting · attempt 3"
        );
        assert_eq!(
            copy::app::live_stream_phase(Language::SimplifiedChinese, &phase),
            "正在重连 · 第 3 次"
        );
    }

    #[test]
    fn core_product_terms_are_consistent_between_navigation_and_object_settings() {
        assert_eq!(
            Language::English.message(Message::Configuration),
            "Configuration"
        );
        assert_eq!(
            Language::SimplifiedChinese.message(Message::Configuration),
            "配置"
        );
        assert_eq!(Language::English.message(Message::Settings), "Settings");
        assert_eq!(
            Language::SimplifiedChinese.message(Message::Settings),
            "设置"
        );
        assert_eq!(
            Language::English.message(Message::PolicyGroups),
            "Policy groups"
        );
        assert_eq!(
            Language::English.message(Message::PolicyGroup),
            "Policy group"
        );
        assert_eq!(
            Language::SimplifiedChinese.message(Message::PolicyGroups),
            "策略组"
        );
        assert_eq!(
            Language::SimplifiedChinese.message(Message::PolicyGroup),
            "策略组"
        );
        assert_eq!(
            Language::English.message(Message::RoutingRules),
            "Routing rules"
        );
        assert_eq!(
            Language::SimplifiedChinese.message(Message::RoutingRules),
            "分流规则"
        );
    }

    #[test]
    fn count_messages_handle_english_pluralization_and_chinese_measure_words() {
        for (noun, singular, plural) in [
            (CountNoun::Node, "node", "nodes"),
            (CountNoun::PolicyGroup, "policy group", "policy groups"),
            (CountNoun::Rule, "rule", "rules"),
            (CountNoun::Source, "source", "sources"),
            (
                CountNoun::Connection,
                "active connection",
                "active connections",
            ),
            (CountNoun::Log, "log", "logs"),
        ] {
            assert_eq!(Language::English.count(noun, 0), format!("0 {plural}"));
            assert_eq!(Language::English.count(noun, 1), format!("1 {singular}"));
            assert_eq!(Language::English.count(noun, 2), format!("2 {plural}"));
            assert_eq!(
                Language::English.count(noun, 10_000),
                format!("10000 {plural}")
            );
        }
        assert_eq!(
            Language::English.count(CountNoun::Connection, 1),
            "1 active connection"
        );
        assert_eq!(
            Language::English.count(CountNoun::Connection, 2),
            "2 active connections"
        );
        assert_eq!(
            Language::SimplifiedChinese.count(CountNoun::Node, 2),
            "2 个节点"
        );
        assert_eq!(
            Language::SimplifiedChinese.count(CountNoun::Connection, 2),
            "2 条活动连接"
        );
    }

    #[test]
    fn follow_system_uses_chinese_only_for_chinese_locales() {
        assert_eq!(
            LanguagePreference::FollowSystem.resolve(Some("zh-Hans-CN")),
            Language::SimplifiedChinese
        );
        assert_eq!(
            LanguagePreference::FollowSystem.resolve(Some("zh_TW.UTF-8")),
            Language::SimplifiedChinese
        );
        assert_eq!(
            LanguagePreference::FollowSystem.resolve(Some("ja-JP")),
            Language::English
        );
        assert_eq!(
            LanguagePreference::FollowSystem.resolve(None),
            Language::English
        );
    }

    #[test]
    fn explicit_language_overrides_the_system_locale() {
        assert_eq!(
            LanguagePreference::English.resolve(Some("zh-CN")),
            Language::English
        );
        assert_eq!(
            LanguagePreference::SimplifiedChinese.resolve(Some("en-US")),
            Language::SimplifiedChinese
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn parses_the_first_macos_preferred_language() {
        assert_eq!(
            super::parse_apple_languages("(\n    \"zh-Hans-CN\",\n    \"en-US\"\n)"),
            Some("zh-Hans-CN".to_owned())
        );
    }

    #[test]
    fn preference_defaults_to_system_and_round_trips_privately() {
        let root =
            std::env::temp_dir().join(format!("manis-language-preference-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);

        assert_eq!(
            load_language_preference_in(&root).expect("missing preference follows system"),
            LanguagePreference::FollowSystem
        );
        let path = save_language_preference_in(&root, LanguagePreference::SimplifiedChinese)
            .expect("preference should persist");
        assert_eq!(
            load_language_preference_in(&root).expect("preference should load"),
            LanguagePreference::SimplifiedChinese
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            assert_eq!(
                fs::metadata(path)
                    .expect("preference metadata")
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }

        fs::remove_dir_all(root).expect("remove fixture");
    }

    #[test]
    fn preference_io_errors_keep_their_source() {
        let error = super::LanguagePreferenceError::unavailable(
            "read language preference",
            std::io::Error::other("fixture failure"),
        );

        assert!(std::error::Error::source(&error).is_some());
        assert!(error.to_string().contains("fixture failure"));
        assert_eq!(
            copy::configuration::language_preference_error(Language::English, &error),
            "The language preference could not be read or saved"
        );
    }
}
