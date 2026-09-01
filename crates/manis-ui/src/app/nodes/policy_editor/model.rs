use super::{
    BTreeMap, BTreeSet, GroupBenchmarkState, ManagedPolicyGroup, ManagedPolicyIcon,
    ManagedPolicyStrategy, NodeIdentity, mihomo,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(in crate::app) enum ManagedPolicyMutationState {
    #[default]
    Idle,
    Saving,
    Removing,
}

impl ManagedPolicyMutationState {
    pub(in crate::app) const fn is_busy(self) -> bool {
        !matches!(self, Self::Idle)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(in crate::app) enum PolicyCandidateMatcherKind {
    #[default]
    All,
    NameContains,
    Explicit,
}

#[derive(Clone, Debug)]
pub(in crate::app) struct ManagedPolicyDraft {
    pub(in crate::app) editing_id: Option<String>,
    pub(in crate::app) icon: ManagedPolicyIcon,
    pub(in crate::app) strategy: ManagedPolicyStrategy,
    pub(in crate::app) test_interval_secs: u32,
    pub(in crate::app) switch_tolerance_ms: u16,
    pub(in crate::app) matcher_kind: PolicyCandidateMatcherKind,
    pub(in crate::app) explicit_members: BTreeSet<NodeIdentity>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::app) enum PolicyEditorPopover {
    Strategy,
    Icon,
    CandidateMode,
    CandidateNodes,
    Interval,
    Tolerance,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(in crate::app) enum ManagedPolicyRuntimeState {
    #[default]
    LocalOnly,
    Ready {
        generation: u64,
        current: Option<String>,
        candidates: BTreeSet<String>,
    },
    Selecting {
        generation: u64,
        current: Option<String>,
        candidates: BTreeSet<String>,
        pending: String,
    },
}

impl ManagedPolicyRuntimeState {
    pub(in crate::app) fn begin_selection(&mut self, generation: u64, selected: &str) -> bool {
        let Self::Ready {
            current,
            candidates,
            ..
        } = self
        else {
            return false;
        };
        if !candidates.contains(selected) {
            return false;
        }
        *self = Self::Selecting {
            generation,
            current: current.take(),
            candidates: std::mem::take(candidates),
            pending: selected.to_owned(),
        };
        true
    }
}

pub(in crate::app) struct ManagedPolicyState {
    pub(in crate::app) groups: Vec<ManagedPolicyGroup>,
    pub(in crate::app) node_selections: mihomo::NodeSelectionPreferences,
    pub(in crate::app) draft: Option<ManagedPolicyDraft>,
    pub(in crate::app) editor_popover: Option<PolicyEditorPopover>,
    pub(in crate::app) pending_benchmark_name: Option<String>,
    pub(in crate::app) benchmarks: BTreeMap<String, GroupBenchmarkState>,
    pub(in crate::app) benchmark_generation: u64,
    pub(in crate::app) active_benchmark_generation: Option<u64>,
    pub(in crate::app) runtime_states: BTreeMap<String, ManagedPolicyRuntimeState>,
    pub(in crate::app) runtime_generation: u64,
    pub(in crate::app) mutation_state: ManagedPolicyMutationState,
}

impl ManagedPolicyState {
    pub(in crate::app) fn restored(
        groups: Vec<ManagedPolicyGroup>,
        node_selections: mihomo::NodeSelectionPreferences,
        benchmarks: BTreeMap<String, GroupBenchmarkState>,
    ) -> Self {
        Self {
            groups,
            node_selections,
            draft: None,
            editor_popover: None,
            pending_benchmark_name: None,
            benchmarks,
            benchmark_generation: 0,
            active_benchmark_generation: None,
            runtime_states: BTreeMap::new(),
            runtime_generation: 0,
            mutation_state: ManagedPolicyMutationState::Idle,
        }
    }
}
