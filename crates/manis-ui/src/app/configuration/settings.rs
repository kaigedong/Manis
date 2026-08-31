fn configuration_section_at_scroll(
    section_tops: &[gpui::Pixels],
    scroll_top: gpui::Pixels,
    at_bottom: bool,
) -> ConfigurationSection {
    // The last section may be shorter than the viewport and never reach its top edge.
    let index = if at_bottom {
        section_tops.len().saturating_sub(1)
    } else {
        section_tops
            .iter()
            .rposition(|top| *top <= scroll_top + px(1.0))
            .unwrap_or(0)
    };
    ConfigurationSection::ALL
        .get(index)
        .copied()
        .unwrap_or_default()
}

impl ManisApp {
    pub(super) fn scroll_to_configuration_section(
        &mut self,
        section: ConfigurationSection,
        cx: &mut Context<Self>,
    ) {
        let index = ConfigurationSection::ALL
            .iter()
            .position(|candidate| *candidate == section)
            .expect("configuration section belongs to the directory");
        self.configuration_section = section;
        self.configuration_scroll.scroll_to_top_of_item(index);
        cx.notify();
    }

    fn sync_configuration_directory(&mut self, cx: &mut Context<Self>) {
        let scroll = &self.configuration_scroll;
        let section_tops: Vec<_> = (0..ConfigurationSection::ALL.len())
            .filter_map(|index| scroll.bounds_for_item(index).map(|bounds| bounds.top()))
            .collect();
        if section_tops.is_empty() {
            return;
        }
        let at_bottom = scroll.max_offset().y > px(0.0)
            && -scroll.offset().y >= scroll.max_offset().y - px(1.0);
        let section = configuration_section_at_scroll(
            &section_tops,
            scroll.bounds().top() - scroll.offset().y,
            at_bottom,
        );
        if self.configuration_section != section {
            self.configuration_section = section;
            cx.notify();
        }
    }

    fn configuration_navigation(&self, theme: Theme, compact: bool, cx: &mut Context<Self>) -> Div {
        let language = self.language();
        let navigation = div()
            .flex_shrink_0()
            .bg(theme.surface_chrome)
            .when(compact, |navigation| {
                navigation
                    .w_full()
                    .px(Space::Md.px())
                    .py(Space::Sm.px())
                    .border_b_1()
            })
            .when(!compact, |navigation| {
                navigation
                    .w(px(228.0))
                    .h_full()
                    .p(Space::Md.px())
                    .border_r_1()
                    .flex()
                    .flex_col()
            })
            .border_color(theme.outline_subtle)
            .when(!compact, |navigation| {
                navigation.child(
                    div()
                        .px(Space::Sm.px())
                        .pb(Space::Sm.px())
                        .text_size(TextRole::Metadata.size())
                        .line_height(TextRole::Metadata.line_height())
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme.text_tertiary)
                        .child(language.localized(copy::configuration::SETTINGS)),
                )
            });
        let items =
            div()
                .id("configuration-navigation-items")
                .flex()
                .gap(if compact {
                    Space::Xs.px()
                } else {
                    Space::Sm.px()
                })
                .when(compact, gpui::StatefulInteractiveElement::overflow_x_scroll)
                .when(!compact, gpui::Styled::flex_col)
                .children(ConfigurationSection::ALL.into_iter().map(|section| {
                    self.configuration_navigation_item(section, theme, compact, cx)
                }));
        navigation.child(items).when(!compact, |navigation| {
            navigation.child(
                div()
                    .mt_auto()
                    .px(Space::Sm.px())
                    .pt(Space::Lg.px())
                    .text_size(TextRole::Metadata.size())
                    .line_height(TextRole::Metadata.line_height())
                    .text_color(theme.text_tertiary)
                    .child(language.localized(copy::configuration::CHANGES_ARE_STORED_LOCALLY)),
            )
        })
    }

    fn configuration_navigation_item(
        &self,
        section: ConfigurationSection,
        theme: Theme,
        compact: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let language = self.language();
        let selected = self.configuration_section == section;
        let metadata = match section {
            ConfigurationSection::General => self.language().display_name().to_owned(),
            ConfigurationSection::Runtime => self.runtime.kind().display_name().to_owned(),
            ConfigurationSection::ProxySources => language.count(
                CountNoun::Source,
                self.imported_subscriptions.len() + self.saved_single_nodes.len(),
            ),
            ConfigurationSection::RuleSources => {
                language.count(CountNoun::Source, self.rule_sources.sources.len())
            }
            ConfigurationSection::Advanced => language
                .localized(copy::configuration::MANAGED_2)
                .to_owned(),
        };
        div()
            .id(format!("configuration-nav-{}", section.key()))
            .role(Role::Button)
            .aria_label(configuration_section_label(section, language))
            .aria_toggled(if selected {
                gpui::Toggled::True
            } else {
                gpui::Toggled::False
            })
            .tab_stop(true)
            .focusable()
            .cursor_pointer()
            .min_w(if compact { px(104.0) } else { px(0.0) })
            .px(Space::Md.px())
            .py(Space::Sm.px())
            .rounded(Radius::Control.px())
            .border_1()
            .border_color(gpui::rgba(0x0000_0000))
            .bg(if selected {
                theme.action_soft
            } else {
                gpui::rgba(0x0000_0000)
            })
            .hover(move |row| {
                row.bg(if selected {
                    theme.action_soft
                } else {
                    theme.surface_high
                })
            })
            .focus_visible(move |row| row.border_color(theme.focus_ring))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(Space::Sm.px())
                    .child(
                        div()
                            .text_size(TextRole::Label.size())
                            .line_height(TextRole::Label.line_height())
                            .font_weight(if selected {
                                FontWeight::SEMIBOLD
                            } else {
                                FontWeight::NORMAL
                            })
                            .text_color(if selected {
                                theme.action_primary
                            } else {
                                theme.text_primary
                            })
                            .child(configuration_section_label(section, language)),
                    )
                    .when(!compact, |row| {
                        row.child(
                            div()
                                .text_size(TextRole::Metadata.size())
                                .line_height(TextRole::Metadata.line_height())
                                .text_color(theme.text_tertiary)
                                .child(metadata),
                        )
                    }),
            )
            .when(!compact, |item| {
                item.child(
                    div()
                        .mt(px(2.0))
                        .text_size(TextRole::Metadata.size())
                        .line_height(TextRole::Metadata.line_height())
                        .text_color(theme.text_secondary)
                        .child(configuration_section_detail(section, language)),
                )
            })
            .on_click(cx.listener(move |this, _, _, cx| {
                this.scroll_to_configuration_section(section, cx);
            }))
            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, _, cx| {
                if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                    this.scroll_to_configuration_section(section, cx);
                    cx.stop_propagation();
                }
            }))
    }

    fn advanced_configuration_panel(&self, theme: Theme, compact: bool) -> Stateful<Div> {
        let language = self.language();
        let profile_source = self.runtime.profile_source();
        let profile_detail = copy::configuration::profile_source_detail(language, profile_source);
        panel_surface("configuration-advanced", compact, theme)
            .child(section_heading(
                language.localized(copy::configuration::ADVANCED_SETTINGS),
                language.localized(copy::configuration::CURRENT_MANAGED_NETWORK_BEHAVIOR),
                Some(
                    status_badge(
                        language.localized(copy::configuration::MANAGED),
                        StatusTone::Neutral,
                        theme,
                    )
                    .into_any_element(),
                ),
                theme,
            ))
            .child(Self::advanced_configuration_row(
                language.localized(copy::configuration::PROXY_MODE),
                proxy_mode_label(language, self.proxy_mode),
                language.localized(copy::configuration::CHANGED_FROM_THE_MAIN_TOOLBAR),
                theme,
            ))
            .child(Self::advanced_configuration_row(
                language.localized(copy::configuration::ROUTING_MODE),
                routing_mode_label(language, self.routing_mode),
                language.localized(copy::configuration::DIRECT_GLOBAL_OR_ORDERED_RULES),
                theme,
            ))
            .child(Self::advanced_configuration_row(
                language.localized(copy::configuration::PROCESS_IDENTIFICATION),
                language.localized(copy::configuration::ALWAYS),
                language.localized(copy::configuration::USED_TO_IMPROVE_NETWORK_ACTIVITY),
                theme,
            ))
            .child(Self::advanced_configuration_row(
                language.localized(copy::configuration::DNS_AND_TUN),
                language.localized(copy::configuration::AUTOMATIC),
                profile_detail,
                theme,
            ))
    }

    fn advanced_configuration_row(
        label: &'static str,
        value: &'static str,
        detail: &'static str,
        theme: Theme,
    ) -> Div {
        div()
            .mt(Space::Md.px())
            .pt(Space::Md.px())
            .border_t_1()
            .border_color(theme.outline_subtle)
            .flex()
            .items_center()
            .justify_between()
            .gap(Space::Lg.px())
            .child(
                div()
                    .min_w(px(0.0))
                    .child(
                        div()
                            .text_size(TextRole::Label.size())
                            .line_height(TextRole::Label.line_height())
                            .font_weight(TextRole::Label.weight())
                            .text_color(theme.text_primary)
                            .child(label),
                    )
                    .child(
                        div()
                            .mt(Space::Xs.px())
                            .text_size(TextRole::Metadata.size())
                            .line_height(TextRole::Metadata.line_height())
                            .text_color(theme.text_secondary)
                            .child(detail),
                    ),
            )
            .child(status_badge(value, StatusTone::Neutral, theme))
    }

    fn language_panel(&self, theme: Theme, compact: bool, cx: &mut Context<Self>) -> Stateful<Div> {
        let language = self.language();
        let current_preference = self.language_preference();
        let current_language = language.display_name();
        panel_surface("configuration-language", compact, theme)
            .child(section_heading(
                language.localized(copy::configuration::INTERFACE_LANGUAGE),
                "",
                Some(
                    status_badge(
                        format!(
                            "{} · {current_language}",
                            language.localized(copy::configuration::CURRENT)
                        ),
                        StatusTone::Neutral,
                        theme,
                    )
                    .into_any_element(),
                ),
                theme,
            ))
            .child(
                div()
                    .mt(Space::Md.px())
                    .grid()
                    .gap(Space::Sm.px())
                    .grid_cols(if compact { 1 } else { 3 })
                    .children(
                        [
                            LanguagePreference::FollowSystem,
                            LanguagePreference::English,
                            LanguagePreference::SimplifiedChinese,
                        ]
                        .into_iter()
                        .map(|preference| {
                            Self::language_option(
                                preference,
                                preference == current_preference,
                                language,
                                theme,
                                cx,
                            )
                        }),
                    ),
            )
    }

    fn app_update_panel(
        &self,
        theme: Theme,
        compact: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let language = self.language();
        let (status, tone) = self.app_update_status(language);
        let (button_label, ready, busy, supported) = self.app_update_action(language);

        panel_surface("configuration-app-update", compact, theme)
            .child(section_heading(
                language.localized(copy::app_update::AUTOMATIC_UPDATES),
                language.localized(copy::app_update::AUTOMATIC_UPDATES_DETAIL),
                Some(status_badge(status, tone, theme).into_any_element()),
                theme,
            ))
            .child(
                div()
                    .mt(Space::Md.px())
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(Space::Md.px())
                    .child(
                        div()
                            .text_size(TextRole::Metadata.size())
                            .line_height(TextRole::Metadata.line_height())
                            .text_color(theme.text_secondary)
                            .child(copy::app_update::current_version(
                                language,
                                app_update::current_version(),
                            )),
                    )
                    .child(
                        style_action_button(
                            Button::new("app-update-action")
                                .accessibility_label(button_label)
                                .label(button_label)
                                .icon(IconName::Redo2)
                                .loading(busy)
                                .disabled(!supported || busy)
                                .tab_stop(supported && !busy),
                            if ready {
                                ActionRole::Primary
                            } else {
                                ActionRole::Secondary
                            },
                            ControlSize::Compact,
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            if matches!(this.app_update_state, AppUpdateState::Ready(_)) {
                                this.restart_with_app_update(cx);
                            } else {
                                this.check_for_app_update(true, cx);
                            }
                        })),
                    ),
            )
    }

    fn app_update_status(&self, language: Language) -> (String, StatusTone) {
        match &self.app_update_state {
            AppUpdateState::Idle => (
                copy::app_update::current_version(language, app_update::current_version()),
                StatusTone::Neutral,
            ),
            AppUpdateState::Checking => (
                language.localized(copy::app_update::CHECKING).to_owned(),
                StatusTone::Neutral,
            ),
            AppUpdateState::Downloading(version) => (
                format!(
                    "{} · {version}",
                    language.localized(copy::app_update::DOWNLOADING)
                ),
                StatusTone::Neutral,
            ),
            AppUpdateState::Ready(staged) => (
                copy::app_update::ready_version(language, &staged.version),
                StatusTone::Success,
            ),
            AppUpdateState::Installing(version) => (
                format!(
                    "{} · {version}",
                    language.localized(copy::app_update::INSTALLING)
                ),
                StatusTone::Warning,
            ),
            AppUpdateState::Current => (
                language.localized(copy::app_update::UP_TO_DATE).to_owned(),
                StatusTone::Success,
            ),
            AppUpdateState::Failed(error) => (
                copy::app_update::error(language, *error).to_owned(),
                StatusTone::Error,
            ),
            AppUpdateState::Unsupported => (
                language.localized(copy::app_update::UNSUPPORTED).to_owned(),
                StatusTone::Neutral,
            ),
        }
    }

    fn app_update_action(&self, language: Language) -> (&'static str, bool, bool, bool) {
        let ready = matches!(self.app_update_state, AppUpdateState::Ready(_));
        let busy = self.app_update_state.is_busy();
        let supported = !matches!(self.app_update_state, AppUpdateState::Unsupported);
        let button_label = if ready {
            language.localized(copy::app_update::RESTART_AND_UPDATE)
        } else if matches!(self.app_update_state, AppUpdateState::Failed(_)) {
            language.localized(copy::app_update::TRY_AGAIN)
        } else if matches!(self.app_update_state, AppUpdateState::Checking) {
            language.localized(copy::app_update::CHECKING)
        } else if matches!(self.app_update_state, AppUpdateState::Downloading(_)) {
            language.localized(copy::app_update::DOWNLOADING)
        } else if matches!(self.app_update_state, AppUpdateState::Installing(_)) {
            language.localized(copy::app_update::INSTALLING)
        } else {
            language.localized(copy::app_update::CHECK_FOR_UPDATES)
        };
        (button_label, ready, busy, supported)
    }

    fn language_option(
        preference: LanguagePreference,
        selected: bool,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let label = language_preference_label(preference, language);
        div()
            .id(format!(
                "language-option-{}",
                preference.persistence_key().replace('-', "_")
            ))
            .role(Role::Button)
            .aria_label(format!(
                "{}: {label}",
                language.localized(copy::configuration::SELECT_LANGUAGE)
            ))
            .aria_toggled(if selected {
                gpui::Toggled::True
            } else {
                gpui::Toggled::False
            })
            .tab_stop(true)
            .focusable()
            .cursor_pointer()
            .min_h(px(52.0))
            .px(Space::Md.px())
            .py(Space::Sm.px())
            .rounded(Radius::Row.px())
            .border_1()
            .border_color(if selected {
                theme.action_primary
            } else {
                theme.outline_subtle
            })
            .bg(if selected {
                theme.action_soft
            } else {
                theme.surface_low
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        div()
                            .text_size(TextRole::Label.size())
                            .line_height(TextRole::Label.line_height())
                            .font_weight(TextRole::Label.weight())
                            .text_color(if selected {
                                theme.action_primary
                            } else {
                                theme.text_primary
                            })
                            .child(label),
                    )
                    .when(selected, |row| {
                        row.child(
                            div()
                                .text_size(TextRole::Metadata.size())
                                .line_height(TextRole::Metadata.line_height())
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(theme.action_primary)
                                .child(language.localized(copy::configuration::SELECTED)),
                        )
                    }),
            )
            .hover(move |style| {
                style.bg(if selected {
                    theme.action_soft
                } else {
                    theme.button_hover
                })
            })
            .active(move |style| {
                style.bg(if selected {
                    theme.action_soft
                } else {
                    theme.button_active
                })
            })
            .on_click(cx.listener(move |this, _, _, cx| {
                this.set_language_preference(preference, cx);
            }))
    }

    fn set_language_preference(&mut self, preference: LanguagePreference, cx: &mut Context<Self>) {
        if self.configuration_transfer.active {
            return;
        }
        self.localizer.set_preference(preference);
        let language = self.language();
        match self.subscription_store_dir.as_ref() {
            Some(store_dir) => match save_language_preference_in(store_dir, preference) {
                Ok(_path) => {
                    self.status = format!(
                        "{} · {}",
                        language.localized(copy::configuration::LANGUAGE_SAVED),
                        language_preference_label(preference, language)
                    );
                }
                Err(error) => {
                    self.status = format!(
                        "{}: {}",
                        language.localized(
                            copy::configuration::LANGUAGE_CHANGED_BUT_COULD_NOT_BE_SAVED
                        ),
                        copy::configuration::language_preference_error(language, error)
                    );
                }
            },
            None => {
                language
                    .localized(copy::configuration::LANGUAGE_CHANGED_FOR_THIS_SESSION_DATA_DIRECTORY_UNAVAILABLE)
                    .clone_into(&mut self.status);
            }
        }
        if let Some(input) = self.proxy_source_editor.input.as_ref() {
            input.update(cx, |input, cx| input.set_language(language, cx));
        }
        if let Some(input) = self.proxy_source_editor.name_input.as_ref() {
            input.update(cx, |input, cx| {
                input.set_placeholder(
                    language.localized(copy::common::FOR_EXAMPLE_MY_SUBSCRIPTION),
                    cx,
                );
            });
        }
        if let Some(input) = self.inputs.policy_group_name.as_ref() {
            input.update(cx, |input, cx| {
                input.set_placeholder(
                    language.localized(copy::common::FOR_EXAMPLE_HONG_KONG_AUTO),
                    cx,
                );
            });
        }
        if let Some(input) = self.inputs.policy_group_filter.as_ref() {
            input.update(cx, |input, cx| {
                input.set_placeholder(language.localized(copy::common::FOR_EXAMPLE_HONG_KONG), cx);
            });
        }
        cx.notify();
    }

    fn kernel_panel(&self, theme: Theme, compact: bool, cx: &mut Context<Self>) -> Stateful<Div> {
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
        if self.saved_single_nodes.is_empty() {
            return (
                language.localized(copy::configuration::AT_LEAST_ONE_SAVED_VLESS_NODE_IS_REQUIRED),
                false,
            );
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
            MihomoCoreUpdateState::Updating => language.localized(copy::configuration::UPDATING_2),
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
                        language.localized(copy::configuration::CURRENT_2)
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
                .on_click(cx.listener(move |this, _, _, cx| {
                    if enabled {
                        this.switch_kernel(kind, cx);
                    }
                })),
            )
    }

    pub(super) fn routing_rules_workspace(
        &mut self,
        theme: Theme,
        size_class: WindowSizeClass,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let compact = size_class == WindowSizeClass::Compact;
        let language = self.language();
        self.ensure_manual_rule_input(theme, window, cx);
        div()
            .flex_1()
            .min_w(px(0.0))
            .h_full()
            .flex()
            .flex_col()
            .bg(theme.surface_base)
            .child(
                div()
                    .flex_shrink_0()
                    .p(Space::Lg.px())
                    .border_b_1()
                    .border_color(theme.outline_subtle)
                    .child(page_heading(
                        language.message(Message::RoutingRules),
                        format!(
                            "{} · {}",
                            self.active_rules_summary(language),
                            language.localized(copy::configuration::GROUPS_MATCH_FROM_TOP_TO_BOTTOM_USE_THE_ARROWS_TO),
                        ),
                        Some(
                            action_button(
                                "open-manual-rule-editor",
                                language.message(Message::AddRule),
                                ActionRole::Primary,
                                ControlSize::Compact,
                            )
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.open_manual_rule_editor(window, cx);
                            }))
                            .into_any_element(),
                        ),
                        theme,
                    )),
            )
            .child(
                div()
                    .id("routing-rules-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .p(if compact { Space::Md.px() } else { Space::Lg.px() })
                    .child(self.active_rules_panel(theme, language, compact, cx)),
            )
    }

    fn workspace_header(
        title: &'static str,
        detail: &'static str,
        badge: &'static str,
        badge_tone: StatusTone,
        theme: Theme,
        compact: bool,
    ) -> Div {
        div()
            .px(if compact {
                Space::Lg.px()
            } else {
                Space::Xl.px()
            })
            .py(Space::Md.px())
            .flex_shrink_0()
            .border_b_1()
            .border_color(theme.outline_subtle)
            .flex()
            .items_start()
            .justify_between()
            .gap(Space::Lg.px())
            .child(page_heading(
                title,
                if compact { "" } else { detail },
                Some(status_badge(badge, badge_tone, theme).into_any_element()),
                theme,
            ))
    }
}
