use crate::localization::LocalizedText;

pub(crate) const CANNOT_PREPARE_SING_BOX_WITHOUT_A_SOURCE_DIRECTORY: LocalizedText =
    LocalizedText::new(
        "Cannot prepare sing-box without a source directory",
        "无法确定来源目录，不能准备 sing-box",
    );
pub(crate) const CONFIGURATION_IS_INVALID: LocalizedText =
    LocalizedText::new("configuration is invalid", "配置无效");
pub(crate) const CONNECTING: LocalizedText = LocalizedText::new("Connecting…", "正在连接…");
pub(crate) const CONNECT_SING_BOX: LocalizedText =
    LocalizedText::new("Connect sing-box", "连接 sing-box");
pub(crate) const KERNEL_SELECTION_CONTAINS_AN_UNKNOWN_VALUE: LocalizedText = LocalizedText::new(
    "Kernel selection contains an unknown value",
    "内核选择包含未知值",
);
pub(crate) const KERNEL_SELECTION_COULD_NOT_BE_READ_OR_SAVED: LocalizedText = LocalizedText::new(
    "Kernel selection could not be read or saved",
    "无法读取或保存内核选择",
);
pub(crate) const KERNEL_SELECTION_IS_NOT_A_SAFE_REGULAR_FILE: LocalizedText = LocalizedText::new(
    "Kernel selection is not a safe regular file",
    "内核选择文件不是安全的普通文件",
);
pub(crate) const MANAGED_KERNEL_CONFIGURED_CLICK_TO_START: LocalizedText = LocalizedText::new(
    "Managed kernel configured · click to start",
    "托管内核已配置 · 点击启动",
);
#[cfg(any(test, feature = "snapshot-fixtures"))]
pub(crate) const NOT_CONNECTED_TO: LocalizedText =
    LocalizedText::new("Not connected to", "尚未连接");
pub(crate) const REFRESH: LocalizedText = LocalizedText::new("Refresh", "刷新数据");
pub(crate) const START_MIHOMO: LocalizedText = LocalizedText::new("Start Mihomo", "启动 Mihomo");
pub(crate) const START_SING_BOX: LocalizedText =
    LocalizedText::new("Start sing-box", "启动 sing-box");
