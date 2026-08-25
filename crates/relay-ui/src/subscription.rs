use std::fmt;

use relay_profile::{Profile, SecretUrl, VlessProxy};

pub(crate) const MAX_SUBSCRIPTION_BYTES: usize = 16 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SourceKind {
    HttpSubscription,
    HttpsSubscription,
    VlessNode,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SourceNodePreview {
    pub name: String,
    pub protocol: &'static str,
    pub endpoint: String,
    pub detail: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SubscriptionPreview {
    pub kind: SourceKind,
    pub nodes: Vec<SourceNodePreview>,
}

#[derive(Clone, Eq, PartialEq)]
pub(crate) struct VlessSource {
    value: String,
    preview: SourceNodePreview,
}

impl fmt::Debug for VlessSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("VlessSource(<redacted>)")
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SubscriptionInputError {
    Empty,
    UnsupportedSource,
    TooLong,
    InvalidPreset,
    InvalidVless,
}

impl fmt::Display for SubscriptionInputError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Empty => "请输入订阅链接或 VLESS 节点链接",
            Self::UnsupportedSource => "无法识别；请输入 HTTP/HTTPS 订阅或 vless:// 节点链接",
            Self::TooLong => "来源地址过长，请确认复制的是完整地址",
            Self::InvalidPreset => "订阅地址有效，但无法生成默认策略",
            Self::InvalidVless => "VLESS 链接不完整，请检查 UUID、服务器和端口",
        })
    }
}

pub(crate) fn validate_subscription_preview(
    input: &str,
) -> Result<SubscriptionPreview, SubscriptionInputError> {
    if input.is_empty() {
        return Err(SubscriptionInputError::Empty);
    }
    if input.len() > MAX_SUBSCRIPTION_BYTES {
        return Err(SubscriptionInputError::TooLong);
    }
    if input.starts_with("vless://") {
        return VlessSource::parse(input).map(VlessSource::into_preview);
    }

    let subscription = SecretUrl::parse_subscription(input)
        .map_err(|_error| SubscriptionInputError::UnsupportedSource)?;
    Profile::qx_default(subscription).map_err(|_error| SubscriptionInputError::InvalidPreset)?;
    Ok(SubscriptionPreview {
        kind: if input.starts_with("https://") {
            SourceKind::HttpsSubscription
        } else {
            SourceKind::HttpSubscription
        },
        nodes: Vec::new(),
    })
}

impl VlessSource {
    pub(crate) fn parse(input: &str) -> Result<Self, SubscriptionInputError> {
        VlessProxy::parse_share_link(input)
            .map_err(|_error| SubscriptionInputError::InvalidVless)?;
        let preview = parse_vless_node(input)?;
        Ok(Self {
            value: input.to_owned(),
            preview,
        })
    }

    pub(crate) fn preview(&self) -> &SourceNodePreview {
        &self.preview
    }

    pub(crate) fn expose_to<T>(&self, use_value: impl FnOnce(&str) -> T) -> T {
        use_value(&self.value)
    }

    fn into_preview(self) -> SubscriptionPreview {
        SubscriptionPreview {
            kind: SourceKind::VlessNode,
            nodes: vec![self.preview],
        }
    }
}

fn parse_vless_node(input: &str) -> Result<SourceNodePreview, SubscriptionInputError> {
    if input.trim() != input || input.chars().any(char::is_control) {
        return Err(SubscriptionInputError::InvalidVless);
    }
    let remainder = input
        .strip_prefix("vless://")
        .ok_or(SubscriptionInputError::InvalidVless)?;
    let (without_fragment, fragment) = remainder
        .split_once('#')
        .map_or((remainder, None), |(value, name)| (value, Some(name)));
    let (authority, query) = without_fragment
        .split_once('?')
        .map_or((without_fragment, ""), |(value, query)| (value, query));
    let (uuid, server) = authority
        .split_once('@')
        .ok_or(SubscriptionInputError::InvalidVless)?;
    if !is_uuid(uuid) {
        return Err(SubscriptionInputError::InvalidVless);
    }
    let (host, port) = parse_server(server).ok_or(SubscriptionInputError::InvalidVless)?;
    let endpoint = if host.contains(':') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    };
    let name = fragment
        .and_then(percent_decode)
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| format!("VLESS · {host}"));
    let security = query_value(query, "security").map_or("未声明安全层", |value| {
        if value.eq_ignore_ascii_case("tls") {
            "TLS"
        } else if value.eq_ignore_ascii_case("reality") {
            "REALITY"
        } else if value.eq_ignore_ascii_case("none") {
            "无 TLS"
        } else {
            "自定义安全层"
        }
    });
    let transport = query_value(query, "type").and_then(transport_label);
    let detail = transport.map_or_else(
        || security.to_owned(),
        |value| format!("{security} · {value}"),
    );

    Ok(SourceNodePreview {
        name,
        protocol: "VLESS",
        endpoint,
        detail,
    })
}

fn is_uuid(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(index, byte)| {
            if matches!(index, 8 | 13 | 18 | 23) {
                byte == b'-'
            } else {
                byte.is_ascii_hexdigit()
            }
        })
}

fn parse_server(value: &str) -> Option<(&str, u16)> {
    let (host, port) = if let Some(ipv6) = value.strip_prefix('[') {
        let (host, port) = ipv6.split_once("]:")?;
        (host, port)
    } else {
        value.rsplit_once(':')?
    };
    let port = port.parse::<u16>().ok().filter(|port| *port > 0)?;
    (!host.is_empty()).then_some((host, port))
}

fn query_value<'a>(query: &'a str, key: &str) -> Option<&'a str> {
    query.split('&').find_map(|pair| {
        let (candidate, value) = pair.split_once('=')?;
        candidate.eq_ignore_ascii_case(key).then_some(value)
    })
}

fn transport_label(value: &str) -> Option<&'static str> {
    if value.eq_ignore_ascii_case("ws") {
        Some("WebSocket")
    } else if value.eq_ignore_ascii_case("grpc") {
        Some("gRPC")
    } else if value.eq_ignore_ascii_case("tcp") {
        Some("TCP")
    } else if value.eq_ignore_ascii_case("h2") {
        Some("HTTP/2")
    } else {
        None
    }
}

fn percent_decode(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let high = *bytes.get(index + 1)?;
            let low = *bytes.get(index + 2)?;
            decoded.push((hex_value(high)? << 4) | hex_value(low)?);
            index += 3;
        } else {
            decoded.push(if bytes[index] == b'+' {
                b' '
            } else {
                bytes[index]
            });
            index += 1;
        }
    }
    String::from_utf8(decoded).ok()
}

fn hex_value(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{SourceKind, SubscriptionInputError, VlessSource, validate_subscription_preview};

    #[test]
    fn recognizes_http_and_https_subscription_sources() {
        let preview = validate_subscription_preview(
            "https://subscription.example.invalid/client?token=fixture-secret",
        )
        .expect("fixture HTTPS subscription should validate");

        assert_eq!(preview.kind, SourceKind::HttpsSubscription);
        assert!(preview.nodes.is_empty());

        let http = validate_subscription_preview(
            "http://subscription.example.invalid/client?token=fixture-secret",
        )
        .expect("Mihomo accepts HTTP proxy providers");
        assert_eq!(http.kind, SourceKind::HttpSubscription);
        assert!(http.nodes.is_empty());
    }

    #[test]
    fn recognizes_vless_share_link_as_a_real_single_node_preview() {
        let preview = validate_subscription_preview(
            "vless://00000000-0000-4000-8000-000000000000@edge.example.invalid:443?security=tls&type=ws#Tokyo%20Edge",
        )
        .expect("valid VLESS share link should be recognized");

        assert_eq!(preview.kind, SourceKind::VlessNode);
        assert_eq!(preview.nodes.len(), 1);
        assert_eq!(preview.nodes[0].name, "Tokyo Edge");
        assert_eq!(preview.nodes[0].protocol, "VLESS");
        assert_eq!(preview.nodes[0].endpoint, "edge.example.invalid:443");
        assert_eq!(preview.nodes[0].detail, "TLS · WebSocket");
        assert!(!format!("{preview:?}").contains("00000000"));
    }

    #[test]
    fn vless_source_keeps_credentials_out_of_debug_output() {
        let source = VlessSource::parse(
            "vless://00000000-0000-4000-8000-000000000000@edge.example.invalid:443?security=tls#Saved",
        )
        .expect("fixture VLESS link should parse");

        assert_eq!(source.preview().name, "Saved");
        assert_eq!(format!("{source:?}"), "VlessSource(<redacted>)");
        assert!(!format!("{source:?}").contains("00000000"));
    }

    #[test]
    fn invalid_source_error_never_contains_the_input() {
        let input = "ftp://subscription.example.invalid/private-token";
        let error = validate_subscription_preview(input).expect_err("FTP must be rejected");

        assert_eq!(error, SubscriptionInputError::UnsupportedSource);
        assert!(!format!("{error:?}").contains(input));
        assert!(!error.to_string().contains("private-token"));
    }
}
