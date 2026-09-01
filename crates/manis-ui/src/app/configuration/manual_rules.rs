use gpui::{
    AnyElement, Context, Div, Entity, Focusable, FontWeight, ParentElement, Role, Stateful, Styled,
    Window, div, prelude::*, px,
};
use gpui_component::{
    Disableable, IconName,
    button::{Button, ButtonVariant, ButtonVariants},
    collapsible::Collapsible,
    dialog::Dialog,
    menu::{ContextMenuExt, PopupMenuItem},
};
use manis_core::WindowSizeClass;
use manis_profile::QxRuleKind;

use crate::app::{ManisApp, QxRuleImportFeedback, QxRuleList};
use crate::{
    components::{
        ActionRole, dialog_footer_surface, dialog_header_surface, empty_state, style_action_button,
        surface_dialog,
    },
    diagnostics::{LogLevel, record_event},
    localization::{Language, Message, copy},
    mihomo::{self, SubscriptionStoreError},
    subscription_input::{SubscriptionTextInput, TextInputSpec},
    theme::{ControlSize, Radius, Space, TextRole, Theme},
};

use super::{
    MANUAL_RULES_EXPANSION_KEY, MAX_MANUAL_RULE_INPUT_BYTES, ManualRuleKeyboardAction,
    RuleGroupRenderContext, field_label, manual_rule_error_label, manual_rule_keyboard_action,
    manual_rule_kind_detail, manual_rule_placeholder, rule_group_is_open,
    rule_source_expansion_key, source_update_label,
};

mod editor;
mod list;
mod model;
mod persistence;
pub(in crate::app) use model::{
    ManualRuleConditionEditor, ManualRuleEditorState, ManualRulePopover,
};
