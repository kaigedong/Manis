#![forbid(unsafe_code)]

mod dns;
mod error;
mod parsing;
mod policy;
mod profile;
mod render;
mod render_api;
mod render_support;
mod rules;
mod source;
mod storage;
mod validation;
mod vless;

pub use dns::ProxyDnsServer;
#[cfg(target_os = "linux")]
pub use dns::{LINUX_TUN_DEVICE, LINUX_TUN_DNS_SERVER};
pub use error::{ProfileError, WriteError};
pub use parsing::{
    QxRule, QxRuleDiagnostic, QxRuleDiagnosticKind, QxRuleImportError, QxRuleKind, QxRuleList,
};
pub use policy::{
    MANIS_GLOBAL_GROUP_NAME, PolicyGroup, PolicyGroupKind, PolicyRef, UserPolicyGroup,
    UserPolicyGroupKind,
};
pub use profile::{LogLevel, Profile, ProfileMode};
pub use render_api::{render_mihomo_yaml, render_mihomo_yaml_with_tun};
pub use rules::{Rule, RuleCondition};
pub use source::{HealthCheck, Name, ProxyProvider, ProxyProviderSource, SecretUrl};
pub use storage::{replace_private_if_unchanged, write_private_atomic};
pub use vless::{OutboundProxy, VlessProxy};

pub(crate) use parsing::{
    decode_query_value, is_https_url, is_plain_value, is_subscription_url, is_uuid, is_vless_host,
    optional_vless_value, parse_vless_query, parse_vless_security, parse_vless_server,
    parse_vless_transport, require_vless_encryption,
};
pub(crate) use render_support::{policy_name, render_rule};
pub(crate) use source::MAX_SECRET_URL_BYTES;
pub(crate) use validation::{
    compile_user_groups, default_proxy_dns_servers, is_rule_value, is_safe_relative_path,
    validate_groups, validate_rule,
};
pub(crate) use vless::{
    MAX_VLESS_FIELD_BYTES, VlessSecurity, VlessSecurityOptions, VlessTransport,
};

const GROUP_TEST_URL: &str = "https://www.gstatic.com/generate_204";
