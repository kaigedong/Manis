use std::fmt;
use std::net::IpAddr;
use std::time::Duration;

use crate::MihomoError;

const DEFAULT_BASE_URL: &str = "http://127.0.0.1:9090";
const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
const DEFAULT_READ_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone)]
pub struct ControllerConfig {
    base_url: String,
    host: String,
    authority: String,
    port: u16,
    secret: Option<String>,
    connect_timeout: Duration,
    read_timeout: Duration,
}

impl ControllerConfig {
    /// Validates and normalizes a plaintext HTTP Mihomo controller base URL.
    ///
    /// # Errors
    ///
    /// Returns an error when the URL is not `http://`, does not point to a loopback host, omits an
    /// explicit port, includes userinfo, or includes any path, query, or fragment component.
    pub fn new(base_url: impl AsRef<str>) -> Result<Self, MihomoError> {
        let parsed = ParsedBaseUrl::parse(base_url.as_ref())?;
        Ok(Self {
            base_url: parsed.base_url,
            host: parsed.host,
            authority: parsed.authority,
            port: parsed.port,
            secret: None,
            connect_timeout: DEFAULT_CONNECT_TIMEOUT,
            read_timeout: DEFAULT_READ_TIMEOUT,
        })
    }

    #[must_use]
    pub fn with_secret(mut self, secret: impl Into<String>) -> Self {
        let secret = secret.into();
        self.secret = if secret.is_empty() {
            None
        } else {
            Some(secret)
        };
        self
    }

    #[must_use]
    pub fn with_timeouts(mut self, connect_timeout: Duration, read_timeout: Duration) -> Self {
        self.connect_timeout = connect_timeout;
        self.read_timeout = read_timeout;
        self
    }

    #[must_use]
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    #[must_use]
    pub fn host(&self) -> &str {
        &self.host
    }

    #[must_use]
    pub fn authority(&self) -> &str {
        &self.authority
    }

    #[must_use]
    pub fn port(&self) -> u16 {
        self.port
    }

    #[must_use]
    pub fn secret(&self) -> Option<&str> {
        self.secret.as_deref()
    }

    #[must_use]
    pub fn connect_timeout(&self) -> Duration {
        self.connect_timeout
    }

    #[must_use]
    pub fn read_timeout(&self) -> Duration {
        self.read_timeout
    }
}

impl Default for ControllerConfig {
    fn default() -> Self {
        Self {
            base_url: DEFAULT_BASE_URL.to_owned(),
            host: "127.0.0.1".to_owned(),
            authority: "127.0.0.1:9090".to_owned(),
            port: 9090,
            secret: None,
            connect_timeout: DEFAULT_CONNECT_TIMEOUT,
            read_timeout: DEFAULT_READ_TIMEOUT,
        }
    }
}

impl fmt::Debug for ControllerConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ControllerConfig")
            .field("base_url", &self.base_url)
            .field("host", &self.host)
            .field("authority", &self.authority)
            .field("port", &self.port)
            .field("secret", &self.secret.as_ref().map(|_secret| "<redacted>"))
            .field("connect_timeout", &self.connect_timeout)
            .field("read_timeout", &self.read_timeout)
            .finish()
    }
}

struct ParsedBaseUrl {
    base_url: String,
    host: String,
    authority: String,
    port: u16,
}

impl ParsedBaseUrl {
    fn parse(input: &str) -> Result<Self, MihomoError> {
        if input.starts_with("https://") {
            return Err(MihomoError::InvalidConfig(
                "HTTPS controller URLs are not supported by the std transport".to_owned(),
            ));
        }

        let Some(authority) = input.strip_prefix("http://") else {
            return Err(MihomoError::InvalidConfig(
                "controller URL must start with http://".to_owned(),
            ));
        };

        if authority.is_empty() {
            return Err(MihomoError::InvalidConfig(
                "controller URL must include host and port".to_owned(),
            ));
        }

        if authority.contains(['/', '?', '#']) {
            return Err(MihomoError::InvalidConfig(
                "controller URL must not include path, query, or fragment".to_owned(),
            ));
        }

        if authority.contains('@') {
            return Err(MihomoError::InvalidConfig(
                "controller URL must not include userinfo".to_owned(),
            ));
        }

        let (host, port, normalized_authority) = parse_authority(authority)?;
        if !is_loopback_host(&host) {
            return Err(MihomoError::InvalidConfig(
                "plaintext HTTP controllers are restricted to localhost and loopback IP addresses"
                    .to_owned(),
            ));
        }
        let base_url = format!("http://{normalized_authority}");

        Ok(Self {
            base_url,
            host,
            authority: normalized_authority,
            port,
        })
    }
}

fn is_loopback_host(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<IpAddr>()
            .is_ok_and(|address| address.is_loopback())
}

fn parse_authority(authority: &str) -> Result<(String, u16, String), MihomoError> {
    if let Some(rest) = authority.strip_prefix('[') {
        let Some(end) = rest.find(']') else {
            return Err(MihomoError::InvalidConfig(
                "IPv6 controller host must be bracketed".to_owned(),
            ));
        };
        let host = &rest[..end];
        let port_part = &rest[end + 1..];
        let Some(port) = port_part.strip_prefix(':') else {
            return Err(MihomoError::InvalidConfig(
                "controller URL must include an explicit port".to_owned(),
            ));
        };
        let port = parse_port(port)?;
        if host.is_empty() {
            return Err(MihomoError::InvalidConfig(
                "controller host must not be empty".to_owned(),
            ));
        }
        return Ok((host.to_owned(), port, format!("[{host}]:{port}")));
    }

    let Some((host, port_part)) = authority.rsplit_once(':') else {
        return Err(MihomoError::InvalidConfig(
            "controller URL must include an explicit port".to_owned(),
        ));
    };

    if host.is_empty() || host.contains(':') {
        return Err(MihomoError::InvalidConfig(
            "controller host must be non-empty; bracket IPv6 addresses".to_owned(),
        ));
    }

    let port = parse_port(port_part)?;
    Ok((host.to_owned(), port, format!("{host}:{port}")))
}

fn parse_port(port: &str) -> Result<u16, MihomoError> {
    let port = port.parse::<u16>().map_err(|_error| {
        MihomoError::InvalidConfig("controller port must be a number from 1 to 65535".to_owned())
    })?;

    if port == 0 {
        return Err(MihomoError::InvalidConfig(
            "controller port must be greater than zero".to_owned(),
        ));
    }

    Ok(port)
}
