/// Internal selector that keeps the node-page global exit independent from rule policy groups.
pub const MANIS_GLOBAL_GROUP_NAME: &str = "__MANIS_GLOBAL__";

use crate::Name;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PolicyGroup {
    pub name: Name,
    pub icon: Option<String>,
    pub kind: PolicyGroupKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PolicyGroupKind {
    Select {
        proxies: Vec<PolicyRef>,
        use_providers: Vec<Name>,
        filter: Option<String>,
    },
    UrlTest {
        proxies: Vec<PolicyRef>,
        use_providers: Vec<Name>,
        filter: Option<String>,
        url: String,
        interval_secs: u32,
        tolerance: Option<u16>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UserPolicyGroup {
    pub name: Name,
    pub icon: Option<String>,
    pub kind: UserPolicyGroupKind,
    pub provider_indexes: Vec<usize>,
    pub direct_proxies: Vec<Name>,
    pub direct_policies: Vec<PolicyRef>,
    pub filter: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UserPolicyGroupKind {
    Select,
    UrlTest { tolerance: u16, interval_secs: u32 },
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum PolicyRef {
    Direct,
    Reject,
    Group(Name),
    Proxy(Name),
}
