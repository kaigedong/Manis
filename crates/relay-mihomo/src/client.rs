use serde::Deserialize;

use crate::models::{ProxiesResponse, RulesResponse};
use crate::{
    ConnectionsState, ControllerConfig, MihomoError, MihomoSnapshot, ReadonlyTransport, VersionInfo,
};

const VERSION_ENDPOINT: &str = "/version";
const PROXIES_ENDPOINT: &str = "/proxies";
const RULES_ENDPOINT: &str = "/rules";
const CONNECTIONS_ENDPOINT: &str = "/connections";

#[derive(Debug, Clone)]
pub struct MihomoClient<T> {
    config: ControllerConfig,
    transport: T,
}

impl<T> MihomoClient<T>
where
    T: ReadonlyTransport,
{
    #[must_use]
    pub fn new(config: ControllerConfig, transport: T) -> Self {
        Self { config, transport }
    }

    /// Fetches `/version`, `/proxies`, `/rules`, and `/connections` and returns one UI-ready snapshot.
    ///
    /// # Errors
    ///
    /// Returns an error if any controller request fails or if a response cannot be decoded as the
    /// expected Mihomo JSON shape.
    pub fn fetch_snapshot(&self) -> Result<MihomoSnapshot, MihomoError> {
        let version = self.fetch_json::<VersionInfo>(VERSION_ENDPOINT)?;
        let proxies = self
            .fetch_json::<ProxiesResponse>(PROXIES_ENDPOINT)?
            .into_proxies();
        let rules = self.fetch_json::<RulesResponse>(RULES_ENDPOINT)?.rules;
        let connections = self.fetch_json::<ConnectionsState>(CONNECTIONS_ENDPOINT)?;

        Ok(MihomoSnapshot {
            version,
            proxies,
            rules,
            connections,
        })
    }

    fn fetch_json<Response>(&self, endpoint: &str) -> Result<Response, MihomoError>
    where
        Response: for<'de> Deserialize<'de>,
    {
        let body = self.transport.get(&self.config, endpoint)?;
        serde_json::from_str(&body).map_err(|source| MihomoError::Json {
            endpoint: endpoint.to_owned(),
            source,
        })
    }
}
