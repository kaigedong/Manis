use crate::{
    app_update::AppUpdateError,
    localization::{Language, LocalizedText},
};

pub(crate) const CLOSE: LocalizedText = LocalizedText::new("Close", "关闭");
pub(crate) const APP_UPDATES: LocalizedText = LocalizedText::new("App updates", "应用更新");
pub(crate) const CHECK_AUTOMATICALLY: LocalizedText =
    LocalizedText::new("Automatic checks", "自动检查更新");
pub(crate) const CHECK_AUTOMATICALLY_DETAIL: LocalizedText = LocalizedText::new(
    "Check at startup and every hour. Download and install updates right here.",
    "启动时及每小时自动检查版本，可直接在应用内下载并安装。",
);
pub(crate) const CHECK_NOW: LocalizedText = LocalizedText::new("Check for updates", "检查更新");
pub(crate) const DOWNLOAD: LocalizedText = LocalizedText::new("Download", "下载");
pub(crate) const DOWNLOADING: LocalizedText =
    LocalizedText::new("Downloading update…", "正在下载更新…");
pub(crate) const INSTALL_AND_RESTART: LocalizedText =
    LocalizedText::new("Install and restart", "安装并重启");
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
        Language::English => format!("Version {version} is ready to download"),
        Language::SimplifiedChinese => format!("发现新版本 {version}，可直接下载"),
    }
}

pub(crate) fn ready_to_install(language: Language, version: &str) -> String {
    match language {
        Language::English => format!("Version {version} is downloaded; install it and restart"),
        Language::SimplifiedChinese => format!("新版本 {version} 已下载，安装后重启即可生效"),
    }
}

pub(crate) fn install_failed(language: Language, message: &str) -> String {
    match language {
        Language::English => format!("The update could not be installed: {message}"),
        Language::SimplifiedChinese => format!("安装更新失败：{message}"),
    }
}

pub(crate) const fn error(language: Language, error: AppUpdateError) -> &'static str {
    language.localized(match error {
        AppUpdateError::NetworkUnavailable => LocalizedText::new(
            "Update check unavailable. View releases on GitHub.",
            "暂时无法检查更新，可前往 GitHub 查看。",
        ),
        AppUpdateError::InvalidMetadata
        | AppUpdateError::InsecureRedirect
        | AppUpdateError::MissingMetadata
        | AppUpdateError::InvalidDigest
        | AppUpdateError::DigestMismatch
        | AppUpdateError::MetadataTooLarge => LocalizedText::new(
            "Could not verify version information. View releases on GitHub.",
            "无法验证版本信息，可前往 GitHub 查看。",
        ),
        AppUpdateError::MissingPackage | AppUpdateError::UnsupportedPlatform => LocalizedText::new(
            "This build has no installable package. View releases on GitHub.",
            "当前构建没有可安装的包，可前往 GitHub 查看。",
        ),
        AppUpdateError::PackageTooLarge => LocalizedText::new(
            "The update package is too large. View releases on GitHub.",
            "更新包超出大小限制，可前往 GitHub 查看。",
        ),
    })
}
