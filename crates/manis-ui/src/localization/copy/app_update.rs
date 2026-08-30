use crate::{
    app_update::AppUpdateError,
    localization::{Language, LocalizedText},
};

pub(crate) const AUTOMATIC_UPDATES: LocalizedText =
    LocalizedText::new("Automatic updates", "自动更新");
pub(crate) const AUTOMATIC_UPDATES_DETAIL: LocalizedText = LocalizedText::new(
    "Manis checks in the background and downloads verified updates. You choose when to restart.",
    "Manis 会在后台检查并下载经过校验的更新，由你决定何时重启。",
);
pub(crate) const CHECK_FOR_UPDATES: LocalizedText =
    LocalizedText::new("Check for updates", "检查更新");
pub(crate) const CHECKING: LocalizedText = LocalizedText::new("Checking…", "正在检查…");
pub(crate) const DOWNLOADING: LocalizedText =
    LocalizedText::new("Downloading update…", "正在下载更新…");
pub(crate) const UP_TO_DATE: LocalizedText =
    LocalizedText::new("Manis is up to date", "Manis 已是最新版本");
pub(crate) const RESTART_AND_UPDATE: LocalizedText =
    LocalizedText::new("Restart and update", "重启并更新");
pub(crate) const INSTALLING: LocalizedText =
    LocalizedText::new("Installing update…", "正在安装更新…");
pub(crate) const TRY_AGAIN: LocalizedText = LocalizedText::new("Try again", "重试");
pub(crate) const UNSUPPORTED: LocalizedText = LocalizedText::new(
    "Automatic updates are unavailable for this installation",
    "当前安装方式不支持自动更新",
);
pub(crate) const UPDATE_FAILED: LocalizedText = LocalizedText::new("Update failed", "更新失败");

pub(crate) fn current_version(language: Language, version: &str) -> String {
    match language {
        Language::English => format!("Current version {version}"),
        Language::SimplifiedChinese => format!("当前版本 {version}"),
    }
}

pub(crate) fn ready_version(language: Language, version: &str) -> String {
    match language {
        Language::English => format!("Version {version} is ready to install"),
        Language::SimplifiedChinese => format!("版本 {version} 已下载并完成校验"),
    }
}

pub(crate) const fn error(language: Language, error: AppUpdateError) -> &'static str {
    let message = match error {
        AppUpdateError::UnsupportedInstallation => LocalizedText::new(
            "This installation cannot be updated automatically",
            "当前安装方式无法自动更新",
        ),
        AppUpdateError::DataDirUnavailable => LocalizedText::new(
            "The Manis data directory is unavailable",
            "Manis 数据目录不可用",
        ),
        AppUpdateError::NetworkUnavailable => LocalizedText::new(
            "The update service is temporarily unavailable",
            "更新服务暂时不可用",
        ),
        AppUpdateError::InvalidMetadata
        | AppUpdateError::MissingAsset
        | AppUpdateError::InvalidDigest => LocalizedText::new(
            "The published update information is incomplete",
            "发布的更新信息不完整",
        ),
        AppUpdateError::InsecureRedirect
        | AppUpdateError::DigestMismatch
        | AppUpdateError::InvalidPackage => LocalizedText::new(
            "The update could not be verified and was not installed",
            "更新无法通过安全校验，未执行安装",
        ),
        AppUpdateError::PackageTooLarge => LocalizedText::new(
            "The update package exceeds the safety limit",
            "更新包超过安全大小限制",
        ),
        AppUpdateError::PermissionDenied => LocalizedText::new(
            "Administrator authorization was cancelled or denied",
            "管理员授权已取消或被拒绝",
        ),
        AppUpdateError::InstallFailed => LocalizedText::new(
            "The operating system could not install the update",
            "操作系统未能安装更新",
        ),
        AppUpdateError::Io => LocalizedText::new(
            "The update could not be saved on this device",
            "无法在此设备上保存更新",
        ),
    };
    language.localized(message)
}
