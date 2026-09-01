use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex};

use manis_core::PolicyGroupId;

pub(in crate::app) struct PolicyBenchmarkRun {
    pub(in crate::app) key: String,
    pub(in crate::app) generation: u64,
    pub(in crate::app) group_id: PolicyGroupId,
    pub(in crate::app) group_kind: manis_core::PolicyGroupKind,
    pub(in crate::app) total: usize,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub(in crate::app) struct GroupBenchmarkSummary {
    pub(in crate::app) total: usize,
    pub(in crate::app) succeeded: usize,
    pub(in crate::app) failed: usize,
    pub(in crate::app) minimum_ms: Option<u16>,
    pub(in crate::app) maximum_ms: Option<u16>,
    pub(in crate::app) average_ms: Option<u16>,
}

impl GroupBenchmarkSummary {
    pub(in crate::app) fn from_delays(total: usize, delays: impl IntoIterator<Item = u16>) -> Self {
        let delays = delays
            .into_iter()
            .filter(|delay| *delay > 0)
            .collect::<Vec<_>>();
        let succeeded = delays.len().min(total);
        let sum = delays.iter().map(|delay| u64::from(*delay)).sum::<u64>();
        let divisor = u64::try_from(delays.len()).unwrap_or(1);
        Self {
            total,
            succeeded,
            failed: total.saturating_sub(succeeded),
            minimum_ms: delays.iter().copied().min(),
            maximum_ms: delays.iter().copied().max(),
            average_ms: (!delays.is_empty()).then(|| {
                let rounded = (sum + divisor / 2) / divisor;
                u16::try_from(rounded).unwrap_or(u16::MAX)
            }),
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub(in crate::app) enum GroupBenchmarkState {
    #[default]
    Idle,
    Running {
        generation: u64,
        results: BTreeMap<String, Option<u16>>,
    },
    Complete {
        generation: u64,
        summary: GroupBenchmarkSummary,
        delays: BTreeMap<String, u16>,
    },
    Failed {
        generation: u64,
        #[serde(default)]
        message: Option<String>,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::app) enum GroupBenchmarkNodeState {
    Idle,
    Pending,
    Measured(u16),
    Failed,
}

pub(in crate::app) type GroupBenchmarkProgressQueue = Arc<Mutex<VecDeque<(String, Option<u16>)>>>;

impl GroupBenchmarkState {
    pub(in crate::app) fn running(generation: u64) -> Self {
        Self::Running {
            generation,
            results: BTreeMap::new(),
        }
    }

    pub(in crate::app) fn is_running(&self) -> bool {
        matches!(self, Self::Running { .. })
    }

    pub(in crate::app) fn complete_delays(&self) -> Option<&BTreeMap<String, u16>> {
        match self {
            Self::Complete { delays, .. } => Some(delays),
            _ => None,
        }
    }

    pub(in crate::app) fn record(
        &mut self,
        generation: u64,
        name: &str,
        delay: Option<u16>,
    ) -> bool {
        let Self::Running {
            generation: active,
            results,
        } = self
        else {
            return false;
        };
        if *active != generation {
            return false;
        }
        results.insert(name.to_owned(), delay.filter(|delay| *delay > 0));
        true
    }

    pub(in crate::app) fn node_state(&self, name: &str) -> GroupBenchmarkNodeState {
        match self {
            Self::Idle => GroupBenchmarkNodeState::Idle,
            Self::Running { results, .. } => match results.get(name) {
                Some(Some(delay)) => GroupBenchmarkNodeState::Measured(*delay),
                Some(None) => GroupBenchmarkNodeState::Failed,
                None => GroupBenchmarkNodeState::Pending,
            },
            Self::Complete { delays, .. } => {
                delays.get(name).copied().filter(|delay| *delay > 0).map_or(
                    GroupBenchmarkNodeState::Failed,
                    GroupBenchmarkNodeState::Measured,
                )
            }
            Self::Failed { .. } => GroupBenchmarkNodeState::Failed,
        }
    }

    pub(in crate::app) fn complete(
        &mut self,
        generation: u64,
        total: usize,
        delays: BTreeMap<String, u16>,
    ) -> bool {
        if !matches!(self, Self::Running { generation: current, .. } if *current == generation) {
            return false;
        }
        let summary = GroupBenchmarkSummary::from_delays(total, delays.values().copied());
        *self = Self::Complete {
            generation,
            summary,
            delays,
        };
        true
    }

    pub(in crate::app) fn fail(&mut self, generation: u64, message: Option<String>) -> bool {
        if !matches!(self, Self::Running { generation: current, .. } if *current == generation) {
            return false;
        }
        *self = Self::Failed {
            generation,
            message,
        };
        true
    }
}
