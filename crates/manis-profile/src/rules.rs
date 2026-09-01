use crate::PolicyRef;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RuleCondition {
    Domain(String),
    DomainKeyword(String),
    DomainSuffix(String),
    DomainWildcard(String),
    IpCidr { value: String, no_resolve: bool },
    IpAsn { asn: u32, no_resolve: bool },
    GeoIp { country: String, no_resolve: bool },
    DstPort(u16),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Rule {
    Domain {
        value: String,
        policy: PolicyRef,
    },
    DomainKeyword {
        value: String,
        policy: PolicyRef,
    },
    DomainSuffix {
        value: String,
        policy: PolicyRef,
    },
    DomainWildcard {
        value: String,
        policy: PolicyRef,
    },
    IpCidr {
        value: String,
        policy: PolicyRef,
        no_resolve: bool,
    },
    IpAsn {
        asn: u32,
        policy: PolicyRef,
        no_resolve: bool,
    },
    GeoIp {
        country: String,
        policy: PolicyRef,
        no_resolve: bool,
    },
    /// Matches by destination port, which is how traffic bypasses the proxy per protocol.
    DstPort {
        port: u16,
        policy: PolicyRef,
    },
    All {
        conditions: Vec<RuleCondition>,
        policy: PolicyRef,
    },
    Match {
        policy: PolicyRef,
    },
}
