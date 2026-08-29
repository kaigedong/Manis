use crate::localization::LocalizedText;

mod format;
pub(crate) use format::*;

pub(crate) const CLEAR_THE_FILTER_OR_SEARCH_BY_OPERATION_ERROR_MESSAGE_OR: LocalizedText =
    LocalizedText::new(
        "Clear the filter or search by operation, error message, or log level.",
        "清除筛选，或尝试搜索操作、错误内容或日志级别。",
    );
pub(crate) const LOGS_WILL_APPEAR_HERE_AFTER_MIHOMO_STARTS_OR_MANIS_PERFORMS: LocalizedText =
    LocalizedText::new(
        "Logs will appear here after Mihomo starts or Manis performs an operation.",
        "启动 Mihomo 或执行操作后，相关日志会显示在这里。",
    );
pub(crate) const REFRESH_LOG_DATA: LocalizedText =
    LocalizedText::new("Refresh log data", "刷新日志数据");
