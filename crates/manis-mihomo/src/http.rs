use std::io::{self, Read};
use std::net::{TcpStream, ToSocketAddrs};
#[cfg(unix)]
use std::os::unix::{fs::FileTypeExt, net::UnixStream};
#[cfg(unix)]
use std::path::{Path, PathBuf};
use std::sync::{Arc, atomic::AtomicBool};

use serde_json::Value;
use ureq::http::{Request, Response};

use crate::http_socket::{self, Socket};
use crate::{ControllerConfig, MihomoError};

pub const DEFAULT_BODY_LIMIT_BYTES: usize = 16 * 1024 * 1024;
const HEADER_LIMIT_BYTES: usize = 64 * 1024;

/// Bounded request transport for the local Mihomo controller.
pub trait ControllerTransport {
    /// Issues a `GET` request to a controller path and returns the response body.
    ///
    /// # Errors
    ///
    /// Returns an error when the request path is invalid, the transport fails, the response is
    /// malformed, the status is non-successful, or the body exceeds the configured limit.
    fn get(&self, config: &ControllerConfig, path: &str) -> Result<String, MihomoError>;

    /// Issues a `PATCH` request with a JSON body to a controller path and returns the response body.
    ///
    /// # Errors
    ///
    /// Returns an error when the request path is invalid, the request cannot be serialized, the
    /// transport fails, the response is malformed, the status is non-successful, or the body
    /// exceeds the configured limit.
    fn patch_json(
        &self,
        config: &ControllerConfig,
        path: &str,
        body: &Value,
    ) -> Result<String, MihomoError>;

    /// Issues a `PUT` request with a JSON body to a controller path and returns the response body.
    ///
    /// # Errors
    ///
    /// Returns an error when the request path is invalid, the request cannot be serialized, the
    /// transport fails, the response is malformed, the status is non-successful, or the body
    /// exceeds the configured limit.
    fn put_json(
        &self,
        config: &ControllerConfig,
        path: &str,
        body: &Value,
    ) -> Result<String, MihomoError>;
}

impl<T> ControllerTransport for &T
where
    T: ControllerTransport + ?Sized,
{
    fn get(&self, config: &ControllerConfig, path: &str) -> Result<String, MihomoError> {
        (*self).get(config, path)
    }

    fn patch_json(
        &self,
        config: &ControllerConfig,
        path: &str,
        body: &Value,
    ) -> Result<String, MihomoError> {
        (*self).patch_json(config, path, body)
    }

    fn put_json(
        &self,
        config: &ControllerConfig,
        path: &str,
        body: &Value,
    ) -> Result<String, MihomoError> {
        (*self).put_json(config, path, body)
    }
}

/// Compatibility name for the restricted, ureq-backed loopback transport.
#[derive(Debug, Clone)]
pub struct StdHttpTransport {
    body_limit: usize,
}

impl StdHttpTransport {
    #[must_use]
    pub fn with_body_limit(body_limit: usize) -> Self {
        Self { body_limit }
    }
}

impl Default for StdHttpTransport {
    fn default() -> Self {
        Self::with_body_limit(DEFAULT_BODY_LIMIT_BYTES)
    }
}

impl ControllerTransport for StdHttpTransport {
    fn get(&self, config: &ControllerConfig, path: &str) -> Result<String, MihomoError> {
        Target::Loopback(config).request("GET", path, None, self.body_limit)
    }

    fn patch_json(
        &self,
        config: &ControllerConfig,
        path: &str,
        body: &Value,
    ) -> Result<String, MihomoError> {
        Target::Loopback(config).request("PATCH", path, Some(body), self.body_limit)
    }

    fn put_json(
        &self,
        config: &ControllerConfig,
        path: &str,
        body: &Value,
    ) -> Result<String, MihomoError> {
        Target::Loopback(config).request("PUT", path, Some(body), self.body_limit)
    }
}

/// Bounded HTTP transport over a local Unix domain socket.
#[cfg(unix)]
#[derive(Debug, Clone)]
pub struct UnixSocketTransport {
    socket_path: PathBuf,
    body_limit: usize,
}

#[cfg(unix)]
impl UnixSocketTransport {
    #[must_use]
    pub fn new(socket_path: impl Into<PathBuf>) -> Self {
        Self {
            socket_path: socket_path.into(),
            body_limit: DEFAULT_BODY_LIMIT_BYTES,
        }
    }

    #[must_use]
    pub fn with_body_limit(mut self, body_limit: usize) -> Self {
        self.body_limit = body_limit;
        self
    }

    #[must_use]
    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }
}

#[cfg(unix)]
impl ControllerTransport for UnixSocketTransport {
    fn get(&self, config: &ControllerConfig, path: &str) -> Result<String, MihomoError> {
        Target::Unix(config, &self.socket_path).request("GET", path, None, self.body_limit)
    }

    fn patch_json(
        &self,
        config: &ControllerConfig,
        path: &str,
        body: &Value,
    ) -> Result<String, MihomoError> {
        Target::Unix(config, &self.socket_path).request("PATCH", path, Some(body), self.body_limit)
    }

    fn put_json(
        &self,
        config: &ControllerConfig,
        path: &str,
        body: &Value,
    ) -> Result<String, MihomoError> {
        Target::Unix(config, &self.socket_path).request("PUT", path, Some(body), self.body_limit)
    }
}

pub(crate) enum Target<'a> {
    Loopback(&'a ControllerConfig),
    #[cfg(unix)]
    Unix(&'a ControllerConfig, &'a Path),
}

impl Target<'_> {
    pub(crate) fn response(
        &self,
        method: &str,
        path: &str,
        body: Option<&Value>,
        cancelled: Option<Arc<AtomicBool>>,
    ) -> Result<Response<ureq::Body>, MihomoError> {
        let config = match self {
            Self::Loopback(config) => config,
            #[cfg(unix)]
            Self::Unix(config, _) => config,
        };
        // Validate the whole request, including authentication, before opening any socket.
        let request = build_request(
            config,
            method,
            path,
            body,
            matches!(self, Self::Loopback(_)),
        )?;
        let socket = match self {
            Self::Loopback(config) => {
                let address = (config.host(), config.port())
                    .to_socket_addrs()?
                    .find(|address| address.ip().is_loopback())
                    .ok_or_else(|| {
                        MihomoError::InvalidConfig(
                            "controller did not resolve to a loopback address".to_owned(),
                        )
                    })?;
                Socket::Tcp(TcpStream::connect_timeout(
                    &address,
                    config.connect_timeout(),
                )?)
            }
            #[cfg(unix)]
            Self::Unix(_, path) => {
                validate_unix_socket_path(path)?;
                Socket::Unix(UnixStream::connect(path)?)
            }
        };
        let settings = ureq::Agent::config_builder()
            .proxy(None)
            .max_redirects(0)
            .max_redirects_will_error(false)
            .http_status_as_error(false)
            .max_idle_connections(0)
            .max_response_header_size(HEADER_LIMIT_BYTES)
            .timeout_send_request(Some(config.connect_timeout()))
            .timeout_send_body(Some(config.connect_timeout()))
            // Mihomo flushes log headers only when the first entry arrives. A quiet
            // controller must remain cancellable without being treated as unreachable.
            .timeout_recv_response(if cancelled.is_some() && path.starts_with("/logs?level=") {
                None
            } else {
                Some(config.read_timeout())
            })
            .timeout_recv_body(if cancelled.is_some() {
                None
            } else {
                Some(config.read_timeout())
            })
            .build();
        let response = http_socket::agent(socket, settings, cancelled)
            .run(request)
            .map_err(http_error)?;
        if !response.status().is_success() {
            return Err(MihomoError::HttpStatus {
                status_code: response.status().as_u16(),
                // Never retain untrusted reason phrases, headers, or error bodies containing secrets.
                reason: response
                    .status()
                    .canonical_reason()
                    .unwrap_or("HTTP error")
                    .to_owned(),
                body_preview: String::new(),
            });
        }
        Ok(response)
    }

    fn request(
        &self,
        method: &str,
        path: &str,
        body: Option<&Value>,
        limit: usize,
    ) -> Result<String, MihomoError> {
        let mut response = self.response(method, path, body, None)?;
        let mut bytes = Vec::new();
        response
            .body_mut()
            .as_reader()
            .take((limit as u64).saturating_add(1))
            .read_to_end(&mut bytes)
            .map_err(body_error)?;
        if bytes.len() > limit {
            return Err(MihomoError::BodyTooLarge { limit });
        }
        String::from_utf8(bytes)
            .map_err(|_| MihomoError::InvalidResponse("body was not UTF-8".to_owned()))
    }
}

#[cfg(unix)]
pub(crate) fn validate_unix_socket_path(socket_path: &Path) -> Result<(), MihomoError> {
    if !socket_path.is_absolute() {
        return Err(MihomoError::InvalidConfig(
            "Unix controller socket path must be absolute".to_owned(),
        ));
    }
    let metadata = std::fs::symlink_metadata(socket_path)?;
    if metadata.file_type().is_symlink() || !metadata.file_type().is_socket() {
        return Err(MihomoError::InvalidConfig(
            "Unix controller path must be a socket and not a symlink".to_owned(),
        ));
    }
    Ok(())
}

fn build_request(
    config: &ControllerConfig,
    method: &str,
    path: &str,
    body: Option<&Value>,
    include_authorization: bool,
) -> Result<Request<Vec<u8>>, MihomoError> {
    if !path.starts_with('/')
        || path.starts_with("//")
        || path.contains('#')
        || path
            .bytes()
            .any(|byte| byte.is_ascii_control() || byte == b' ')
    {
        return Err(MihomoError::InvalidRequestPath(path.to_owned()));
    }
    let mut request = Request::builder()
        .method(method)
        .uri(format!("http://{}{path}", config.authority()))
        .header("Accept", "application/json")
        .header("User-Agent", "manis-mihomo/0.1")
        .header("Connection", "close");
    if let Some(secret) = config.secret().filter(|_| include_authorization) {
        if secret.bytes().any(|byte| byte.is_ascii_control()) {
            return Err(MihomoError::InvalidConfig(
                "controller secret must not contain control characters".to_owned(),
            ));
        }
        let mut authorization = ureq::http::HeaderValue::from_str(&format!("Bearer {secret}"))
            .map_err(|_| MihomoError::InvalidConfig("invalid controller secret".to_owned()))?;
        authorization.set_sensitive(true);
        request = request.header("Authorization", authorization);
    }
    let bytes = match body {
        Some(body) => {
            request = request.header("Content-Type", "application/json");
            serde_json::to_vec(body).map_err(|source| MihomoError::Json {
                endpoint: path.to_owned(),
                source,
            })?
        }
        None => Vec::new(),
    };
    request
        .body(bytes)
        .map_err(|_| MihomoError::InvalidRequestPath(path.to_owned()))
}

fn http_error(error: ureq::Error) -> MihomoError {
    match error {
        ureq::Error::Io(error) => body_error(error),
        ureq::Error::Timeout(_) => io::Error::from(io::ErrorKind::TimedOut).into(),
        _ => MihomoError::InvalidResponse("controller HTTP protocol error".to_owned()),
    }
}

pub(crate) fn body_error(error: io::Error) -> MihomoError {
    // ureq embeds protocol errors in io::Error; do not log their server-controlled contents.
    if error.get_ref().is_some() {
        MihomoError::InvalidResponse("controller HTTP body error".to_owned())
    } else {
        error.into()
    }
}

#[cfg(test)]
mod tests {
    use super::build_request;
    use crate::ControllerConfig;

    #[test]
    fn request_builder_omits_empty_secret() {
        let request = build_request(
            &ControllerConfig::default().with_secret(""),
            "GET",
            "/version",
            None,
            true,
        )
        .unwrap();
        assert!(!request.headers().contains_key("authorization"));
    }

    #[test]
    fn request_builder_rejects_secret_header_injection() {
        assert!(
            build_request(
                &ControllerConfig::default().with_secret("token\r\nX-Injected: yes"),
                "GET",
                "/version",
                None,
                true
            )
            .is_err()
        );
    }

    #[test]
    fn request_path_cannot_change_authority_or_inject_headers() {
        for path in [
            "/version\tHTTP/1.1",
            "//example.com/version",
            "/version#ignored",
            "https://example.com",
        ] {
            assert!(build_request(&ControllerConfig::default(), "GET", path, None, true).is_err());
        }
    }

    #[test]
    fn authorization_header_debug_is_redacted() {
        let request = build_request(
            &ControllerConfig::default().with_secret("secret-token"),
            "GET",
            "/version",
            None,
            true,
        )
        .unwrap();
        assert!(!format!("{request:?}").contains("secret-token"));
    }
}
