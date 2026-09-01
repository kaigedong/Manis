use std::fmt;

use crate::{
    MAX_SECRET_URL_BYTES, Name, ProfileError, decode_query_value, is_plain_value, is_uuid,
    is_vless_host, optional_vless_value, parse_vless_query, parse_vless_security,
    parse_vless_server, parse_vless_transport, require_vless_encryption,
};

pub(crate) const MAX_VLESS_FIELD_BYTES: usize = 1024;

#[derive(Clone, Eq, PartialEq)]
pub enum OutboundProxy {
    Vless(VlessProxy),
}

impl OutboundProxy {
    #[must_use]
    pub fn name(&self) -> &Name {
        match self {
            Self::Vless(proxy) => proxy.name(),
        }
    }

    pub(crate) fn validate(&self) -> Result<(), ProfileError> {
        match self {
            Self::Vless(proxy) => proxy.validate(),
        }
    }
}

impl fmt::Debug for OutboundProxy {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Vless(_) => formatter.write_str("OutboundProxy::Vless(<redacted>)"),
        }
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct VlessProxy {
    pub(crate) name: Name,
    pub(crate) server: String,
    pub(crate) port: u16,
    pub(crate) uuid: String,
    pub(crate) flow: Option<String>,
    pub(crate) packet_encoding: Option<String>,
    pub(crate) security: VlessSecurity,
    pub(crate) servername: Option<String>,
    pub(crate) alpn: Vec<String>,
    pub(crate) client_fingerprint: Option<String>,
    pub(crate) skip_cert_verify: bool,
    pub(crate) reality_public_key: Option<String>,
    pub(crate) reality_short_id: Option<String>,
    pub(crate) transport: VlessTransport,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VlessSecurity {
    None,
    Tls,
    Reality,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum VlessTransport {
    Tcp,
    Ws {
        path: Option<String>,
        host: Option<String>,
    },
    Http {
        path: Option<String>,
        host: Option<String>,
    },
    H2 {
        path: Option<String>,
        host: Option<String>,
    },
    Grpc {
        service_name: Option<String>,
    },
    Xhttp {
        path: Option<String>,
        host: Option<String>,
        mode: Option<String>,
    },
}

pub(crate) struct VlessSecurityOptions {
    pub(crate) security: VlessSecurity,
    pub(crate) servername: Option<String>,
    pub(crate) alpn: Vec<String>,
    pub(crate) client_fingerprint: Option<String>,
    pub(crate) skip_cert_verify: bool,
    pub(crate) reality_public_key: Option<String>,
    pub(crate) reality_short_id: Option<String>,
}

impl VlessProxy {
    /// Parses the explicitly supported subset of a VLESS share link.
    ///
    /// Unknown or duplicate query keys are rejected instead of being silently ignored. Errors
    /// never contain source material.
    ///
    /// # Errors
    /// Returns a fixed-category [`ProfileError`] for malformed or unsupported links.
    pub fn parse_share_link(input: &str) -> Result<Self, ProfileError> {
        if input.len() > MAX_SECRET_URL_BYTES
            || input.trim() != input
            || input.chars().any(char::is_control)
        {
            return Err(ProfileError::InvalidVless);
        }
        let remainder = input
            .strip_prefix("vless://")
            .ok_or(ProfileError::InvalidVless)?;
        let (without_fragment, fragment) = remainder
            .split_once('#')
            .map_or((remainder, None), |(value, name)| (value, Some(name)));
        let (authority, query) = without_fragment
            .split_once('?')
            .map_or((without_fragment, ""), |(value, query)| (value, query));
        let (uuid, server_port) = authority
            .split_once('@')
            .ok_or(ProfileError::InvalidVless)?;
        if !is_uuid(uuid) {
            return Err(ProfileError::InvalidVless);
        }
        let (server, port) = parse_vless_server(server_port)?;
        let fields = parse_vless_query(query)?;
        require_vless_encryption(fields.get("encryption").map(String::as_str))?;

        let name = match fragment {
            Some(value) => decode_query_value(value)
                .ok_or(ProfileError::InvalidVless)?
                .trim()
                .to_owned(),
            None => String::new(),
        };
        let name = if name.is_empty() {
            format!("VLESS · {server}")
        } else {
            name
        };
        let name = Name::parse(&name).map_err(|_error| ProfileError::InvalidVless)?;
        let flow = optional_vless_value(&fields, "flow")?;
        if flow
            .as_deref()
            .is_some_and(|value| value != "xtls-rprx-vision")
        {
            return Err(ProfileError::UnsupportedVless);
        }
        let packet_encoding = optional_vless_value(&fields, "packetencoding")?;
        if packet_encoding
            .as_deref()
            .is_some_and(|value| !matches!(value, "xudp" | "packetaddr"))
        {
            return Err(ProfileError::UnsupportedVless);
        }
        let security = parse_vless_security(&fields)?;
        let transport = parse_vless_transport(&fields)?;
        let proxy = Self {
            name,
            server,
            port,
            uuid: uuid.to_ascii_lowercase(),
            flow,
            packet_encoding,
            security: security.security,
            servername: security.servername,
            alpn: security.alpn,
            client_fingerprint: security.client_fingerprint,
            skip_cert_verify: security.skip_cert_verify,
            reality_public_key: security.reality_public_key,
            reality_short_id: security.reality_short_id,
            transport,
        };
        proxy.validate()?;
        Ok(proxy)
    }

    #[must_use]
    pub fn name(&self) -> &Name {
        &self.name
    }

    pub(crate) fn validate(&self) -> Result<(), ProfileError> {
        if self.port == 0
            || !is_vless_host(&self.server)
            || !is_uuid(&self.uuid)
            || self
                .reality_public_key
                .as_deref()
                .is_some_and(|value| !is_plain_value(value, MAX_VLESS_FIELD_BYTES))
            || self
                .reality_short_id
                .as_deref()
                .is_some_and(|value| !is_plain_value(value, 64))
        {
            return Err(ProfileError::InvalidVless);
        }
        Ok(())
    }
}

impl fmt::Debug for VlessProxy {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("VlessProxy(<redacted>)")
    }
}
