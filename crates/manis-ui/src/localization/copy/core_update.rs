use crate::{
    app_update::DownloadProgress,
    localization::{Language, LocalizedText, copy},
};

pub(crate) const MANAGED_STABLE_CORE: LocalizedText =
    LocalizedText::new("Manis-managed stable core", "Manis 托管稳定版内核");
pub(crate) const NOT_INSTALLED_HINT: LocalizedText = LocalizedText::new(
    "Core not installed; download the stable release",
    "尚未安装内核，请下载稳定版",
);
pub(crate) const CHECKING: LocalizedText =
    LocalizedText::new("Checking for updates…", "正在检查新版本…");
pub(crate) const UP_TO_DATE: LocalizedText =
    LocalizedText::new("Mihomo is up to date", "Mihomo 已是最新版本");
pub(crate) const FETCHING_RELEASE: LocalizedText =
    LocalizedText::new("Fetching latest stable release…", "正在获取最新稳定版信息…");
pub(crate) const UPDATE_ACTION: LocalizedText = LocalizedText::new("Update", "更新");
pub(crate) const DOWNLOAD_STABLE: LocalizedText =
    LocalizedText::new("Download stable", "下载稳定版");
pub(crate) const INSTALL_ACTION: LocalizedText = LocalizedText::new("Install update", "安装更新");
pub(crate) const INSTALLING_BUTTON: LocalizedText = LocalizedText::new("Installing…", "安装中…");
pub(crate) const CHECK_ACCESSIBILITY_LABEL: LocalizedText =
    LocalizedText::new("Check for Mihomo core updates", "检查 Mihomo 内核更新");

pub(crate) fn available_version(language: Language, version: &str) -> String {
    match language {
        Language::English => format!("Version {version} is ready to update"),
        Language::SimplifiedChinese => format!("发现新版本 {version}，可直接更新"),
    }
}

pub(crate) fn downloading_status(
    language: Language,
    version: Option<&str>,
    progress: DownloadProgress,
) -> String {
    let Some(version) = version else {
        return language.localized(FETCHING_RELEASE).to_owned();
    };
    if let Some(total) = progress.total_bytes.filter(|total| *total > 0)
        && progress.downloaded_bytes > 0
    {
        let downloaded = copy::app_update::format_compact_size(progress.downloaded_bytes);
        let total = copy::app_update::format_compact_size(total);
        return match language {
            Language::English => format!("Downloading {version} ({downloaded} / {total})"),
            Language::SimplifiedChinese => format!("正在下载 {version}（{downloaded} / {total}）"),
        };
    }
    match language {
        Language::English => format!("Downloading Mihomo {version}…"),
        Language::SimplifiedChinese => format!("正在下载新版本 {version}…"),
    }
}

pub(crate) fn ready_to_install(language: Language, version: &str, proxy_active: bool) -> String {
    match (language, proxy_active) {
        (Language::English, false) => format!("Version {version} is downloaded; ready to install"),
        (Language::SimplifiedChinese, false) => format!("新版本 {version} 已下载，点击即可安装"),
        (Language::English, true) => {
            format!("Version {version} is downloaded; pause proxy to install")
        }
        (Language::SimplifiedChinese, true) => {
            format!("新版本 {version} 已下载，暂停代理后即可安装")
        }
    }
}

pub(crate) fn installing_version(language: Language, version: &str) -> String {
    match language {
        Language::English => format!("Verifying and installing {version}…"),
        Language::SimplifiedChinese => format!("正在校验并安装 {version}…"),
    }
}

pub(crate) fn updated_version(language: Language, version: &str) -> String {
    match language {
        Language::English => format!("Updated to {version}"),
        Language::SimplifiedChinese => format!("已更新至 {version}"),
    }
}

pub(crate) const UNSUPPORTED_PLATFORM: LocalizedText = LocalizedText::new(
    "Automatic Mihomo core updates are unavailable on this platform",
    "当前平台暂不支持自动更新 Mihomo 内核",
);
pub(crate) const DATA_DIR_UNAVAILABLE: LocalizedText = LocalizedText::new(
    "The Manis data directory could not be located",
    "无法定位 Manis 数据目录",
);
pub(crate) const NETWORK_UNAVAILABLE: LocalizedText = LocalizedText::new(
    "The Mihomo release download failed; check the network and try again",
    "Mihomo release 下载失败，请检查网络后重试",
);
pub(crate) const INVALID_RELEASE_METADATA: LocalizedText = LocalizedText::new(
    "The Mihomo release metadata is invalid",
    "Mihomo release 元数据格式无效",
);
pub(crate) const INSECURE_REDIRECT: LocalizedText = LocalizedText::new(
    "The Mihomo release download redirected to a non-HTTPS page",
    "Mihomo release 下载跳转到了非 HTTPS 页面",
);
pub(crate) const MISSING_ASSET: LocalizedText = LocalizedText::new(
    "No Mihomo core package is available for this platform",
    "未找到适用于当前平台的 Mihomo 内核包",
);
pub(crate) const MISSING_DIGEST: LocalizedText = LocalizedText::new(
    "The Mihomo release asset is missing its sha256 digest",
    "Mihomo release asset 缺少 sha256 digest",
);
pub(crate) const INVALID_DIGEST: LocalizedText = LocalizedText::new(
    "The Mihomo release asset digest is invalid",
    "Mihomo release asset digest 格式无效",
);
pub(crate) const DIGEST_MISMATCH: LocalizedText = LocalizedText::new(
    "The Mihomo core package failed sha256 verification",
    "Mihomo 内核包 sha256 校验失败",
);
pub(crate) const PACKAGE_TOO_LARGE: LocalizedText = LocalizedText::new(
    "The Mihomo core package exceeds 64 MiB",
    "Mihomo 内核包超过 64 MiB",
);
pub(crate) const INVALID_ARCHIVE: LocalizedText = LocalizedText::new(
    "The Mihomo core package has an invalid archive format",
    "Mihomo 内核包格式无效",
);
pub(crate) const IO: LocalizedText = LocalizedText::new(
    "The Mihomo core file could not be read or written",
    "Mihomo 内核文件读写失败",
);
pub(crate) const VERSION_MISMATCH: LocalizedText = LocalizedText::new(
    "The Mihomo core version could not be verified",
    "Mihomo 内核版本验证失败",
);
pub(crate) const PUBLISH_FAILED: LocalizedText = LocalizedText::new(
    "The Mihomo core update could not be published",
    "Mihomo 内核发布失败",
);
pub(crate) const ROLLBACK_FAILED: LocalizedText = LocalizedText::new(
    "The previous Mihomo core could not be restored",
    "Mihomo 内核回滚失败",
);
