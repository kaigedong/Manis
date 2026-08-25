use relay_core::{PolicyGroupId, ProxyId};

#[derive(Clone, Copy)]
pub(crate) struct DemoNode {
    pub id: ProxyId,
    pub name: &'static str,
    pub provider: &'static str,
    pub detail: &'static str,
    pub latency_ms: u16,
}

#[derive(Clone, Copy)]
pub(crate) struct DemoRule {
    pub index: u16,
    pub kind: &'static str,
    pub payload: &'static str,
}

#[derive(Clone, Copy)]
pub(crate) struct DemoPolicy {
    pub id: PolicyGroupId,
    pub name: &'static str,
    pub kind: &'static str,
    pub target: &'static str,
    pub rules_count: u16,
    pub nodes: &'static [DemoNode],
    pub rules: &'static [DemoRule],
}

const STREAMING_NODES: &[DemoNode] = &[
    DemoNode {
        id: ProxyId("hk-01"),
        name: "香港 HK-01",
        provider: "Provider A",
        detail: "HK · Hysteria2",
        latency_ms: 38,
    },
    DemoNode {
        id: ProxyId("sg-02"),
        name: "新加坡 SG-02",
        provider: "Provider A",
        detail: "SG · VLESS",
        latency_ms: 54,
    },
    DemoNode {
        id: ProxyId("jp-03"),
        name: "日本 JP-03",
        provider: "Provider B",
        detail: "JP · Trojan",
        latency_ms: 67,
    },
    DemoNode {
        id: ProxyId("us-01"),
        name: "美国 US-01",
        provider: "Provider A",
        detail: "US · VLESS",
        latency_ms: 142,
    },
];

const SEARCH_NODES: &[DemoNode] = &[
    STREAMING_NODES[1],
    STREAMING_NODES[2],
    STREAMING_NODES[0],
    STREAMING_NODES[3],
];

const STREAMING_RULES: &[DemoRule] = &[
    DemoRule {
        index: 18,
        kind: "DOMAIN-SUFFIX",
        payload: "youtube.com",
    },
    DemoRule {
        index: 19,
        kind: "DOMAIN-SUFFIX",
        payload: "netflix.com",
    },
    DemoRule {
        index: 20,
        kind: "GEOSITE",
        payload: "category-streaming",
    },
];

const SEARCH_RULES: &[DemoRule] = &[
    DemoRule {
        index: 27,
        kind: "DOMAIN-SUFFIX",
        payload: "openai.com",
    },
    DemoRule {
        index: 28,
        kind: "DOMAIN-SUFFIX",
        payload: "google.com",
    },
    DemoRule {
        index: 29,
        kind: "GEOSITE",
        payload: "category-ai-!cn",
    },
];

const POLICIES: &[DemoPolicy] = &[
    DemoPolicy {
        id: PolicyGroupId("streaming"),
        name: "视频服务",
        kind: "手动选择",
        target: "香港 HK-01",
        rules_count: 12,
        nodes: STREAMING_NODES,
        rules: STREAMING_RULES,
    },
    DemoPolicy {
        id: PolicyGroupId("search"),
        name: "搜索与 AI",
        kind: "自动选择",
        target: "新加坡 SG-02",
        rules_count: 8,
        nodes: SEARCH_NODES,
        rules: SEARCH_RULES,
    },
    DemoPolicy {
        id: PolicyGroupId("development"),
        name: "开发服务",
        kind: "故障转移",
        target: "日本 JP-03",
        rules_count: 17,
        nodes: STREAMING_NODES,
        rules: STREAMING_RULES,
    },
    DemoPolicy {
        id: PolicyGroupId("social"),
        name: "社交网络",
        kind: "负载均衡",
        target: "新加坡 SG-02",
        rules_count: 9,
        nodes: SEARCH_NODES,
        rules: SEARCH_RULES,
    },
    DemoPolicy {
        id: PolicyGroupId("direct"),
        name: "国内直连",
        kind: "直连",
        target: "DIRECT",
        rules_count: 31,
        nodes: &[DemoNode {
            id: ProxyId("direct"),
            name: "DIRECT",
            provider: "系统内置",
            detail: "本地网络",
            latency_ms: 4,
        }],
        rules: STREAMING_RULES,
    },
];

pub(crate) fn policies() -> &'static [DemoPolicy] {
    POLICIES
}

pub(crate) fn policy(id: PolicyGroupId) -> &'static DemoPolicy {
    POLICIES
        .iter()
        .find(|item| item.id == id)
        .map_or(&POLICIES[0], |item| item)
}

pub(crate) fn node(policy: &DemoPolicy, id: ProxyId) -> DemoNode {
    policy
        .nodes
        .iter()
        .find(|item| item.id == id)
        .copied()
        .unwrap_or(policy.nodes[0])
}
