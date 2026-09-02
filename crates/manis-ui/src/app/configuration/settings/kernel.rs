use gpui::{IntoElement, ParentElement, Stateful, Styled, div, prelude::*, px};
use gpui_component::{Disableable, IconName, Selectable, button::Button};
use manis_core::{KernelKind, ProxyMode};

use super::{
    ActionRole, Context, ControlSize, Div, Language, ManisApp, MihomoCoreUpdateState, Space,
    StatusTone, TextRole, Theme, action_button, copy, mihomo, panel_surface, section_heading,
    status_badge, style_action_button,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SingBoxSavedNodeCompatibility {
    Missing,
    Unsupported,
    Supported,
}

fn sing_box_saved_node_compatibility(
    nodes: &[mihomo::StoredSingleNode],
) -> SingBoxSavedNodeCompatibility {
    let mut has_enabled_node = false;
    for node in nodes.iter().filter(|node| node.enabled) {
        has_enabled_node = true;
        if !node.source.is_vless() {
            return SingBoxSavedNodeCompatibility::Unsupported;
        }
    }
    if has_enabled_node {
        SingBoxSavedNodeCompatibility::Supported
    } else {
        SingBoxSavedNodeCompatibility::Missing
    }
}

impl ManisApp {
    pub(in crate::app) fn kernel_panel(
        &self,
        theme: Theme,
        compact: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let language = self.language();
        let active = self.runtime.kind();
        let (sing_box_reason, sing_box_supported) = self.sing_box_support(language);
        let sing_box_enabled = sing_box_supported
            && !self.kernel_switch_state.is_busy()
            && active != KernelKind::SingBox;

        panel_surface("configuration-kernel", compact, theme)
            .child(section_heading(
                language.localized(copy::configuration::RUNTIME_KERNEL),
                "",
                Some(
                    status_badge(
                        if self.kernel_switch_state.is_busy() {
                            language.localized(copy::configuration::VALIDATING)
                        } else {
                            active.display_name()
                        },
                        if self.kernel_switch_state.is_busy() {
                            StatusTone::Warning
                        } else {
                            StatusTone::Neutral
                        },
                        theme,
                    )
                    .into_any_element(),
                ),
                theme,
            ))
            .child(Self::kernel_option_row(
                KernelKind::Mihomo,
                language
                    .localized(copy::configuration::SUBSCRIPTIONS_POLICY_GROUPS_AND_LATENCY_TESTS),
                !self.kernel_switch_state.is_busy() && active != KernelKind::Mihomo,
                active == KernelKind::Mihomo,
                language,
                theme,
                cx,
            ))
            .child(self.mihomo_core_update_row(language, theme, cx))
            .child(Self::kernel_option_row(
                KernelKind::SingBox,
                sing_box_reason,
                sing_box_enabled,
                active == KernelKind::SingBox,
                language,
                theme,
                cx,
            ))
    }

    fn sing_box_support(&self, language: Language) -> (&'static str, bool) {
        if !mihomo::sing_box_binary_available() {
            return (
                language.localized(copy::configuration::SING_BOX_WAS_NOT_FOUND_ON_THIS_DEVICE),
                false,
            );
        }
        if self
            .imported_subscriptions
            .iter()
            .any(|subscription| subscription.enabled)
        {
            return (
                language.localized(copy::configuration::CLASH_SUBSCRIPTIONS_ARE_PRESENT_MANIS_NEEDS_ITS_NATIVE_PARSER_FIRST),
                false,
            );
        }
        match sing_box_saved_node_compatibility(&self.saved_single_nodes) {
            SingBoxSavedNodeCompatibility::Missing => {
                return (
                    language.localized(
                        copy::configuration::AT_LEAST_ONE_ENABLED_SAVED_VLESS_NODE_IS_REQUIRED,
                    ),
                    false,
                );
            }
            SingBoxSavedNodeCompatibility::Unsupported => {
                return (
                    language.localized(
                        copy::configuration::DISABLE_NON_VLESS_SAVED_NODES_BEFORE_SWITCHING_TO_SING_BOX,
                    ),
                    false,
                );
            }
            SingBoxSavedNodeCompatibility::Supported => {}
        }
        (
            language.localized(
                copy::configuration::SUPPORTS_MANUAL_VLESS_SELECTORS_URL_TESTS_AND_ROUTING_RULES,
            ),
            true,
        )
    }

    fn mihomo_core_update_row(
        &self,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
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
        div()
            .mt(Space::Sm.px())
            .ml(Space::Md.px())
            .flex()
            .items_center()
            .justify_between()
            .gap_3()
            .child(
                div()
                    .text_size(TextRole::Metadata.size())
                    .line_height(TextRole::Metadata.line_height())
                    .text_color(theme.text_secondary)
                    .child(copy::configuration::managed_core_version(language, version)),
            )
            .child(
                style_action_button(
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
                .on_click(cx.listener(|this, _, _, cx| this.update_mihomo_core(cx))),
            )
    }

    fn kernel_option_row(
        kind: KernelKind,
        detail: &str,
        enabled: bool,
        selected: bool,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        div()
            .mt(Space::Md.px())
            .pt(Space::Md.px())
            .border_t_1()
            .border_color(theme.outline_subtle)
            .flex()
            .items_center()
            .justify_between()
            .gap(Space::Md.px())
            .child(
                div()
                    .min_w(px(0.0))
                    .child(
                        div()
                            .text_size(TextRole::Label.size())
                            .line_height(TextRole::Label.line_height())
                            .font_weight(TextRole::Label.weight())
                            .child(kind.display_name()),
                    )
                    .child(
                        div()
                            .mt(Space::Xs.px())
                            .text_size(TextRole::Metadata.size())
                            .line_height(TextRole::Metadata.line_height())
                            .text_color(theme.text_secondary)
                            .child(detail.to_owned()),
                    ),
            )
            .child(
                action_button(
                    format!("kernel-select-{}", kind.persistence_key()),
                    if selected {
                        language.localized(copy::configuration::CURRENT_KERNEL)
                    } else {
                        language.localized(copy::configuration::SWITCH_AND_VALIDATE)
                    },
                    ActionRole::Secondary,
                    ControlSize::Compact,
                )
                .accessibility_label(format!(
                    "{} {}",
                    language.localized(copy::configuration::SWITCH_TO),
                    kind.display_name()
                ))
                .selected(selected)
                .disabled(!enabled)
                .when(!enabled, gpui::Styled::cursor_default)
                .on_click(cx.listener(move |this, _, _, cx| {
                    if enabled {
                        this.switch_kernel(kind, cx);
                    }
                })),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::{SingBoxSavedNodeCompatibility, sing_box_saved_node_compatibility};

    fn saved_node(input: &str, enabled: bool) -> crate::mihomo::StoredSingleNode {
        crate::mihomo::StoredSingleNode {
            id: format!("saved-{enabled}"),
            name: "Saved".to_owned(),
            source: crate::subscription::SingleNodeSource::parse(input).expect("saved node"),
            enabled,
        }
    }

    #[test]
    fn sing_box_requires_enabled_vless_nodes_and_rejects_other_enabled_protocols() {
        let vless = "vless://11111111-1111-1111-1111-111111111111@example.com:443#VLESS";
        let trojan = "trojan://secret@example.com:443#Trojan";

        assert_eq!(
            sing_box_saved_node_compatibility(&[]),
            SingBoxSavedNodeCompatibility::Missing
        );
        assert_eq!(
            sing_box_saved_node_compatibility(&[saved_node(vless, false)]),
            SingBoxSavedNodeCompatibility::Missing
        );
        assert_eq!(
            sing_box_saved_node_compatibility(&[saved_node(vless, true)]),
            SingBoxSavedNodeCompatibility::Supported
        );
        assert_eq!(
            sing_box_saved_node_compatibility(&[saved_node(trojan, true)]),
            SingBoxSavedNodeCompatibility::Unsupported
        );
        assert_eq!(
            sing_box_saved_node_compatibility(&[
                saved_node(vless, true),
                saved_node(trojan, false),
            ]),
            SingBoxSavedNodeCompatibility::Supported
        );
        assert_eq!(
            sing_box_saved_node_compatibility(
                &[saved_node(vless, true), saved_node(trojan, true),]
            ),
            SingBoxSavedNodeCompatibility::Unsupported
        );
    }
}
