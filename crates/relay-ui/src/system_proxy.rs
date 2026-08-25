use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ProxyPorts {
    pub http: Option<u16>,
    pub socks: Option<u16>,
}

impl ProxyPorts {
    pub(crate) fn usable(self) -> Result<Self, SystemProxyError> {
        if self.http.is_none() && self.socks.is_none() {
            Err(SystemProxyError::Unavailable(
                "Mihomo 没有开放 HTTP、mixed 或 SOCKS 端口".to_owned(),
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
    pub(crate) fn enable(&mut self, ports: ProxyPorts) -> Result<(), SystemProxyError> {
        let ports = ports.usable()?;
        if self.applied {
            self.disable()?;
        }

        #[cfg(target_os = "macos")]
        {
            self.previous = macos::enable(ports)?;
        }
        #[cfg(target_os = "linux")]
        {
            self.previous = Some(linux::enable(ports)?);
        }
        #[cfg(target_os = "windows")]
        {
            self.previous = Some(windows::enable(ports)?);
        }
        #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
        {
            let _ = ports;
            return Err(SystemProxyError::Unavailable(
                "当前平台尚未实现系统代理适配器".to_owned(),
            ));
        }

        self.applied = true;
        Ok(())
    }

    pub(crate) fn disable(&mut self) -> Result<(), SystemProxyError> {
        if !self.applied {
            return Ok(());
        }

        #[cfg(target_os = "macos")]
        macos::restore(&self.previous)?;
        #[cfg(target_os = "linux")]
        if let Some(previous) = self.previous.as_ref() {
            linux::restore(previous)?;
        }
        #[cfg(target_os = "windows")]
        if let Some(previous) = self.previous.as_ref() {
            windows::restore(previous)?;
        }

        self.applied = false;
        Ok(())
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use std::process::Command;

    use super::{ProxyPorts, SystemProxyError};

    #[derive(Debug)]
    pub(super) struct ServiceSnapshot {
        name: String,
        web: ProxySetting,
        secure_web: ProxySetting,
        socks: ProxySetting,
    }

    #[derive(Debug)]
    struct ProxySetting {
        enabled: bool,
        server: String,
        port: u16,
    }

    pub(super) fn enable(ports: ProxyPorts) -> Result<Vec<ServiceSnapshot>, SystemProxyError> {
        let services = output(&["-listallnetworkservices"])?;
        let services: Vec<_> = services
            .lines()
            .skip(1)
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('*'))
            .map(str::to_owned)
            .collect();
        if services.is_empty() {
            return Err(SystemProxyError::Unavailable(
                "macOS 没有可配置的网络服务".to_owned(),
            ));
        }

        let mut snapshots = Vec::with_capacity(services.len());
        for service in services {
            let snapshot = ServiceSnapshot {
                web: read_setting("-getwebproxy", &service)?,
                secure_web: read_setting("-getsecurewebproxy", &service)?,
                socks: read_setting("-getsocksfirewallproxy", &service)?,
                name: service.clone(),
            };
            snapshots.push(snapshot);
            if let Err(error) = apply_service(&service, ports) {
                let _ = restore(&snapshots);
                return Err(error);
            }
        }
        Ok(snapshots)
    }

    fn apply_service(service: &str, ports: ProxyPorts) -> Result<(), SystemProxyError> {
        if let Some(port) = ports.http {
            run(&["-setwebproxy", service, "127.0.0.1", &port.to_string()])?;
            run(&[
                "-setsecurewebproxy",
                service,
                "127.0.0.1",
                &port.to_string(),
            ])?;
        } else {
            run(&["-setwebproxystate", service, "off"])?;
            run(&["-setsecurewebproxystate", service, "off"])?;
        }
        if let Some(port) = ports.socks {
            run(&[
                "-setsocksfirewallproxy",
                service,
                "127.0.0.1",
                &port.to_string(),
            ])?;
        } else {
            run(&["-setsocksfirewallproxystate", service, "off"])?;
        }
        Ok(())
    }

    pub(super) fn restore(previous: &[ServiceSnapshot]) -> Result<(), SystemProxyError> {
        for service in previous {
            restore_setting(
                &service.name,
                "-setwebproxy",
                "-setwebproxystate",
                &service.web,
            )?;
            restore_setting(
                &service.name,
                "-setsecurewebproxy",
                "-setsecurewebproxystate",
                &service.secure_web,
            )?;
            restore_setting(
                &service.name,
                "-setsocksfirewallproxy",
                "-setsocksfirewallproxystate",
                &service.socks,
            )?;
        }
        Ok(())
    }

    fn read_setting(command: &str, service: &str) -> Result<ProxySetting, SystemProxyError> {
        let value = output(&[command, service])?;
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
    ) -> Result<(), SystemProxyError> {
        if !setting.server.is_empty() && setting.port > 0 {
            run(&[
                set_command,
                service,
                &setting.server,
                &setting.port.to_string(),
            ])?;
        }
        run(&[
            state_command,
            service,
            if setting.enabled { "on" } else { "off" },
        ])
    }

    fn run(args: &[&str]) -> Result<(), SystemProxyError> {
        let status = Command::new("networksetup")
            .args(args)
            .status()
            .map_err(|_| SystemProxyError::Unavailable("无法启动 macOS networksetup".to_owned()))?;
        if status.success() {
            Ok(())
        } else {
            Err(SystemProxyError::CommandFailed(format!(
                "macOS 系统代理命令失败（退出码 {}）",
                status.code().unwrap_or(-1)
            )))
        }
    }

    fn output(args: &[&str]) -> Result<String, SystemProxyError> {
        let output = Command::new("networksetup")
            .args(args)
            .output()
            .map_err(|_| SystemProxyError::Unavailable("无法启动 macOS networksetup".to_owned()))?;
        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).into_owned())
        } else {
            Err(SystemProxyError::CommandFailed(
                "无法读取 macOS 系统代理状态".to_owned(),
            ))
        }
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use std::process::Command;

    use super::{ProxyPorts, SystemProxyError};

    #[derive(Debug)]
    pub(super) struct GnomeSnapshot {
        mode: String,
        http_host: String,
        http_port: String,
        https_host: String,
        https_port: String,
        socks_host: String,
        socks_port: String,
    }

    pub(super) fn enable(ports: ProxyPorts) -> Result<GnomeSnapshot, SystemProxyError> {
        let snapshot = GnomeSnapshot {
            mode: get("org.gnome.system.proxy", "mode")?,
            http_host: get("org.gnome.system.proxy.http", "host")?,
            http_port: get("org.gnome.system.proxy.http", "port")?,
            https_host: get("org.gnome.system.proxy.https", "host")?,
            https_port: get("org.gnome.system.proxy.https", "port")?,
            socks_host: get("org.gnome.system.proxy.socks", "host")?,
            socks_port: get("org.gnome.system.proxy.socks", "port")?,
        };
        if let Some(port) = ports.http {
            set("org.gnome.system.proxy.http", "host", "'127.0.0.1'")?;
            set("org.gnome.system.proxy.http", "port", &port.to_string())?;
            set("org.gnome.system.proxy.https", "host", "'127.0.0.1'")?;
            set("org.gnome.system.proxy.https", "port", &port.to_string())?;
        }
        if let Some(port) = ports.socks {
            set("org.gnome.system.proxy.socks", "host", "'127.0.0.1'")?;
            set("org.gnome.system.proxy.socks", "port", &port.to_string())?;
        }
        set("org.gnome.system.proxy", "mode", "'manual'")?;
        Ok(snapshot)
    }

    pub(super) fn restore(snapshot: &GnomeSnapshot) -> Result<(), SystemProxyError> {
        set("org.gnome.system.proxy.http", "host", &snapshot.http_host)?;
        set("org.gnome.system.proxy.http", "port", &snapshot.http_port)?;
        set("org.gnome.system.proxy.https", "host", &snapshot.https_host)?;
        set("org.gnome.system.proxy.https", "port", &snapshot.https_port)?;
        set("org.gnome.system.proxy.socks", "host", &snapshot.socks_host)?;
        set("org.gnome.system.proxy.socks", "port", &snapshot.socks_port)?;
        set("org.gnome.system.proxy", "mode", &snapshot.mode)
    }

    fn get(schema: &str, key: &str) -> Result<String, SystemProxyError> {
        let output = Command::new("gsettings")
            .args(["get", schema, key])
            .output()
            .map_err(|_| SystemProxyError::Unavailable("当前桌面不支持 gsettings".to_owned()))?;
        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
        } else {
            Err(SystemProxyError::CommandFailed(
                "无法读取 GNOME 系统代理状态".to_owned(),
            ))
        }
    }

    fn set(schema: &str, key: &str, value: &str) -> Result<(), SystemProxyError> {
        let status = Command::new("gsettings")
            .args(["set", schema, key, value])
            .status()
            .map_err(|_| SystemProxyError::Unavailable("当前桌面不支持 gsettings".to_owned()))?;
        status.success().then_some(()).ok_or_else(|| {
            SystemProxyError::CommandFailed("无法写入 GNOME 系统代理设置".to_owned())
        })
    }
}

#[cfg(target_os = "windows")]
mod windows {
    use std::process::Command;

    use super::{ProxyPorts, SystemProxyError};

    const INTERNET_SETTINGS: &str =
        r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings";

    #[derive(Debug)]
    pub(super) struct WinInetSnapshot {
        enabled: Option<String>,
        server: Option<String>,
    }

    pub(super) fn enable(ports: ProxyPorts) -> Result<WinInetSnapshot, SystemProxyError> {
        let snapshot = WinInetSnapshot {
            enabled: query("ProxyEnable"),
            server: query("ProxyServer"),
        };
        let http = ports.http.or(ports.socks).expect("ports were validated");
        let socks = ports.socks.or(ports.http).expect("ports were validated");
        add(
            "ProxyServer",
            "REG_SZ",
            &format!("http=127.0.0.1:{http};https=127.0.0.1:{http};socks=127.0.0.1:{socks}"),
        )?;
        add("ProxyEnable", "REG_DWORD", "1")?;
        notify()?;
        Ok(snapshot)
    }

    pub(super) fn restore(snapshot: &WinInetSnapshot) -> Result<(), SystemProxyError> {
        match snapshot.server.as_deref() {
            Some(value) => add("ProxyServer", "REG_SZ", value)?,
            None => delete("ProxyServer")?,
        }
        match snapshot.enabled.as_deref() {
            Some(value) => add("ProxyEnable", "REG_DWORD", value)?,
            None => delete("ProxyEnable")?,
        }
        notify()
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

    fn add(name: &str, kind: &str, value: &str) -> Result<(), SystemProxyError> {
        run_reg([
            "add",
            INTERNET_SETTINGS,
            "/v",
            name,
            "/t",
            kind,
            "/d",
            value,
            "/f",
        ])
    }

    fn delete(name: &str) -> Result<(), SystemProxyError> {
        let status = Command::new("reg")
            .args(["delete", INTERNET_SETTINGS, "/v", name, "/f"])
            .status()
            .map_err(|_| SystemProxyError::Unavailable("无法启动 Windows reg".to_owned()))?;
        if status.success() || status.code() == Some(1) {
            Ok(())
        } else {
            Err(SystemProxyError::CommandFailed(
                "无法恢复 Windows 系统代理设置".to_owned(),
            ))
        }
    }

    fn run_reg<const N: usize>(args: [&str; N]) -> Result<(), SystemProxyError> {
        let status = Command::new("reg")
            .args(args)
            .status()
            .map_err(|_| SystemProxyError::Unavailable("无法启动 Windows reg".to_owned()))?;
        status.success().then_some(()).ok_or_else(|| {
            SystemProxyError::CommandFailed("无法写入 Windows 系统代理设置".to_owned())
        })
    }

    fn notify() -> Result<(), SystemProxyError> {
        let script = r#"$sig='[DllImport(\"wininet.dll\")]public static extern bool InternetSetOption(IntPtr h,int o,IntPtr b,int l);';Add-Type -MemberDefinition $sig -Name Native -Namespace Relay;[Relay.Native]::InternetSetOption([IntPtr]::Zero,39,[IntPtr]::Zero,0);[Relay.Native]::InternetSetOption([IntPtr]::Zero,37,[IntPtr]::Zero,0)"#;
        let status = Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", script])
            .status()
            .map_err(|_| {
                SystemProxyError::Unavailable("无法通知 Windows 代理设置更新".to_owned())
            })?;
        status.success().then_some(()).ok_or_else(|| {
            SystemProxyError::CommandFailed("Windows 代理已写入，但系统刷新失败".to_owned())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{ProxyPorts, SystemProxyError};

    #[test]
    fn proxy_ports_require_at_least_one_listener() {
        let error = ProxyPorts {
            http: None,
            socks: None,
        }
        .usable()
        .expect_err("empty ports must fail closed");

        assert!(matches!(error, SystemProxyError::Unavailable(_)));
    }
}
