use std::fmt;

use crate::{ProfileError, decode_query_value, is_https_url, is_plain_value, is_subscription_url};

pub(crate) const MAX_SECRET_URL_BYTES: usize = 16 * 1024;
const MAX_SUBSCRIPTION_NAME_BYTES: usize = 96;

#[derive(Clone, Eq, PartialEq)]
pub struct SecretUrl(pub(crate) String);

impl SecretUrl {
    /// Parses an HTTPS subscription URL while keeping its value out of diagnostics.
    ///
    /// # Errors
    /// Returns [`ProfileError::InvalidUrl`] without including any part of `input`.
    pub fn parse_https(input: &str) -> Result<Self, ProfileError> {
        if !is_https_url(input) || input.len() > MAX_SECRET_URL_BYTES {
            return Err(ProfileError::InvalidUrl);
        }
        Ok(Self(input.to_owned()))
    }

    /// Parses an HTTP or HTTPS Mihomo proxy-provider URL while keeping its value out of
    /// diagnostics.
    ///
    /// # Errors
    /// Returns [`ProfileError::InvalidUrl`] without including any part of `input`.
    pub fn parse_subscription(input: &str) -> Result<Self, ProfileError> {
        if !is_subscription_url(input) || input.len() > MAX_SECRET_URL_BYTES {
            return Err(ProfileError::InvalidUrl);
        }
        Ok(Self(input.to_owned()))
    }

    /// Reports whether this subscription uses encrypted HTTPS transport without exposing it.
    #[must_use]
    pub fn is_https(&self) -> bool {
        self.0.starts_with("https://")
    }

    /// Exposes the URL only for the lifetime of a caller-owned closure.
    ///
    /// This keeps the secret out of `Display`, `Debug`, and ordinary return values while allowing
    /// storage and network boundaries to consume it without cloning it into UI state.
    pub fn expose_to<T>(&self, use_secret: impl FnOnce(&str) -> T) -> T {
        use_secret(&self.0)
    }

    /// Returns a bounded user-facing subscription label from an explicit `name` query field.
    ///
    /// Other query values, including bearer tokens, are never used as display fallbacks.
    #[must_use]
    pub fn subscription_name(&self) -> Option<String> {
        let query = self.0.split_once('?')?.1.split('#').next()?;
        query.split('&').find_map(|field| {
            let (key, value) = field.split_once('=')?;
            if !key.eq_ignore_ascii_case("name") {
                return None;
            }
            let decoded = decode_query_value(value)?;
            let name = decoded.trim();
            is_plain_value(name, MAX_SUBSCRIPTION_NAME_BYTES).then(|| name.to_owned())
        })
    }
}

impl fmt::Debug for SecretUrl {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SecretUrl(<redacted>)")
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Name(String);

impl Name {
    /// Creates a provider or policy-group name safe for Mihomo rule scalars.
    ///
    /// # Errors
    /// Returns [`ProfileError::InvalidName`] for an unsafe value.
    pub fn parse(input: &str) -> Result<Self, ProfileError> {
        if !is_plain_value(input, 128) || input.contains(',') {
            return Err(ProfileError::InvalidName);
        }
        Ok(Self(input.to_owned()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProxyProvider {
    pub name: Name,
    pub source: ProxyProviderSource,
    pub interval_secs: u32,
    pub path: String,
    pub health_check: HealthCheck,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProxyProviderSource {
    Http(SecretUrl),
    File,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HealthCheck {
    pub enabled: bool,
    pub interval_secs: u32,
    pub url: String,
}
