//! Connection page shared by Android and the desktop shells.

use serde::{Deserialize, Serialize};

/// Values the platform host already knows about the live connection.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionPageInput {
    pub active: bool,
    pub node: String,
    pub mode: String,
    pub summary: String,
    pub error: String,
    pub unreachable: bool,
    pub warnings: Vec<String>,
    pub warning_count: usize,
    pub has_sources: bool,
}

/// The connection page payload the NSH root consumes.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionPageView {
    pub active: bool,
    pub node: String,
    pub mode: String,
    pub summary: String,
    pub error: String,
    pub unreachable: bool,
    pub warnings: Vec<String>,
    pub warning_count: usize,
    pub has_sources: bool,
}

impl ConnectionPageView {
    /// Builds the page from platform values.
    #[must_use]
    pub fn build(input: ConnectionPageInput) -> Self {
        Self {
            active: input.active,
            node: input.node,
            mode: input.mode,
            summary: input.summary,
            error: input.error,
            unreachable: input.unreachable,
            warnings: input.warnings.into_iter().take(4).collect(),
            warning_count: input.warning_count,
            has_sources: input.has_sources,
        }
    }

    /// Parses a JSON input and builds the page.
    ///
    /// # Errors
    /// Returns a JSON error when the input does not match the connection shape.
    pub fn from_json(request: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str::<ConnectionPageInput>(request).map(Self::build)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limits_warnings_and_keeps_total() {
        let view = ConnectionPageView::from_json(
            r#"{
                "active": true,
                "node": "HK 04",
                "mode": "rule",
                "summary": "ok",
                "error": "",
                "unreachable": false,
                "warnings": ["a", "b", "c", "d", "e"],
                "warningCount": 5,
                "hasSources": true
            }"#,
        )
        .expect("connection page");
        assert_eq!(view.warnings, ["a", "b", "c", "d"]);
        assert_eq!(view.warning_count, 5);
        assert_eq!(view.node, "HK 04");
        assert!(view.has_sources);
    }
}
