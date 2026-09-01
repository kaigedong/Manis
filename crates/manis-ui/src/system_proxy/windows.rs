use std::process::Command;

use crate::localization::{Language, copy};

use super::{
    ProxyPorts, RECOVERY_VERSION, SystemProxyError, decode_string, delete_recovery_snapshot,
    encode_string, read_recovery_snapshot, write_recovery_snapshot,
};

const INTERNET_SETTINGS: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings";

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
        enabled: query("ProxyEnable", language)?,
        server: query("ProxyServer", language)?,
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
                .localized(copy::system_proxy::MANIS_SYSTEM_PROXY_RECOVERY_SNAPSHOT_IS_INVALID)
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

fn query(name: &str, language: Language) -> Result<Option<String>, SystemProxyError> {
    let output = Command::new("reg")
        .args(["query", INTERNET_SETTINGS, "/v", name])
        .output()
        .map_err(|_| {
            SystemProxyError::Unavailable(
                language
                    .localized(copy::system_proxy::COULD_NOT_START_WINDOWS_REG)
                    .to_owned(),
            )
        })?;
    if !output.status.success() {
        if output.status.code() == Some(1) {
            return Ok(None);
        }
        return Err(SystemProxyError::CommandFailed(
            "could not read Windows system proxy status".to_owned(),
        ));
    }
    let value = String::from_utf8_lossy(&output.stdout)
        .lines()
        .find(|line| line.contains(name))
        .and_then(|line| line.split_whitespace().last())
        .map(str::to_owned)
        .ok_or_else(|| {
            SystemProxyError::CommandFailed("could not read Windows system proxy status".to_owned())
        })?;
    Ok(Some(value))
}

fn add(name: &str, kind: &str, value: &str, language: Language) -> Result<(), SystemProxyError> {
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
                    .localized(copy::system_proxy::COULD_NOT_START_WINDOWS_REG)
                    .to_owned(),
            )
        })?;
    if status.success() || status.code() == Some(1) {
        Ok(())
    } else {
        Err(SystemProxyError::CommandFailed(
            language
                .localized(copy::system_proxy::COULD_NOT_RESTORE_WINDOWS_SYSTEM_PROXY_SETTINGS)
                .to_owned(),
        ))
    }
}

fn run_reg<const N: usize>(args: [&str; N], language: Language) -> Result<(), SystemProxyError> {
    let status = Command::new("reg").args(args).status().map_err(|_| {
        SystemProxyError::Unavailable(
            language
                .localized(copy::system_proxy::COULD_NOT_START_WINDOWS_REG)
                .to_owned(),
        )
    })?;
    status.success().then_some(()).ok_or_else(|| {
        SystemProxyError::CommandFailed(
            language
                .localized(copy::system_proxy::COULD_NOT_WRITE_WINDOWS_SYSTEM_PROXY_SETTINGS)
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
                    .localized(
                        copy::system_proxy::COULD_NOT_NOTIFY_WINDOWS_THAT_PROXY_SETTINGS_CHANGED,
                    )
                    .to_owned(),
            )
        })?;
    status.success().then_some(()).ok_or_else(|| {
        SystemProxyError::CommandFailed(
            language
                .localized(
                    copy::system_proxy::WINDOWS_PROXY_WAS_WRITTEN_BUT_THE_SYSTEM_REFRESH_FAILED,
                )
                .to_owned(),
        )
    })
}
