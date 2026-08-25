use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::{Map, Value};

use crate::models::{ProvidersResponse, ProxiesResponse, RulesResponse};
use crate::{
    ConnectionsState, ControllerConfig, ControllerTransport, MihomoError, MihomoPolicyGroup,
    MihomoSnapshot, ProxyProvider, RuntimeConfig, VersionInfo,
};

const VERSION_ENDPOINT: &str = "/version";
const PROXIES_ENDPOINT: &str = "/proxies";
const PROVIDERS_ENDPOINT: &str = "/providers/proxies";
const RULES_ENDPOINT: &str = "/rules";
const CONNECTIONS_ENDPOINT: &str = "/connections";
const CONFIGS_ENDPOINT: &str = "/configs";

#[derive(Debug, Deserialize)]
struct ProxyDelayResponse {
    delay: u16,
}

#[derive(Debug, Clone)]
pub struct MihomoClient<T> {
    config: ControllerConfig,
    transport: T,
}

impl<T> MihomoClient<T>
where
    T: ControllerTransport,
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

    /// Tests all nodes in a policy group and returns fresh latency values keyed by node name.
    ///
    /// # Errors
    /// Returns an error if the controller request fails or the delay payload cannot be decoded.
    pub fn fetch_group_delay(
        &self,
        group_name: &str,
        test_url: &str,
        timeout_ms: u16,
    ) -> Result<BTreeMap<String, u16>, MihomoError> {
        let endpoint = format!(
            "/group/{}/delay?url={}&timeout={timeout_ms}",
            percent_encode_component(group_name),
            percent_encode_component(test_url)
        );
        self.fetch_json(&endpoint)
    }

    /// Tests one proxy node and returns its fresh latency in milliseconds.
    ///
    /// # Errors
    /// Returns an error if the controller request fails or the delay payload cannot be decoded.
    pub fn fetch_proxy_delay(
        &self,
        proxy_name: &str,
        test_url: &str,
        timeout_ms: u16,
    ) -> Result<u16, MihomoError> {
        let endpoint = format!(
            "/proxies/{}/delay?url={}&timeout={timeout_ms}",
            percent_encode_component(proxy_name),
            percent_encode_component(test_url)
        );
        Ok(self.fetch_json::<ProxyDelayResponse>(&endpoint)?.delay)
    }

    /// Fetches minimal state for a Mihomo policy group or proxy entry.
    ///
    /// # Errors
    /// Returns an error if the controller request fails or the payload cannot be decoded.
    pub fn fetch_policy_group(&self, group_name: &str) -> Result<MihomoPolicyGroup, MihomoError> {
        let endpoint = format!("/proxies/{}", percent_encode_component(group_name));
        self.fetch_json(&endpoint)
    }

    /// Selects a candidate node for a Mihomo selector-style policy group.
    ///
    /// # Errors
    /// Returns an error if the controller request fails or the request body cannot be serialized.
    pub fn select_policy_group_node(
        &self,
        group_name: &str,
        selected_name: &str,
    ) -> Result<(), MihomoError> {
        let endpoint = format!("/proxies/{}", percent_encode_component(group_name));
        self.transport.put_json(
            &self.config,
            &endpoint,
            &serde_json::json!({ "name": selected_name }),
        )?;
        Ok(())
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

fn percent_encode_component(input: &str) -> String {
    let mut encoded = String::with_capacity(input.len());
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                encoded.push(char::from(byte));
            }
            _ => {
                encoded.push('%');
                encoded.push(hex_digit(byte >> 4));
                encoded.push(hex_digit(byte & 0x0f));
            }
        }
    }
    encoded
}

fn hex_digit(value: u8) -> char {
    match value {
        0..=9 => char::from(b'0' + value),
        10..=15 => char::from(b'A' + value - 10),
        _ => unreachable!("nibble must be in 0..=15"),
    }
}
