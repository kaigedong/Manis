use crate::localization::LocalizedText;

pub(crate) const COULD_NOT_CREATE_THE_SYSTEM_TRAY_ICON: LocalizedText = LocalizedText::new(
    "Could not create the system tray icon",
    "无法创建系统托盘图标",
);
pub(crate) const COULD_NOT_CREATE_THE_SYSTEM_TRAY_MENU: LocalizedText = LocalizedText::new(
    "Could not create the system tray menu",
    "无法创建系统托盘菜单",
);
#[cfg(target_os = "linux")]
pub(crate) const COULD_NOT_INITIALIZE_THE_LINUX_GTK_TRAY_EVENT_LOOP: LocalizedText =
    LocalizedText::new(
        "Could not initialize the Linux GTK tray event loop",
        "无法初始化 Linux GTK 托盘事件循环",
    );
pub(crate) const MANIS_RULE_ROUTING: LocalizedText =
    LocalizedText::new("Manis · rule routing", "Manis · 规则路由");
pub(crate) const OPEN_MANIS: LocalizedText = LocalizedText::new("Open Manis", "打开 Manis");
pub(crate) const QUIT_MANIS: LocalizedText = LocalizedText::new("Quit Manis", "退出 Manis");
pub(crate) const RULE_ROUTING_STATUS_IS_AVAILABLE_IN_THE_MAIN_WINDOW: LocalizedText =
    LocalizedText::new(
        "Rule routing · status is available in the main window",
        "规则路由 · 状态请在主窗口查看",
    );
pub(crate) const SYSTEM_TRAY_IS_UNAVAILABLE: LocalizedText =
    LocalizedText::new("System tray is unavailable", "系统托盘不可用");
