use crate::localization::LocalizedText;

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
