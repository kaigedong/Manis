use gpui::{Div, IntoElement, ParentElement, Stateful, Styled, div, prelude::*};
use gpui_component::{Disableable, IconName, button::Button};
use manis_core::ProxyMode;

use super::{
    ActionRole, Context, ControlSize, Language, ManisApp, MihomoCoreUpdateState, Theme, copy,
    settings_group_card, settings_row, settings_section_header, style_action_button,
};

impl ManisApp {
    pub(in crate::app) fn kernel_panel(
        &self,
        theme: Theme,
        _compact: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let language = self.language();
        let runtime_name = self.runtime.kind().display_name();
        div()
            .id("configuration-kernel-section")
            .w_full()
            .child(settings_section_header(
                language.localized(copy::configuration::RUNTIME),
                Some(runtime_name.into()),
                None,
                theme,
            ))
            .child(
                settings_group_card("configuration-kernel", theme)
                    .child(self.mihomo_core_update_row(language, theme, cx)),
            )
    }

    fn mihomo_core_update_row(
        &self,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let updating = self.mihomo_core_update_state.is_busy();
        let enabled = !updating && self.proxy_mode == ProxyMode::Off;
        let missing = matches!(
            self.mihomo_core_update_state,
            MihomoCoreUpdateState::Missing
        );
        let version = match &self.mihomo_core_update_state {
            MihomoCoreUpdateState::Ready(version) if version.is_empty() => {
                language.localized(copy::configuration::INSTALLED)
            }
            MihomoCoreUpdateState::Ready(version) => version.as_str(),
            MihomoCoreUpdateState::Missing => {
                language.localized(copy::configuration::NOT_INSTALLED)
            }
            MihomoCoreUpdateState::Updating => {
                language.localized(copy::configuration::UPDATE_STATUS)
            }
        };
        let action = style_action_button(
            Button::new("mihomo-core-update")
                .accessibility_label(language.localized(
                    copy::configuration::DOWNLOAD_OR_UPDATE_THE_MANIS_MANAGED_MIHOMO_CORE,
                ))
                .label(if updating {
                    language.localized(copy::configuration::UPDATING)
                } else if missing {
                    language.localized(copy::configuration::DOWNLOAD_STABLE)
                } else {
                    language.localized(copy::configuration::CHECK_FOR_UPDATE)
                })
                .icon(IconName::Redo2)
                .loading(updating)
                .disabled(!enabled)
                .tab_stop(enabled),
            ActionRole::Secondary,
            ControlSize::Compact,
        )
        .when(updating || !enabled, gpui::Styled::cursor_default)
        .on_click(cx.listener(|this, _, _, cx| this.update_mihomo_core(cx)))
        .into_any_element();

        settings_row(
            IconName::Cpu,
            language.localized(copy::configuration::RUNTIME_KERNEL),
            Some(copy::configuration::managed_core_version(language, version).into()),
            action,
            theme,
        )
    }
}
