//! Subscriptions page shared by Android and the desktop shells.

use serde::{Deserialize, Serialize};

/// One saved subscription, before display formatting.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionRowInput {
    pub name: String,
    pub url: String,
    pub enabled: bool,
    #[serde(default)]
    pub id: Option<String>,
}

/// One saved manual node.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionNodeInput {
    pub name: String,
    pub enabled: bool,
    #[serde(default)]
    pub id: Option<String>,
}

/// One managed policy group.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionGroupInput {
    pub name: String,
    pub strategy: String,
    #[serde(default)]
    pub subscription_indexes: Vec<usize>,
    #[serde(default)]
    pub node_indexes: Vec<usize>,
}

/// Raw workspace pieces for the subscriptions page.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionsPageInput {
    pub subscriptions: Vec<SubscriptionRowInput>,
    pub nodes: Vec<SubscriptionNodeInput>,
    pub groups: Vec<SubscriptionGroupInput>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionRowView {
    pub index: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub name: String,
    pub display_url: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionNodeView {
    pub index: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub name: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionGroupView {
    pub index: usize,
    pub name: String,
    pub strategy: String,
    pub subscription_indexes: Vec<usize>,
    pub node_indexes: Vec<usize>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionsPageView {
    pub subscriptions: Vec<SubscriptionRowView>,
    pub nodes: Vec<SubscriptionNodeView>,
    pub groups: Vec<SubscriptionGroupView>,
}

impl SubscriptionsPageView {
    /// Formats the workspace pieces for the page.
    #[must_use]
    pub fn build(input: SubscriptionsPageInput) -> Self {
        Self {
            subscriptions: input
                .subscriptions
                .into_iter()
                .enumerate()
                .map(|(index, row)| SubscriptionRowView {
                    index,
                    id: row.id,
                    name: if row.name.trim().is_empty() {
                        format!("订阅 {}", index + 1)
                    } else {
                        row.name
                    },
                    display_url: mask_url(&row.url),
                    enabled: row.enabled,
                })
                .collect(),
            nodes: input
                .nodes
                .into_iter()
                .enumerate()
                .map(|(index, row)| SubscriptionNodeView {
                    index,
                    id: row.id,
                    name: row.name,
                    enabled: row.enabled,
                })
                .collect(),
            groups: input
                .groups
                .into_iter()
                .enumerate()
                .map(|(index, row)| SubscriptionGroupView {
                    index,
                    name: row.name,
                    strategy: row.strategy,
                    subscription_indexes: row.subscription_indexes,
                    node_indexes: row.node_indexes,
                })
                .collect(),
        }
    }

    /// Parses a JSON request and formats the page.
    ///
    /// # Errors
    /// Returns a JSON error when the input does not match the subscriptions shape.
    pub fn from_json(request: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str::<SubscriptionsPageInput>(request).map(Self::build)
    }
}

/// Hides credentials and paths while keeping the scheme and host visible.
#[must_use]
pub fn mask_url(url: &str) -> String {
    let scheme = url.split_once("://").map_or("", |(scheme, _)| scheme);
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let host = rest.split('/').next().unwrap_or(rest);
    let host = host.split('?').next().unwrap_or(host);
    let host = host.rsplit('@').next().unwrap_or(host);
    if scheme.is_empty() {
        host.to_owned()
    } else {
        format!("{scheme}://{host}/…")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_indices_and_masks_urls() {
        let view = SubscriptionsPageView::from_json(
            r#"{
                "subscriptions": [
                    {"name": "", "url": "https://user:pass@example.test/sub?token=1", "enabled": true},
                    {"name": "Naiu", "url": "https://naiu.test/api", "enabled": false, "id": "s1"}
                ],
                "nodes": [{"name": "sozenth", "enabled": true}],
                "groups": [{"name": "HK", "strategy": "select", "subscriptionIndexes": [0], "nodeIndexes": [1]}]
            }"#,
        )
        .expect("subscriptions page");
        assert_eq!(view.subscriptions[0].index, 0);
        assert_eq!(view.subscriptions[0].name, "订阅 1");
        assert_eq!(view.subscriptions[0].display_url, "https://example.test/…");
        assert_eq!(view.subscriptions[1].id.as_deref(), Some("s1"));
        assert_eq!(view.groups[0].subscription_indexes, [0]);
    }
}
