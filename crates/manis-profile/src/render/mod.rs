//! Private wire documents: the typed profile remains the validation boundary.
//! Never log these trees or serializer errors: both may contain subscription/node credentials.

mod mihomo;
mod sing_box;

pub(super) use mihomo::render as mihomo;
pub(super) use sing_box::render as sing_box;

use serde::ser::{Serialize, SerializeMap, SerializeSeq, Serializer};
use serde_json::Value;

/// Preserve the existing human-readable double-quoted values without implementing YAML escaping.
/// Keys and indentation are handled by serde-saphyr; object insertion order is deterministic.
struct QuotedYaml<'a>(&'a Value);

impl Serialize for QuotedYaml<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self.0 {
            Value::String(value) => serde_saphyr::DoubleQuoted(value).serialize(serializer),
            Value::Array(values) => {
                let mut sequence = serializer.serialize_seq(Some(values.len()))?;
                for value in values {
                    sequence.serialize_element(&Self(value))?;
                }
                sequence.end()
            }
            Value::Object(values) => {
                let mut map = serializer.serialize_map(Some(values.len()))?;
                for (key, value) in values {
                    map.serialize_entry(key, &Self(value))?;
                }
                map.end()
            }
            value => value.serialize(serializer),
        }
    }
}

fn optional(document: &mut Value, key: &str, value: Option<&str>) {
    if let Some(value) = value {
        document[key] = value.into();
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        Profile, SecretUrl, SingBoxOptions, VlessProxy, VlessTransport, render_mihomo_yaml,
        render_sing_box_json,
    };
    use serde_json::{Value, json};

    fn proxy() -> VlessProxy {
        VlessProxy::parse_share_link("vless://00000000-0000-4000-8000-000000000000@edge.example.invalid:443?security=tls&type=tcp&sni=cdn.example.invalid#Fixture").unwrap()
    }

    #[test]
    fn yaml_scalar_style_preserves_unicode_escapes_and_ambiguous_strings() {
        let value = json!({"true": ["null", "true", "001", "1e3", "港澳😀", "quote\" slash\\", "line\n\t\r", "\u{0085}\u{2028}\u{2029}"]});
        let yaml = serde_saphyr::to_string(&super::QuotedYaml(&value)).unwrap();
        let decoded: Value = serde_saphyr::from_str(&yaml).unwrap();
        assert_eq!(decoded, value);
    }

    #[test]
    fn mihomo_preserves_each_transport_options_shape() {
        let path = Some("/socket?query=\"quoted\"\\path".to_owned());
        let host = Some("cdn.example.invalid".to_owned());
        let scalar = json!({"path": path, "headers": {"Host": host}});
        let lists = json!({"path": [path], "headers": {"Host": [host]}});
        for (transport, network, key, expected) in [
            (VlessTransport::Tcp, "tcp", "tcp-opts", Value::Null),
            (
                VlessTransport::Ws {
                    path: path.clone(),
                    host: host.clone(),
                },
                "ws",
                "ws-opts",
                scalar.clone(),
            ),
            (
                VlessTransport::Http {
                    path: path.clone(),
                    host: host.clone(),
                },
                "http",
                "http-opts",
                lists.clone(),
            ),
            (
                VlessTransport::H2 {
                    path: path.clone(),
                    host: host.clone(),
                },
                "h2",
                "h2-opts",
                lists,
            ),
            (
                VlessTransport::Grpc {
                    service_name: Some("service".into()),
                },
                "grpc",
                "grpc-opts",
                json!({"grpc-service-name": "service"}),
            ),
            (
                VlessTransport::Xhttp {
                    path,
                    host,
                    mode: Some("auto".into()),
                },
                "xhttp",
                "xhttp-opts",
                json!({"path": scalar["path"], "headers": scalar["headers"], "mode": "auto"}),
            ),
        ] {
            let mut proxy = proxy();
            proxy.transport = transport;
            let profile = Profile::qx_sources(vec![], vec![proxy], 17890).unwrap();
            let yaml = render_mihomo_yaml(&profile).unwrap();
            assert_eq!(yaml, render_mihomo_yaml(&profile).unwrap());
            let document: Value = serde_saphyr::from_str(&yaml).unwrap();
            assert_eq!(document["proxies"][0]["network"], network);
            assert_eq!(document["proxies"][0][key], expected);
            assert_eq!(document["proxies"][0]["port"], 443);
            assert_eq!(document["proxies"][0]["tls"], true);
        }
    }

    #[test]
    fn file_provider_does_not_gain_http_fields_and_dns_keeps_types() {
        let mut profile = Profile::qx_default(
            SecretUrl::parse_https("https://example.invalid/sub?token=fixture").unwrap(),
        )
        .unwrap();
        profile.providers[0].source = crate::ProxyProviderSource::File;
        let name = profile.providers[0].name.as_str();
        let document: Value =
            serde_saphyr::from_str(&render_mihomo_yaml(&profile).unwrap()).unwrap();
        let provider = &document["proxy-providers"][name];
        assert_eq!(provider["type"], "file");
        assert!(provider.get("url").is_none());
        assert!(provider.get("interval").is_none());
        assert_eq!(provider["proxy"], "DIRECT");
        assert_eq!(provider["health-check"]["timeout"], 5000);
        assert_eq!(provider["health-check"]["expected-status"], 204);
        assert_eq!(document["tun"]["strict-route"], cfg!(target_os = "linux"));
        assert_eq!(document["dns"]["enhanced-mode"], "fake-ip");
        assert_eq!(document["profile"]["store-fake-ip"], true);
        assert_eq!(document["dns"]["ipv6"], false);
        assert_eq!(
            document["dns"]["default-nameserver"],
            json!(["223.5.5.5", "1.12.12.12"])
        );
    }

    #[test]
    fn sing_box_preserves_tls_flags_and_controller_secret_escaping() {
        let mut proxy = proxy();
        proxy.alpn = vec!["h2".into(), "http/1.1".into()];
        proxy.client_fingerprint = Some("chrome".into());
        proxy.skip_cert_verify = true;
        proxy.packet_encoding = Some("xudp".into());
        let profile = Profile::qx_sources(vec![], vec![proxy], 17890).unwrap();
        let secret = "fixture-\"quoted\"-\\-港😀";
        let options = SingBoxOptions::new("127.0.0.1:19090", secret);
        let json = render_sing_box_json(&profile, &options).unwrap();
        assert_eq!(json, render_sing_box_json(&profile, &options).unwrap());
        let document: Value = serde_json::from_str(&json).unwrap();
        let outbound = &document["outbounds"][2];
        assert_eq!(outbound["packet_encoding"], "xudp");
        assert_eq!(outbound["tls"]["insecure"], true);
        assert_eq!(outbound["tls"]["alpn"], json!(["h2", "http/1.1"]));
        assert_eq!(
            outbound["tls"]["utls"],
            json!({"enabled": true, "fingerprint": "chrome"})
        );
        assert_eq!(document["experimental"]["clash_api"]["secret"], secret);
        assert!(!format!("{options:?}").contains(secret));
    }
}
