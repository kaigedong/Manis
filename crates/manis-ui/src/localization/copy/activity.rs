use crate::localization::LocalizedText;

mod format;
pub(crate) use format::*;

pub(crate) const CONNECTIONS: LocalizedText = LocalizedText::new("connections", "条连接");
pub(crate) const LIVE_TRAFFIC_APPEARS_HERE_AS_SOON_AS_THE_KERNEL_REPORTS: LocalizedText =
    LocalizedText::new(
        "Live traffic appears here as soon as the kernel reports a connection.",
        "内核上报新连接后，实时流量会显示在这里。",
    );
pub(crate) const NO_MATCHED_RULE: LocalizedText =
    LocalizedText::new("No matched rule", "未匹配规则");
pub(crate) const REFRESH_ACTIVITY_DATA_BY_RECONNECTING_THE_KERNEL: LocalizedText =
    LocalizedText::new(
        "Refresh activity data by reconnecting the kernel",
        "重新连接内核并刷新网络活动数据",
    );
pub(crate) const ROUTE_UNAVAILABLE: LocalizedText =
    LocalizedText::new("Route unavailable", "路由未返回");
pub(crate) const TRY_A_TARGET_HOST_PROCESS_NAME_RULE_OR_ROUTE_STAGE: LocalizedText =
    LocalizedText::new(
        "Try a target host, process name, rule, or route stage.",
        "可以尝试输入目标域名、进程名、规则或路径节点。",
    );
pub(crate) const UNKNOWN_PROCESS: LocalizedText = LocalizedText::new("Unknown process", "未知进程");
pub(crate) const UNKNOWN_TARGET: LocalizedText = LocalizedText::new("Unknown target", "未知目标");
