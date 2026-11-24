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
    ProcessName(String),
    ProcessPath(String),
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
    /// Matches the client process name the kernel resolved for the connection.
    ProcessName {
        value: String,
        policy: PolicyRef,
    },
    /// Matches the executable path the kernel resolved for the connection, whole-string and
    /// case-insensitive, so a versioned path stops matching when the app updates.
    ProcessPath {
        value: String,
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
