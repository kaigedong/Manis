use crate::localization::Language;

pub(crate) fn unmapped_macos_interface(language: Language, interface: &str) -> String {
    match language {
        Language::English => {
            format!("Could not map macOS interface {interface} to a network service")
        }
        Language::SimplifiedChinese => {
            format!("无法将 macOS 接口 {interface} 映射到网络服务")
        }
    }
}

pub(crate) fn macos_command_failed(language: Language, exit_code: i32) -> String {
    match language {
        Language::English => {
            format!("macOS system proxy command failed with exit code {exit_code}")
        }
        Language::SimplifiedChinese => {
            format!("macOS 系统代理命令失败（退出码 {exit_code}）")
        }
    }
}
