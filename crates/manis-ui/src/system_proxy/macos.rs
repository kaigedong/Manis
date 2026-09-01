use std::net::IpAddr;
use std::process::{Command, Stdio};

use crate::localization::{Language, copy};

use super::{
    ProxyPorts, RECOVERY_VERSION, SystemProxyError, TUN_DNS_RECOVERY_VERSION, decode_string,
    delete_recovery_snapshot, delete_recovery_snapshot_at, encode_string, write_recovery_snapshot,
    write_recovery_snapshot_at, write_tun_dns_recovery_snapshot,
};
#[cfg(not(test))]
use super::{
    delete_tun_dns_recovery_snapshot, read_recovery_snapshot, read_tun_dns_recovery_snapshot,
};

const TUN_DNS_SERVER: &str = "114.114.114.114";

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ServiceSnapshot {
    name: String,
    web: ProxySetting,
    secure_web: ProxySetting,
    socks: ProxySetting,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct DnsSnapshot {
    service: String,
    servers: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ProxySetting {
    enabled: bool,
    server: String,
    port: u16,
}

pub(super) fn prepare_tun_dns(language: Language) -> Result<DnsSnapshot, SystemProxyError> {
    let interface = default_interface(language)?;
    let mut runner = NetworkSetupRunner;
    prepare_tun_dns_with_runner(&interface, language, &mut runner)
}

fn prepare_tun_dns_with_runner(
    interface: &str,
    language: Language,
    runner: &mut impl CommandRunner,
) -> Result<DnsSnapshot, SystemProxyError> {
    prepare_tun_dns_with_runner_at(interface, language, runner, None)
}

fn prepare_tun_dns_with_runner_at(
    interface: &str,
    language: Language,
    runner: &mut impl CommandRunner,
    recovery_path: Option<&std::path::Path>,
) -> Result<DnsSnapshot, SystemProxyError> {
    let ordering = runner.output(&["-listnetworkserviceorder"], language)?;
    let service = network_service_for_interface(&ordering, interface).ok_or_else(|| {
        SystemProxyError::Unavailable(copy::system_proxy::unmapped_macos_interface(
            language, interface,
        ))
    })?;
    let servers = parse_dns_servers(
        &runner.output(&["-getdnsservers", &service], language)?,
        language,
    )?;
    let snapshot = DnsSnapshot { service, servers };
    write_tun_dns_recovery(&encode_dns_snapshot(&snapshot), language, recovery_path)?;
    Ok(snapshot)
}

pub(super) fn apply_tun_dns(
    snapshot: &DnsSnapshot,
    language: Language,
) -> Result<(), SystemProxyError> {
    apply_tun_dns_with_runner(snapshot, language, &mut NetworkSetupRunner)
}

fn apply_tun_dns_with_runner(
    snapshot: &DnsSnapshot,
    language: Language,
    runner: &mut impl CommandRunner,
) -> Result<(), SystemProxyError> {
    runner.run(
        &["-setdnsservers", &snapshot.service, TUN_DNS_SERVER],
        language,
    )
}

pub(super) fn restore_tun_dns(
    snapshot: &DnsSnapshot,
    language: Language,
) -> Result<(), SystemProxyError> {
    restore_tun_dns_with_runner(snapshot, language, &mut NetworkSetupRunner)
}

fn restore_tun_dns_with_runner(
    snapshot: &DnsSnapshot,
    language: Language,
    runner: &mut impl CommandRunner,
) -> Result<(), SystemProxyError> {
    let mut args = vec!["-setdnsservers", snapshot.service.as_str()];
    if snapshot.servers.is_empty() {
        args.push("Empty");
    } else {
        args.extend(snapshot.servers.iter().map(String::as_str));
    }
    runner.run(&args, language)
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

#[cfg(test)]
fn recover_stale_tun_dns_at(
    path: &std::path::Path,
    language: Language,
    runner: &mut impl CommandRunner,
) -> Result<(), SystemProxyError> {
    let Some(contents) = super::read_recovery_snapshot_at(path, language)? else {
        return Ok(());
    };
    let snapshot = decode_dns_snapshot(&contents).ok_or_else(|| {
        SystemProxyError::CommandFailed("invalid TUN DNS recovery snapshot".to_owned())
    })?;
    restore_tun_dns_with_runner(&snapshot, language, runner)?;
    delete_recovery_snapshot_at(path, language)
}

fn default_interface(language: Language) -> Result<String, SystemProxyError> {
    let output = Command::new("/sbin/route")
        .args(["-n", "get", "default"])
        .env_clear()
        .stdin(Stdio::null())
        .output()
        .map_err(|_| {
            SystemProxyError::Unavailable(
                language
                    .localized(copy::system_proxy::COULD_NOT_INSPECT_THE_MACOS_DEFAULT_ROUTE)
                    .to_owned(),
            )
        })?;
    if !output.status.success() {
        return Err(SystemProxyError::Unavailable(
            language
                .localized(copy::system_proxy::COULD_NOT_INSPECT_THE_MACOS_DEFAULT_ROUTE)
                .to_owned(),
        ));
    }
    let output = String::from_utf8_lossy(&output.stdout);
    output
        .lines()
        .find_map(|line| line.trim().strip_prefix("interface:").map(str::trim))
        .filter(|interface| !interface.is_empty() && !interface.chars().any(char::is_whitespace))
        .map(str::to_owned)
        .ok_or_else(|| {
            SystemProxyError::Unavailable(
                language
                    .localized(
                        copy::system_proxy::THE_MACOS_DEFAULT_ROUTE_DID_NOT_IDENTIFY_AN_INTERFACE,
                    )
                    .to_owned(),
            )
        })
}

fn network_service_for_interface(ordering: &str, interface: &str) -> Option<String> {
    let mut service = None;
    for line in ordering.lines().map(str::trim) {
        if line.starts_with('(')
            && line.as_bytes().get(1).is_some_and(u8::is_ascii_digit)
            && let Some((_index, name)) = line.split_once(") ")
        {
            service = Some(name.trim().to_owned());
            continue;
        }
        let Some(device) = line
            .split_once("Device:")
            .map(|(_prefix, value)| value.trim().trim_end_matches(')'))
        else {
            continue;
        };
        if device == interface {
            return service;
        }
    }
    None
}

fn parse_dns_servers(output: &str, language: Language) -> Result<Vec<String>, SystemProxyError> {
    if output
        .trim()
        .starts_with("There aren't any DNS Servers set on")
    {
        return Ok(Vec::new());
    }
    let mut servers = Vec::new();
    for value in output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        value.parse::<IpAddr>().map_err(|_| {
            SystemProxyError::CommandFailed(
                language
                    .localized(copy::system_proxy::MACOS_RETURNED_AN_INVALID_DNS_SERVER_ADDRESS)
                    .to_owned(),
            )
        })?;
        servers.push(value.to_owned());
    }
    Ok(servers)
}

fn encode_dns_snapshot(snapshot: &DnsSnapshot) -> String {
    let mut output = format!(
        "{TUN_DNS_RECOVERY_VERSION}\nplatform=macos\nservice\t{}",
        encode_string(&snapshot.service)
    );
    for server in &snapshot.servers {
        output.push('\t');
        output.push_str(&encode_string(server));
    }
    output.push('\n');
    output
}

fn decode_dns_snapshot(contents: &str) -> Option<DnsSnapshot> {
    let mut lines = contents.lines();
    (lines.next()? == TUN_DNS_RECOVERY_VERSION).then_some(())?;
    (lines.next()? == "platform=macos").then_some(())?;
    let fields: Vec<_> = lines.next()?.split('\t').collect();
    (fields.len() >= 2 && fields[0] == "service").then_some(())?;
    let servers = fields[2..]
        .iter()
        .map(|value| decode_string(value))
        .collect::<Option<Vec<_>>>()?;
    servers
        .iter()
        .all(|server| server.parse::<IpAddr>().is_ok())
        .then_some(())?;
    lines.all(|line| line.trim().is_empty()).then_some(())?;
    Some(DnsSnapshot {
        service: decode_string(fields[1])?,
        servers,
    })
}

fn write_tun_dns_recovery(
    contents: &str,
    language: Language,
    path: Option<&std::path::Path>,
) -> Result<(), SystemProxyError> {
    if let Some(path) = path {
        write_recovery_snapshot_at(path, contents, language)
    } else {
        write_tun_dns_recovery_snapshot(contents, language)
    }
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
                .localized(copy::system_proxy::MACOS_HAS_NO_CONFIGURABLE_NETWORK_SERVICES)
                .to_owned(),
        ));
    }

    let mut snapshots = Vec::with_capacity(services.len());
    for service in &services {
        snapshots.push(ServiceSnapshot {
            web: read_setting_with_runner("-getwebproxy", service, language, runner)?,
            secure_web: read_setting_with_runner("-getsecurewebproxy", service, language, runner)?,
            socks: read_setting_with_runner("-getsocksfirewallproxy", service, language, runner)?,
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

#[cfg(not(test))]
pub(super) fn recover_stale(language: Language) -> Result<(), SystemProxyError> {
    let Some(contents) = read_recovery_snapshot(language)? else {
        return Ok(());
    };
    let snapshots = decode_snapshots(&contents).ok_or_else(|| {
        SystemProxyError::CommandFailed(
            language
                .localized(copy::system_proxy::MANIS_SYSTEM_PROXY_RECOVERY_SNAPSHOT_IS_INVALID)
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
    let snapshots = decode_snapshots(&contents)
        .ok_or_else(|| SystemProxyError::CommandFailed("invalid recovery snapshot".to_owned()))?;
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
    fn output(&mut self, args: &[&str], language: Language) -> Result<String, SystemProxyError>;
}

struct NetworkSetupRunner;

impl CommandRunner for NetworkSetupRunner {
    fn run(&mut self, args: &[&str], language: Language) -> Result<(), SystemProxyError> {
        let status = Command::new("/usr/sbin/networksetup")
            .args(args)
            .env_clear()
            .stdin(Stdio::null())
            .status()
            .map_err(|_| {
                SystemProxyError::Unavailable(
                    language
                        .localized(copy::system_proxy::COULD_NOT_START_MACOS_NETWORKSETUP)
                        .to_owned(),
                )
            })?;
        if status.success() {
            Ok(())
        } else {
            let code = status.code().unwrap_or(-1);
            Err(SystemProxyError::CommandFailed(
                copy::system_proxy::macos_command_failed(language, code),
            ))
        }
    }

    fn output(&mut self, args: &[&str], language: Language) -> Result<String, SystemProxyError> {
        let output = Command::new("/usr/sbin/networksetup")
            .args(args)
            .env_clear()
            .stdin(Stdio::null())
            .output()
            .map_err(|_| {
                SystemProxyError::Unavailable(
                    language
                        .localized(copy::system_proxy::COULD_NOT_START_MACOS_NETWORKSETUP)
                        .to_owned(),
                )
            })?;
        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).into_owned())
        } else {
            Err(SystemProxyError::CommandFailed(
                language
                    .localized(copy::system_proxy::COULD_NOT_READ_MACOS_SYSTEM_PROXY_STATUS)
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
        CommandRunner, DnsSnapshot, ProxyPorts, ProxySetting, ServiceSnapshot, SystemProxyError,
        apply_tun_dns_with_runner, decode_dns_snapshot, decode_snapshots, enable_with_runner_at,
        encode_dns_snapshot, encode_snapshots, network_service_for_interface,
        prepare_tun_dns_with_runner_at, recover_stale_at, recover_stale_tun_dns_at,
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
                .ok_or_else(|| SystemProxyError::CommandFailed("missing fake output".to_owned()))
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
    fn macos_tun_dns_snapshot_roundtrips_and_rejects_non_ip_servers() {
        let snapshot = DnsSnapshot {
            service: "Wi-Fi".to_owned(),
            servers: vec!["1.1.1.1".to_owned(), "2001:4860:4860::8888".to_owned()],
        };

        let encoded = encode_dns_snapshot(&snapshot);
        assert_eq!(decode_dns_snapshot(&encoded), Some(snapshot));
        assert!(
            decode_dns_snapshot(
                "manis-tun-dns-v1\nplatform=macos\nservice\t57692d4669\t6e6f742d616e2d6970\n"
            )
            .is_none()
        );
    }

    #[test]
    fn macos_tun_dns_maps_the_default_interface_to_its_service() {
        let ordering = concat!(
            "An asterisk (*) denotes that a network service is disabled.\n",
            "(1) USB 10/100/1000 LAN\n",
            "(Hardware Port: USB 10/100/1000 LAN, Device: en4)\n",
            "(2) Wi-Fi\n",
            "(Hardware Port: Wi-Fi, Device: en1)\n",
        );

        assert_eq!(
            network_service_for_interface(ordering, "en1").as_deref(),
            Some("Wi-Fi")
        );
        assert_eq!(network_service_for_interface(ordering, "en9"), None);
    }

    #[test]
    fn macos_tun_dns_prepares_recovery_before_setting_public_dns() {
        let root = test_directory("manis-tun-dns-enable");
        let recovery = root.join("tun-dns.recovery");
        let mut runner = fake_dns_runner("1.1.1.1\n8.8.8.8\n");

        let snapshot =
            prepare_tun_dns_with_runner_at("en1", Language::English, &mut runner, Some(&recovery))
                .expect("TUN DNS should use fake runner");

        let written = std::fs::read_to_string(&recovery).expect("recovery file should exist");
        assert_eq!(decode_dns_snapshot(&written).as_ref(), Some(&snapshot));
        assert!(runner.runs.is_empty());

        apply_tun_dns_with_runner(&snapshot, Language::English, &mut runner)
            .expect("TUN DNS should use fake runner");
        assert_eq!(
            runner.runs,
            vec![vec![
                "-setdnsservers".to_owned(),
                "Wi-Fi".to_owned(),
                "114.114.114.114".to_owned(),
            ]]
        );

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn macos_tun_dns_stale_recovery_restores_automatic_dns() {
        let root = test_directory("manis-tun-dns-recover");
        let recovery = root.join("tun-dns.recovery");
        let snapshot = DnsSnapshot {
            service: "Wi-Fi".to_owned(),
            servers: Vec::new(),
        };
        super::write_recovery_snapshot_at(
            &recovery,
            &encode_dns_snapshot(&snapshot),
            Language::English,
        )
        .expect("recovery write should succeed");
        let mut runner = FakeRunner::default();

        recover_stale_tun_dns_at(&recovery, Language::English, &mut runner)
            .expect("stale DNS recovery should restore through fake runner");

        assert!(!recovery.exists());
        assert_eq!(
            runner.runs,
            vec![vec![
                "-setdnsservers".to_owned(),
                "Wi-Fi".to_owned(),
                "Empty".to_owned(),
            ]]
        );

        let _ = std::fs::remove_dir_all(root);
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

    fn fake_dns_runner(current_dns: &str) -> FakeRunner {
        FakeRunner::default()
            .with_output(
                &["-listnetworkserviceorder"],
                concat!(
                    "An asterisk (*) denotes that a network service is disabled.\n",
                    "(1) Wi-Fi\n",
                    "(Hardware Port: Wi-Fi, Device: en1)\n",
                ),
            )
            .with_output(&["-getdnsservers", "Wi-Fi"], current_dns)
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
