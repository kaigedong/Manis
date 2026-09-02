use crate::localization::LocalizedText;

#[cfg(target_os = "macos")]
mod format;
#[cfg(target_os = "macos")]
pub(crate) use format::*;

pub(crate) const COULD_NOT_APPLY_THE_SYSTEM_PROXY_OR_RESTORE_EVERY_PREVIOUS: LocalizedText =
    LocalizedText::new(
        "Could not apply the system proxy or restore every previous setting; the recovery snapshot was retained",
        "系统代理应用失败，且未能完整恢复原设置；恢复快照已保留",
    );
pub(crate) const COULD_NOT_CREATE_MANIS_SYSTEM_PROXY_RECOVERY_DIRECTORY: LocalizedText =
    LocalizedText::new(
        "Could not create Manis system proxy recovery directory",
        "无法创建 Manis 系统代理恢复目录",
    );
pub(crate) const COULD_NOT_CREATE_MANIS_SYSTEM_PROXY_RECOVERY_SNAPSHOT: LocalizedText =
    LocalizedText::new(
        "Could not create Manis system proxy recovery snapshot",
        "无法创建 Manis 系统代理恢复快照",
    );
pub(crate) const COULD_NOT_DETERMINE_MANIS_DATA_DIRECTORY_FOR_SYSTEM_PROXY_RECOVERY: LocalizedText =
    LocalizedText::new(
        "Could not determine Manis data directory for system proxy recovery",
        "无法确定 Manis 系统代理恢复目录",
    );
pub(crate) const COULD_NOT_DETERMINE_MANIS_DATA_DIRECTORY_FOR_TUN_DNS_RECOVERY: LocalizedText =
    LocalizedText::new(
        "Could not determine Manis data directory for TUN DNS recovery",
        "无法确定 Manis TUN DNS 恢复目录",
    );
pub(crate) const COULD_NOT_DETERMINE_MANIS_SYSTEM_PROXY_RECOVERY_DIRECTORY: LocalizedText =
    LocalizedText::new(
        "Could not determine Manis system proxy recovery directory",
        "无法确定 Manis 系统代理恢复目录",
    );
pub(crate) const COULD_NOT_FLUSH_MANIS_SYSTEM_PROXY_RECOVERY_SNAPSHOT: LocalizedText =
    LocalizedText::new(
        "Could not flush Manis system proxy recovery snapshot",
        "无法刷写 Manis 系统代理恢复快照",
    );
pub(crate) const COULD_NOT_INSPECT_MANIS_SYSTEM_PROXY_RECOVERY_DIRECTORY: LocalizedText =
    LocalizedText::new(
        "Could not inspect Manis system proxy recovery directory",
        "无法检查 Manis 系统代理恢复目录",
    );
#[cfg(target_os = "macos")]
pub(crate) const COULD_NOT_INSPECT_THE_MACOS_DEFAULT_ROUTE: LocalizedText = LocalizedText::new(
    "Could not inspect the macOS default route",
    "无法检查 macOS 默认路由",
);
#[cfg(target_os = "windows")]
pub(crate) const COULD_NOT_NOTIFY_WINDOWS_THAT_PROXY_SETTINGS_CHANGED: LocalizedText =
    LocalizedText::new(
        "Could not notify Windows that proxy settings changed",
        "无法通知 Windows 代理设置更新",
    );
pub(crate) const COULD_NOT_PROTECT_MANIS_SYSTEM_PROXY_RECOVERY_DIRECTORY: LocalizedText =
    LocalizedText::new(
        "Could not protect Manis system proxy recovery directory",
        "无法保护 Manis 系统代理恢复目录",
    );
pub(crate) const COULD_NOT_PROTECT_MANIS_SYSTEM_PROXY_RECOVERY_SNAPSHOT: LocalizedText =
    LocalizedText::new(
        "Could not protect Manis system proxy recovery snapshot",
        "无法保护 Manis 系统代理恢复快照",
    );
#[cfg(target_os = "linux")]
pub(crate) const COULD_NOT_READ_GNOME_SYSTEM_PROXY_STATUS: LocalizedText = LocalizedText::new(
    "Could not read GNOME system proxy status",
    "无法读取 GNOME 系统代理状态",
);
#[cfg(target_os = "macos")]
pub(crate) const COULD_NOT_READ_MACOS_SYSTEM_PROXY_STATUS: LocalizedText = LocalizedText::new(
    "Could not read macOS system proxy status",
    "无法读取 macOS 系统代理状态",
);
pub(crate) const COULD_NOT_REMOVE_MANIS_SYSTEM_PROXY_RECOVERY_SNAPSHOT: LocalizedText =
    LocalizedText::new(
        "Could not remove Manis system proxy recovery snapshot",
        "无法删除 Manis 系统代理恢复快照",
    );
pub(crate) const COULD_NOT_REPLACE_MANIS_SYSTEM_PROXY_RECOVERY_SNAPSHOT: LocalizedText =
    LocalizedText::new(
        "Could not replace Manis system proxy recovery snapshot",
        "无法替换 Manis 系统代理恢复快照",
    );
#[cfg(target_os = "windows")]
pub(crate) const COULD_NOT_RESTORE_WINDOWS_SYSTEM_PROXY_SETTINGS: LocalizedText =
    LocalizedText::new(
        "Could not restore Windows system proxy settings",
        "无法恢复 Windows 系统代理设置",
    );
pub(crate) const COULD_NOT_SAFELY_READ_MANIS_SYSTEM_PROXY_RECOVERY_SNAPSHOT: LocalizedText =
    LocalizedText::new(
        "Could not safely read Manis system proxy recovery snapshot",
        "无法安全读取 Manis 系统代理恢复快照",
    );
#[cfg(target_os = "macos")]
pub(crate) const COULD_NOT_START_MACOS_NETWORKSETUP: LocalizedText = LocalizedText::new(
    "Could not start macOS networksetup",
    "无法启动 macOS networksetup",
);
#[cfg(target_os = "windows")]
pub(crate) const COULD_NOT_START_WINDOWS_REG: LocalizedText =
    LocalizedText::new("Could not start Windows reg", "无法启动 Windows reg");
#[cfg(target_os = "linux")]
pub(crate) const COULD_NOT_WRITE_GNOME_SYSTEM_PROXY_SETTINGS: LocalizedText = LocalizedText::new(
    "Could not write GNOME system proxy settings",
    "无法写入 GNOME 系统代理设置",
);
pub(crate) const COULD_NOT_WRITE_MANIS_SYSTEM_PROXY_RECOVERY_SNAPSHOT: LocalizedText =
    LocalizedText::new(
        "Could not write Manis system proxy recovery snapshot",
        "无法写入 Manis 系统代理恢复快照",
    );
#[cfg(target_os = "windows")]
pub(crate) const COULD_NOT_WRITE_WINDOWS_SYSTEM_PROXY_SETTINGS: LocalizedText = LocalizedText::new(
    "Could not write Windows system proxy settings",
    "无法写入 Windows 系统代理设置",
);
#[cfg(target_os = "macos")]
pub(crate) const MACOS_HAS_NO_CONFIGURABLE_NETWORK_SERVICES: LocalizedText = LocalizedText::new(
    "macOS has no configurable network services",
    "macOS 没有可配置的网络服务",
);
#[cfg(target_os = "macos")]
pub(crate) const MACOS_RETURNED_AN_INVALID_DNS_SERVER_ADDRESS: LocalizedText = LocalizedText::new(
    "macOS returned an invalid DNS server address",
    "macOS 返回了无效的 DNS 服务器地址",
);
#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
pub(crate) const MANIS_CANNOT_CONFIGURE_THE_SYSTEM_PROXY_ON_THIS_DESKTOP_YET: LocalizedText =
    LocalizedText::new(
        "Manis cannot configure the system proxy on this desktop yet",
        "Manis 暂不支持在当前桌面上自动设置系统代理",
    );
pub(crate) const MANIS_SYSTEM_PROXY_RECOVERY_DIRECTORY_IS_UNSAFE: LocalizedText =
    LocalizedText::new(
        "Manis system proxy recovery directory is unsafe",
        "Manis 系统代理恢复目录不安全",
    );
#[cfg(not(test))]
pub(crate) const MANIS_SYSTEM_PROXY_RECOVERY_SNAPSHOT_IS_INVALID: LocalizedText =
    LocalizedText::new(
        "Manis system proxy recovery snapshot is invalid",
        "Manis 系统代理恢复快照无效",
    );
#[cfg(not(test))]
pub(crate) const MANIS_TUN_DNS_RECOVERY_SNAPSHOT_IS_INVALID: LocalizedText = LocalizedText::new(
    "Manis TUN DNS recovery snapshot is invalid",
    "Manis TUN DNS 恢复快照无效",
);
pub(crate) const MIHOMO_HAS_NO_OPEN_HTTP_MIXED_OR_SOCKS_LISTENER: LocalizedText =
    LocalizedText::new(
        "Mihomo has no open HTTP, mixed, or SOCKS listener",
        "Mihomo 没有开放 HTTP、mixed 或 SOCKS 端口",
    );
#[cfg(target_os = "macos")]
pub(crate) const THE_MACOS_DEFAULT_ROUTE_DID_NOT_IDENTIFY_AN_INTERFACE: LocalizedText =
    LocalizedText::new(
        "The macOS default route did not identify an interface",
        "macOS 默认路由未提供出口接口",
    );
#[cfg(target_os = "linux")]
pub(crate) const THIS_DESKTOP_DOES_NOT_SUPPORT_GSETTINGS: LocalizedText = LocalizedText::new(
    "This desktop does not support gsettings",
    "当前桌面不支持 gsettings",
);
pub(crate) const TUN_DNS_WAS_NOT_PREPARED_BEFORE_ACTIVATION: LocalizedText = LocalizedText::new(
    "TUN DNS was not prepared before activation",
    "TUN DNS 激活前尚未完成准备",
);
#[cfg(target_os = "windows")]
pub(crate) const WINDOWS_PROXY_WAS_WRITTEN_BUT_THE_SYSTEM_REFRESH_FAILED: LocalizedText =
    LocalizedText::new(
        "Windows proxy was written, but the system refresh failed",
        "Windows 代理已写入，但系统刷新失败",
    );
