use std::process::Command;

use crate::localization::{Language, copy};

use super::{
    ProxyPorts, RECOVERY_VERSION, SystemProxyError, TUN_DNS_RECOVERY_VERSION, decode_string,
    delete_recovery_snapshot, encode_string, read_recovery_snapshot, write_recovery_snapshot,
    write_tun_dns_recovery_snapshot,
};
#[cfg(not(test))]
use super::{delete_tun_dns_recovery_snapshot, read_tun_dns_recovery_snapshot};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct DnsSnapshot {
    device: String,
}

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

pub(super) fn prepare_tun_dns(language: Language) -> Result<DnsSnapshot, SystemProxyError> {
    let snapshot = DnsSnapshot {
        device: manis_profile::LINUX_TUN_DEVICE.to_owned(),
    };
    write_tun_dns_recovery_snapshot(&encode_dns_snapshot(&snapshot), language)?;
    Ok(snapshot)
}

pub(super) fn apply_tun_dns(
    snapshot: &DnsSnapshot,
    _language: Language,
) -> Result<(), SystemProxyError> {
    if snapshot.device != manis_profile::LINUX_TUN_DEVICE {
        return Err(SystemProxyError::CommandFailed(
            "Linux TUN DNS recovery snapshot names an unexpected interface".to_owned(),
        ));
    }
    crate::linux_privileged::install_tun_dns()
        .map_err(|error| SystemProxyError::CommandFailed(error.to_string()))
}

pub(super) fn restore_tun_dns(
    snapshot: &DnsSnapshot,
    _language: Language,
) -> Result<(), SystemProxyError> {
    if snapshot.device != manis_profile::LINUX_TUN_DEVICE {
        return Err(SystemProxyError::CommandFailed(
            "Linux TUN DNS recovery snapshot names an unexpected interface".to_owned(),
        ));
    }
    crate::linux_privileged::restore_tun_dns()
        .map_err(|error| SystemProxyError::CommandFailed(error.to_string()))
}

#[cfg(not(test))]
pub(super) fn recover_stale_tun_dns(language: Language) -> Result<(), SystemProxyError> {
    let Some(contents) = read_tun_dns_recovery_snapshot(language)? else {
        return Ok(());
    };
    let snapshot = decode_dns_snapshot(&contents).ok_or_else(|| {
        SystemProxyError::CommandFailed(
            language
                .localized(copy::system_proxy::MANIS_TUN_DNS_RECOVERY_SNAPSHOT_IS_INVALID)
                .to_owned(),
        )
    })?;
    restore_tun_dns(&snapshot, language)?;
    delete_tun_dns_recovery_snapshot(language)
}

fn encode_dns_snapshot(snapshot: &DnsSnapshot) -> String {
    format!(
        "{TUN_DNS_RECOVERY_VERSION}\nplatform=linux-resolved\ndevice\t{}\n",
        encode_string(&snapshot.device)
    )
}

fn decode_dns_snapshot(contents: &str) -> Option<DnsSnapshot> {
    let mut lines = contents.lines();
    (lines.next()? == TUN_DNS_RECOVERY_VERSION).then_some(())?;
    (lines.next()? == "platform=linux-resolved").then_some(())?;
    let fields: Vec<_> = lines.next()?.split('\t').collect();
    (fields.len() == 2 && fields[0] == "device").then_some(())?;
    lines.all(|line| line.trim().is_empty()).then_some(())?;
    let device = decode_string(fields[1])?;
    (device == manis_profile::LINUX_TUN_DEVICE).then_some(DnsSnapshot { device })
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
        for (schema, key, value) in gnome_proxy_settings_for_ports(ports) {
            set(schema, key, &value, language)?;
        }
        Ok(())
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

fn gnome_proxy_settings_for_ports(ports: ProxyPorts) -> Vec<(&'static str, &'static str, String)> {
    let (http_host, http_port) = ports
        .http
        .map_or(("''".to_owned(), "0".to_owned()), |port| {
            ("'127.0.0.1'".to_owned(), port.to_string())
        });
    let (socks_host, socks_port) = ports
        .socks
        .map_or(("''".to_owned(), "0".to_owned()), |port| {
            ("'127.0.0.1'".to_owned(), port.to_string())
        });
    vec![
        ("org.gnome.system.proxy.http", "host", http_host.clone()),
        ("org.gnome.system.proxy.http", "port", http_port.clone()),
        ("org.gnome.system.proxy.https", "host", http_host),
        ("org.gnome.system.proxy.https", "port", http_port),
        ("org.gnome.system.proxy.socks", "host", socks_host),
        ("org.gnome.system.proxy.socks", "port", socks_port),
        ("org.gnome.system.proxy", "mode", "'manual'".to_owned()),
    ]
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
                    .localized(copy::system_proxy::THIS_DESKTOP_DOES_NOT_SUPPORT_GSETTINGS)
                    .to_owned(),
            )
        })?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    } else {
        Err(SystemProxyError::CommandFailed(
            language
                .localized(copy::system_proxy::COULD_NOT_READ_GNOME_SYSTEM_PROXY_STATUS)
                .to_owned(),
        ))
    }
}

fn set(schema: &str, key: &str, value: &str, language: Language) -> Result<(), SystemProxyError> {
    let status = Command::new("gsettings")
        .args(["set", schema, key, value])
        .status()
        .map_err(|_| {
            SystemProxyError::Unavailable(
                language
                    .localized(copy::system_proxy::THIS_DESKTOP_DOES_NOT_SUPPORT_GSETTINGS)
                    .to_owned(),
            )
        })?;
    status.success().then_some(()).ok_or_else(|| {
        SystemProxyError::CommandFailed(
            language
                .localized(copy::system_proxy::COULD_NOT_WRITE_GNOME_SYSTEM_PROXY_SETTINGS)
                .to_owned(),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::{
        DnsSnapshot, decode_dns_snapshot, encode_dns_snapshot, gnome_proxy_settings_for_ports,
    };
    use crate::system_proxy::ProxyPorts;

    #[test]
    fn linux_tun_dns_snapshot_is_bound_to_the_managed_interface() {
        let snapshot = DnsSnapshot {
            device: manis_profile::LINUX_TUN_DEVICE.to_owned(),
        };
        let encoded = encode_dns_snapshot(&snapshot);
        assert_eq!(decode_dns_snapshot(&encoded), Some(snapshot));
        assert!(
            decode_dns_snapshot("manis-tun-dns-v1\nplatform=linux-resolved\ndevice\t65746830\n")
                .is_none()
        );
    }

    #[test]
    fn linux_proxy_plan_clears_protocols_without_ports() {
        let http_only = gnome_proxy_settings_for_ports(ProxyPorts {
            http: Some(8080),
            socks: None,
        });
        assert!(http_only.contains(&("org.gnome.system.proxy.http", "port", "8080".to_owned(),)));
        assert!(http_only.contains(&("org.gnome.system.proxy.socks", "host", "''".to_owned(),)));
        assert!(http_only.contains(&("org.gnome.system.proxy.socks", "port", "0".to_owned(),)));

        let socks_only = gnome_proxy_settings_for_ports(ProxyPorts {
            http: None,
            socks: Some(1080),
        });
        assert!(socks_only.contains(&("org.gnome.system.proxy.http", "host", "''".to_owned(),)));
        assert!(socks_only.contains(&("org.gnome.system.proxy.https", "port", "0".to_owned(),)));
        assert!(socks_only.contains(&("org.gnome.system.proxy.socks", "port", "1080".to_owned(),)));
    }
}
