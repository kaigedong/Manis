//! Login-item integration for the three supported desktop platforms.
//!
//! Manis keeps the user's choice in `config.toml`. The platform artifact is
//! derived from that choice on every launch, so moving the executable or
//! repairing a missing login item does not require a second preference file.

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use crate::diagnostics::{LogLevel, record_event};

const PREFERENCE_FILE: &str = "startup.launch_at_login";
const MAX_PREFERENCE_BYTES: u64 = 16;
const DEFAULT_LAUNCH_AT_LOGIN: bool = true;
const ENABLED_VALUE: &str = "enabled";
const DISABLED_VALUE: &str = "disabled";

#[cfg(target_os = "linux")]
const LINUX_DESKTOP_FILE: &str = "dev.manis.app.desktop";
#[cfg(target_os = "macos")]
const MACOS_LAUNCH_AGENT: &str = "dev.manis.app.plist";
#[cfg(windows)]
const WINDOWS_RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
#[cfg(windows)]
const WINDOWS_VALUE_NAME: &str = "Manis";
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Debug)]
pub(crate) enum AutostartError {
    Unsupported,
    Unavailable {
        operation: &'static str,
        source: std::io::Error,
    },
    #[cfg_attr(not(any(target_os = "linux", target_os = "macos")), allow(dead_code))]
    InvalidPath,
    #[cfg_attr(not(windows), allow(dead_code))]
    CommandFailed {
        operation: &'static str,
        status: std::process::ExitStatus,
    },
}

impl AutostartError {
    fn unavailable(operation: &'static str, source: std::io::Error) -> Self {
        Self::Unavailable { operation, source }
    }
}

impl fmt::Display for AutostartError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported => formatter.write_str("login startup is not supported here"),
            Self::Unavailable { operation, source } => {
                write!(formatter, "{operation} failed: {source}")
            }
            Self::InvalidPath => formatter.write_str("the login item path cannot be represented"),
            Self::CommandFailed { operation, status } => {
                write!(formatter, "{operation} failed ({status})")
            }
        }
    }
}

impl std::error::Error for AutostartError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Unavailable { source, .. } => Some(source),
            Self::Unsupported | Self::InvalidPath | Self::CommandFailed { .. } => None,
        }
    }
}

#[must_use]
pub(crate) const fn supported() -> bool {
    cfg!(any(
        target_os = "linux",
        target_os = "macos",
        target_os = "windows"
    ))
}

/// Reads the saved choice, defaulting to enabled on first use.
#[must_use]
pub(crate) fn load_preference(directory: Option<&Path>) -> bool {
    directory.map_or(DEFAULT_LAUNCH_AT_LOGIN, load_preference_in)
}

fn load_preference_in(directory: &Path) -> bool {
    match crate::config_toml::read_entry(directory, PREFERENCE_FILE, MAX_PREFERENCE_BYTES) {
        Ok(Some(value)) => match value.trim() {
            ENABLED_VALUE => true,
            DISABLED_VALUE => false,
            _ => {
                record_event(
                    LogLevel::Warn,
                    "autostart.preference.invalid",
                    "saved startup value is not recognized",
                );
                DEFAULT_LAUNCH_AT_LOGIN
            }
        },
        Ok(None) => DEFAULT_LAUNCH_AT_LOGIN,
        Err(error) => {
            record_event(
                LogLevel::Warn,
                "autostart.preference.read_failed",
                error.to_string(),
            );
            DEFAULT_LAUNCH_AT_LOGIN
        }
    }
}

pub(crate) fn save_preference_in(
    directory: &Path,
    enabled: bool,
) -> Result<PathBuf, crate::config_toml::ConfigTomlError> {
    let value = if enabled {
        ENABLED_VALUE
    } else {
        DISABLED_VALUE
    };
    crate::config_toml::write_entry(directory, PREFERENCE_FILE, &format!("{value}\n"))
}

/// Applies the current choice to the platform login configuration.
pub(crate) fn set_enabled(enabled: bool) -> Result<(), AutostartError> {
    if !supported() {
        return Err(AutostartError::Unsupported);
    }
    if enabled {
        let executable = executable_path()?;
        platform_enable(&executable)
    } else {
        platform_disable()
    }
}

fn executable_path() -> Result<PathBuf, AutostartError> {
    #[cfg(target_os = "linux")]
    if let Some(appimage) = std::env::var_os("APPIMAGE") {
        let path = PathBuf::from(appimage);
        if path.is_absolute() {
            return Ok(path);
        }
    }
    std::env::current_exe().map_err(|error| AutostartError::unavailable("locate executable", error))
}

#[cfg(target_os = "linux")]
fn platform_enable(executable: &Path) -> Result<(), AutostartError> {
    let path = linux_autostart_path().ok_or(AutostartError::InvalidPath)?;
    let parent = path.parent().ok_or(AutostartError::InvalidPath)?;
    fs::create_dir_all(parent)
        .map_err(|error| AutostartError::unavailable("create login startup directory", error))?;
    fs::write(&path, linux_desktop_entry(executable))
        .map_err(|error| AutostartError::unavailable("write login startup entry", error))
}

#[cfg(target_os = "linux")]
fn platform_disable() -> Result<(), AutostartError> {
    let path = linux_autostart_path().ok_or(AutostartError::InvalidPath)?;
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(AutostartError::unavailable(
            "remove login startup entry",
            error,
        )),
    }
}

#[cfg(target_os = "linux")]
fn linux_autostart_path() -> Option<PathBuf> {
    let config_root = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .filter(|path| path.is_absolute())
                .map(|home| home.join(".config"))
        })?;
    Some(config_root.join("autostart").join(LINUX_DESKTOP_FILE))
}

#[cfg(target_os = "linux")]
fn linux_desktop_entry(executable: &Path) -> String {
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=Manis\n\
         Comment=Policy-based proxy routing workbench\n\
         Comment[zh_CN]=基于策略的代理分流工作台\n\
         Icon=dev.manis.app\n\
         Exec={}\n\
         Terminal=false\n\
         StartupNotify=true\n\
         Categories=Network;Utility;\n\
         X-GNOME-Autostart-enabled=true\n",
        desktop_exec_argument(executable)
    )
}

#[cfg(target_os = "linux")]
fn desktop_exec_argument(executable: &Path) -> String {
    let escaped = executable
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('`', "\\`")
        .replace('$', "\\$")
        .replace('%', "%%");
    format!("\"{escaped}\"")
}

#[cfg(target_os = "macos")]
fn platform_enable(executable: &Path) -> Result<(), AutostartError> {
    let path = macos_launch_agent_path().ok_or(AutostartError::InvalidPath)?;
    let parent = path.parent().ok_or(AutostartError::InvalidPath)?;
    fs::create_dir_all(parent)
        .map_err(|error| AutostartError::unavailable("create login item directory", error))?;
    fs::write(&path, macos_launch_agent(executable))
        .map_err(|error| AutostartError::unavailable("write login item", error))
}

#[cfg(target_os = "macos")]
fn platform_disable() -> Result<(), AutostartError> {
    let path = macos_launch_agent_path().ok_or(AutostartError::InvalidPath)?;
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(AutostartError::unavailable("remove login item", error)),
    }
}

#[cfg(target_os = "macos")]
fn macos_launch_agent_path() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .map(|home| home.join("Library/LaunchAgents").join(MACOS_LAUNCH_AGENT))
}

#[cfg(target_os = "macos")]
fn macos_launch_agent(executable: &Path) -> String {
    let arguments = macos_launch_arguments(executable)
        .into_iter()
        .map(|argument| format!("        <string>{}</string>\n", xml_escape(&argument)))
        .collect::<String>();
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \
         \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
         <plist version=\"1.0\">\n\
         <dict>\n\
         \x20   <key>Label</key>\n\
         \x20   <string>dev.manis.app</string>\n\
         \x20   <key>ProgramArguments</key>\n\
         \x20   <array>\n\
         {arguments}\
         \x20   </array>\n\
         \x20   <key>RunAtLoad</key>\n\
         \x20   <true/>\n\
         \x20   <key>KeepAlive</key>\n\
         \x20   <false/>\n\
         \x20   <key>ProcessType</key>\n\
         \x20   <string>Interactive</string>\n\
         \x20   <key>LimitLoadToSessionType</key>\n\
         \x20   <string>Aqua</string>\n\
         </dict>\n\
         </plist>\n"
    )
}

#[cfg(target_os = "macos")]
fn macos_launch_arguments(executable: &Path) -> Vec<String> {
    app_bundle(executable).map_or_else(
        || vec![executable.to_string_lossy().into_owned()],
        |bundle| {
            vec![
                "/usr/bin/open".to_owned(),
                "-a".to_owned(),
                bundle.to_string_lossy().into_owned(),
            ]
        },
    )
}

#[cfg(target_os = "macos")]
fn app_bundle(executable: &Path) -> Option<&Path> {
    let macos = executable.parent()?;
    if macos.file_name()? != "MacOS" {
        return None;
    }
    let contents = macos.parent()?;
    if contents.file_name()? != "Contents" {
        return None;
    }
    contents.parent()
}

#[cfg(target_os = "macos")]
fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(windows)]
fn platform_enable(executable: &Path) -> Result<(), AutostartError> {
    use std::os::windows::process::CommandExt as _;

    let mut value = std::ffi::OsString::from("\"");
    value.push(executable.as_os_str());
    value.push("\"");
    let output = std::process::Command::new("reg")
        .args([
            "add",
            WINDOWS_RUN_KEY,
            "/v",
            WINDOWS_VALUE_NAME,
            "/t",
            "REG_SZ",
            "/d",
        ])
        .arg(&value)
        .arg("/f")
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|error| AutostartError::unavailable("update login item", error))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(AutostartError::CommandFailed {
            operation: "update login item",
            status: output.status,
        })
    }
}

#[cfg(windows)]
fn platform_disable() -> Result<(), AutostartError> {
    use std::os::windows::process::CommandExt as _;

    let query = std::process::Command::new("reg")
        .args(["query", WINDOWS_RUN_KEY, "/v", WINDOWS_VALUE_NAME])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|error| AutostartError::unavailable("read login item", error))?;
    if !query.status.success() {
        return Ok(());
    }
    let output = std::process::Command::new("reg")
        .args(["delete", WINDOWS_RUN_KEY, "/v", WINDOWS_VALUE_NAME, "/f"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|error| AutostartError::unavailable("remove login item", error))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(AutostartError::CommandFailed {
            operation: "remove login item",
            status: output.status,
        })
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn platform_enable(_executable: &Path) -> Result<(), AutostartError> {
    Err(AutostartError::Unsupported)
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn platform_disable() -> Result<(), AutostartError> {
    Err(AutostartError::Unsupported)
}

#[cfg(test)]
mod tests {
    use std::fs;

    #[cfg(target_os = "linux")]
    use super::{desktop_exec_argument, linux_desktop_entry};
    use super::{load_preference, load_preference_in, save_preference_in};

    fn fixture(name: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "manis-autostart-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or_default()
        ));
        fs::create_dir_all(&path).expect("fixture directory");
        path
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn login_entry_quotes_spaces_and_percent_field_codes() {
        let entry = linux_desktop_entry(std::path::Path::new("/opt/Manis App/manis"));
        assert!(entry.contains("Exec=\"/opt/Manis App/manis\""));
        assert!(entry.contains("X-GNOME-Autostart-enabled=true"));
        assert_eq!(
            desktop_exec_argument(std::path::Path::new("/tmp/100%/manis")),
            "\"/tmp/100%%/manis\""
        );
        assert_eq!(
            desktop_exec_argument(std::path::Path::new("/tmp/$HOME/`id`")),
            "\"/tmp/\\$HOME/\\`id\\`\""
        );
    }

    #[test]
    fn first_use_defaults_to_enabled_and_disabling_is_persistent() {
        let directory = fixture("preference");
        assert!(load_preference(None));
        assert!(load_preference_in(&directory));
        save_preference_in(&directory, false).expect("save preference");
        assert!(!load_preference_in(&directory));
        save_preference_in(&directory, true).expect("save preference");
        assert!(load_preference_in(&directory));
        fs::remove_dir_all(directory).expect("cleanup");
    }
}
