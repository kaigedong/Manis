use super::{
    GroupBenchmarkNodeState, Language, ManagedPolicyGroup, ManagedPolicyIcon, PolicyGroup,
    PolicyGroupId, PolicyNode, Theme,
};
use manis_core::ProxyId;

#[derive(Clone)]
pub(in crate::app) struct PolicySelectionRequest {
    pub(in crate::app) group_id: PolicyGroupId,
    pub(in crate::app) group_name: String,
    pub(in crate::app) node_id: ProxyId,
    pub(in crate::app) node_name: String,
}

pub(in crate::app) struct PolicyNodeRowContext {
    pub(in crate::app) source: String,
    pub(in crate::app) selection: PolicySelectionRequest,
    pub(in crate::app) current: bool,
    pub(in crate::app) manually_selectable: bool,
    pub(in crate::app) selection_busy: bool,
    pub(in crate::app) benchmark_state: GroupBenchmarkNodeState,
    pub(in crate::app) language: Language,
    pub(in crate::app) theme: Theme,
}

pub(in crate::app) struct OfflinePolicyCardView<'a> {
    pub(in crate::app) policy: &'a ManagedPolicyGroup,
    pub(in crate::app) candidates: Vec<PolicyNode>,
    pub(in crate::app) selected_name: Option<String>,
    pub(in crate::app) expanded: bool,
    pub(in crate::app) benchmarking: bool,
}

pub(in crate::app) struct PolicyListCardView<'a> {
    pub(in crate::app) item: &'a PolicyGroup,
    pub(in crate::app) expanded: bool,
    pub(in crate::app) editable_group_id: Option<String>,
    pub(in crate::app) icon: ManagedPolicyIcon,
    pub(in crate::app) benchmark_key: String,
    pub(in crate::app) benchmarking: bool,
}
