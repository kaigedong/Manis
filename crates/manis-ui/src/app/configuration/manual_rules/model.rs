use super::{Entity, SubscriptionTextInput};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::app) enum ManualRulePopover {
    Kind(usize),
    Target,
}

pub(in crate::app) struct ManualRuleConditionEditor {
    pub(in crate::app) kind: crate::manual_rule::ManualRuleKind,
    pub(in crate::app) input: Entity<SubscriptionTextInput>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(in crate::app) enum ManualRuleEditorState {
    #[default]
    Closed,
    Creating,
    Editing(usize),
}

impl ManualRuleEditorState {
    pub(in crate::app) const fn is_open(self) -> bool {
        !matches!(self, Self::Closed)
    }

    pub(in crate::app) const fn editing_index(self) -> Option<usize> {
        match self {
            Self::Editing(index) => Some(index),
            Self::Closed | Self::Creating => None,
        }
    }
}
