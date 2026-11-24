use super::{Entity, LocalizedText, SubscriptionTextInput};

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
pub(in crate::app) enum ManualRuleInsert {
    /// Appended last, so older rules keep precedence.
    #[default]
    Append,
    /// Inserted first, used when a connection row asks for a rule that must win.
    Prepend,
}

/// One value a connection already knows, offered as a chip so the rule editor never asks the user
/// to retype a process name or a process path.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::app) struct ManualRulePrefill {
    pub(in crate::app) kind: crate::manual_rule::ManualRuleKind,
    pub(in crate::app) value: String,
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

/// Which rule list a rule editor or rule row acts on.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(in crate::app) enum RuleRowScope {
    #[default]
    Manual,
    Local(usize),
}

impl RuleRowScope {
    pub(in crate::app) const fn local_group(self) -> Option<usize> {
        match self {
            Self::Manual => None,
            Self::Local(index) => Some(index),
        }
    }
}

/// Everything the group editor is editing, kept out of the application state proper.
pub(in crate::app) struct LocalRuleGroupDraft {
    pub(in crate::app) name_input: Option<Entity<SubscriptionTextInput>>,
    pub(in crate::app) target: String,
    pub(in crate::app) enabled: bool,
    pub(in crate::app) target_popover_open: bool,
    pub(in crate::app) error: Option<LocalizedText>,
}

impl LocalRuleGroupDraft {
    pub(in crate::app) fn new(target: String) -> Self {
        Self {
            name_input: None,
            target,
            enabled: true,
            target_popover_open: false,
            error: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(in crate::app) enum LocalRuleGroupEditorState {
    #[default]
    Closed,
    Creating,
    Editing(usize),
}

impl LocalRuleGroupEditorState {
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
