use gpui::{
    Context, Div, FontWeight, ParentElement, Role, Stateful, Styled, Window, div, prelude::*, px,
};
use manis_core::WindowSizeClass;

use crate::app::{AppUpdateState, ConfigurationSection, ManisApp, MihomoCoreUpdateState};
use crate::{
    app_update,
    components::{
        ActionRole, StatusTone, action_button, section_header, status_badge, style_action_button,
    },
    localization::{
        CountNoun, Language, LanguagePreference, Message, copy, save_language_preference_in,
    },
    theme::{ControlSize, Radius, Space, TextRole, Theme},
};

use super::{configuration_section_label, language_preference_label, panel_surface};

mod kernel;
mod language;
pub(in crate::app) mod navigation;
mod routing;
mod startup;
mod updates;
