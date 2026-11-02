use std::fmt;

use manis_profile::{Profile, SecretUrl, VlessProxy};

pub(crate) const MAX_SUBSCRIPTION_BYTES: usize = 16 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SourceKind {
    HttpSubscription,
    HttpsSubscription,
    SingleNode,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SourceNodePreview {
    pub name: String,
    pub protocol: &'static str,
    pub endpoint: String,
    pub detail: SourceNodeDetail,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SourceNodeDetail {
    SingleNode,
    Vless {
        security: SourceNodeSecurity,
        transport: Option<&'static str>,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SourceNodeSecurity {
    Unspecified,
    Tls,
    Reality,
    None,
    Custom,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SubscriptionPreview {
    pub kind: SourceKind,
    pub nodes: Vec<SourceNodePreview>,
}

#[derive(Clone, Eq, PartialEq)]
pub(crate) struct SingleNodeSource {
    value: String,
    preview: SourceNodePreview,
}

impl fmt::Debug for SingleNodeSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SingleNodeSource(<redacted>)")
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SubscriptionInputError {
    Empty,
    UnsupportedSource,
    TooLong,
    InvalidPreset,
    InvalidVless,
    InvalidSingleNode,
}

impl fmt::Display for SubscriptionInputError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Empty => "enter a subscription URL or single-node share link",
            Self::UnsupportedSource => {
                "unsupported source; expected an HTTP/HTTPS subscription or single-node share link"
            }
            Self::TooLong => "source URL exceeds the supported length",
            Self::InvalidPreset => "subscription URL is valid but its default profile is not",
            Self::InvalidVless | Self::InvalidSingleNode => "single-node share link is invalid",
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

pub(crate) fn validate_single_node_preview(
    input: &str,
) -> Result<SubscriptionPreview, SubscriptionInputError> {
    SingleNodeSource::parse(input).map(SingleNodeSource::into_preview)
}

impl SingleNodeSource {
    pub(crate) fn parse(input: &str) -> Result<Self, SubscriptionInputError> {
        let preview = if input.starts_with("vless://") {
            VlessProxy::parse_share_link(input)
                .map_err(|_error| SubscriptionInputError::InvalidVless)?;
            parse_vless_node(input)?
        } else {
            parse_generic_single_node(input)?
        };
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

    #[cfg(test)]
    pub(crate) fn input_with_name(
        input: &str,
        name: &str,
    ) -> Result<String, SubscriptionInputError> {
        let name = name.trim();
        if name.is_empty() || name.len() > 96 || name.chars().any(char::is_control) {
            return Err(SubscriptionInputError::InvalidVless);
        }
        Self::parse(input)?;
        if input.starts_with("vmess://") || input.starts_with("ssr://") {
            return Ok(input.to_owned());
        }
        let base = input
            .split_once('#')
            .map_or(input, |(base, _fragment)| base);
        let mut encoded = String::with_capacity(name.len());
        for byte in name.as_bytes() {
            if byte.is_ascii_alphanumeric() || matches!(*byte, b'-' | b'.' | b'_' | b'~') {
                encoded.push(char::from(*byte));
            } else {
                use std::fmt::Write as _;
                write!(&mut encoded, "%{byte:02X}")
                    .map_err(|_error| SubscriptionInputError::InvalidVless)?;
            }
        }
        let renamed = format!("{base}#{encoded}");
        Self::parse(&renamed)?;
        Ok(renamed)
    }

    fn into_preview(self) -> SubscriptionPreview {
        SubscriptionPreview {
            kind: SourceKind::SingleNode,
            nodes: vec![self.preview],
        }
    }
}

fn parse_generic_single_node(input: &str) -> Result<SourceNodePreview, SubscriptionInputError> {
    if input.trim() != input
        || input.chars().any(char::is_control)
        || input.len() > MAX_SUBSCRIPTION_BYTES
    {
        return Err(SubscriptionInputError::InvalidSingleNode);
    }
    let (scheme, remainder) = input
        .split_once("://")
        .ok_or(SubscriptionInputError::InvalidSingleNode)?;
    if remainder.is_empty() || remainder.chars().any(char::is_whitespace) {
        return Err(SubscriptionInputError::InvalidSingleNode);
    }
    let protocol = match scheme {
        "vmess" => "VMess",
        "ss" => "Shadowsocks",
        "ssr" => "ShadowsocksR",
        "trojan" => "Trojan",
        "hysteria" => "Hysteria",
        "hysteria2" | "hy2" => "Hysteria2",
        "tuic" => "TUIC",
        "wireguard" => "WireGuard",
        _ if scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https") => {
            return Err(SubscriptionInputError::InvalidSingleNode);
        }
        _ => "Single node",
    };
    let (without_fragment, fragment) = remainder
        .split_once('#')
        .map_or((remainder, None), |(value, fragment)| {
            (value, Some(fragment))
        });
    let name = fragment
        .and_then(percent_decode)
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| format!("{protocol} node"));
    let endpoint = without_fragment
        .split('?')
        .next()
        .and_then(|value| value.rsplit('@').next())
        .filter(|value| value.contains(':') && value.len() <= 128)
        .unwrap_or("Single node")
        .to_owned();
    Ok(SourceNodePreview {
        name,
        protocol,
        endpoint,
        detail: SourceNodeDetail::SingleNode,
    })
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
    let security =
        query_value(query, "security").map_or(SourceNodeSecurity::Unspecified, |value| {
            if value.eq_ignore_ascii_case("tls") {
                SourceNodeSecurity::Tls
            } else if value.eq_ignore_ascii_case("reality") {
                SourceNodeSecurity::Reality
            } else if value.eq_ignore_ascii_case("none") {
                SourceNodeSecurity::None
            } else {
                SourceNodeSecurity::Custom
            }
        });
    let transport = query_value(query, "type").and_then(transport_label);

    Ok(SourceNodePreview {
        name,
        protocol: "VLESS",
        endpoint,
        detail: SourceNodeDetail::Vless {
            security,
            transport,
        },
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
    use super::{
        SingleNodeSource, SourceKind, SourceNodeDetail, SourceNodeSecurity, SubscriptionInputError,
        validate_single_node_preview, validate_subscription_preview,
    };

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
        let preview = validate_single_node_preview(
            "vless://00000000-0000-4000-8000-000000000000@edge.example.invalid:443?security=tls&type=ws#Tokyo%20Edge",
        )
        .expect("valid VLESS share link should be recognized");

        assert_eq!(preview.kind, SourceKind::SingleNode);
        assert_eq!(preview.nodes.len(), 1);
        assert_eq!(preview.nodes[0].name, "Tokyo Edge");
        assert_eq!(preview.nodes[0].protocol, "VLESS");
        assert_eq!(preview.nodes[0].endpoint, "edge.example.invalid:443");
        assert_eq!(
            preview.nodes[0].detail,
            SourceNodeDetail::Vless {
                security: SourceNodeSecurity::Tls,
                transport: Some("WebSocket"),
            }
        );
        assert!(!format!("{preview:?}").contains("00000000"));
    }

    #[test]
    fn recognizes_reality_tcp_with_an_empty_optional_header_type() {
        let preview = validate_single_node_preview(
            "vless://00000000-0000-4000-8000-000000000000@198.51.100.7:443?security=reality&encryption=none&pbk=fixture_reality-public-key&headerType=&fp=chrome&type=tcp&flow=xtls-rprx-vision&sni=cdn.example.invalid#Reality%20TCP",
        )
        .expect("QX-style empty optional query fields should be accepted");

        assert_eq!(preview.kind, SourceKind::SingleNode);
        assert_eq!(preview.nodes[0].name, "Reality TCP");
        assert_eq!(
            preview.nodes[0].detail,
            SourceNodeDetail::Vless {
                security: SourceNodeSecurity::Reality,
                transport: Some("TCP"),
            }
        );
    }

    #[test]
    fn vless_source_keeps_credentials_out_of_debug_output() {
        let source = SingleNodeSource::parse(
            "vless://00000000-0000-4000-8000-000000000000@edge.example.invalid:443?security=tls#Saved",
        )
        .expect("fixture VLESS link should parse");

        assert_eq!(source.preview().name, "Saved");
        assert_eq!(format!("{source:?}"), "SingleNodeSource(<redacted>)");
        assert!(!format!("{source:?}").contains("00000000"));
    }

    #[test]
    fn vless_source_name_field_replaces_the_share_link_fragment() {
        let input = "vless://00000000-0000-4000-8000-000000000000@edge.example.invalid:443?security=tls#Old";
        let renamed = SingleNodeSource::input_with_name(input, "香港 节点").expect("renamed VLESS");
        let source = SingleNodeSource::parse(&renamed).expect("renamed source should parse");

        assert_eq!(source.preview().name, "香港 节点");
        assert!(renamed.ends_with("#%E9%A6%99%E6%B8%AF%20%E8%8A%82%E7%82%B9"));
    }

    #[test]
    fn invalid_source_error_never_contains_the_input() {
        let input = "ftp://subscription.example.invalid/private-token";
        let error = validate_subscription_preview(input).expect_err("FTP must be rejected");

        assert_eq!(error, SubscriptionInputError::UnsupportedSource);
        assert!(!format!("{error:?}").contains(input));
        assert!(!error.to_string().contains("private-token"));
    }

    #[test]
    fn generic_single_node_failures_are_not_reported_as_vless_parse_errors() {
        assert_eq!(
            validate_single_node_preview("https://subscription.example.invalid/profile"),
            Err(SubscriptionInputError::InvalidSingleNode)
        );
        assert_eq!(
            validate_single_node_preview("ss://with whitespace"),
            Err(SubscriptionInputError::InvalidSingleNode)
        );
    }
}
