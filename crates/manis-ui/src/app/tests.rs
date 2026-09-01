use std::collections::BTreeMap;
use std::fs;

use manis_core::{
    ManagedPolicyGroup, NodeIdentity, PolicyCandidateKind, PolicyCatalog, PolicyGroup,
    PolicyGroupId, PolicyGroupKind, PolicyNode, ProxyId,
};

use manis_core::ProxyMode;

use super::PreferencePersistence;
use super::{
    ControllerReadiness, DueRemoteSource, GroupBenchmarkState, GroupBenchmarkSummary,
    ImportedSubscription, ImportedSubscriptionState, ManisApp, ProxyModeBlock, SourceRuntimeApply,
    TunSupport, controller_status_label, next_due_remote_source, policy_target_is_selectable,
    proxy_mode_block, stored_workspace, tun_dns_log_details,
};
use crate::subscription::SourceKind;
use crate::{
    localization::Language,
    mihomo::{self, ControllerState},
};

fn complete_benchmark(delay_name: &str, delay_ms: u16) -> GroupBenchmarkState {
    GroupBenchmarkState::Complete {
        generation: 1,
        summary: GroupBenchmarkSummary {
            total: 1,
            succeeded: 1,
            failed: 0,
            minimum_ms: Some(delay_ms),
            maximum_ms: Some(delay_ms),
            average_ms: Some(delay_ms),
        },
        delays: BTreeMap::from([(delay_name.to_owned(), delay_ms)]),
    }
}

#[path = "tests/benchmark.rs"]
mod benchmark;
#[path = "tests/policy.rs"]
mod policy;
#[path = "tests/proxy_mode.rs"]
mod proxy_mode;
#[path = "tests/runtime.rs"]
mod runtime;
#[path = "tests/subscription.rs"]
mod subscription;
