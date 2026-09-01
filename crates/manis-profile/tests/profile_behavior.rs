use std::fs;
use std::path::Path;

use manis_profile::{
    HealthCheck, LogLevel, MANIS_GLOBAL_GROUP_NAME, Name, OutboundProxy, PolicyGroup,
    PolicyGroupKind, PolicyRef, Profile, ProfileError, ProfileMode, ProxyDnsServer, ProxyProvider,
    QxRuleDiagnosticKind, QxRuleKind, QxRuleList, Rule, SecretUrl, SingBoxOptions, UserPolicyGroup,
    UserPolicyGroupKind, VlessProxy, render_mihomo_yaml, render_mihomo_yaml_with_tun,
    render_sing_box_json, write_private_atomic,
};

fn fixture_secret() -> SecretUrl {
    SecretUrl::parse_https("https://subscription.example.invalid/client?token=fixture-secret")
        .expect("fixture url is valid")
}

fn global_exit_policy() -> PolicyRef {
    PolicyRef::Group(Name::parse(MANIS_GLOBAL_GROUP_NAME).expect("valid internal group"))
}

#[path = "profile_behavior/groups.rs"]
mod groups;
#[path = "profile_behavior/rules.rs"]
mod rules;
#[path = "profile_behavior/sources.rs"]
mod sources;
#[path = "profile_behavior/validation_storage.rs"]
mod validation_storage;
#[path = "profile_behavior/vless_render.rs"]
mod vless_render;

fn test_temp_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("{name}-{}", std::process::id()));
    if Path::new(&dir).exists() {
        fs::remove_dir_all(&dir).expect("cleanup stale temp");
    }
    fs::create_dir(&dir).expect("create temp");
    dir
}
