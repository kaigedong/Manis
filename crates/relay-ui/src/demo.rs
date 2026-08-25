use relay_core::{
    PolicyCandidateKind, PolicyCatalog, PolicyGroup, PolicyGroupId, PolicyGroupKind, PolicyNode,
    PolicyRule, ProxyId,
};

pub(crate) fn catalog() -> PolicyCatalog {
    let streaming_nodes = common_nodes();
    let search_nodes = vec![
        streaming_nodes[1].clone(),
        streaming_nodes[2].clone(),
        streaming_nodes[0].clone(),
        streaming_nodes[3].clone(),
    ];
    let streaming_rules = streaming_rules();
    let search_rules = search_rules();

    let primary = group(
        "streaming",
        "视频服务",
        PolicyGroupKind::Selector,
        "香港 HK-01",
        12,
        streaming_nodes.clone(),
        streaming_rules.clone(),
    );
    let remaining = vec![
        group(
            "search",
            "搜索与 AI",
            PolicyGroupKind::UrlTest,
            "新加坡 SG-02",
            8,
            search_nodes.clone(),
            search_rules,
        ),
        group(
            "development",
            "开发服务",
            PolicyGroupKind::Fallback,
            "日本 JP-03",
            17,
            streaming_nodes.clone(),
            streaming_rules.clone(),
        ),
        group(
            "social",
            "社交网络",
            PolicyGroupKind::LoadBalance,
            "新加坡 SG-02",
            9,
            search_nodes,
            streaming_rules.clone(),
        ),
        group(
            "direct",
            "国内直连",
            PolicyGroupKind::Direct,
            "DIRECT",
            31,
            vec![node(
                "direct",
                "DIRECT",
                Some("系统内置"),
                "本地网络",
                Some(4),
            )],
            streaming_rules,
        ),
    ];

    PolicyCatalog::from_primary(primary, remaining)
}

#[allow(clippy::too_many_arguments)]
fn group(
    id: &str,
    name: &str,
    kind: PolicyGroupKind,
    target: &str,
    rules_total: usize,
    nodes: Vec<PolicyNode>,
    rules: Vec<PolicyRule>,
) -> PolicyGroup {
    PolicyGroup {
        id: PolicyGroupId::new(id),
        name: name.to_owned(),
        kind,
        target: target.to_owned(),
        nodes,
        rules,
        rules_total,
    }
}

fn common_nodes() -> Vec<PolicyNode> {
    vec![
        node(
            "hk-01",
            "香港 HK-01",
            Some("Provider A"),
            "HK · Hysteria2",
            Some(38),
        ),
        node(
            "sg-02",
            "新加坡 SG-02",
            Some("Provider A"),
            "SG · VLESS",
            Some(54),
        ),
        node(
            "jp-03",
            "日本 JP-03",
            Some("Provider B"),
            "JP · Trojan",
            Some(67),
        ),
        node(
            "us-01",
            "美国 US-01",
            Some("Provider A"),
            "US · VLESS",
            Some(142),
        ),
    ]
}

fn node(
    id: &str,
    name: &str,
    provider: Option<&str>,
    detail: &str,
    latency_ms: Option<u16>,
) -> PolicyNode {
    PolicyNode {
        id: ProxyId::new(id),
        name: name.to_owned(),
        kind: PolicyCandidateKind::Node,
        provider: provider.map(str::to_owned),
        detail: detail.to_owned(),
        latency_ms,
        alive: Some(true),
    }
}

fn streaming_rules() -> Vec<PolicyRule> {
    vec![
        rule(18, "DOMAIN-SUFFIX", "youtube.com"),
        rule(19, "DOMAIN-SUFFIX", "netflix.com"),
        rule(20, "GEOSITE", "category-streaming"),
    ]
}

fn search_rules() -> Vec<PolicyRule> {
    vec![
        rule(27, "DOMAIN-SUFFIX", "openai.com"),
        rule(28, "DOMAIN-SUFFIX", "google.com"),
        rule(29, "GEOSITE", "category-ai-!cn"),
    ]
}

fn rule(index: u32, kind: &str, payload: &str) -> PolicyRule {
    PolicyRule {
        index,
        kind: kind.to_owned(),
        payload: payload.to_owned(),
        hit_count: None,
        disabled: false,
    }
}
