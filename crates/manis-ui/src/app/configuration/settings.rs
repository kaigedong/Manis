use gpui::{
    Context, Div, FontWeight, ParentElement, Role, Stateful, Styled, Window, div, prelude::*, px,
};
use manis_core::WindowSizeClass;

use crate::app::{
    AppUpdateState, ConfigurationSection, ManisApp, MihomoCoreUpdateState, proxy_mode_label,
    routing_mode_label,
};
use crate::{
    app_update,
    components::{
        ActionRole, StatusTone, action_button, page_heading, section_heading, status_badge,
        style_action_button,
    },
    localization::{
        CountNoun, Language, LanguagePreference, Message, copy, save_language_preference_in,
    },
    mihomo::{self},
    theme::{ControlSize, Radius, Space, TextRole, Theme},
};

use super::{
    configuration_section_detail, configuration_section_label, language_preference_label,
    panel_surface,
};

mod advanced;
mod kernel;
mod language;
pub(in crate::app) mod navigation;
mod routing;
mod updates;
