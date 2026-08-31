use serde_json::{Map, Value, json};

use super::{QuotedYaml, optional};
use crate::{
    LogLevel, Name, OutboundProxy, PolicyGroup, PolicyGroupKind, Profile, ProfileError,
    ProxyDnsServer, ProxyProviderSource, SUBSCRIPTION_METADATA_EXCLUDE_FILTER, VlessProxy,
    VlessSecurity, VlessTransport, policy_name, render_rule,
};

pub(crate) fn render(profile: &Profile, tun_enabled: bool) -> Result<String, ProfileError> {
    let mut providers = Map::new();
    for provider in &profile.providers {
        let mut value = match &provider.source {
            ProxyProviderSource::Http(url) => json!({"type": "http", "url": url.0}),
            ProxyProviderSource::File => json!({"type": "file"}),
        };
        value["path"] = json!(provider.path);
        if matches!(provider.source, ProxyProviderSource::Http(_)) {
            value["interval"] = json!(provider.interval_secs);
        }
        value["exclude-filter"] = json!(SUBSCRIPTION_METADATA_EXCLUDE_FILTER);
        value["proxy"] = json!("DIRECT");
        value["health-check"] = json!({
            "enable": provider.health_check.enabled,
            "url": provider.health_check.url,
            "interval": provider.health_check.interval_secs,
            "timeout": 5000, "lazy": true, "expected-status": 204,
        });
        providers.insert(provider.name.as_str().to_owned(), value);
    }
    let mut tun = json!({"enable": tun_enabled, "stack": "gvisor", "auto-route": true});
    #[cfg(target_os = "linux")]
    {
        tun["device"] = json!(crate::LINUX_TUN_DEVICE);
    }
    // systemd-resolved's loopback stub must not bypass Mihomo's DNS hijack on Linux.
    tun["strict-route"] = json!(cfg!(target_os = "linux"));
    tun["auto-detect-interface"] = json!(true);
    tun["dns-hijack"] = json!(["any:53", "tcp://any:53"]);
    let document = json!({
        "mode": profile.mode.as_mihomo_mode(),
        "unified-delay": true, "find-process-mode": "always", "allow-lan": false,
        "bind-address": "127.0.0.1", "ipv6": false, "mixed-port": profile.mixed_port,
        "log-level": match profile.log_level { LogLevel::Silent => "silent", LogLevel::Warning => "warning", LogLevel::Info => "info" },
        "profile": {"store-selected": profile.store_selected, "store-fake-ip": true},
        "tun": tun,
        "dns": {
            "enable": true, "ipv6": false, "enhanced-mode": "fake-ip",
            "fake-ip-range": "198.18.0.1/16",
            "default-nameserver": ["223.5.5.5", "1.12.12.12"],
            "nameserver": ["https://223.5.5.5/dns-query", "https://1.12.12.12/dns-query"],
            "proxy-server-nameserver": profile.proxy_server_nameservers.iter().map(ProxyDnsServer::as_str).collect::<Vec<_>>(),
        },
        "proxies": profile.proxies.iter().map(|proxy| match proxy { OutboundProxy::Vless(proxy) => vless(proxy) }).collect::<Vec<_>>(),
        "proxy-providers": providers,
        "proxy-groups": profile.groups.iter().map(group).collect::<Vec<_>>(),
        "rules": profile.rules.iter().map(render_rule).collect::<Vec<_>>(),
    });
    let options =
        serde_saphyr::ser_options! { compact_list_indent: false, prefer_block_scalars: false };
    serde_saphyr::to_string_with_options(&QuotedYaml(&document), options)
        .map_err(|_| ProfileError::Serialization("Mihomo YAML"))
}

fn group(group: &PolicyGroup) -> Value {
    let mut value = json!({"name": group.name.as_str()});
    optional(&mut value, "icon", group.icon.as_deref());
    let (proxies, providers, filter) = match &group.kind {
        PolicyGroupKind::Select {
            proxies,
            use_providers,
            filter,
        } => {
            value["type"] = json!("select");
            (proxies, use_providers, filter)
        }
        PolicyGroupKind::UrlTest {
            proxies,
            use_providers,
            filter,
            ..
        } => {
            value["type"] = json!("url-test");
            (proxies, use_providers, filter)
        }
    };
    if !proxies.is_empty() {
        value["proxies"] = json!(proxies.iter().map(policy_name).collect::<Vec<_>>());
    }
    optional(&mut value, "filter", filter.as_deref());
    if !providers.is_empty() {
        value["use"] = json!(providers.iter().map(Name::as_str).collect::<Vec<_>>());
    }
    if let PolicyGroupKind::UrlTest {
        url,
        interval_secs,
        tolerance,
        ..
    } = &group.kind
    {
        value["url"] = json!(url);
        value["interval"] = json!(interval_secs);
        if let Some(tolerance) = tolerance {
            value["tolerance"] = json!(tolerance);
        }
        value["lazy"] = json!(true);
    }
    value
}

fn vless(proxy: &VlessProxy) -> Value {
    let mut value = json!({
        "name": proxy.name.as_str(), "type": "vless", "server": proxy.server,
        "port": proxy.port, "uuid": proxy.uuid, "udp": true,
    });
    optional(&mut value, "flow", proxy.flow.as_deref());
    optional(
        &mut value,
        "packet-encoding",
        proxy.packet_encoding.as_deref(),
    );
    value["network"] = json!(match proxy.transport {
        VlessTransport::Tcp => "tcp",
        VlessTransport::Ws { .. } => "ws",
        VlessTransport::Http { .. } => "http",
        VlessTransport::H2 { .. } => "h2",
        VlessTransport::Grpc { .. } => "grpc",
        VlessTransport::Xhttp { .. } => "xhttp",
    });
    value["tls"] = json!(proxy.security != VlessSecurity::None);
    optional(&mut value, "servername", proxy.servername.as_deref());
    if !proxy.alpn.is_empty() {
        value["alpn"] = json!(proxy.alpn);
    }
    optional(
        &mut value,
        "client-fingerprint",
        proxy.client_fingerprint.as_deref(),
    );
    if proxy.skip_cert_verify {
        value["skip-cert-verify"] = json!(true);
    }
    if proxy.security == VlessSecurity::Reality {
        let mut reality = json!({});
        optional(
            &mut reality,
            "public-key",
            proxy.reality_public_key.as_deref(),
        );
        optional(&mut reality, "short-id", proxy.reality_short_id.as_deref());
        value["reality-opts"] = reality;
    }
    match &proxy.transport {
        VlessTransport::Tcp => {}
        VlessTransport::Ws { path, host } => {
            transport(
                &mut value,
                "ws-opts",
                path.as_deref(),
                host.as_deref(),
                false,
                None,
            );
        }
        VlessTransport::Http { path, host } => {
            transport(
                &mut value,
                "http-opts",
                path.as_deref(),
                host.as_deref(),
                true,
                None,
            );
        }
        VlessTransport::H2 { path, host } => {
            transport(
                &mut value,
                "h2-opts",
                path.as_deref(),
                host.as_deref(),
                true,
                None,
            );
        }
        VlessTransport::Grpc { service_name } => {
            if let Some(name) = service_name {
                value["grpc-opts"] = json!({"grpc-service-name": name});
            }
        }
        VlessTransport::Xhttp { path, host, mode } => {
            transport(
                &mut value,
                "xhttp-opts",
                path.as_deref(),
                host.as_deref(),
                false,
                mode.as_deref(),
            );
        }
    }
    value
}

fn transport(
    document: &mut Value,
    key: &str,
    path: Option<&str>,
    host: Option<&str>,
    lists: bool,
    mode: Option<&str>,
) {
    if path.is_none() && host.is_none() && mode.is_none() {
        return;
    }
    let mut options = json!({});
    if let Some(path) = path {
        options["path"] = if lists { json!([path]) } else { json!(path) };
    }
    if let Some(host) = host {
        options["headers"] = json!({"Host": if lists { json!([host]) } else { json!(host) }});
    }
    optional(&mut options, "mode", mode);
    document[key] = options;
}
