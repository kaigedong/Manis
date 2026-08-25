use serde::Deserialize;
use serde_json::{Map, Value};

use crate::models::{ProvidersResponse, ProxiesResponse, RulesResponse};
use crate::{
    ConnectionsState, ControllerConfig, MihomoError, MihomoSnapshot, ProxyProvider,
    ReadonlyTransport, RuntimeConfig, VersionInfo,
};

const VERSION_ENDPOINT: &str = "/version";
const PROXIES_ENDPOINT: &str = "/proxies";
const PROVIDERS_ENDPOINT: &str = "/providers/proxies";
const RULES_ENDPOINT: &str = "/rules";
const CONNECTIONS_ENDPOINT: &str = "/connections";
const CONFIGS_ENDPOINT: &str = "/configs";

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

    /// Fetches the read-only controller data needed for policy and source browsing.
    ///
    /// # Errors
    ///
    /// Returns an error if any controller request fails or if a response cannot be decoded as the
    /// expected Mihomo JSON shape.
    pub fn fetch_snapshot(&self) -> Result<MihomoSnapshot, MihomoError> {
        let version = self.fetch_version()?;
        let proxies = self
            .fetch_json::<ProxiesResponse>(PROXIES_ENDPOINT)?
            .into_proxies();
        let providers = self.fetch_proxy_providers()?;
        let rules = self.fetch_json::<RulesResponse>(RULES_ENDPOINT)?.rules;
        let connections = self.fetch_json::<ConnectionsState>(CONNECTIONS_ENDPOINT)?;
        let runtime = self.fetch_runtime_config()?;

        Ok(MihomoSnapshot {
            version,
            proxies,
            providers,
            rules,
            connections,
            runtime,
        })
    }

    /// Fetches only `/version` for a lightweight controller readiness check.
    ///
    /// # Errors
    ///
    /// Returns an error if the read-only request fails or the response is not a valid Mihomo
    /// version payload.
    pub fn fetch_version(&self) -> Result<VersionInfo, MihomoError> {
        self.fetch_json::<VersionInfo>(VERSION_ENDPOINT)
    }

    /// Fetches all proxy providers and their parsed nodes without reading unrelated runtime data.
    ///
    /// # Errors
    /// Returns an error if the controller request fails or the provider payload cannot be decoded.
    pub fn fetch_proxy_providers(&self) -> Result<Vec<ProxyProvider>, MihomoError> {
        Ok(self
            .fetch_json::<ProvidersResponse>(PROVIDERS_ENDPOINT)?
            .into_providers())
    }

    /// Fetches Mihomo's mutable runtime configuration surface from `/configs`.
    ///
    /// # Errors
    /// Returns an error if the controller request fails or the payload cannot be decoded.
    pub fn fetch_runtime_config(&self) -> Result<RuntimeConfig, MihomoError> {
        self.fetch_json::<RuntimeConfig>(CONFIGS_ENDPOINT)
    }

    /// Toggles `tun.enable` while preserving every other currently reported TUN field.
    ///
    /// # Errors
    /// Returns an error if the controller cannot be read, the `tun` field is not an object or
    /// `null`, or the PATCH request fails.
    pub fn set_tun_enabled(&self, enabled: bool) -> Result<(), MihomoError> {
        let config = self.fetch_json::<Value>(CONFIGS_ENDPOINT)?;
        let mut tun = match config.get("tun") {
            Some(Value::Object(tun)) => tun.clone(),
            Some(Value::Null) | None => Map::new(),
            Some(_other) => {
                return Err(MihomoError::InvalidResponse(
                    "/configs tun field was not an object".to_owned(),
                ));
            }
        };
        tun.insert("enable".to_owned(), Value::Bool(enabled));

        let mut patch = Map::new();
        patch.insert("tun".to_owned(), Value::Object(tun));
        self.transport
            .patch_json(&self.config, CONFIGS_ENDPOINT, &Value::Object(patch))?;
        Ok(())
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
