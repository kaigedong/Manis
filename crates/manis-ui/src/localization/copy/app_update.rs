use crate::{
    app_update::AppUpdateError,
    localization::{Language, LocalizedText},
};

pub(crate) const CLOSE: LocalizedText = LocalizedText::new("Close", "关闭");
pub(crate) const APP_UPDATES: LocalizedText = LocalizedText::new("App updates", "应用更新");
pub(crate) const CHECK_AUTOMATICALLY: LocalizedText =
    LocalizedText::new("Automatic checks", "自动检查更新");
pub(crate) const CHECK_AUTOMATICALLY_DETAIL: LocalizedText = LocalizedText::new(
    "Check at startup and every hour. Download and install updates yourself from GitHub.",
    "启动时及每小时自动检查版本，由你前往 GitHub 下载和安装。",
);
pub(crate) const OPEN_GITHUB: LocalizedText = LocalizedText::new("View on GitHub", "前往 GitHub");
pub(crate) const CHECK_PENDING: LocalizedText =
    LocalizedText::new("Waiting to check for updates", "等待自动检查更新");
pub(crate) const CHECKING: LocalizedText =
    LocalizedText::new("Checking for updates…", "正在检查新版本…");
pub(crate) const UP_TO_DATE: LocalizedText =
    LocalizedText::new("Manis is up to date", "Manis 已是最新版本");

pub(crate) fn current_version(language: Language, version: &str) -> String {
    match language {
        Language::English => format!("Current version · {version}"),
        Language::SimplifiedChinese => format!("当前版本 · {version}"),
    }
}

pub(crate) fn available_version(language: Language, version: &str) -> String {
    match language {
        Language::English => format!("Version {version} is available on GitHub"),
        Language::SimplifiedChinese => format!("发现新版本 {version}，可前往 GitHub 下载"),
    }
}

pub(crate) const fn error(language: Language, error: AppUpdateError) -> &'static str {
    language.localized(match error {
        AppUpdateError::NetworkUnavailable => LocalizedText::new(
            "Update check unavailable. View releases on GitHub.",
            "暂时无法检查更新，可前往 GitHub 查看。",
        ),
        _ => LocalizedText::new(
            "Could not verify version information. View releases on GitHub.",
            "无法验证版本信息，可前往 GitHub 查看。",
        ),
    })
}
