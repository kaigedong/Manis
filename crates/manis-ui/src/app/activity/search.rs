use manis_mihomo::Connection;
use manis_profile::MANIS_GLOBAL_GROUP_NAME;

pub(super) struct ConnectionSearchText {
    normalized: String,
}

impl ConnectionSearchText {
    pub(super) fn new(connection: &Connection, rule_group: Option<&str>) -> Self {
        let mut normalized = String::new();
        for value in [
            connection.metadata.sniff_host.as_deref(),
            connection.metadata.host.as_deref(),
            connection.metadata.destination_ip.as_deref(),
            connection.metadata.remote_destination.as_deref(),
            connection.metadata.destination_port.as_deref(),
            connection.metadata.process.as_deref(),
            connection.metadata.process_path.as_deref(),
            connection.rule.as_deref(),
            connection.rule_payload.as_deref(),
            rule_group,
        ]
        .into_iter()
        .flatten()
        .chain(user_route_stages(&connection.chains))
        {
            push_normalized_field(&mut normalized, value);
        }
        Self { normalized }
    }

    pub(super) fn matches(&self, normalized_query: &str) -> bool {
        normalized_query.is_empty() || self.normalized.contains(normalized_query)
    }
}

pub(super) fn user_route_stages(chains: &[String]) -> impl DoubleEndedIterator<Item = &str> {
    chains.iter().map(|stage| stage.trim()).filter(|stage| {
        !stage.is_empty() && *stage != MANIS_GLOBAL_GROUP_NAME && *stage != "GLOBAL"
    })
}

fn push_normalized_field(output: &mut String, value: &str) {
    output.extend(value.chars().flat_map(char::to_lowercase));
    output.push('\n');
}

#[cfg(test)]
mod tests {
    use manis_mihomo::{Connection, ConnectionMetadata};

    fn connection() -> Connection {
        Connection {
            id: Some("fixture".to_owned()),
            metadata: ConnectionMetadata {
                host: Some("www.example.com".to_owned()),
                process: Some("Browser".to_owned()),
                destination_port: Some("443".to_owned()),
                ..ConnectionMetadata::default()
            },
            upload: 12,
            download: 34,
            start: None,
            chains: vec![
                "Hong Kong".to_owned(),
                "__MANIS_GLOBAL__".to_owned(),
                "GLOBAL".to_owned(),
            ],
            provider_chains: Vec::new(),
            rule: Some("DomainSuffix".to_owned()),
            rule_payload: Some("example.com".to_owned()),
        }
    }

    #[test]
    fn activity_search_matches_target_process_rule_route_and_group() {
        let search = super::ConnectionSearchText::new(&connection(), Some("Streaming rules"));

        for query in [
            "EXAMPLE",
            "browser",
            "domainsuffix",
            "hong kong",
            "streaming",
            "",
        ] {
            assert!(search.matches(&query.to_lowercase()));
        }
        assert!(!search.matches("telegram"));
        assert!(!search.matches("__manis_global__"));
    }

    #[test]
    fn user_route_stages_hide_internal_runtime_groups() {
        let chains = vec![
            "Hong Kong".to_owned(),
            "__MANIS_GLOBAL__".to_owned(),
            "GLOBAL".to_owned(),
            " ".to_owned(),
        ];
        assert_eq!(
            super::user_route_stages(&chains).collect::<Vec<_>>(),
            vec!["Hong Kong"]
        );
    }
}
