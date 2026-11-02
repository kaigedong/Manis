//! Settings page shared by Android and the desktop shells.

use serde::{Deserialize, Serialize};

/// Resolver settings shown on the page.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsProxyDns {
    pub mode: String,
    #[serde(default)]
    pub custom: Vec<String>,
    #[serde(default)]
    pub discovered: Vec<String>,
    pub label: String,
}

/// Download size and link of an available update.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsUpdateInfo {
    pub version: String,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub release_url: String,
}

/// Current update flow state.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsUpdate {
    pub state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub update: Option<SettingsUpdateInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl Default for SettingsUpdate {
    fn default() -> Self {
        Self {
            state: "idle".into(),
            update: None,
            progress: None,
            message: None,
        }
    }
}

/// Raw values for the settings page.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsPageInput {
    pub status: String,
    pub mode: String,
    pub proxy_dns: SettingsProxyDns,
    pub kernel_version: String,
    pub summary: String,
    pub subscriptions: usize,
    pub nodes: usize,
    pub groups: usize,
    pub abi: String,
    pub data_dir: String,
    pub installed_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_start: Option<bool>,
    #[serde(default)]
    pub update: SettingsUpdate,
}

/// The settings payload the NSH root and Android consume.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsPageView {
    pub status: String,
    pub mode: String,
    pub proxy_dns: SettingsProxyDns,
    pub kernel_version: String,
    pub summary: String,
    pub subscriptions: usize,
    pub nodes: usize,
    pub groups: usize,
    pub abi: String,
    pub data_dir: String,
    pub installed_version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_start: Option<bool>,
    pub update: SettingsUpdate,
}

impl SettingsPageView {
    /// Normalizes blank status values and builds the payload.
    #[must_use]
    pub fn build(input: SettingsPageInput) -> Self {
        Self {
            status: non_blank(input.status, "未连接"),
            mode: input.mode,
            proxy_dns: input.proxy_dns,
            kernel_version: non_blank(input.kernel_version, "—"),
            summary: non_blank(input.summary, "—"),
            subscriptions: input.subscriptions,
            nodes: input.nodes,
            groups: input.groups,
            abi: non_blank(input.abi, "unknown"),
            data_dir: non_blank(input.data_dir, "—"),
            installed_version: non_blank(input.installed_version, "未知"),
            auto_start: input.auto_start,
            update: input.update,
        }
    }

    /// Parses a JSON request and builds the page.
    ///
    /// # Errors
    /// Returns a JSON error when the input does not match the settings shape.
    pub fn from_json(request: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str::<SettingsPageInput>(request).map(Self::build)
    }
}

fn non_blank(value: String, fallback: &str) -> String {
    if value.trim().is_empty() {
        fallback.to_owned()
    } else {
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fills_blank_values_and_keeps_update() {
        let view = SettingsPageView::from_json(
            r#"{
                "status": "",
                "mode": "rule",
                "proxyDns": {"mode": "doh", "label": "DoH"},
                "kernelVersion": "",
                "summary": "",
                "subscriptions": 1,
                "nodes": 2,
                "groups": 3,
                "abi": "arm64-v8a",
                "dataDir": "files/mihomo",
                "installedVersion": "",
                "update": {"state": "available", "update": {"version": "1.2.3", "size": 42}}
            }"#,
        )
        .expect("settings page");
        assert_eq!(view.status, "未连接");
        assert_eq!(view.kernel_version, "—");
        assert_eq!(view.summary, "—");
        assert_eq!(view.installed_version, "未知");
        assert_eq!(view.update.update.expect("update").version, "1.2.3");
        assert!(view.auto_start.is_none());
    }
}
