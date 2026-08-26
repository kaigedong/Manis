use std::{
    fmt, fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use crate::brand;
use crate::localization::Language;

const RECOVERY_FILE: &str = "system-proxy.recovery";
const RECOVERY_VERSION: &str = "manis-system-proxy-v1";
const LEGACY_RELAY_RECOVERY_VERSION: &str = "relay-system-proxy-v1";
const MAX_RECOVERY_BYTES: u64 = 1024 * 1024;

fn recovery_version_supported(version: &str) -> bool {
    matches!(version, RECOVERY_VERSION | LEGACY_RELAY_RECOVERY_VERSION)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ProxyPorts {
    pub http: Option<u16>,
    pub socks: Option<u16>,
}

impl ProxyPorts {
    pub(crate) fn usable_with_language(self, language: Language) -> Result<Self, SystemProxyError> {
        if self.http.is_none() && self.socks.is_none() {
            Err(SystemProxyError::Unavailable(
                language
                    .text(
                        "Mihomo has no open HTTP, mixed, or SOCKS listener",
                        "Mihomo 没有开放 HTTP、mixed 或 SOCKS 端口",
                    )
                    .to_owned(),
            ))
        } else {
            Ok(self)
        }
    }
}

#[derive(Debug)]
pub(crate) enum SystemProxyError {
    Unavailable(String),
    CommandFailed(String),
}

impl fmt::Display for SystemProxyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable(message) | Self::CommandFailed(message) => {
                formatter.write_str(message)
            }
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct SystemProxySession {
    #[cfg(target_os = "macos")]
    previous: Vec<macos::ServiceSnapshot>,
    #[cfg(target_os = "linux")]
    previous: Option<linux::GnomeSnapshot>,
    #[cfg(target_os = "windows")]
    previous: Option<windows::WinInetSnapshot>,
    applied: bool,
}

impl SystemProxySession {
    #[must_use]
    pub(crate) const fn is_applied(&self) -> bool {
        self.applied
    }

    #[allow(dead_code)]
    pub(crate) fn recover_stale_with_language(
        &mut self,
        language: Language,
    ) -> Result<(), SystemProxyError> {
        #[cfg(target_os = "macos")]
        macos::recover_stale(language)?;
        #[cfg(target_os = "linux")]
        linux::recover_stale(language)?;
        #[cfg(target_os = "windows")]
        windows::recover_stale(language)?;

        self.applied = false;
        Ok(())
    }

    pub(crate) fn enable_with_language(
        &mut self,
        ports: ProxyPorts,
        language: Language,
    ) -> Result<(), SystemProxyError> {
        let ports = ports.usable_with_language(language)?;
        if self.applied {
            self.disable_with_language(language)?;
        }

        #[cfg(target_os = "macos")]
        {
            self.previous = macos::enable(ports, language)?;
        }
        #[cfg(target_os = "linux")]
        {
            self.previous = Some(linux::enable(ports, language)?);
        }
        #[cfg(target_os = "windows")]
        {
            self.previous = Some(windows::enable(ports, language)?);
        }
        #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
        {
            let _ = ports;
            return Err(SystemProxyError::Unavailable(
                language
                    .text(
                        "This platform does not have a system proxy adapter yet",
                        "当前平台尚未实现系统代理适配器",
                    )
                    .to_owned(),
            ));
        }

        self.applied = true;
        Ok(())
    }

    pub(crate) fn shutdown_with_language(
        &mut self,
        language: Language,
    ) -> Result<(), SystemProxyError> {
        self.disable_with_language(language)
    }

    pub(crate) fn disable_with_language(
        &mut self,
        language: Language,
    ) -> Result<(), SystemProxyError> {
        if !self.applied {
            return Ok(());
        }

        #[cfg(target_os = "macos")]
        {
            macos::restore(&self.previous, language)?;
            delete_recovery_snapshot(language)?;
        }
        #[cfg(target_os = "linux")]
        if let Some(previous) = self.previous.as_ref() {
            linux::restore(previous, language)?;
            delete_recovery_snapshot(language)?;
        }
        #[cfg(target_os = "windows")]
        if let Some(previous) = self.previous.as_ref() {
            windows::restore(previous, language)?;
            delete_recovery_snapshot(language)?;
        }

        self.applied = false;
        Ok(())
    }
}

fn recovery_snapshot_path(language: Language) -> Result<PathBuf, SystemProxyError> {
    brand::data_dir()
        .map(|directory| directory.join(RECOVERY_FILE))
        .ok_or_else(|| {
            SystemProxyError::Unavailable(
                language
                    .text(
                        "Could not determine Manis data directory for system proxy recovery",
                        "无法确定 Manis 系统代理恢复目录",
                    )
                    .to_owned(),
            )
        })
}

#[allow(dead_code)]
fn read_recovery_snapshot(language: Language) -> Result<Option<String>, SystemProxyError> {
    let path = recovery_snapshot_path(language)?;
    read_recovery_snapshot_at(&path, language)
}

fn read_recovery_snapshot_at(
    path: &Path,
    language: Language,
) -> Result<Option<String>, SystemProxyError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(_error) => return Err(recovery_read_error(language)),
    };
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.len() > MAX_RECOVERY_BYTES
    {
        return Err(recovery_read_error(language));
    }
    #[cfg(unix)]
    if metadata.permissions().mode() & 0o077 != 0 {
        return Err(recovery_read_error(language));
    }
    let file = fs::File::open(path).map_err(|_error| recovery_read_error(language))?;
    let mut contents = String::new();
    file.take(MAX_RECOVERY_BYTES + 1)
        .read_to_string(&mut contents)
        .map_err(|_error| recovery_read_error(language))?;
    if contents.len() as u64 > MAX_RECOVERY_BYTES {
        return Err(recovery_read_error(language));
    }
    Ok(Some(contents))
}

fn recovery_read_error(language: Language) -> SystemProxyError {
    SystemProxyError::CommandFailed(
        language
            .text(
                "Could not safely read Manis system proxy recovery snapshot",
                "无法安全读取 Manis 系统代理恢复快照",
            )
            .to_owned(),
    )
}

fn write_recovery_snapshot(contents: &str, language: Language) -> Result<(), SystemProxyError> {
    let path = recovery_snapshot_path(language)?;
    write_recovery_snapshot_at(&path, contents, language)
}

fn write_recovery_snapshot_at(
    path: &Path,
    contents: &str,
    language: Language,
) -> Result<(), SystemProxyError> {
    let Some(directory) = path.parent() else {
        return Err(SystemProxyError::Unavailable(
            language
                .text(
                    "Could not determine Manis system proxy recovery directory",
                    "无法确定 Manis 系统代理恢复目录",
                )
                .to_owned(),
        ));
    };
    prepare_recovery_directory(directory, language)?;

    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(RECOVERY_FILE);
    let temporary = directory.join(format!(".{file_name}.{}.tmp", std::process::id()));
    let _ = fs::remove_file(&temporary);
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|_error| {
            SystemProxyError::CommandFailed(
                language
                    .text(
                        "Could not create Manis system proxy recovery snapshot",
                        "无法创建 Manis 系统代理恢复快照",
                    )
                    .to_owned(),
            )
        })?;
    #[cfg(unix)]
    file.set_permissions(fs::Permissions::from_mode(0o600))
        .map_err(|_error| {
            SystemProxyError::CommandFailed(
                language
                    .text(
                        "Could not protect Manis system proxy recovery snapshot",
                        "无法保护 Manis 系统代理恢复快照",
                    )
                    .to_owned(),
            )
        })?;
    file.write_all(contents.as_bytes()).map_err(|_error| {
        SystemProxyError::CommandFailed(
            language
                .text(
                    "Could not write Manis system proxy recovery snapshot",
                    "无法写入 Manis 系统代理恢复快照",
                )
                .to_owned(),
        )
    })?;
    file.sync_all().map_err(|_error| {
        SystemProxyError::CommandFailed(
            language
                .text(
                    "Could not flush Manis system proxy recovery snapshot",
                    "无法刷写 Manis 系统代理恢复快照",
                )
                .to_owned(),
        )
    })?;
    drop(file);
    fs::rename(&temporary, path).map_err(|_error| {
        let _ = fs::remove_file(&temporary);
        SystemProxyError::CommandFailed(
            language
                .text(
                    "Could not replace Manis system proxy recovery snapshot",
                    "无法替换 Manis 系统代理恢复快照",
                )
                .to_owned(),
        )
    })
}

fn prepare_recovery_directory(
    directory: &Path,
    language: Language,
) -> Result<(), SystemProxyError> {
    match fs::symlink_metadata(directory) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            return Err(SystemProxyError::CommandFailed(
                language
                    .text(
                        "Manis system proxy recovery directory is unsafe",
                        "Manis 系统代理恢复目录不安全",
                    )
                    .to_owned(),
            ));
        }
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            fs::create_dir_all(directory).map_err(|_error| {
                SystemProxyError::CommandFailed(
                    language
                        .text(
                            "Could not create Manis system proxy recovery directory",
                            "无法创建 Manis 系统代理恢复目录",
                        )
                        .to_owned(),
                )
            })?;
        }
        Err(_error) => {
            return Err(SystemProxyError::CommandFailed(
                language
                    .text(
                        "Could not inspect Manis system proxy recovery directory",
                        "无法检查 Manis 系统代理恢复目录",
                    )
                    .to_owned(),
            ));
        }
    }
    #[cfg(unix)]
    fs::set_permissions(directory, fs::Permissions::from_mode(0o700)).map_err(|_error| {
        SystemProxyError::CommandFailed(
            language
                .text(
                    "Could not protect Manis system proxy recovery directory",
                    "无法保护 Manis 系统代理恢复目录",
                )
                .to_owned(),
        )
    })?;
    Ok(())
}

fn delete_recovery_snapshot(language: Language) -> Result<(), SystemProxyError> {
    let path = recovery_snapshot_path(language)?;
    delete_recovery_snapshot_at(&path, language)
}

fn delete_recovery_snapshot_at(path: &Path, language: Language) -> Result<(), SystemProxyError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(_error) => Err(SystemProxyError::CommandFailed(
            language
                .text(
                    "Could not remove Manis system proxy recovery snapshot",
                    "无法删除 Manis 系统代理恢复快照",
                )
                .to_owned(),
        )),
    }
}

fn rollback_failed_message(language: Language) -> SystemProxyError {
    SystemProxyError::CommandFailed(
        language
            .text(
                "Could not apply the system proxy or restore every previous setting; the recovery snapshot was retained",
                "系统代理应用失败，且未能完整恢复原设置；恢复快照已保留",
            )
            .to_owned(),
    )
}

fn encode_string(value: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(value.len() * 2);
    for byte in value.as_bytes() {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

fn decode_string(value: &str) -> Option<String> {
    if !value.len().is_multiple_of(2) {
        return None;
    }
    let mut bytes = Vec::with_capacity(value.len() / 2);
    for index in (0..value.len()).step_by(2) {
        bytes.push(u8::from_str_radix(&value[index..index + 2], 16).ok()?);
    }
    String::from_utf8(bytes).ok()
}

#[cfg(target_os = "macos")]
mod macos {
    use std::process::Command;

    use crate::localization::Language;

    use super::{
        ProxyPorts, RECOVERY_VERSION, SystemProxyError, decode_string, delete_recovery_snapshot,
        delete_recovery_snapshot_at, encode_string, read_recovery_snapshot,
        write_recovery_snapshot, write_recovery_snapshot_at,
    };

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub(super) struct ServiceSnapshot {
        name: String,
        web: ProxySetting,
        secure_web: ProxySetting,
        socks: ProxySetting,
    }

    #[derive(Clone, Debug, Eq, PartialEq)]
    struct ProxySetting {
        enabled: bool,
        server: String,
        port: u16,
    }

    pub(super) fn enable(
        ports: ProxyPorts,
        language: Language,
    ) -> Result<Vec<ServiceSnapshot>, SystemProxyError> {
        let mut runner = NetworkSetupRunner;
        enable_with_runner(ports, language, &mut runner)
    }

    fn enable_with_runner(
        ports: ProxyPorts,
        language: Language,
        runner: &mut impl CommandRunner,
    ) -> Result<Vec<ServiceSnapshot>, SystemProxyError> {
        enable_with_runner_at(ports, language, runner, None)
    }

    fn enable_with_runner_at(
        ports: ProxyPorts,
        language: Language,
        runner: &mut impl CommandRunner,
        recovery_path: Option<&std::path::Path>,
    ) -> Result<Vec<ServiceSnapshot>, SystemProxyError> {
        let services = runner.output(&["-listallnetworkservices"], language)?;
        let services: Vec<_> = services
            .lines()
            .skip(1)
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('*'))
            .map(str::to_owned)
            .collect();
        if services.is_empty() {
            return Err(SystemProxyError::Unavailable(
                language
                    .text(
                        "macOS has no configurable network services",
                        "macOS 没有可配置的网络服务",
                    )
                    .to_owned(),
            ));
        }

        let mut snapshots = Vec::with_capacity(services.len());
        for service in &services {
            snapshots.push(ServiceSnapshot {
                web: read_setting_with_runner("-getwebproxy", service, language, runner)?,
                secure_web: read_setting_with_runner(
                    "-getsecurewebproxy",
                    service,
                    language,
                    runner,
                )?,
                socks: read_setting_with_runner(
                    "-getsocksfirewallproxy",
                    service,
                    language,
                    runner,
                )?,
                name: service.clone(),
            });
        }

        write_recovery(&encode_snapshots(&snapshots), language, recovery_path)?;
        for service in &services {
            if let Err(error) = apply_service_with_runner(service, ports, language, runner) {
                if restore_with_runner(&snapshots, language, runner).is_err() {
                    return Err(super::rollback_failed_message(language));
                }
                delete_recovery(language, recovery_path)?;
                return Err(error);
            }
        }
        Ok(snapshots)
    }

    fn apply_service_with_runner(
        service: &str,
        ports: ProxyPorts,
        language: Language,
        runner: &mut impl CommandRunner,
    ) -> Result<(), SystemProxyError> {
        if let Some(port) = ports.http {
            runner.run(
                &["-setwebproxy", service, "127.0.0.1", &port.to_string()],
                language,
            )?;
            runner.run(
                &[
                    "-setsecurewebproxy",
                    service,
                    "127.0.0.1",
                    &port.to_string(),
                ],
                language,
            )?;
        } else {
            runner.run(&["-setwebproxystate", service, "off"], language)?;
            runner.run(&["-setsecurewebproxystate", service, "off"], language)?;
        }
        if let Some(port) = ports.socks {
            runner.run(
                &[
                    "-setsocksfirewallproxy",
                    service,
                    "127.0.0.1",
                    &port.to_string(),
                ],
                language,
            )?;
        } else {
            runner.run(&["-setsocksfirewallproxystate", service, "off"], language)?;
        }
        Ok(())
    }

    pub(super) fn restore(
        previous: &[ServiceSnapshot],
        language: Language,
    ) -> Result<(), SystemProxyError> {
        let mut runner = NetworkSetupRunner;
        restore_with_runner(previous, language, &mut runner)
    }

    fn restore_with_runner(
        previous: &[ServiceSnapshot],
        language: Language,
        runner: &mut impl CommandRunner,
    ) -> Result<(), SystemProxyError> {
        for service in previous {
            restore_setting(
                &service.name,
                "-setwebproxy",
                "-setwebproxystate",
                &service.web,
                language,
                runner,
            )?;
            restore_setting(
                &service.name,
                "-setsecurewebproxy",
                "-setsecurewebproxystate",
                &service.secure_web,
                language,
                runner,
            )?;
            restore_setting(
                &service.name,
                "-setsocksfirewallproxy",
                "-setsocksfirewallproxystate",
                &service.socks,
                language,
                runner,
            )?;
        }
        Ok(())
    }

    fn read_setting_with_runner(
        command: &str,
        service: &str,
        language: Language,
        runner: &mut impl CommandRunner,
    ) -> Result<ProxySetting, SystemProxyError> {
        let value = runner.output(&[command, service], language)?;
        let mut enabled = false;
        let mut server = String::new();
        let mut port = 0;
        for line in value.lines() {
            let Some((key, value)) = line.split_once(':') else {
                continue;
            };
            match key.trim() {
                "Enabled" => enabled = value.trim().eq_ignore_ascii_case("yes"),
                "Server" => value.trim().clone_into(&mut server),
                "Port" => port = value.trim().parse().unwrap_or(0),
                _ => {}
            }
        }
        Ok(ProxySetting {
            enabled,
            server,
            port,
        })
    }

    fn restore_setting(
        service: &str,
        set_command: &str,
        state_command: &str,
        setting: &ProxySetting,
        language: Language,
        runner: &mut impl CommandRunner,
    ) -> Result<(), SystemProxyError> {
        if !setting.server.is_empty() && setting.port > 0 {
            runner.run(
                &[
                    set_command,
                    service,
                    &setting.server,
                    &setting.port.to_string(),
                ],
                language,
            )?;
        }
        runner.run(
            &[
                state_command,
                service,
                if setting.enabled { "on" } else { "off" },
            ],
            language,
        )
    }

    #[allow(dead_code)]
    pub(super) fn recover_stale(language: Language) -> Result<(), SystemProxyError> {
        let Some(contents) = read_recovery_snapshot(language)? else {
            return Ok(());
        };
        let snapshots = decode_snapshots(&contents).ok_or_else(|| {
            SystemProxyError::CommandFailed(
                language
                    .text(
                        "Manis system proxy recovery snapshot is invalid",
                        "Manis 系统代理恢复快照无效",
                    )
                    .to_owned(),
            )
        })?;
        restore(&snapshots, language)?;
        delete_recovery_snapshot(language)
    }

    #[cfg(test)]
    fn recover_stale_at(
        path: &std::path::Path,
        language: Language,
        runner: &mut impl CommandRunner,
    ) -> Result<(), SystemProxyError> {
        let Some(contents) = super::read_recovery_snapshot_at(path, language)? else {
            return Ok(());
        };
        let snapshots = decode_snapshots(&contents).ok_or_else(|| {
            SystemProxyError::CommandFailed("invalid recovery snapshot".to_owned())
        })?;
        restore_with_runner(&snapshots, language, runner)?;
        delete_recovery_snapshot_at(path, language)
    }

    fn encode_snapshots(snapshots: &[ServiceSnapshot]) -> String {
        let mut output = format!("{RECOVERY_VERSION}\nplatform=macos\n");
        for snapshot in snapshots {
            output.push_str("service");
            output.push('\t');
            output.push_str(&encode_string(&snapshot.name));
            for setting in [&snapshot.web, &snapshot.secure_web, &snapshot.socks] {
                output.push('\t');
                output.push_str(if setting.enabled { "1" } else { "0" });
                output.push('\t');
                output.push_str(&encode_string(&setting.server));
                output.push('\t');
                output.push_str(&setting.port.to_string());
            }
            output.push('\n');
        }
        output
    }

    fn decode_snapshots(contents: &str) -> Option<Vec<ServiceSnapshot>> {
        let mut lines = contents.lines();
        super::recovery_version_supported(lines.next()?).then_some(())?;
        (lines.next()? == "platform=macos").then_some(())?;
        let mut snapshots = Vec::new();
        for line in lines {
            if line.trim().is_empty() {
                continue;
            }
            let fields: Vec<_> = line.split('\t').collect();
            if fields.len() != 11 || fields[0] != "service" {
                return None;
            }
            snapshots.push(ServiceSnapshot {
                name: decode_string(fields[1])?,
                web: decode_setting(&fields[2..5])?,
                secure_web: decode_setting(&fields[5..8])?,
                socks: decode_setting(&fields[8..11])?,
            });
        }
        Some(snapshots)
    }

    fn decode_setting(fields: &[&str]) -> Option<ProxySetting> {
        Some(ProxySetting {
            enabled: match fields.first().copied()? {
                "0" => false,
                "1" => true,
                _ => return None,
            },
            server: decode_string(fields.get(1).copied()?)?,
            port: fields.get(2)?.parse().ok()?,
        })
    }

    fn write_recovery(
        contents: &str,
        language: Language,
        path: Option<&std::path::Path>,
    ) -> Result<(), SystemProxyError> {
        if let Some(path) = path {
            write_recovery_snapshot_at(path, contents, language)
        } else {
            write_recovery_snapshot(contents, language)
        }
    }

    fn delete_recovery(
        language: Language,
        path: Option<&std::path::Path>,
    ) -> Result<(), SystemProxyError> {
        if let Some(path) = path {
            delete_recovery_snapshot_at(path, language)
        } else {
            delete_recovery_snapshot(language)
        }
    }

    trait CommandRunner {
        fn run(&mut self, args: &[&str], language: Language) -> Result<(), SystemProxyError>;
        fn output(&mut self, args: &[&str], language: Language)
        -> Result<String, SystemProxyError>;
    }

    struct NetworkSetupRunner;

    impl CommandRunner for NetworkSetupRunner {
        fn run(&mut self, args: &[&str], language: Language) -> Result<(), SystemProxyError> {
            let status = Command::new("networksetup")
                .args(args)
                .status()
                .map_err(|_| {
                    SystemProxyError::Unavailable(
                        language
                            .text(
                                "Could not start macOS networksetup",
                                "无法启动 macOS networksetup",
                            )
                            .to_owned(),
                    )
                })?;
            if status.success() {
                Ok(())
            } else {
                let code = status.code().unwrap_or(-1);
                Err(SystemProxyError::CommandFailed(match language {
                    Language::English => {
                        format!("macOS system proxy command failed with exit code {code}")
                    }
                    Language::SimplifiedChinese => {
                        format!("macOS 系统代理命令失败（退出码 {code}）")
                    }
                }))
            }
        }

        fn output(
            &mut self,
            args: &[&str],
            language: Language,
        ) -> Result<String, SystemProxyError> {
            let output = Command::new("networksetup")
                .args(args)
                .output()
                .map_err(|_| {
                    SystemProxyError::Unavailable(
                        language
                            .text(
                                "Could not start macOS networksetup",
                                "无法启动 macOS networksetup",
                            )
                            .to_owned(),
                    )
                })?;
            if output.status.success() {
                Ok(String::from_utf8_lossy(&output.stdout).into_owned())
            } else {
                Err(SystemProxyError::CommandFailed(
                    language
                        .text(
                            "Could not read macOS system proxy status",
                            "无法读取 macOS 系统代理状态",
                        )
                        .to_owned(),
                ))
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use std::{
            collections::HashMap,
            path::PathBuf,
            time::{SystemTime, UNIX_EPOCH},
        };

        use crate::localization::Language;

        use super::{
            CommandRunner, ProxyPorts, ProxySetting, ServiceSnapshot, SystemProxyError,
            decode_snapshots, enable_with_runner_at, encode_snapshots, recover_stale_at,
        };

        #[derive(Default)]
        struct FakeRunner {
            outputs: HashMap<Vec<String>, String>,
            runs: Vec<Vec<String>>,
            fail_runs: bool,
        }

        impl FakeRunner {
            fn with_output(mut self, args: &[&str], output: &str) -> Self {
                self.outputs.insert(
                    args.iter().map(|value| (*value).to_owned()).collect(),
                    output.to_owned(),
                );
                self
            }
        }

        impl CommandRunner for FakeRunner {
            fn run(&mut self, args: &[&str], _language: Language) -> Result<(), SystemProxyError> {
                self.runs
                    .push(args.iter().map(|value| (*value).to_owned()).collect());
                if self.fail_runs {
                    Err(SystemProxyError::CommandFailed(
                        "injected command failure".to_owned(),
                    ))
                } else {
                    Ok(())
                }
            }

            fn output(
                &mut self,
                args: &[&str],
                _language: Language,
            ) -> Result<String, SystemProxyError> {
                self.outputs
                    .get(
                        &args
                            .iter()
                            .map(|value| (*value).to_owned())
                            .collect::<Vec<_>>(),
                    )
                    .cloned()
                    .ok_or_else(|| {
                        SystemProxyError::CommandFailed("missing fake output".to_owned())
                    })
            }
        }

        #[test]
        fn macos_recovery_snapshot_roundtrips_proxy_settings() {
            let snapshots = vec![ServiceSnapshot {
                name: "Wi-Fi".to_owned(),
                web: ProxySetting {
                    enabled: true,
                    server: "proxy.example".to_owned(),
                    port: 8080,
                },
                secure_web: ProxySetting {
                    enabled: false,
                    server: String::new(),
                    port: 0,
                },
                socks: ProxySetting {
                    enabled: true,
                    server: "127.0.0.1".to_owned(),
                    port: 7891,
                },
            }];

            let encoded = encode_snapshots(&snapshots);
            assert_eq!(decode_snapshots(&encoded), Some(snapshots));
        }

        #[test]
        fn macos_enable_writes_recovery_before_applying_proxy() {
            let root = test_directory("manis-system-proxy-enable");
            let recovery = root.join("system-proxy.recovery");
            let mut runner = fake_macos_runner();

            let snapshots = enable_with_runner_at(
                ProxyPorts {
                    http: Some(7890),
                    socks: Some(7891),
                },
                Language::English,
                &mut runner,
                Some(&recovery),
            )
            .expect("enable should use fake runner");

            let written = std::fs::read_to_string(&recovery).expect("recovery file should exist");
            assert_eq!(decode_snapshots(&written), Some(snapshots));
            assert_eq!(
                runner.runs.first().map(Vec::as_slice),
                Some(
                    ["-setwebproxy", "Wi-Fi", "127.0.0.1", "7890",]
                        .map(str::to_owned)
                        .as_slice()
                )
            );

            let _ = std::fs::remove_dir_all(root);
        }

        #[test]
        fn macos_failed_apply_keeps_recovery_when_rollback_also_fails() {
            let root = test_directory("manis-system-proxy-failed-rollback");
            let recovery = root.join("system-proxy.recovery");
            let mut runner = fake_macos_runner();
            runner.fail_runs = true;

            let result = enable_with_runner_at(
                ProxyPorts {
                    http: Some(7890),
                    socks: Some(7891),
                },
                Language::English,
                &mut runner,
                Some(&recovery),
            );

            assert!(result.is_err());
            assert!(recovery.is_file());
            let written = std::fs::read_to_string(&recovery).expect("recovery file should remain");
            assert!(decode_snapshots(&written).is_some());

            let _ = std::fs::remove_dir_all(root);
        }

        #[test]
        fn macos_recover_stale_restores_snapshot_and_removes_marker() {
            let root = test_directory("manis-system-proxy-recover");
            let recovery = root.join("system-proxy.recovery");
            let snapshots = vec![ServiceSnapshot {
                name: "Wi-Fi".to_owned(),
                web: ProxySetting {
                    enabled: true,
                    server: "corp.proxy".to_owned(),
                    port: 8080,
                },
                secure_web: ProxySetting {
                    enabled: false,
                    server: String::new(),
                    port: 0,
                },
                socks: ProxySetting {
                    enabled: false,
                    server: String::new(),
                    port: 0,
                },
            }];
            super::write_recovery_snapshot_at(
                &recovery,
                &encode_snapshots(&snapshots),
                Language::English,
            )
            .expect("recovery write should succeed");
            let mut runner = FakeRunner::default();

            recover_stale_at(&recovery, Language::English, &mut runner)
                .expect("stale recovery should restore through fake runner");

            assert!(!recovery.exists(), "recovery marker should be deleted");
            assert_eq!(
                runner.runs,
                vec![
                    vec![
                        "-setwebproxy".to_owned(),
                        "Wi-Fi".to_owned(),
                        "corp.proxy".to_owned(),
                        "8080".to_owned(),
                    ],
                    vec![
                        "-setwebproxystate".to_owned(),
                        "Wi-Fi".to_owned(),
                        "on".to_owned(),
                    ],
                    vec![
                        "-setsecurewebproxystate".to_owned(),
                        "Wi-Fi".to_owned(),
                        "off".to_owned(),
                    ],
                    vec![
                        "-setsocksfirewallproxystate".to_owned(),
                        "Wi-Fi".to_owned(),
                        "off".to_owned(),
                    ],
                ]
            );

            let _ = std::fs::remove_dir_all(root);
        }

        fn fake_macos_runner() -> FakeRunner {
            FakeRunner::default()
                .with_output(
                    "-listallnetworkservices"
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .as_slice(),
                    "An asterisk (*) denotes that a network service is disabled.\nWi-Fi\n",
                )
                .with_output(
                    &["-getwebproxy", "Wi-Fi"],
                    "Enabled: Yes\nServer: corp.proxy\nPort: 8080\n",
                )
                .with_output(
                    &["-getsecurewebproxy", "Wi-Fi"],
                    "Enabled: No\nServer: \nPort: 0\n",
                )
                .with_output(
                    &["-getsocksfirewallproxy", "Wi-Fi"],
                    "Enabled: No\nServer: \nPort: 0\n",
                )
        }

        fn test_directory(prefix: &str) -> PathBuf {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time before unix epoch")
                .as_nanos();
            let directory =
                std::env::temp_dir().join(format!("{prefix}-{}-{nanos}", std::process::id()));
            std::fs::create_dir_all(&directory).expect("test directory should be created");
            directory
        }
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use std::process::Command;

    use crate::localization::Language;

    use super::{
        ProxyPorts, RECOVERY_VERSION, SystemProxyError, decode_string, delete_recovery_snapshot,
        encode_string, read_recovery_snapshot, write_recovery_snapshot,
    };

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub(super) struct GnomeSnapshot {
        mode: String,
        http_host: String,
        http_port: String,
        https_host: String,
        https_port: String,
        socks_host: String,
        socks_port: String,
    }

    pub(super) fn enable(
        ports: ProxyPorts,
        language: Language,
    ) -> Result<GnomeSnapshot, SystemProxyError> {
        let snapshot = GnomeSnapshot {
            mode: get("org.gnome.system.proxy", "mode", language)?,
            http_host: get("org.gnome.system.proxy.http", "host", language)?,
            http_port: get("org.gnome.system.proxy.http", "port", language)?,
            https_host: get("org.gnome.system.proxy.https", "host", language)?,
            https_port: get("org.gnome.system.proxy.https", "port", language)?,
            socks_host: get("org.gnome.system.proxy.socks", "host", language)?,
            socks_port: get("org.gnome.system.proxy.socks", "port", language)?,
        };
        write_recovery_snapshot(&encode_snapshot(&snapshot), language)?;
        let apply_result = (|| {
            if let Some(port) = ports.http {
                set(
                    "org.gnome.system.proxy.http",
                    "host",
                    "'127.0.0.1'",
                    language,
                )?;
                set(
                    "org.gnome.system.proxy.http",
                    "port",
                    &port.to_string(),
                    language,
                )?;
                set(
                    "org.gnome.system.proxy.https",
                    "host",
                    "'127.0.0.1'",
                    language,
                )?;
                set(
                    "org.gnome.system.proxy.https",
                    "port",
                    &port.to_string(),
                    language,
                )?;
            }
            if let Some(port) = ports.socks {
                set(
                    "org.gnome.system.proxy.socks",
                    "host",
                    "'127.0.0.1'",
                    language,
                )?;
                set(
                    "org.gnome.system.proxy.socks",
                    "port",
                    &port.to_string(),
                    language,
                )?;
            }
            set("org.gnome.system.proxy", "mode", "'manual'", language)
        })();
        if let Err(error) = apply_result {
            if restore(&snapshot, language).is_err() {
                return Err(super::rollback_failed_message(language));
            }
            delete_recovery_snapshot(language)?;
            return Err(error);
        }
        Ok(snapshot)
    }

    pub(super) fn restore(
        snapshot: &GnomeSnapshot,
        language: Language,
    ) -> Result<(), SystemProxyError> {
        set(
            "org.gnome.system.proxy.http",
            "host",
            &snapshot.http_host,
            language,
        )?;
        set(
            "org.gnome.system.proxy.http",
            "port",
            &snapshot.http_port,
            language,
        )?;
        set(
            "org.gnome.system.proxy.https",
            "host",
            &snapshot.https_host,
            language,
        )?;
        set(
            "org.gnome.system.proxy.https",
            "port",
            &snapshot.https_port,
            language,
        )?;
        set(
            "org.gnome.system.proxy.socks",
            "host",
            &snapshot.socks_host,
            language,
        )?;
        set(
            "org.gnome.system.proxy.socks",
            "port",
            &snapshot.socks_port,
            language,
        )?;
        set("org.gnome.system.proxy", "mode", &snapshot.mode, language)
    }

    pub(super) fn recover_stale(language: Language) -> Result<(), SystemProxyError> {
        let Some(contents) = read_recovery_snapshot(language)? else {
            return Ok(());
        };
        let snapshot = decode_snapshot(&contents).ok_or_else(|| {
            SystemProxyError::CommandFailed(
                language
                    .text(
                        "Manis system proxy recovery snapshot is invalid",
                        "Manis 系统代理恢复快照无效",
                    )
                    .to_owned(),
            )
        })?;
        restore(&snapshot, language)?;
        delete_recovery_snapshot(language)
    }

    fn encode_snapshot(snapshot: &GnomeSnapshot) -> String {
        format!(
            "{RECOVERY_VERSION}\nplatform=linux-gnome\nsnapshot\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
            encode_string(&snapshot.mode),
            encode_string(&snapshot.http_host),
            encode_string(&snapshot.http_port),
            encode_string(&snapshot.https_host),
            encode_string(&snapshot.https_port),
            encode_string(&snapshot.socks_host),
            encode_string(&snapshot.socks_port),
        )
    }

    fn decode_snapshot(contents: &str) -> Option<GnomeSnapshot> {
        let mut lines = contents.lines();
        super::recovery_version_supported(lines.next()?).then_some(())?;
        (lines.next()? == "platform=linux-gnome").then_some(())?;
        let fields: Vec<_> = lines.next()?.split('\t').collect();
        if fields.len() != 8 || fields[0] != "snapshot" {
            return None;
        }
        Some(GnomeSnapshot {
            mode: decode_string(fields[1])?,
            http_host: decode_string(fields[2])?,
            http_port: decode_string(fields[3])?,
            https_host: decode_string(fields[4])?,
            https_port: decode_string(fields[5])?,
            socks_host: decode_string(fields[6])?,
            socks_port: decode_string(fields[7])?,
        })
    }

    fn get(schema: &str, key: &str, language: Language) -> Result<String, SystemProxyError> {
        let output = Command::new("gsettings")
            .args(["get", schema, key])
            .output()
            .map_err(|_| {
                SystemProxyError::Unavailable(
                    language
                        .text(
                            "This desktop does not support gsettings",
                            "当前桌面不支持 gsettings",
                        )
                        .to_owned(),
                )
            })?;
        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
        } else {
            Err(SystemProxyError::CommandFailed(
                language
                    .text(
                        "Could not read GNOME system proxy status",
                        "无法读取 GNOME 系统代理状态",
                    )
                    .to_owned(),
            ))
        }
    }

    fn set(
        schema: &str,
        key: &str,
        value: &str,
        language: Language,
    ) -> Result<(), SystemProxyError> {
        let status = Command::new("gsettings")
            .args(["set", schema, key, value])
            .status()
            .map_err(|_| {
                SystemProxyError::Unavailable(
                    language
                        .text(
                            "This desktop does not support gsettings",
                            "当前桌面不支持 gsettings",
                        )
                        .to_owned(),
                )
            })?;
        status.success().then_some(()).ok_or_else(|| {
            SystemProxyError::CommandFailed(
                language
                    .text(
                        "Could not write GNOME system proxy settings",
                        "无法写入 GNOME 系统代理设置",
                    )
                    .to_owned(),
            )
        })
    }
}

#[cfg(target_os = "windows")]
mod windows {
    use std::process::Command;

    use crate::localization::Language;

    use super::{
        ProxyPorts, RECOVERY_VERSION, SystemProxyError, decode_string, delete_recovery_snapshot,
        encode_string, read_recovery_snapshot, write_recovery_snapshot,
    };

    const INTERNET_SETTINGS: &str =
        r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings";

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub(super) struct WinInetSnapshot {
        enabled: Option<String>,
        server: Option<String>,
    }

    pub(super) fn enable(
        ports: ProxyPorts,
        language: Language,
    ) -> Result<WinInetSnapshot, SystemProxyError> {
        let snapshot = WinInetSnapshot {
            enabled: query("ProxyEnable"),
            server: query("ProxyServer"),
        };
        write_recovery_snapshot(&encode_snapshot(&snapshot), language)?;
        let http = ports.http.or(ports.socks).expect("ports were validated");
        let socks = ports.socks.or(ports.http).expect("ports were validated");
        let apply_result = (|| {
            add(
                "ProxyServer",
                "REG_SZ",
                &format!("http=127.0.0.1:{http};https=127.0.0.1:{http};socks=127.0.0.1:{socks}"),
                language,
            )?;
            add("ProxyEnable", "REG_DWORD", "1", language)?;
            notify(language)
        })();
        if let Err(error) = apply_result {
            if restore(&snapshot, language).is_err() {
                return Err(super::rollback_failed_message(language));
            }
            delete_recovery_snapshot(language)?;
            return Err(error);
        }
        Ok(snapshot)
    }

    pub(super) fn restore(
        snapshot: &WinInetSnapshot,
        language: Language,
    ) -> Result<(), SystemProxyError> {
        match snapshot.server.as_deref() {
            Some(value) => add("ProxyServer", "REG_SZ", value, language)?,
            None => delete("ProxyServer", language)?,
        }
        match snapshot.enabled.as_deref() {
            Some(value) => add("ProxyEnable", "REG_DWORD", value, language)?,
            None => delete("ProxyEnable", language)?,
        }
        notify(language)
    }

    pub(super) fn recover_stale(language: Language) -> Result<(), SystemProxyError> {
        let Some(contents) = read_recovery_snapshot(language)? else {
            return Ok(());
        };
        let snapshot = decode_snapshot(&contents).ok_or_else(|| {
            SystemProxyError::CommandFailed(
                language
                    .text(
                        "Manis system proxy recovery snapshot is invalid",
                        "Manis 系统代理恢复快照无效",
                    )
                    .to_owned(),
            )
        })?;
        restore(&snapshot, language)?;
        delete_recovery_snapshot(language)
    }

    fn encode_snapshot(snapshot: &WinInetSnapshot) -> String {
        format!(
            "{RECOVERY_VERSION}\nplatform=windows-wininet\nsnapshot\t{}\t{}\n",
            encode_optional(snapshot.enabled.as_deref()),
            encode_optional(snapshot.server.as_deref()),
        )
    }

    fn decode_snapshot(contents: &str) -> Option<WinInetSnapshot> {
        let mut lines = contents.lines();
        super::recovery_version_supported(lines.next()?).then_some(())?;
        (lines.next()? == "platform=windows-wininet").then_some(())?;
        let fields: Vec<_> = lines.next()?.split('\t').collect();
        if fields.len() != 3 || fields[0] != "snapshot" {
            return None;
        }
        Some(WinInetSnapshot {
            enabled: decode_optional(fields[1])?,
            server: decode_optional(fields[2])?,
        })
    }

    fn encode_optional(value: Option<&str>) -> String {
        value.map_or_else(|| "-".to_owned(), encode_string)
    }

    fn decode_optional(value: &str) -> Option<Option<String>> {
        if value == "-" {
            Some(None)
        } else {
            decode_string(value).map(Some)
        }
    }

    fn query(name: &str) -> Option<String> {
        let output = Command::new("reg")
            .args(["query", INTERNET_SETTINGS, "/v", name])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .find(|line| line.contains(name))
            .and_then(|line| line.split_whitespace().last())
            .map(str::to_owned)
    }

    fn add(
        name: &str,
        kind: &str,
        value: &str,
        language: Language,
    ) -> Result<(), SystemProxyError> {
        run_reg(
            [
                "add",
                INTERNET_SETTINGS,
                "/v",
                name,
                "/t",
                kind,
                "/d",
                value,
                "/f",
            ],
            language,
        )
    }

    fn delete(name: &str, language: Language) -> Result<(), SystemProxyError> {
        let status = Command::new("reg")
            .args(["delete", INTERNET_SETTINGS, "/v", name, "/f"])
            .status()
            .map_err(|_| {
                SystemProxyError::Unavailable(
                    language
                        .text("Could not start Windows reg", "无法启动 Windows reg")
                        .to_owned(),
                )
            })?;
        if status.success() || status.code() == Some(1) {
            Ok(())
        } else {
            Err(SystemProxyError::CommandFailed(
                language
                    .text(
                        "Could not restore Windows system proxy settings",
                        "无法恢复 Windows 系统代理设置",
                    )
                    .to_owned(),
            ))
        }
    }

    fn run_reg<const N: usize>(
        args: [&str; N],
        language: Language,
    ) -> Result<(), SystemProxyError> {
        let status = Command::new("reg").args(args).status().map_err(|_| {
            SystemProxyError::Unavailable(
                language
                    .text("Could not start Windows reg", "无法启动 Windows reg")
                    .to_owned(),
            )
        })?;
        status.success().then_some(()).ok_or_else(|| {
            SystemProxyError::CommandFailed(
                language
                    .text(
                        "Could not write Windows system proxy settings",
                        "无法写入 Windows 系统代理设置",
                    )
                    .to_owned(),
            )
        })
    }

    fn notify(language: Language) -> Result<(), SystemProxyError> {
        let script = r#"$sig='[DllImport(\"wininet.dll\")]public static extern bool InternetSetOption(IntPtr h,int o,IntPtr b,int l);';Add-Type -MemberDefinition $sig -Name Native -Namespace Manis;[Manis.Native]::InternetSetOption([IntPtr]::Zero,39,[IntPtr]::Zero,0);[Manis.Native]::InternetSetOption([IntPtr]::Zero,37,[IntPtr]::Zero,0)"#;
        let status = Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", script])
            .status()
            .map_err(|_| {
                SystemProxyError::Unavailable(
                    language
                        .text(
                            "Could not notify Windows that proxy settings changed",
                            "无法通知 Windows 代理设置更新",
                        )
                        .to_owned(),
                )
            })?;
        status.success().then_some(()).ok_or_else(|| {
            SystemProxyError::CommandFailed(
                language
                    .text(
                        "Windows proxy was written, but the system refresh failed",
                        "Windows 代理已写入，但系统刷新失败",
                    )
                    .to_owned(),
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::localization::Language;

    use super::{ProxyPorts, SystemProxyError, read_recovery_snapshot_at};

    #[test]
    fn legacy_relay_recovery_version_remains_readable() {
        assert!(super::recovery_version_supported(
            super::LEGACY_RELAY_RECOVERY_VERSION
        ));
        assert!(super::recovery_version_supported(super::RECOVERY_VERSION));
    }

    #[test]
    fn proxy_ports_require_at_least_one_listener() {
        let error = ProxyPorts {
            http: None,
            socks: None,
        }
        .usable_with_language(Language::SimplifiedChinese)
        .expect_err("empty ports must fail closed");

        assert!(matches!(error, SystemProxyError::Unavailable(_)));
    }

    #[cfg(unix)]
    #[test]
    fn recovery_reader_rejects_symbolic_links() {
        use std::os::unix::fs::symlink;
        use std::time::{SystemTime, UNIX_EPOCH};

        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should follow Unix epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "manis-system-proxy-symlink-{}-{nonce}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).expect("create fixture directory");
        let target = root.join("target");
        let link = root.join("system-proxy.recovery");
        std::fs::write(&target, "manis-system-proxy-v1\nplatform=macos\n")
            .expect("write fixture target");
        symlink(&target, &link).expect("create fixture symlink");

        assert!(read_recovery_snapshot_at(&link, Language::English).is_err());

        std::fs::remove_dir_all(root).expect("remove fixture directory");
    }
}
