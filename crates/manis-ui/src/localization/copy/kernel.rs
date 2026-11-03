use crate::localization::LocalizedText;

pub(crate) const CONFIGURATION_IS_INVALID: LocalizedText =
    LocalizedText::new("configuration is invalid", "配置无效");
pub(crate) const CONNECTING: LocalizedText = LocalizedText::new("Connecting…", "正在连接…");
pub(crate) const MANAGED_KERNEL_CONFIGURED_CLICK_TO_START: LocalizedText = LocalizedText::new(
    "Managed kernel configured · click to start",
    "托管内核已配置 · 点击启动",
);
#[cfg(any(test, feature = "snapshot-fixtures"))]
pub(crate) const NOT_CONNECTED_TO: LocalizedText =
    LocalizedText::new("Not connected to", "尚未连接");
pub(crate) const REFRESH: LocalizedText = LocalizedText::new("Refresh", "刷新数据");
pub(crate) const START_MIHOMO: LocalizedText = LocalizedText::new("Start Mihomo", "启动 Mihomo");
