use std::path::Path;

use gpui::{
    AnyElement, Context, Div, Entity, Focusable, FontWeight, KeyDownEvent, ParentElement, Role,
    Stateful, StyleRefinement, Styled, Window, div, prelude::*, px,
};
use gpui_component::{
    Disableable, IconName, Selectable, Sizable, Size, WindowExt as _,
    accordion::Accordion,
    button::{Button, ButtonVariant, ButtonVariants},
    checkbox::Checkbox,
    dialog::Dialog,
    menu::{ContextMenuExt, PopupMenuItem},
};
use manis_core::{KernelKind, ProxyMode, WindowSizeClass};
use manis_profile::{QxRuleKind, SecretUrl};

use super::{
    ConfigurationSection, ImportQxRuleError, ImportQxRuleSuccess, ImportedSubscriptionState,
    ManisApp, ManualRulePopover, MihomoCoreUpdateState, ProxySourceEditorKind,
    QxRuleImportFeedback, QxRuleList, QxRuleSourceRefreshState, SourceRuntimeApply,
    SubscriptionFeedback, proxy_mode_label, routing_mode_label,
};
use crate::{
    components::{
        ActionRole, StatusTone, action_button, empty_state, page_heading, section_heading,
        status_badge, style_action_button,
    },
    diagnostics::{LogLevel, UiEvent, begin_operation, record_event, record_operation, trace_ui},
    localization::{
        CountNoun, Language, LanguagePreference, Message, copy, save_language_preference_in,
    },
    mihomo::{self, RemoteSourceRefreshInterval, SubscriptionStoreError},
    rule_source::{download_qx_rule_document, download_qx_rule_document_secret},
    subscription::{SourceKind, validate_single_node_preview, validate_subscription_preview},
    subscription_input::{SubscriptionTextInput, TextInputSpec},
    theme::{ControlSize, Radius, Space, TextRole, Theme},
};

const MAX_MANUAL_RULE_INPUT_BYTES: usize = 1_024;
const MANUAL_RULES_EXPANSION_KEY: &str = "routing-manual-rules";

struct QxRuleSaveRequest {
    url: String,
    target: String,
    editing_id: Option<String>,
    refresh_interval: RemoteSourceRefreshInterval,
}

struct SubscriptionToggleCompletion {
    id: String,
    generation: u64,
    kind: SourceKind,
    previous_state: ImportedSubscriptionState,
    previous_enabled: bool,
}

struct SubscriptionCardPresentation {
    state: String,
    activity: SubscriptionCardActivity,
    controls_enabled: bool,
    updated: String,
}

#[derive(Clone, Copy)]
enum SubscriptionCardActivity {
    Idle { healthy: bool },
    Busy,
}

#[derive(Clone, Copy)]
struct ProxySourceEditorView {
    direct_input: bool,
    editing: bool,
    activity: ProxySourceEditorActivity,
    enabled: bool,
    dialog_width: f32,
}

#[derive(Clone, Copy)]
enum ProxySourceEditorActivity {
    Idle,
    Busy,
}

impl ProxySourceEditorView {
    const fn busy(self) -> bool {
        matches!(self.activity, ProxySourceEditorActivity::Busy)
    }
}

struct ProxySourceEditorInputs {
    source: Entity<SubscriptionTextInput>,
    name: Entity<SubscriptionTextInput>,
    interval_select: AnyElement,
}

#[derive(Clone, Copy)]
struct QxRuleEditorView {
    editing: bool,
    busy: bool,
    dialog_width: f32,
}

struct RuleSourceCardPresentation {
    name: String,
    refresh: RuleSourceRefreshPresentation,
    duplicate: bool,
    controls_enabled: bool,
    target_policy: String,
    last_update: String,
}

enum RuleSourceRefreshPresentation {
    Idle,
    Refreshing,
    Failed(String),
}

#[derive(Clone, Copy)]
struct RuleGroupRenderContext {
    position: usize,
    group_count: usize,
    compact: bool,
    language: Language,
    theme: Theme,
}

impl RuleSourceRefreshPresentation {
    const fn is_refreshing(&self) -> bool {
        matches!(self, Self::Refreshing)
    }
}

impl SubscriptionCardActivity {
    const fn is_busy(self) -> bool {
        matches!(self, Self::Busy)
    }

    const fn is_healthy(self) -> bool {
        matches!(self, Self::Idle { healthy: true } | Self::Busy)
    }
}

fn save_qx_rule_source(
    runtime: &super::KernelRuntime,
    store_dir: &Path,
    request: QxRuleSaveRequest,
) -> super::QxRuleImportResult {
    let QxRuleSaveRequest {
        url,
        target,
        editing_id,
        refresh_interval,
    } = request;
    let content = download_qx_rule_document(&url).map_err(ImportQxRuleError::Download)?;
    if QxRuleList::parse(&content).rules.is_empty() {
        return Err(ImportQxRuleError::InvalidDocument);
    }
    if let Some(editing_id) = editing_id {
        return replace_qx_rule_source(
            runtime,
            store_dir,
            &editing_id,
            &url,
            &target,
            &content,
            refresh_interval,
        );
    }
    create_qx_rule_source(
        runtime,
        store_dir,
        &url,
        &target,
        &content,
        refresh_interval,
    )
}

fn replace_qx_rule_source(
    runtime: &super::KernelRuntime,
    store_dir: &Path,
    id: &str,
    url: &str,
    target: &str,
    content: &str,
    refresh_interval: RemoteSourceRefreshInterval,
) -> super::QxRuleImportResult {
    let transaction = super::mutate_saved_sources(runtime, store_dir, || {
        mihomo::replace_qx_rule_source_definition_in(
            store_dir,
            id,
            url,
            target,
            content,
            refresh_interval,
            mihomo::current_unix_secs(),
        )
    })
    .map_err(ImportQxRuleError::Store)?;
    Ok(match transaction.value {
        Some(stored) => ImportQxRuleSuccess::Imported {
            stored,
            apply: transaction.apply,
        },
        None => ImportQxRuleSuccess::RolledBack {
            apply: transaction.apply,
            rollback_error: transaction.rollback_error,
        },
    })
}

fn create_qx_rule_source(
    runtime: &super::KernelRuntime,
    store_dir: &Path,
    url: &str,
    target: &str,
    content: &str,
    refresh_interval: RemoteSourceRefreshInterval,
) -> super::QxRuleImportResult {
    let transaction = super::mutate_saved_sources(runtime, store_dir, || {
        let outcome = mihomo::save_qx_rule_source_in(store_dir, url, target, content)?;
        let mihomo::SaveQxRuleSourceOutcome::Created(mut stored) = outcome else {
            return Ok(outcome);
        };
        if refresh_interval != RemoteSourceRefreshInterval::Manual {
            stored = mihomo::update_qx_rule_source_refresh_interval_in(
                store_dir,
                &stored.id,
                refresh_interval,
            )?;
        }
        Ok(mihomo::SaveQxRuleSourceOutcome::Created(stored))
    })
    .map_err(ImportQxRuleError::Store)?;
    Ok(match transaction.value {
        Some(mihomo::SaveQxRuleSourceOutcome::Created(stored)) => ImportQxRuleSuccess::Imported {
            stored,
            apply: transaction.apply,
        },
        Some(mihomo::SaveQxRuleSourceOutcome::Existing(stored)) => {
            ImportQxRuleSuccess::AlreadyExists { stored }
        }
        None => ImportQxRuleSuccess::RolledBack {
            apply: transaction.apply,
            rollback_error: transaction.rollback_error,
        },
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ManualRuleKeyboardAction {
    Edit,
    Toggle,
    Delete,
}

fn manual_rule_keyboard_action(event: &KeyDownEvent) -> Option<ManualRuleKeyboardAction> {
    manual_rule_keyboard_action_for(
        event.keystroke.key.as_str(),
        event.keystroke.modifiers.modified(),
        event.is_held,
    )
}

fn manual_rule_keyboard_action_for(
    key: &str,
    modified: bool,
    held: bool,
) -> Option<ManualRuleKeyboardAction> {
    if held || modified {
        return None;
    }
    match key {
        "enter" => Some(ManualRuleKeyboardAction::Edit),
        "space" => Some(ManualRuleKeyboardAction::Toggle),
        "backspace" | "delete" => Some(ManualRuleKeyboardAction::Delete),
        _ => None,
    }
}

fn rule_group_is_open(workspace: &manis_core::NodeWorkspaceState, key: &str) -> bool {
    !workspace.is_group_collapsed(key)
}

fn panel_surface(id: &'static str, compact: bool, theme: Theme) -> Stateful<Div> {
    div()
        .id(id)
        .w_full()
        .p(if compact {
            Space::Md.px()
        } else {
            Space::Lg.px()
        })
        .rounded(Radius::Pane.px())
        .border_1()
        .border_color(theme.outline_subtle)
        .bg(theme.surface_high)
}

fn field_label(label: impl Into<gpui::SharedString>, theme: Theme) -> Div {
    let label = label.into();
    div()
        .mb(Space::Xs.px())
        .text_size(TextRole::Label.size())
        .line_height(TextRole::Label.line_height())
        .font_weight(TextRole::Label.weight())
        .text_color(theme.text_secondary)
        .child(label)
}

fn accordion_content_style() -> StyleRefinement {
    let mut style = StyleRefinement::default();
    style.padding.top = Some(px(0.0).into());
    style.padding.right = Some(px(0.0).into());
    style.padding.bottom = Some(px(0.0).into());
    style.padding.left = Some(px(0.0).into());
    style
}

fn accordion_title_style(compact: bool) -> StyleRefinement {
    let mut style = StyleRefinement::default();
    if compact {
        style.padding.right = Some(px(12.0).into());
        style.padding.left = Some(px(12.0).into());
    }
    style
}

fn manual_rule_placeholder(
    kind: crate::manual_rule::ManualRuleKind,
    language: Language,
) -> &'static str {
    use crate::manual_rule::ManualRuleKind;
    match kind {
        ManualRuleKind::Host
        | ManualRuleKind::HostSuffix
        | ManualRuleKind::HostWildcard
        | ManualRuleKind::HostKeyword => "example.com",
        ManualRuleKind::UserAgent => "*abc?",
        ManualRuleKind::IpCidr => "192.168.0.1/24",
        ManualRuleKind::Ip6Cidr => "2001:4860:4860::8888/32",
        ManualRuleKind::GeoIp => language.localized(copy::configuration::US),
        ManualRuleKind::IpAsn => "6185",
        ManualRuleKind::DstPort => "22",
        ManualRuleKind::Final => "",
    }
}

fn manual_rule_kind_detail(
    kind: crate::manual_rule::ManualRuleKind,
    language: Language,
) -> &'static str {
    use crate::manual_rule::ManualRuleKind;
    match kind {
        ManualRuleKind::Host => language.localized(copy::configuration::EXACT_DOMAIN),
        ManualRuleKind::HostSuffix => language.localized(copy::configuration::DOMAIN_SUFFIX),
        ManualRuleKind::HostWildcard => language.localized(copy::configuration::WILDCARD_DOMAIN),
        ManualRuleKind::HostKeyword => {
            language.localized(copy::configuration::DOMAIN_CONTAINS_KEYWORD)
        }
        ManualRuleKind::UserAgent => language.localized(copy::configuration::BROWSER_USER_AGENT),
        ManualRuleKind::IpCidr => language.localized(copy::configuration::IPV4_ADDRESS_RANGE),
        ManualRuleKind::Ip6Cidr => language.localized(copy::configuration::IPV6_ADDRESS_RANGE),
        ManualRuleKind::GeoIp => language.localized(copy::configuration::COUNTRY_OR_REGION),
        ManualRuleKind::IpAsn => language.localized(copy::configuration::AUTONOMOUS_SYSTEM),
        ManualRuleKind::DstPort => language.localized(copy::configuration::DESTINATION_PORT),
        ManualRuleKind::Final => {
            language.localized(copy::configuration::FALLBACK_FOR_TRAFFIC_NOT_MATCHED_ABOVE)
        }
    }
}

fn manual_rule_error_label(
    error: crate::manual_rule::ManualRuleError,
    language: Language,
) -> &'static str {
    use crate::manual_rule::ManualRuleError;
    match error {
        ManualRuleError::Empty => language.localized(copy::configuration::ENTER_A_MATCH_PARAMETER),
        ManualRuleError::InvalidDomain => {
            language.localized(copy::configuration::ENTER_A_PLAIN_DOMAIN_SUCH_AS_EXAMPLE_COM)
        }
        ManualRuleError::InvalidWildcard => {
            language.localized(copy::configuration::ENTER_A_DOMAIN_PATTERN_SUCH_AS_EXAMPLE_COM)
        }
        ManualRuleError::InvalidKeyword => language.localized(
            copy::configuration::THE_PARAMETER_CANNOT_CONTAIN_COMMAS_TABS_OR_LINE_BREAKS,
        ),
        ManualRuleError::InvalidIpv4Cidr => {
            language.localized(copy::configuration::ENTER_AN_IPV4_CIDR_SUCH_AS_192_168_0_1)
        }
        ManualRuleError::InvalidIpv6Cidr => {
            language.localized(copy::configuration::ENTER_AN_IPV6_CIDR_SUCH_AS_2001_4860_4860_8888)
        }
        ManualRuleError::InvalidGeoIp => {
            language.localized(copy::configuration::ENTER_A_TWO_LETTER_COUNTRY_CODE_SUCH_AS_US)
        }
        ManualRuleError::InvalidAsn => {
            language.localized(copy::configuration::ENTER_AN_ASN_NUMBER_SUCH_AS_6185)
        }
        ManualRuleError::InvalidDestinationPort => {
            language.localized(copy::configuration::ENTER_A_DESTINATION_PORT_BETWEEN_1_AND_65535)
        }
        ManualRuleError::InvalidPolicy => {
            language.localized(copy::configuration::CHOOSE_AN_EXISTING_POLICY_GROUP)
        }
        ManualRuleError::UnsupportedByKernel => language.localized(
            copy::configuration::THIS_RULE_TYPE_CANNOT_BE_MATCHED_EXACTLY_BY_THE_CURRENT,
        ),
        ManualRuleError::Duplicate => {
            language.localized(copy::configuration::THIS_MANUAL_RULE_ALREADY_EXISTS)
        }
        ManualRuleError::DuplicateCondition => {
            language.localized(copy::configuration::THE_SAME_CONDITION_APPEARS_MORE_THAN_ONCE)
        }
        ManualRuleError::TooManyConditions => {
            language.localized(copy::configuration::A_RULE_CAN_CONTAIN_AT_MOST_FOUR_CONDITIONS)
        }
        ManualRuleError::FinalMustStandAlone => {
            language.localized(copy::configuration::FINAL_CANNOT_BE_COMBINED_WITH_ANOTHER_CONDITION)
        }
        ManualRuleError::FinalHasNoParameter => {
            language.localized(copy::configuration::FINAL_DOES_NOT_NEED_A_MATCH_PARAMETER)
        }
        ManualRuleError::FinalAlreadyExists => {
            language.localized(copy::configuration::ONLY_ONE_FINAL_RULE_CAN_BE_CONFIGURED)
        }
    }
}

fn source_kind_label(kind: SourceKind, language: Language) -> &'static str {
    match kind {
        SourceKind::HttpSubscription => language.localized(copy::common::HTTP_SUBSCRIPTION),
        SourceKind::HttpsSubscription => language.localized(copy::common::HTTPS_SUBSCRIPTION),
        SourceKind::SingleNode => language.localized(copy::configuration::SINGLE_NODE),
    }
}

fn source_update_label(
    last_successful_update_unix_secs: u64,
    now_unix_secs: u64,
    language: Language,
) -> String {
    if last_successful_update_unix_secs == 0 {
        return language
            .localized(copy::configuration::NEVER_UPDATED)
            .to_owned();
    }
    let elapsed = now_unix_secs.saturating_sub(last_successful_update_unix_secs);
    match elapsed {
        0..=59 => language
            .localized(copy::configuration::UPDATED_JUST_NOW)
            .to_owned(),
        60..=3_599 => {
            let minutes = elapsed / 60;
            copy::configuration::updated_minutes_ago(language, minutes)
        }
        3_600..=86_399 => {
            let hours = elapsed / 3_600;
            copy::configuration::updated_hours_ago(language, hours)
        }
        _ => {
            let days = elapsed / 86_400;
            copy::configuration::updated_days_ago(language, days)
        }
    }
}

fn rule_source_expansion_key(source_id: &str) -> String {
    format!("routing-rule-source:{source_id}")
}

fn refresh_interval_label(
    interval: RemoteSourceRefreshInterval,
    language: Language,
) -> &'static str {
    match interval {
        RemoteSourceRefreshInterval::Manual => language.localized(copy::configuration::MANUAL),
        RemoteSourceRefreshInterval::Hourly => {
            language.localized(copy::configuration::EVERY_1_HOUR)
        }
        RemoteSourceRefreshInterval::SixHours => {
            language.localized(copy::configuration::EVERY_6_HOURS)
        }
        RemoteSourceRefreshInterval::TwelveHours => {
            language.localized(copy::configuration::EVERY_12_HOURS)
        }
        RemoteSourceRefreshInterval::Daily => language.localized(copy::configuration::DAILY),
    }
}

fn language_preference_label(preference: LanguagePreference, language: Language) -> &'static str {
    match preference {
        LanguagePreference::FollowSystem => language.localized(copy::configuration::FOLLOW_SYSTEM),
        LanguagePreference::English => "English",
        LanguagePreference::SimplifiedChinese => "中文",
    }
}

fn configuration_section_label(section: ConfigurationSection, language: Language) -> &'static str {
    match section {
        ConfigurationSection::General => language.localized(copy::configuration::GENERAL),
        ConfigurationSection::Runtime => language.localized(copy::configuration::RUNTIME),
        ConfigurationSection::ProxySources => {
            language.localized(copy::configuration::PROXY_SOURCES)
        }
        ConfigurationSection::RuleSources => language.localized(copy::configuration::RULE_SOURCES),
        ConfigurationSection::Advanced => language.localized(copy::configuration::ADVANCED),
    }
}

fn configuration_section_detail(section: ConfigurationSection, language: Language) -> &'static str {
    match section {
        ConfigurationSection::General => {
            language.localized(copy::configuration::INTERFACE_LANGUAGE)
        }
        ConfigurationSection::Runtime => language.localized(copy::configuration::CORE_AND_UPDATES),
        ConfigurationSection::ProxySources => {
            language.localized(copy::configuration::SUBSCRIPTIONS_AND_NODES)
        }
        ConfigurationSection::RuleSources => {
            language.localized(copy::configuration::REMOTE_RULE_SETS)
        }
        ConfigurationSection::Advanced => language.localized(copy::configuration::NETWORK_BEHAVIOR),
    }
}

impl ManisApp {
    pub(super) fn configuration_workspace(
        &self,
        theme: Theme,
        size_class: WindowSizeClass,
        cx: &mut Context<Self>,
    ) -> Div {
        let compact = size_class == WindowSizeClass::Compact;
        let language = self.language();
        let rule_input = self
            .inputs
            .qx_rule
            .as_ref()
            .expect("QX rule input is initialized before rendering")
            .clone();
        let rule_busy = self.rule_sources.feedback == QxRuleImportFeedback::Importing
            || self.source_refresh_busy();
        let selected_section = self.configuration_section;
        let detail: AnyElement = match selected_section {
            ConfigurationSection::General => {
                self.language_panel(theme, compact, cx).into_any_element()
            }
            ConfigurationSection::Runtime => {
                self.kernel_panel(theme, compact, cx).into_any_element()
            }
            ConfigurationSection::ProxySources => {
                self.source_panel(theme, compact, cx).into_any_element()
            }
            ConfigurationSection::RuleSources => self
                .rule_source_manager(rule_input, rule_busy, theme, compact, cx)
                .into_any_element(),
            ConfigurationSection::Advanced => self
                .advanced_configuration_panel(theme, compact)
                .into_any_element(),
        };
        let navigation = self.configuration_navigation(theme, compact, cx);
        let content = div()
            .id("configuration-detail-scroll")
            .flex_1()
            .min_w(px(0.0))
            .min_h(px(0.0))
            .overflow_y_scroll()
            .p(if compact { px(12.0) } else { px(24.0) })
            .pb(px(56.0))
            .child(div().w_full().max_w(px(900.0)).mx_auto().child(detail));
        div()
            .flex_1()
            .min_w(px(0.0))
            .h_full()
            .flex()
            .flex_col()
            .bg(theme.surface_base)
            .child(Self::workspace_header(
                language.message(Message::Configuration),
                language.localized(copy::configuration::MANAGE_MANIS_PREFERENCES_AND_DATA_SOURCES),
                configuration_section_label(selected_section, language),
                StatusTone::Neutral,
                theme,
                compact,
            ))
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.0))
                    .flex()
                    .when(compact, gpui::Styled::flex_col)
                    .child(navigation)
                    .child(content),
            )
    }

    fn configuration_navigation(&self, theme: Theme, compact: bool, cx: &mut Context<Self>) -> Div {
        let language = self.language();
        let navigation = div()
            .flex_shrink_0()
            .bg(theme.surface_low)
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
            .border_color(if selected {
                theme.action_primary
            } else {
                theme.surface_low
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
                this.configuration_section = section;
                cx.notify();
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
            .on_click(cx.listener(move |this, _, _, cx| {
                this.set_language_preference(preference, cx);
            }))
    }

    fn set_language_preference(&mut self, preference: LanguagePreference, cx: &mut Context<Self>) {
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
                        "{}: {error}",
                        language.localized(
                            copy::configuration::LANGUAGE_CHANGED_BUT_COULD_NOT_BE_SAVED
                        )
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
                    ActionRole::Quiet,
                    ControlSize::Compact,
                )
                .when(enabled, gpui::Styled::cursor_pointer)
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
                div()
                    .id(format!("kernel-select-{}", kind.persistence_key()))
                    .role(Role::Button)
                    .aria_label(format!(
                        "{} {}",
                        language.localized(copy::configuration::SWITCH_TO),
                        kind.display_name()
                    ))
                    .tab_stop(enabled)
                    .focusable()
                    .when(enabled, gpui::Styled::cursor_pointer)
                    .h(ControlSize::Compact.height())
                    .px(Space::Md.px())
                    .rounded(Radius::Control.px())
                    .border_1()
                    .border_color(if selected {
                        theme.action_primary
                    } else {
                        theme.outline_subtle
                    })
                    .bg(if selected {
                        theme.action_soft
                    } else {
                        theme.surface_high
                    })
                    .text_size(TextRole::Label.size())
                    .line_height(TextRole::Label.line_height())
                    .font_weight(TextRole::Label.weight())
                    .text_color(if selected || enabled {
                        theme.action_primary
                    } else {
                        theme.text_tertiary
                    })
                    .child(if selected {
                        language.localized(copy::configuration::CURRENT_2)
                    } else {
                        language.localized(copy::configuration::SWITCH_AND_VALIDATE)
                    })
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
            .child(Self::workspace_header(
                language.message(Message::RoutingRules),
                language.localized(copy::configuration::INSPECT_THE_ORDERED_RULES_THAT_ACTUALLY_PARTICIPATE_IN_MATCHING_MANAGE),
                language.localized(copy::configuration::TOP_DOWN),
                StatusTone::Route,
                theme,
                compact,
            ))
            .child(
                div()
                    .id("routing-rules-scroll")
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_y_scroll()
                    .p(if compact { px(12.0) } else { px(20.0) })
                    .pb(px(56.0))
                    .child(
                        div()
                            .max_w(px(1040.0))
                            .mx_auto()
                            .child(self.active_rules_panel(theme, language, compact, cx)),
                    ),
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
            .bg(theme.surface_high)
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

    fn source_panel(&self, theme: Theme, compact: bool, cx: &mut Context<Self>) -> Div {
        let language = self.language();
        let saved_source_count = self.imported_subscriptions.len() + self.saved_single_nodes.len();
        let add_action = action_button(
            "configuration-add-proxy-source",
            language.localized(copy::configuration::ADD_SOURCE),
            ActionRole::Primary,
            ControlSize::Compact,
        )
        .bg(theme.action_primary)
        .text_color(theme.action_on_primary)
        .on_click(cx.listener(|this, _, window, cx| {
            this.open_new_subscription_editor(cx);
            this.open_proxy_source_dialog(window, cx);
        }));

        let panel = panel_surface("configuration-source", compact, theme)
            .child(section_heading(
                language.localized(copy::configuration::PROXY_SOURCES),
                language.count(CountNoun::Source, saved_source_count),
                Some(add_action.into_any_element()),
                theme,
            ))
            .when_some(self.source_store_error, |panel, error| {
                panel.child(Self::subscription_error(
                    language.localized(copy::configuration::SOME_LOCAL_SOURCES_COULD_NOT_BE_RESTORED),
                    error.to_string(),
                    Some(language.localized(copy::configuration::OTHER_SAFELY_READABLE_SOURCES_ARE_KEPT_CHECK_THE_USER_DATA)),
                    theme,
                ))
            })
            .child(
                div()
                    .mt(Space::Lg.px())
                    .pt(Space::Md.px())
                    .border_t_1()
                    .border_color(theme.outline_subtle)
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(TextRole::Label.size())
                            .line_height(TextRole::Label.line_height())
                            .font_weight(TextRole::Label.weight())
                            .child(language.localized(copy::common::SAVED)),
                    ),
            )
            .when(saved_source_count == 0, |panel| {
                panel.child(
                    empty_state(
                        language.localized(copy::configuration::NO_PROXY_SOURCES),
                        language.localized(copy::configuration::ADD_A_SUBSCRIPTION_OR_A_SINGLE_NODE_SOURCE),
                        Some(
                            action_button(
                                "configuration-empty-add-proxy-source",
                                language.localized(copy::configuration::ADD_SOURCE),
                                ActionRole::Primary,
                                ControlSize::Compact,
                            )
                            .bg(theme.action_primary)
                            .text_color(theme.action_on_primary)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.open_new_subscription_editor(cx);
                                this.open_proxy_source_dialog(window, cx);
                            }))
                            .into_any_element(),
                        ),
                        theme,
                    )
                    .mt(Space::Md.px()),
                )
            })
            .child(self.imported_subscription_cards(theme, cx))
            .child(self.saved_single_node_cards(theme, cx));
        div().w_full().child(panel)
    }

    fn open_new_subscription_editor(&mut self, cx: &mut Context<Self>) {
        self.proxy_source_editor.subscription_source_id = None;
        self.proxy_source_editor.single_node_source_id = None;
        self.proxy_source_editor.kind = ProxySourceEditorKind::Subscription;
        self.proxy_source_editor.refresh_interval = RemoteSourceRefreshInterval::Manual;
        self.proxy_source_editor.interval_popover = false;
        self.proxy_source_editor.enabled = true;
        self.proxy_source_editor.error = None;
        self.proxy_source_editor.feedback = SubscriptionFeedback::Idle;
        if let Some(input) = self.proxy_source_editor.name_input.as_ref() {
            input.update(cx, SubscriptionTextInput::clear_without_event);
        }
        if let Some(input) = self.proxy_source_editor.input.as_ref() {
            input.update(cx, SubscriptionTextInput::clear_without_event);
        }
        cx.notify();
    }

    fn open_subscription_editor(&mut self, id: String, cx: &mut Context<Self>) {
        let Some(subscription) = self
            .imported_subscriptions
            .iter()
            .find(|subscription| subscription.id == id)
        else {
            return;
        };
        let name = subscription.name.clone();
        let url = subscription.source.expose_to(str::to_owned);
        self.proxy_source_editor.subscription_source_id = Some(id);
        self.proxy_source_editor.single_node_source_id = None;
        self.proxy_source_editor.kind = ProxySourceEditorKind::Subscription;
        self.proxy_source_editor.refresh_interval = subscription.refresh_interval;
        self.proxy_source_editor.interval_popover = false;
        self.proxy_source_editor.enabled = subscription.enabled;
        self.proxy_source_editor.error = None;
        self.proxy_source_editor.feedback = SubscriptionFeedback::Idle;
        if let Some(input) = self.proxy_source_editor.name_input.as_ref() {
            input.update(cx, |input, cx| input.set_value_without_event(name, cx));
        }
        if let Some(input) = self.proxy_source_editor.input.as_ref() {
            input.update(cx, |input, cx| input.set_value_without_event(url, cx));
        }
        cx.notify();
    }

    fn open_single_node_editor(&mut self, id: String, cx: &mut Context<Self>) {
        let Some(saved) = self.saved_single_nodes.iter().find(|saved| saved.id == id) else {
            return;
        };
        let name = saved.name.clone();
        let url = saved.source.expose_to(str::to_owned);
        self.proxy_source_editor.subscription_source_id = None;
        self.proxy_source_editor.single_node_source_id = Some(id);
        self.proxy_source_editor.kind = ProxySourceEditorKind::SingleNode;
        self.proxy_source_editor.refresh_interval = RemoteSourceRefreshInterval::Manual;
        self.proxy_source_editor.interval_popover = false;
        self.proxy_source_editor.enabled = saved.enabled;
        self.proxy_source_editor.error = None;
        self.proxy_source_editor.feedback = SubscriptionFeedback::Idle;
        if let Some(input) = self.proxy_source_editor.name_input.as_ref() {
            input.update(cx, |input, cx| input.set_value_without_event(name, cx));
        }
        if let Some(input) = self.proxy_source_editor.input.as_ref() {
            input.update(cx, |input, cx| input.set_value_without_event(url, cx));
        }
        cx.notify();
    }

    fn open_proxy_source_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let app = cx.entity();
        window.open_dialog(cx, move |dialog, window, cx| {
            app.update(cx, |this, cx| {
                let theme = this.theme();
                this.proxy_source_editor_modal(dialog, theme, this.language(), window, cx)
            })
        });
        if let Some(input) = self.proxy_source_editor.name_input.as_ref() {
            input.focus_handle(cx).focus(window, cx);
        }
        cx.notify();
    }

    fn close_subscription_editor(&mut self, cx: &mut Context<Self>) {
        self.configuration_add_section = None;
        self.proxy_source_editor.subscription_source_id = None;
        self.proxy_source_editor.single_node_source_id = None;
        self.proxy_source_editor.interval_popover = false;
        self.proxy_source_editor.error = None;
        self.proxy_source_editor.feedback = SubscriptionFeedback::Idle;
        if let Some(input) = self.proxy_source_editor.name_input.as_ref() {
            input.update(cx, SubscriptionTextInput::clear_without_event);
        }
        if let Some(input) = self.proxy_source_editor.input.as_ref() {
            input.update(cx, SubscriptionTextInput::clear_without_event);
        }
        cx.notify();
    }

    fn proxy_source_editor_modal(
        &self,
        dialog: Dialog,
        theme: Theme,
        language: Language,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Dialog {
        let input = self
            .proxy_source_editor
            .input
            .as_ref()
            .expect("subscription input is initialized before rendering")
            .clone();
        let name_input = self
            .proxy_source_editor
            .name_input
            .as_ref()
            .expect("subscription name input is initialized before rendering")
            .clone();
        let viewport = window.viewport_size();
        let view = ProxySourceEditorView {
            direct_input: self.proxy_source_editor.kind == ProxySourceEditorKind::SingleNode,
            editing: self.proxy_source_editor.subscription_source_id.is_some()
                || self.proxy_source_editor.single_node_source_id.is_some(),
            activity: if matches!(
                self.proxy_source_editor.feedback,
                SubscriptionFeedback::Importing(_)
            ) {
                ProxySourceEditorActivity::Busy
            } else {
                ProxySourceEditorActivity::Idle
            },
            enabled: self.proxy_source_editor.enabled,
            dialog_width: (viewport.width.as_f32() - 32.0).clamp(300.0, 620.0),
        };
        let interval_select = self.proxy_source_interval_select(view, language, theme, cx);
        let body = self.proxy_source_editor_body(
            ProxySourceEditorInputs {
                source: input.clone(),
                name: name_input,
                interval_select,
            },
            view,
            language,
            theme,
            cx,
        );
        let footer = Self::proxy_source_editor_footer(input, view, language, theme, cx);
        let app = cx.entity();
        dialog
            .width(px(view.dialog_width))
            .max_h(px((viewport.height.as_f32() - 32.0).max(320.0)))
            .margin_top(px(((viewport.height.as_f32() - 480.0) / 2.0).max(16.0)))
            .overlay(true)
            .overlay_closable(true)
            .keyboard(true)
            .close_button(false)
            .p_0()
            .rounded_md()
            .bg(theme.surface_high)
            .overflow_hidden()
            .title(Self::proxy_source_editor_title(view, language, theme))
            .child(body)
            .footer(footer)
            .on_close(move |_, _, cx| {
                app.update(cx, ManisApp::close_subscription_editor);
            })
    }

    fn proxy_source_interval_select(
        &self,
        view: ProxySourceEditorView,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut menu = div().p_1();
        for interval in [
            RemoteSourceRefreshInterval::Manual,
            RemoteSourceRefreshInterval::Hourly,
            RemoteSourceRefreshInterval::SixHours,
            RemoteSourceRefreshInterval::TwelveHours,
            RemoteSourceRefreshInterval::Daily,
        ] {
            let selected = interval == self.proxy_source_editor.refresh_interval;
            menu = menu.child(
                div()
                    .id(format!("subscription-refresh-option-{interval:?}"))
                    .role(Role::Button)
                    .aria_label(refresh_interval_label(interval, language))
                    .tab_stop(true)
                    .focusable()
                    .cursor_pointer()
                    .px_3()
                    .py_2()
                    .rounded(Radius::Control.px())
                    .bg(if selected {
                        theme.action_soft
                    } else {
                        theme.surface_high
                    })
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
                    .child(refresh_interval_label(interval, language))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.proxy_source_editor.refresh_interval = interval;
                        this.proxy_source_editor.interval_popover = false;
                        cx.notify();
                    })),
            );
        }
        let trigger = Button::new("subscription-editor-refresh-interval")
            .accessibility_label(
                language.localized(copy::configuration::CHOOSE_SUBSCRIPTION_UPDATE_INTERVAL),
            )
            .dropdown_caret(true)
            .with_variant(ButtonVariant::Default)
            .with_size(ControlSize::Standard.component_size())
            .h(ControlSize::Standard.height())
            .w_full()
            .child(refresh_interval_label(
                self.proxy_source_editor.refresh_interval,
                language,
            ))
            .disabled(view.busy());
        let app = cx.entity();
        crate::components::anchored_popover(
            "subscription-editor-refresh-popover",
            trigger,
            menu,
            (view.dialog_width - 40.0).max(240.0),
            280.0,
        )
        .open(self.proxy_source_editor.interval_popover)
        .on_open_change(move |open, _, cx| {
            app.update(cx, |this, cx| {
                this.proxy_source_editor.interval_popover = *open;
                cx.notify();
            });
        })
        .into_any_element()
    }

    fn proxy_source_editor_body(
        &self,
        inputs: ProxySourceEditorInputs,
        view: ProxySourceEditorView,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        div()
            .id("proxy-source-modal-body")
            .flex_1()
            .min_h(px(0.0))
            .overflow_y_scroll()
            .px_5()
            .py_4()
            .when(!view.editing, |body| {
                body.child(field_label(
                    language.localized(copy::configuration::SOURCE_TYPE),
                    theme,
                ))
                .child(Self::proxy_source_kind_picker(view, language, theme, cx))
            })
            .child(field_label(
                if view.direct_input {
                    language.localized(copy::configuration::NODE_NAME)
                } else {
                    language.localized(copy::configuration::SOURCE_NAME)
                },
                theme,
            ))
            .child(inputs.name)
            .child(field_label(language.localized(copy::configuration::SOURCE_URL), theme).mt_4())
            .child(inputs.source)
            .when(!view.direct_input, |body| {
                body.child(
                    field_label(
                        language.localized(copy::configuration::UPDATE_INTERVAL),
                        theme,
                    )
                    .mt_4(),
                )
                .child(inputs.interval_select)
            })
            .child(
                Checkbox::new("proxy-source-editor-enabled")
                    .label(language.localized(copy::configuration::USE_THIS_SOURCE))
                    .checked(view.enabled)
                    .disabled(view.busy())
                    .tab_stop(!view.busy())
                    .cursor_pointer()
                    .mt_4()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if !view.busy() {
                            this.proxy_source_editor.enabled = !view.enabled;
                            cx.notify();
                        }
                    })),
            )
            .when_some(self.proxy_source_editor.error.clone(), |body, error| {
                body.child(
                    div()
                        .mt_3()
                        .text_size(TextRole::Metadata.size())
                        .text_color(theme.status_error)
                        .child(error),
                )
            })
    }

    fn proxy_source_kind_picker(
        view: ProxySourceEditorView,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        div().mt_1().flex().gap_2().children(
            [
                (
                    ProxySourceEditorKind::Subscription,
                    "proxy-source-kind-subscription",
                ),
                (
                    ProxySourceEditorKind::SingleNode,
                    "proxy-source-kind-single-node",
                ),
            ]
            .map(|(kind, id)| {
                let selected = (kind == ProxySourceEditorKind::SingleNode) == view.direct_input;
                action_button(
                    id,
                    match kind {
                        ProxySourceEditorKind::Subscription => {
                            language.localized(copy::configuration::SUBSCRIPTION)
                        }
                        ProxySourceEditorKind::SingleNode => {
                            language.localized(copy::configuration::SINGLE_NODE_2)
                        }
                    },
                    if selected {
                        ActionRole::Primary
                    } else {
                        ActionRole::Secondary
                    },
                    ControlSize::Compact,
                )
                .cursor_pointer()
                .bg(if selected {
                    theme.action_primary
                } else {
                    theme.surface_high
                })
                .text_color(if selected {
                    theme.action_on_primary
                } else {
                    theme.text_secondary
                })
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.proxy_source_editor.kind = kind;
                    this.proxy_source_editor.error = None;
                    cx.notify();
                }))
            }),
        )
    }

    fn proxy_source_editor_footer(
        input: Entity<SubscriptionTextInput>,
        view: ProxySourceEditorView,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        div()
            .flex_shrink_0()
            .px_5()
            .py_4()
            .border_t_1()
            .border_color(theme.outline_subtle)
            .flex()
            .items_center()
            .justify_end()
            .gap_2()
            .child(
                style_action_button(
                    Button::new("cancel-proxy-source").label(language.message(Message::Cancel)),
                    ActionRole::Secondary,
                    ControlSize::Standard,
                )
                .px(Space::Lg.px())
                .cursor_pointer()
                .on_click(cx.listener(|this, _, window, cx| {
                    this.close_subscription_editor(cx);
                    window.close_dialog(cx);
                })),
            )
            .child(
                style_action_button(
                    Button::new("save-proxy-source")
                        .label(if view.busy() {
                            language.localized(copy::configuration::PROCESSING)
                        } else if view.editing {
                            language.message(Message::SaveChanges)
                        } else {
                            language.localized(copy::configuration::ADD_SOURCE)
                        })
                        .loading(view.busy()),
                    ActionRole::Primary,
                    ControlSize::Standard,
                )
                .px(Space::Lg.px())
                .when(!view.busy(), gpui::Styled::cursor_pointer)
                .bg(if view.busy() {
                    theme.action_soft
                } else {
                    theme.action_primary
                })
                .text_color(if view.busy() {
                    theme.action_primary
                } else {
                    theme.action_on_primary
                })
                .on_click(cx.listener(move |this, _, window, cx| {
                    if !view.busy() && this.submit_source_import(&input, cx) {
                        window.close_dialog(cx);
                    }
                })),
            )
    }

    fn proxy_source_editor_title(
        view: ProxySourceEditorView,
        language: Language,
        theme: Theme,
    ) -> Div {
        div()
            .px_5()
            .py_4()
            .border_b_1()
            .border_color(theme.outline_subtle)
            .child(
                div()
                    .text_size(px(17.0))
                    .font_weight(TextRole::SectionTitle.weight())
                    .child(if view.editing {
                        language.localized(copy::configuration::EDIT_PROXY_SOURCE)
                    } else {
                        language.localized(copy::configuration::ADD_PROXY_SOURCE)
                    }),
            )
            .child(
                div()
                    .mt_1()
                    .text_size(TextRole::Metadata.size())
                    .text_color(theme.text_secondary)
                    .child(if view.direct_input {
                        language.localized(copy::configuration::A_SINGLE_NODE_SOURCE_DOES_NOT_NEED_AN_UPDATE_INTERVAL)
                    } else {
                        language.localized(copy::configuration::CHOOSE_A_SUBSCRIPTION_OR_A_SINGLE_NODE_SHARE_LINK)
                    }),
            )
    }

    fn submit_source_import(
        &mut self,
        input: &Entity<SubscriptionTextInput>,
        cx: &mut Context<Self>,
    ) -> bool {
        let name = self
            .proxy_source_editor
            .name_input
            .as_ref()
            .map(|input| input.read(cx).value().trim().to_owned())
            .unwrap_or_default();
        if name.is_empty() {
            self.proxy_source_editor.error = Some(
                self.language()
                    .localized(copy::configuration::ENTER_A_SOURCE_NAME)
                    .to_owned(),
            );
            cx.notify();
            return false;
        }
        self.proxy_source_editor.error = None;
        let (input_value, result) = {
            let input = input.read(cx);
            (
                input.value().to_owned(),
                match self.proxy_source_editor.kind {
                    ProxySourceEditorKind::Subscription => {
                        validate_subscription_preview(input.value())
                    }
                    ProxySourceEditorKind::SingleNode => {
                        validate_single_node_preview(input.value())
                    }
                },
            )
        };
        match result {
            Ok(preview) if preview.kind == SourceKind::SingleNode => {
                if self.proxy_source_editor.subscription_source_id.is_some() {
                    self.proxy_source_editor.error = Some(
                        self.language()
                            .localized(copy::configuration::AN_EXISTING_SUBSCRIPTION_MUST_KEEP_AN_HTTP_HTTPS_URL)
                            .to_owned(),
                    );
                    cx.notify();
                    return false;
                }
                self.import_single_node(input_value, name, preview, cx)
            }
            Ok(preview) => {
                if self.proxy_source_editor.single_node_source_id.is_some() {
                    self.proxy_source_editor.error = Some(
                        self.language()
                            .localized(copy::configuration::THIS_SOURCE_MUST_REMAIN_A_SINGLE_NODE_SHARE_LINK)
                            .to_owned(),
                    );
                    cx.notify();
                    return false;
                }
                trace_ui(UiEvent::SourceRecognitionSucceeded);
                self.import_remote_subscription(
                    super::SubscriptionImportRequest {
                        input: input_value,
                        name,
                        refresh_interval: self.proxy_source_editor.refresh_interval,
                        enabled: self.proxy_source_editor.enabled,
                        editing_id: self.proxy_source_editor.subscription_source_id.clone(),
                        kind: preview.kind,
                    },
                    cx,
                );
                true
            }
            Err(error) => {
                self.proxy_source_editor.feedback = SubscriptionFeedback::InvalidInput(error);
                self.status = format!(
                    "{}: {error}",
                    self.language()
                        .localized(copy::configuration::SOURCE_RECOGNITION_FAILED)
                );
                trace_ui(UiEvent::SourceRecognitionFailed);
                cx.notify();
                false
            }
        }
    }

    fn import_single_node(
        &mut self,
        input_value: String,
        name: String,
        preview: crate::subscription::SubscriptionPreview,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(store_dir) = self.subscription_store_dir.clone() else {
            self.proxy_source_editor.feedback =
                SubscriptionFeedback::StoreFailed(SubscriptionStoreError::DataDirectoryUnavailable);
            self.language()
                .localized(copy::configuration::COULD_NOT_DETERMINE_WHERE_TO_SAVE_THE_NODE)
                .clone_into(&mut self.status);
            trace_ui(UiEvent::SourceImportFailed);
            cx.notify();
            return false;
        };
        self.subscription_preview_generation = self.subscription_preview_generation.wrapping_add(1);
        let generation = self.subscription_preview_generation;
        self.proxy_source_editor.feedback = SubscriptionFeedback::Importing(SourceKind::SingleNode);
        self.language()
            .localized(copy::configuration::VALIDATING_AND_SAVING_SINGLE_NODE_SOURCE)
            .clone_into(&mut self.status);
        if let Some(input) = self.proxy_source_editor.input.as_ref() {
            input.update(cx, |input, cx| input.set_enabled(false, cx));
        }
        let runtime = self.runtime.clone();
        let editing_id = self.proxy_source_editor.single_node_source_id.clone();
        let enabled = self.proxy_source_editor.enabled;
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn(async move {
                    let providers = mihomo::preview_single_node(&input_value)?;
                    let transaction = super::mutate_saved_sources(&runtime, &store_dir, || {
                        if let Some(id) = editing_id {
                            mihomo::update_single_node_source_in(
                                &store_dir,
                                &id,
                                &input_value,
                                &name,
                                enabled,
                            )
                        } else {
                            mihomo::save_single_node_source_with_options_in(
                                &store_dir,
                                &input_value,
                                &name,
                                enabled,
                            )
                        }
                    })?;
                    Ok::<_, SubscriptionStoreError>((transaction, providers))
                })
                .await;
            this.update(cx, |this, cx| {
                this.finish_single_node_import(generation, preview, result, cx);
            })
            .ok();
        })
        .detach();
        cx.notify();
        true
    }

    fn finish_single_node_import(
        &mut self,
        generation: u64,
        preview: crate::subscription::SubscriptionPreview,
        result: super::SingleNodeImportResult,
        cx: &mut Context<Self>,
    ) {
        if self.subscription_preview_generation != generation {
            return;
        }
        if let Some(input) = self.proxy_source_editor.input.as_ref() {
            input.update(cx, |input, cx| input.set_enabled(true, cx));
        }
        match result {
            Ok((transaction, providers)) => {
                self.finish_saved_single_node(transaction, providers, preview, cx);
            }
            Err(error) => {
                self.proxy_source_editor.feedback = SubscriptionFeedback::StoreFailed(error);
                self.status = format!(
                    "{}: {error}",
                    self.language()
                        .localized(copy::configuration::SINGLE_NODE_SOURCE_SAVE_FAILED)
                );
                trace_ui(UiEvent::SourceImportFailed);
                cx.notify();
            }
        }
    }

    fn finish_saved_single_node(
        &mut self,
        mut transaction: super::SourceMutation<mihomo::StoredSingleNode>,
        providers: Vec<mihomo::LoadedProvider>,
        preview: crate::subscription::SubscriptionPreview,
        cx: &mut Context<Self>,
    ) {
        let language = self.language();
        transaction.apply.reconcile_proxy_mode(&mut self.proxy_mode);
        let Some(stored) = transaction.value.take() else {
            self.proxy_source_editor.feedback =
                SubscriptionFeedback::StoreFailed(SubscriptionStoreError::StoreUnavailable);
            self.status = format!(
                "{}{}",
                language.localized(copy::configuration::SINGLE_NODE_SOURCE_SAVE_FAILED),
                transaction
                    .apply
                    .status_suffix_after_source_rollback(language)
            );
            trace_ui(UiEvent::SourceImportFailed);
            cx.notify();
            return;
        };
        if let Some(existing) = self
            .saved_single_nodes
            .iter_mut()
            .find(|node| node.id == stored.id)
        {
            *existing = stored;
        } else {
            self.saved_single_nodes.push(stored);
        }
        self.subscription_preview_providers = providers;
        self.proxy_source_editor.feedback = SubscriptionFeedback::Valid(preview);
        if let Some(input) = self.proxy_source_editor.input.as_ref() {
            input.update(cx, SubscriptionTextInput::clear_without_event);
        }
        if let Some(input) = self.proxy_source_editor.name_input.as_ref() {
            input.update(cx, SubscriptionTextInput::clear_without_event);
        }
        self.configuration_add_section = None;
        self.status = copy::configuration::single_node_saved(
            language,
            &transaction.apply.status_suffix(language),
        );
        trace_ui(UiEvent::SourceImportSucceeded);
        cx.notify();
    }

    fn imported_subscription_cards(&self, theme: Theme, cx: &mut Context<Self>) -> Div {
        let language = self.language();
        let now = mihomo::current_unix_secs();
        div().children(self.imported_subscriptions.iter().map(|subscription| {
            let presentation = self.subscription_card_presentation(subscription, now, language);
            Self::imported_subscription_card(subscription, &presentation, language, theme, cx)
        }))
    }

    fn subscription_card_presentation(
        &self,
        subscription: &super::ImportedSubscription,
        now: u64,
        language: Language,
    ) -> SubscriptionCardPresentation {
        let node_count = subscription
            .providers
            .iter()
            .map(|provider| provider.nodes.len())
            .sum::<usize>();
        let (state, activity) = match &subscription.state {
            ImportedSubscriptionState::None => (
                language
                    .localized(copy::configuration::DISABLED_2)
                    .to_owned(),
                SubscriptionCardActivity::Idle { healthy: true },
            ),
            ImportedSubscriptionState::Pending(_) | ImportedSubscriptionState::Refreshing(_) => (
                language
                    .localized(copy::configuration::UPDATING_2)
                    .to_owned(),
                SubscriptionCardActivity::Busy,
            ),
            ImportedSubscriptionState::Ready(kind) => (
                copy::configuration::source_nodes(
                    language,
                    source_kind_label(*kind, language),
                    node_count,
                ),
                SubscriptionCardActivity::Idle { healthy: true },
            ),
            ImportedSubscriptionState::Unavailable(_, _)
            | ImportedSubscriptionState::StoreError(_) => (
                language
                    .localized(copy::configuration::UPDATE_FAILED)
                    .to_owned(),
                SubscriptionCardActivity::Idle { healthy: false },
            ),
            ImportedSubscriptionState::Removing(_) => (
                language.localized(copy::configuration::REMOVING).to_owned(),
                SubscriptionCardActivity::Busy,
            ),
        };
        let controls_enabled = !activity.is_busy()
            && !matches!(
                self.proxy_source_editor.feedback,
                SubscriptionFeedback::Importing(_)
            )
            && !self.source_refresh_busy();
        SubscriptionCardPresentation {
            state,
            activity,
            controls_enabled,
            updated: source_update_label(
                subscription.last_successful_update_unix_secs,
                now,
                language,
            ),
        }
    }

    fn imported_subscription_card(
        subscription: &super::ImportedSubscription,
        presentation: &SubscriptionCardPresentation,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let card_edit_id = subscription.id.clone();
        let controls_enabled = presentation.controls_enabled;
        div()
            .id(format!("subscription-card-{card_edit_id}"))
            .role(Role::Button)
            .aria_label(language.localized(copy::configuration::EDIT_THIS_SUBSCRIPTION))
            .tab_stop(controls_enabled)
            .focusable()
            .when(controls_enabled, gpui::Styled::cursor_pointer)
            .mt_2()
            .px_3()
            .py_2()
            .rounded(Radius::Row.px())
            .border_1()
            .border_color(theme.outline_subtle)
            .bg(theme.surface_low)
            .child(Self::subscription_card_header(
                subscription,
                presentation,
                language,
                theme,
                cx,
            ))
            .child(
                div()
                    .mt_1()
                    .ml_7()
                    .overflow_x_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_size(TextRole::Metadata.size())
                    .text_color(theme.text_tertiary)
                    .child(subscription.source.expose_to(str::to_owned)),
            )
            .child(Self::subscription_card_actions(
                subscription,
                presentation,
                language,
                theme,
                cx,
            ))
            .on_click(cx.listener(move |this, _, window, cx| {
                if controls_enabled {
                    this.open_subscription_editor(card_edit_id.clone(), cx);
                    this.open_proxy_source_dialog(window, cx);
                }
            }))
    }

    fn subscription_card_header(
        subscription: &super::ImportedSubscription,
        presentation: &SubscriptionCardPresentation,
        _language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        let toggle_id = subscription.id.clone();
        let enabled = subscription.enabled;
        let controls_enabled = presentation.controls_enabled;
        div()
            .flex()
            .items_center()
            .gap_2()
            .child(
                Checkbox::new(format!("subscription-enabled-{toggle_id}"))
                    .label("")
                    .checked(enabled)
                    .disabled(!controls_enabled)
                    .tab_stop(controls_enabled)
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        if controls_enabled {
                            this.set_subscription_enabled(&toggle_id, !enabled, cx);
                        }
                    })),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .min_w(px(0.0))
                            .overflow_x_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(TextRole::Label.size())
                            .font_weight(TextRole::Label.weight())
                            .text_color(if !enabled {
                                theme.text_secondary
                            } else if presentation.activity.is_healthy() {
                                theme.text_primary
                            } else {
                                theme.status_error
                            })
                            .child(subscription.name.clone()),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_size(TextRole::Metadata.size())
                            .text_color(theme.text_tertiary)
                            .child(presentation.state.clone()),
                    ),
            )
            .when(presentation.activity.is_busy(), |row| {
                row.child(Self::benchmark_latency_spinner(
                    format!("source-refresh-{}", subscription.id),
                    theme,
                ))
            })
    }

    fn subscription_card_actions(
        subscription: &super::ImportedSubscription,
        presentation: &SubscriptionCardPresentation,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        let refresh_id = subscription.id.clone();
        let remove_id = subscription.id.clone();
        let refresh_enabled = presentation.controls_enabled && subscription.enabled;
        let controls_enabled = presentation.controls_enabled;
        let busy = presentation.activity.is_busy();
        div()
            .mt_1()
            .ml_7()
            .flex()
            .items_center()
            .gap_2()
            .text_size(TextRole::Metadata.size())
            .text_color(theme.text_secondary)
            .child(refresh_interval_label(
                subscription.refresh_interval,
                language,
            ))
            .child("·")
            .child(presentation.updated.clone())
            .child(div().flex_1())
            .child(
                action_button(
                    format!("subscription-refresh-{refresh_id}"),
                    if busy {
                        language.localized(copy::configuration::UPDATING)
                    } else {
                        language.localized(copy::configuration::UPDATE_NOW)
                    },
                    ActionRole::Quiet,
                    ControlSize::Compact,
                )
                .accessibility_label(
                    language.localized(copy::configuration::UPDATE_THIS_SUBSCRIPTION_NOW),
                )
                .disabled(!refresh_enabled)
                .loading(busy)
                .when(refresh_enabled, gpui::Styled::cursor_pointer)
                .px_3()
                .border_1()
                .border_color(theme.outline_subtle)
                .bg(theme.surface_high)
                .text_color(theme.action_primary)
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    if refresh_enabled {
                        this.refresh_imported_subscription(refresh_id.clone(), cx);
                    }
                })),
            )
            .when(controls_enabled, |row| {
                row.child(
                    action_button(
                        format!("remove-{remove_id}"),
                        language.localized(copy::configuration::REMOVE),
                        ActionRole::Quiet,
                        ControlSize::Compact,
                    )
                    .accessibility_label(
                        language.localized(copy::configuration::REMOVE_THIS_SUBSCRIPTION),
                    )
                    .cursor_pointer()
                    .px_3()
                    .text_color(theme.status_error)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.remove_imported_subscription(remove_id.clone(), cx);
                    })),
                )
            })
    }

    fn saved_single_node_cards(&self, theme: Theme, cx: &mut Context<Self>) -> Div {
        let language = self.language();
        let controls_enabled = !matches!(
            self.proxy_source_editor.feedback,
            SubscriptionFeedback::Importing(_)
        ) && !self.source_refresh_busy();
        div().children(self.saved_single_nodes.iter().map(|saved| {
            Self::saved_single_node_card(saved, controls_enabled, language, theme, cx)
        }))
    }

    fn saved_single_node_card(
        saved: &mihomo::StoredSingleNode,
        controls_enabled: bool,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let card_edit_id = saved.id.clone();
        let node = saved.source.preview();
        div()
            .id(format!("single-node-card-{card_edit_id}"))
            .role(Role::Button)
            .aria_label(language.localized(copy::configuration::EDIT_THIS_SINGLE_NODE_SOURCE))
            .tab_stop(controls_enabled)
            .focusable()
            .when(controls_enabled, gpui::Styled::cursor_pointer)
            .mt_2()
            .px_3()
            .py_2()
            .rounded(Radius::Row.px())
            .border_1()
            .border_color(theme.outline_subtle)
            .bg(theme.surface_low)
            .child(Self::saved_single_node_header(
                saved,
                node,
                controls_enabled,
                theme,
                cx,
            ))
            .child(
                div()
                    .mt_1()
                    .ml_7()
                    .overflow_x_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_size(TextRole::Metadata.size())
                    .text_color(theme.text_tertiary)
                    .child(saved.source.expose_to(str::to_owned)),
            )
            .child(Self::saved_single_node_actions(
                saved,
                node,
                controls_enabled,
                language,
                theme,
                cx,
            ))
            .on_click(cx.listener(move |this, _, window, cx| {
                if controls_enabled {
                    this.open_single_node_editor(card_edit_id.clone(), cx);
                    this.open_proxy_source_dialog(window, cx);
                }
            }))
    }

    fn saved_single_node_header(
        saved: &mihomo::StoredSingleNode,
        node: &crate::subscription::SourceNodePreview,
        controls_enabled: bool,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        let toggle_id = saved.id.clone();
        let enabled = saved.enabled;
        div()
            .flex()
            .items_center()
            .gap_2()
            .child(
                Checkbox::new(format!("single-node-enabled-{toggle_id}"))
                    .label("")
                    .checked(enabled)
                    .disabled(!controls_enabled)
                    .tab_stop(controls_enabled)
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        if controls_enabled {
                            this.set_single_node_enabled(toggle_id.clone(), !enabled, cx);
                        }
                    })),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .min_w(px(0.0))
                            .overflow_x_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(TextRole::Label.size())
                            .font_weight(TextRole::Label.weight())
                            .text_color(if enabled {
                                theme.text_primary
                            } else {
                                theme.text_secondary
                            })
                            .child(saved.name.clone()),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_size(TextRole::Metadata.size())
                            .text_color(theme.text_tertiary)
                            .child(node.protocol),
                    ),
            )
    }

    fn saved_single_node_actions(
        saved: &mihomo::StoredSingleNode,
        node: &crate::subscription::SourceNodePreview,
        controls_enabled: bool,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        let remove_id = saved.id.clone();
        div()
            .mt_1()
            .ml_7()
            .flex()
            .items_center()
            .gap_2()
            .text_size(TextRole::Metadata.size())
            .text_color(theme.text_secondary)
            .child(format!("{} · {}", node.endpoint, node.detail))
            .child(div().flex_1())
            .when(controls_enabled, |row| {
                row.child(
                    action_button(
                        format!("remove-single-node-{remove_id}"),
                        language.localized(copy::configuration::REMOVE),
                        ActionRole::Quiet,
                        ControlSize::Compact,
                    )
                    .accessibility_label(
                        language.localized(copy::configuration::REMOVE_SINGLE_NODE_SOURCE),
                    )
                    .cursor_pointer()
                    .px_3()
                    .text_color(theme.status_error)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.remove_saved_single_node(remove_id.clone(), cx);
                    })),
                )
            })
    }

    fn set_single_node_enabled(&mut self, id: String, enabled: bool, cx: &mut Context<Self>) {
        let Some(store_dir) = self.subscription_store_dir.clone() else {
            return;
        };
        let runtime = self.runtime.clone();
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn(async move {
                    super::mutate_saved_sources(&runtime, &store_dir, || {
                        mihomo::update_single_node_source_enabled_in(&store_dir, &id, enabled)
                    })
                })
                .await;
            this.update(cx, |this, cx| {
                match result {
                    Ok(transaction) => {
                        let language = this.language();
                        transaction.apply.reconcile_proxy_mode(&mut this.proxy_mode);
                        if let Some(stored) = transaction.value {
                            if let Some(existing) = this
                                .saved_single_nodes
                                .iter_mut()
                                .find(|existing| existing.id == stored.id)
                            {
                                *existing = stored;
                            }
                            this.status = format!(
                                "{}{}",
                                language.localized(copy::configuration::SINGLE_NODE_SOURCE_UPDATED),
                                transaction.apply.status_suffix(language)
                            );
                        } else {
                            this.status = format!(
                                "{}{}",
                                language.localized(copy::configuration::COULD_NOT_UPDATE_SOURCE),
                                transaction.apply.status_suffix_after_rollback_attempt(
                                    language,
                                    transaction.rollback_error.as_ref(),
                                )
                            );
                        }
                    }
                    Err(error) => {
                        this.status = format!(
                            "{}: {error}",
                            this.language()
                                .localized(copy::configuration::COULD_NOT_UPDATE_SOURCE)
                        );
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn remove_saved_single_node(&mut self, id: String, cx: &mut Context<Self>) {
        let Some(store_dir) = self.subscription_store_dir.clone() else {
            return;
        };
        let runtime = self.runtime.clone();
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn(async move {
                    super::mutate_saved_sources(&runtime, &store_dir, || {
                        mihomo::remove_single_node_source_in(&store_dir, &id).map(|()| id.clone())
                    })
                })
                .await;
            this.update(cx, |this, cx| {
                match result {
                    Ok(transaction) => {
                        let language = this.language();
                        transaction.apply.reconcile_proxy_mode(&mut this.proxy_mode);
                        if let Some(deleted_id) = transaction.value {
                            this.saved_single_nodes.retain(|node| node.id != deleted_id);
                            this.status = format!(
                                "{}{}",
                                language.localized(copy::configuration::SINGLE_NODE_SOURCE_REMOVED),
                                transaction.apply.status_suffix(language)
                            );
                        } else {
                            this.status = format!(
                                "{}{}",
                                language.localized(copy::configuration::FAILED_TO_REMOVE_SOURCE),
                                transaction.apply.status_suffix_after_rollback_attempt(
                                    language,
                                    transaction.rollback_error.as_ref(),
                                )
                            );
                        }
                    }
                    Err(error) => {
                        this.status = format!(
                            "{}: {error}",
                            this.language()
                                .localized(copy::configuration::FAILED_TO_REMOVE_SOURCE)
                        );
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn subscription_error(
        title: &'static str,
        message: String,
        recovery: Option<&'static str>,
        theme: Theme,
    ) -> Div {
        div()
            .mt(Space::Md.px())
            .p(Space::Md.px())
            .rounded(Radius::Pane.px())
            .border_1()
            .border_color(theme.outline_strong)
            .bg(theme.surface_low)
            .child(
                div()
                    .text_size(TextRole::Label.size())
                    .line_height(TextRole::Label.line_height())
                    .font_weight(TextRole::Label.weight())
                    .child(title),
            )
            .child(
                div()
                    .mt(Space::Xs.px())
                    .text_size(TextRole::Metadata.size())
                    .line_height(TextRole::Metadata.line_height())
                    .text_color(theme.text_secondary)
                    .child(message),
            )
            .when_some(recovery, |card, recovery| {
                card.child(
                    div()
                        .mt(Space::Sm.px())
                        .text_size(TextRole::Metadata.size())
                        .line_height(TextRole::Metadata.line_height())
                        .text_color(theme.text_tertiary)
                        .child(recovery),
                )
            })
    }

    fn rule_source_manager(
        &self,
        _input: Entity<SubscriptionTextInput>,
        busy: bool,
        theme: Theme,
        compact: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let language = self.language();
        let add_action = action_button(
            "configuration-add-rule-source",
            language.localized(copy::configuration::ADD_SOURCE),
            ActionRole::Primary,
            ControlSize::Compact,
        )
        .bg(theme.action_primary)
        .text_color(theme.action_on_primary)
        .on_click(cx.listener(move |this, _, window, cx| {
            this.open_new_qx_rule_editor(cx);
            this.open_qx_rule_source_dialog(window, cx);
        }));
        let mut panel = panel_surface("configuration-rule-sources", compact, theme)
            .child(section_heading(
                language.localized(copy::configuration::RULE_SOURCES),
                language.count(CountNoun::Source, self.rule_sources.sources.len()),
                Some(add_action.into_any_element()),
                theme,
            ))
            .child(
                div()
                    .mt(Space::Lg.px())
                    .pt(Space::Md.px())
                    .border_t_1()
                    .border_color(theme.outline_subtle)
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(TextRole::Label.size())
                            .line_height(TextRole::Label.line_height())
                            .font_weight(TextRole::Label.weight())
                            .child(language.localized(copy::common::SAVED)),
                    ),
            );

        if self.rule_sources.sources.is_empty() {
            panel = panel.child(
                empty_state(
                    language.localized(copy::configuration::NO_RULE_SOURCES),
                    language.localized(copy::configuration::ADD_A_REMOTE_QX_RULE_SET),
                    Some(
                        action_button(
                            "configuration-empty-add-rule-source",
                            language.localized(copy::configuration::ADD_SOURCE),
                            ActionRole::Primary,
                            ControlSize::Compact,
                        )
                        .bg(theme.action_primary)
                        .text_color(theme.action_on_primary)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.open_new_qx_rule_editor(cx);
                            this.open_qx_rule_source_dialog(window, cx);
                        }))
                        .into_any_element(),
                    ),
                    theme,
                )
                .mt(Space::Md.px()),
            );
        }
        for (index, source) in self.rule_sources.sources.iter().enumerate() {
            panel = panel.child(self.rule_source_card(index, source, busy, theme, cx));
        }
        panel
    }

    fn open_new_qx_rule_editor(&mut self, cx: &mut Context<Self>) {
        self.rule_sources.editor_source_id = None;
        self.rule_sources.editor_refresh_interval = RemoteSourceRefreshInterval::Manual;
        self.rule_sources.editor_popover = super::QxRuleEditorPopover::None;
        self.rule_sources.feedback = QxRuleImportFeedback::Idle;
        if !self
            .qx_rule_targets()
            .contains(&self.rule_sources.target_policy)
            && let Some(target) = self.qx_rule_targets().into_iter().next()
        {
            self.rule_sources.target_policy = target;
        }
        if let Some(input) = self.inputs.qx_rule.as_ref() {
            input.update(cx, SubscriptionTextInput::clear_without_event);
        }
        cx.notify();
    }

    fn open_qx_rule_editor(&mut self, id: String, cx: &mut Context<Self>) {
        let Some(source) = self
            .rule_sources
            .sources
            .iter()
            .find(|source| source.id == id)
        else {
            return;
        };
        let url = source.source.expose_to(str::to_owned);
        let target = self.effective_rule_target(source.target_policy.as_str(), self.language());
        self.rule_sources.editor_source_id = Some(id);
        self.rule_sources.editor_refresh_interval = source.refresh_interval;
        self.rule_sources.editor_popover = super::QxRuleEditorPopover::None;
        self.rule_sources.target_policy = target;
        self.rule_sources.feedback = QxRuleImportFeedback::Idle;
        if let Some(input) = self.inputs.qx_rule.as_ref() {
            input.update(cx, |input, cx| input.set_value_without_event(url, cx));
        }
        cx.notify();
    }

    fn open_qx_rule_source_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let app = cx.entity();
        window.open_dialog(cx, move |dialog, window, cx| {
            app.update(cx, |this, cx| {
                this.qx_rule_source_editor_modal(dialog, this.theme(), this.language(), window, cx)
            })
        });
        if let Some(input) = self.inputs.qx_rule.as_ref() {
            input.focus_handle(cx).focus(window, cx);
        }
        cx.notify();
    }

    fn close_qx_rule_editor(&mut self, cx: &mut Context<Self>) {
        self.rule_sources.editor_source_id = None;
        self.rule_sources.editor_popover = super::QxRuleEditorPopover::None;
        self.rule_sources.feedback = QxRuleImportFeedback::Idle;
        if let Some(input) = self.inputs.qx_rule.as_ref() {
            input.update(cx, SubscriptionTextInput::clear_without_event);
        }
        cx.notify();
    }

    fn qx_rule_source_editor_modal(
        &self,
        dialog: Dialog,
        theme: Theme,
        language: Language,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Dialog {
        let input = self
            .inputs
            .qx_rule
            .as_ref()
            .expect("QX rule input is initialized before rendering")
            .clone();
        let viewport = window.viewport_size();
        let view = QxRuleEditorView {
            editing: self.rule_sources.editor_source_id.is_some(),
            busy: self.rule_sources.feedback == QxRuleImportFeedback::Importing,
            dialog_width: (viewport.width.as_f32() - 32.0).clamp(300.0, 620.0),
        };
        let body = div()
            .id("qx-rule-source-modal-body")
            .flex_1()
            .min_h(px(0.0))
            .overflow_y_scroll()
            .px_5()
            .py_4()
            .child(field_label(
                language.localized(copy::configuration::RULE_URL),
                theme,
            ))
            .child(input.clone())
            .child(
                field_label(
                    language.localized(copy::configuration::TARGET_POLICY),
                    theme,
                )
                .mt_4(),
            )
            .child(self.qx_rule_target_select(view, language, theme, cx))
            .child(
                field_label(
                    language.localized(copy::configuration::UPDATE_INTERVAL),
                    theme,
                )
                .mt_4(),
            )
            .child(self.qx_rule_interval_select(view, language, theme, cx))
            .child(self.qx_rule_import_feedback(theme, language));
        let app = cx.entity();
        dialog
            .width(px(view.dialog_width))
            .max_h(px((viewport.height.as_f32() - 32.0).max(320.0)))
            .margin_top(px(((viewport.height.as_f32() - 440.0) / 2.0).max(16.0)))
            .overlay(true)
            .overlay_closable(true)
            .keyboard(true)
            .close_button(false)
            .p_0()
            .rounded_md()
            .bg(theme.surface_high)
            .overflow_hidden()
            .title(Self::qx_rule_editor_title(view.editing, language, theme))
            .child(body)
            .footer(Self::qx_rule_editor_footer(
                input, view, language, theme, cx,
            ))
            .on_close(move |_, _, cx| {
                app.update(cx, ManisApp::close_qx_rule_editor);
            })
    }

    fn qx_rule_target_select(
        &self,
        view: QxRuleEditorView,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut menu = div().p_1();
        for target in self.qx_rule_targets() {
            let selected = target == self.rule_sources.target_policy;
            menu = menu.child(Self::qx_rule_editor_option(
                format!("qx-rule-editor-target-{target}"),
                target.clone(),
                selected,
                theme,
                cx.listener(move |this, _, _, cx| {
                    this.rule_sources.target_policy.clone_from(&target);
                    this.rule_sources.editor_popover = super::QxRuleEditorPopover::None;
                    this.rule_sources.feedback = QxRuleImportFeedback::Idle;
                    cx.notify();
                }),
            ));
        }
        let trigger = Button::new("qx-rule-editor-target")
            .accessibility_label(language.localized(copy::configuration::CHOOSE_TARGET_POLICY))
            .dropdown_caret(true)
            .with_variant(ButtonVariant::Default)
            .with_size(ControlSize::Standard.component_size())
            .h(ControlSize::Standard.height())
            .w_full()
            .child(self.rule_sources.target_policy.clone())
            .disabled(view.busy);
        let app = cx.entity();
        crate::components::anchored_popover(
            "qx-rule-editor-target-popover",
            trigger,
            menu,
            (view.dialog_width - 40.0).max(240.0),
            320.0,
        )
        .open(self.rule_sources.editor_popover == super::QxRuleEditorPopover::Target)
        .on_open_change(move |open, _, cx| {
            app.update(cx, |this, cx| {
                this.rule_sources.editor_popover = if *open {
                    super::QxRuleEditorPopover::Target
                } else {
                    super::QxRuleEditorPopover::None
                };
                cx.notify();
            });
        })
        .into_any_element()
    }

    fn qx_rule_interval_select(
        &self,
        view: QxRuleEditorView,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut menu = div().p_1();
        for interval in [
            RemoteSourceRefreshInterval::Manual,
            RemoteSourceRefreshInterval::Hourly,
            RemoteSourceRefreshInterval::SixHours,
            RemoteSourceRefreshInterval::TwelveHours,
            RemoteSourceRefreshInterval::Daily,
        ] {
            let label = refresh_interval_label(interval, language);
            menu = menu.child(Self::qx_rule_editor_option(
                format!("qx-rule-editor-interval-{interval:?}"),
                label.to_owned(),
                interval == self.rule_sources.editor_refresh_interval,
                theme,
                cx.listener(move |this, _, _, cx| {
                    this.rule_sources.editor_refresh_interval = interval;
                    this.rule_sources.editor_popover = super::QxRuleEditorPopover::None;
                    cx.notify();
                }),
            ));
        }
        let trigger = Button::new("qx-rule-editor-refresh-interval")
            .accessibility_label(
                language.localized(copy::configuration::CHOOSE_RULE_UPDATE_INTERVAL),
            )
            .dropdown_caret(true)
            .with_variant(ButtonVariant::Default)
            .with_size(ControlSize::Standard.component_size())
            .h(ControlSize::Standard.height())
            .w_full()
            .child(refresh_interval_label(
                self.rule_sources.editor_refresh_interval,
                language,
            ))
            .disabled(view.busy);
        let app = cx.entity();
        crate::components::anchored_popover(
            "qx-rule-editor-refresh-popover",
            trigger,
            menu,
            (view.dialog_width - 40.0).max(240.0),
            280.0,
        )
        .open(self.rule_sources.editor_popover == super::QxRuleEditorPopover::Interval)
        .on_open_change(move |open, _, cx| {
            app.update(cx, |this, cx| {
                this.rule_sources.editor_popover = if *open {
                    super::QxRuleEditorPopover::Interval
                } else {
                    super::QxRuleEditorPopover::None
                };
                cx.notify();
            });
        })
        .into_any_element()
    }

    fn qx_rule_editor_option(
        id: impl Into<gpui::ElementId>,
        label: String,
        selected: bool,
        theme: Theme,
        listener: impl Fn(&gpui::ClickEvent, &mut Window, &mut gpui::App) + 'static,
    ) -> Stateful<Div> {
        div()
            .id(id)
            .role(Role::Button)
            .aria_label(label.clone())
            .tab_stop(true)
            .focusable()
            .cursor_pointer()
            .px_3()
            .py_2()
            .rounded(Radius::Control.px())
            .bg(if selected {
                theme.action_soft
            } else {
                theme.surface_high
            })
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
            .child(label)
            .on_click(listener)
    }

    fn qx_rule_editor_footer(
        input: Entity<SubscriptionTextInput>,
        view: QxRuleEditorView,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        div()
            .flex_shrink_0()
            .px_5()
            .py_4()
            .border_t_1()
            .border_color(theme.outline_subtle)
            .flex()
            .items_center()
            .justify_end()
            .gap_2()
            .child(
                style_action_button(
                    Button::new("cancel-qx-rule-source").label(language.message(Message::Cancel)),
                    ActionRole::Secondary,
                    ControlSize::Standard,
                )
                .px(Space::Lg.px())
                .cursor_pointer()
                .on_click(cx.listener(|this, _, window, cx| {
                    this.close_qx_rule_editor(cx);
                    window.close_dialog(cx);
                })),
            )
            .child(
                style_action_button(
                    Button::new("save-qx-rule-source")
                        .label(if view.busy {
                            language.localized(copy::configuration::PROCESSING)
                        } else if view.editing {
                            language.message(Message::SaveChanges)
                        } else {
                            language.localized(copy::configuration::ADD_SOURCE)
                        })
                        .loading(view.busy),
                    ActionRole::Primary,
                    ControlSize::Standard,
                )
                .px(Space::Lg.px())
                .when(!view.busy, gpui::Styled::cursor_pointer)
                .bg(if view.busy {
                    theme.action_soft
                } else {
                    theme.action_primary
                })
                .text_color(if view.busy {
                    theme.action_primary
                } else {
                    theme.action_on_primary
                })
                .on_click(cx.listener(move |this, _, window, cx| {
                    if !view.busy && this.submit_qx_rule_import(&input, cx) {
                        window.close_dialog(cx);
                    }
                })),
            )
    }

    fn qx_rule_editor_title(editing: bool, language: Language, theme: Theme) -> Div {
        div()
            .px_5()
            .py_4()
            .border_b_1()
            .border_color(theme.outline_subtle)
            .child(
                div()
                    .text_size(px(17.0))
                    .font_weight(TextRole::SectionTitle.weight())
                    .child(if editing {
                        language.localized(copy::configuration::EDIT_RULE_SOURCE)
                    } else {
                        language.localized(copy::configuration::ADD_RULE_SOURCE)
                    }),
            )
            .child(
                div()
                    .mt_1()
                    .text_size(TextRole::Metadata.size())
                    .text_color(theme.text_secondary)
                    .child(language.localized(
                        copy::configuration::THE_TARGET_POLICY_IS_USED_BY_EVERY_RULE_IN_THIS,
                    )),
            )
    }

    fn rule_source_card(
        &self,
        index: usize,
        source: &mihomo::StoredQxRuleSource,
        busy: bool,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let language = self.language();
        let presentation = self.rule_source_card_presentation(index, source, busy, language);
        let edit_id = source.id.clone();
        let controls_enabled = presentation.controls_enabled;
        div()
            .id(format!("qx-rule-source-card-{}", source.id))
            .role(Role::Button)
            .aria_label(language.localized(copy::configuration::EDIT_THIS_RULE_SOURCE))
            .tab_stop(controls_enabled)
            .focusable()
            .when(controls_enabled, gpui::Styled::cursor_pointer)
            .mt(Space::Sm.px())
            .p(Space::Md.px())
            .rounded(Radius::Row.px())
            .border_1()
            .border_color(if presentation.duplicate {
                theme.status_warning
            } else {
                theme.outline_subtle
            })
            .bg(theme.surface_low)
            .child(Self::rule_source_card_header(
                source,
                &presentation,
                language,
                theme,
                cx,
            ))
            .child(
                div()
                    .mt_1()
                    .ml_7()
                    .overflow_x_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_size(TextRole::Metadata.size())
                    .text_color(theme.text_tertiary)
                    .child(source.source.expose_to(str::to_owned)),
            )
            .when_some(
                Self::rule_source_refresh_error(&presentation),
                |card, error| card.child(Self::rule_source_error(error, language, theme)),
            )
            .child(Self::rule_source_card_actions(
                index,
                source,
                &presentation,
                language,
                theme,
                cx,
            ))
            .on_click(cx.listener(move |this, _, window, cx| {
                if controls_enabled {
                    this.open_qx_rule_editor(edit_id.clone(), cx);
                    this.open_qx_rule_source_dialog(window, cx);
                }
            }))
    }

    fn rule_source_card_presentation(
        &self,
        index: usize,
        source: &mihomo::StoredQxRuleSource,
        busy: bool,
        language: Language,
    ) -> RuleSourceCardPresentation {
        let refresh = match self.rule_sources.refreshes.get(&source.id) {
            Some(QxRuleSourceRefreshState::Refreshing { .. }) => {
                RuleSourceRefreshPresentation::Refreshing
            }
            Some(QxRuleSourceRefreshState::Failed { message, .. }) => {
                RuleSourceRefreshPresentation::Failed(message.clone())
            }
            None => RuleSourceRefreshPresentation::Idle,
        };
        RuleSourceCardPresentation {
            name: source
                .source
                .subscription_name()
                .unwrap_or_else(|| copy::configuration::numbered_rule_source(language, index + 1)),
            refresh,
            duplicate: matches!(
                &self.rule_sources.feedback,
                QxRuleImportFeedback::AlreadyExists { source_id, .. } if source_id == &source.id
            ),
            controls_enabled: !busy && !self.source_refresh_busy(),
            target_policy: self.effective_rule_target(source.target_policy.as_str(), language),
            last_update: source_update_label(
                source.last_successful_update_unix_secs,
                mihomo::current_unix_secs(),
                language,
            ),
        }
    }

    fn rule_source_card_header(
        source: &mihomo::StoredQxRuleSource,
        presentation: &RuleSourceCardPresentation,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        let toggle_id = source.id.clone();
        let enabled = source.enabled;
        let controls_enabled = presentation.controls_enabled;
        div()
            .flex()
            .items_center()
            .gap_2()
            .child(
                Checkbox::new(format!("qx-rule-enabled-{toggle_id}"))
                    .label("")
                    .checked(enabled)
                    .disabled(!controls_enabled)
                    .tab_stop(controls_enabled)
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        if controls_enabled {
                            this.set_qx_rule_source_enabled(toggle_id.clone(), !enabled, cx);
                        }
                    })),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .overflow_x_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_size(TextRole::Label.size())
                    .font_weight(TextRole::Label.weight())
                    .text_color(if enabled {
                        theme.text_primary
                    } else {
                        theme.text_secondary
                    })
                    .child(presentation.name.clone()),
            )
            .when(presentation.refresh.is_refreshing(), |header| {
                header.child(Self::benchmark_latency_spinner(
                    format!("qx-rule-refresh-{}", source.id),
                    theme,
                ))
            })
            .when(presentation.duplicate, |header| {
                header.child(Self::rule_source_state_label(
                    language.localized(copy::configuration::ALREADY_ADDED),
                    theme.status_warning,
                ))
            })
            .when(!enabled, |header| {
                header.child(Self::rule_source_state_label(
                    language.localized(copy::configuration::DISABLED_2),
                    theme.text_tertiary,
                ))
            })
    }

    fn rule_source_state_label(label: &'static str, color: gpui::Rgba) -> Div {
        div()
            .flex_shrink_0()
            .text_size(TextRole::Metadata.size())
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(color)
            .child(label)
    }

    fn rule_source_refresh_error(presentation: &RuleSourceCardPresentation) -> Option<&str> {
        match &presentation.refresh {
            RuleSourceRefreshPresentation::Failed(message) => Some(message),
            RuleSourceRefreshPresentation::Idle | RuleSourceRefreshPresentation::Refreshing => None,
        }
    }

    fn rule_source_error(error: &str, language: Language, theme: Theme) -> Div {
        div()
            .mt_1()
            .ml_7()
            .text_size(TextRole::Metadata.size())
            .line_height(TextRole::Metadata.line_height())
            .text_color(theme.route_trace)
            .child(format!(
                "{}: {error}",
                language.localized(copy::configuration::LAST_UPDATE_FAILED)
            ))
    }

    fn rule_source_card_actions(
        index: usize,
        source: &mihomo::StoredQxRuleSource,
        presentation: &RuleSourceCardPresentation,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        let refresh_id = source.id.clone();
        let remove_id = source.id.clone();
        let refreshing = presentation.refresh.is_refreshing();
        let refresh_enabled = presentation.controls_enabled && source.enabled;
        let controls_enabled = presentation.controls_enabled;
        div()
            .mt_1()
            .ml_7()
            .flex()
            .items_center()
            .gap_2()
            .text_size(TextRole::Metadata.size())
            .text_color(theme.text_secondary)
            .child(copy::configuration::rule_source_counts(
                language,
                source.rule_count,
                source.diagnostic_count,
            ))
            .child("·")
            .child(format!(
                "{} {}",
                language.localized(copy::configuration::TARGET),
                presentation.target_policy
            ))
            .child("·")
            .child(refresh_interval_label(source.refresh_interval, language))
            .child("·")
            .child(presentation.last_update.clone())
            .child(div().flex_1())
            .child(Self::rule_source_refresh_button(
                refresh_id,
                refreshing,
                refresh_enabled,
                language,
                theme,
                cx,
            ))
            .child(
                action_button(
                    format!("qx-rule-remove-{index}"),
                    language.localized(copy::configuration::REMOVE),
                    ActionRole::Quiet,
                    ControlSize::Compact,
                )
                .disabled(!controls_enabled)
                .when(controls_enabled, gpui::Styled::cursor_pointer)
                .px_3()
                .text_color(theme.status_error)
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    if controls_enabled {
                        this.remove_qx_rule_source(remove_id.clone(), cx);
                    }
                })),
            )
    }

    fn rule_source_refresh_button(
        id: String,
        refreshing: bool,
        enabled: bool,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Button {
        action_button(
            format!("qx-rule-refresh-{id}"),
            if refreshing {
                language.localized(copy::configuration::UPDATING)
            } else {
                language.localized(copy::configuration::UPDATE_NOW)
            },
            ActionRole::Quiet,
            ControlSize::Compact,
        )
        .disabled(!enabled)
        .loading(refreshing)
        .when(enabled, gpui::Styled::cursor_pointer)
        .px_3()
        .border_1()
        .border_color(theme.outline_subtle)
        .bg(theme.surface_high)
        .text_color(theme.action_primary)
        .on_click(cx.listener(move |this, _, _, cx| {
            cx.stop_propagation();
            if enabled {
                this.refresh_qx_rule_source(id.clone(), cx);
            }
        }))
    }

    fn qx_rule_source_target_menu(
        &self,
        source_id: &str,
        selected_target: &str,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        let mut choices = div();
        for target in self.qx_rule_targets() {
            let selected = target == selected_target;
            let target_id = target.clone();
            let source_id = source_id.to_owned();
            choices = choices.child(
                Button::new(format!("qx-rule-source-target-{source_id}-{target_id}"))
                    .accessibility_label(format!("Target {target}"))
                    .selected(selected)
                    .with_variant(ButtonVariant::Text)
                    .with_size(ControlSize::Compact.component_size())
                    .w_full()
                    .min_h(ControlSize::Standard.min_pointer_target())
                    .px(Space::Md.px())
                    .py(Space::Sm.px())
                    .border_b_1()
                    .border_color(theme.outline_subtle)
                    .bg(if selected {
                        theme.action_soft
                    } else {
                        theme.surface_high
                    })
                    .text_color(if selected {
                        theme.action_primary
                    } else {
                        theme.text_primary
                    })
                    .child(
                        div()
                            .w_full()
                            .text_size(TextRole::Label.size())
                            .line_height(TextRole::Label.line_height())
                            .font_weight(TextRole::Label.weight())
                            .child(target),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.update_qx_rule_source_target(source_id.clone(), target_id.clone(), cx);
                    })),
            );
        }
        choices
    }

    fn qx_rule_source_target_select(
        &self,
        source: &crate::mihomo::StoredQxRuleSource,
        enabled: bool,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let language = self.language();
        let source_id = source.id.clone();
        let selected_target = self.effective_rule_target(source.target_policy.as_str(), language);
        let open = self.rule_sources.target_popover.as_deref() == Some(source.id.as_str());
        let updating = self.rule_sources.target_updates.contains_key(&source.id);
        let menu = self.qx_rule_source_target_menu(&source.id, &selected_target, theme, cx);
        let display_value = if updating {
            language.localized(copy::configuration::SAVING).to_owned()
        } else {
            format!(
                "{} · {selected_target}",
                language.message(Message::PolicyGroup)
            )
        };
        let trigger = Button::new(format!("qx-rule-target-select-{}", source.id))
            .accessibility_label(
                language.localized(copy::configuration::CHANGE_TARGET_POLICY_FOR_THIS_RULE_SOURCE),
            )
            .dropdown_caret(true)
            .with_variant(ButtonVariant::Default)
            .with_size(ControlSize::Compact.component_size())
            .h(ControlSize::Compact.height())
            .child(
                div()
                    .min_w(px(0.0))
                    .overflow_x_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_size(TextRole::Label.size())
                    .line_height(TextRole::Label.line_height())
                    .font_weight(TextRole::Label.weight())
                    .child(display_value),
            )
            .disabled(!enabled);
        let app = cx.entity();
        crate::components::anchored_popover(
            format!("qx-rule-target-popover-{}", source.id),
            trigger,
            menu,
            240.0,
            320.0,
        )
        .open(open)
        .on_open_change(move |open, _, cx| {
            app.update(cx, |this, cx| {
                this.rule_sources.target_popover = open.then(|| source_id.clone());
                cx.notify();
            });
        })
        .into_any_element()
    }

    fn open_manual_rule_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.manual_rule_editor_state.is_open() {
            if let Some(condition) = self.manual_rule_conditions.first() {
                condition.input.focus_handle(cx).focus(window, cx);
            }
            return;
        }
        self.manual_rule_editor_state = super::ManualRuleEditorState::Creating;
        self.manual_rule_popover = None;
        self.manual_rule_error = None;
        self.manual_rule_condition_count = 1;
        for condition in &self.manual_rule_conditions {
            condition
                .input
                .update(cx, SubscriptionTextInput::clear_without_event);
        }
        for (index, condition) in self.manual_rule_conditions.iter_mut().enumerate() {
            condition.kind = if index == 1 {
                crate::manual_rule::ManualRuleKind::DstPort
            } else {
                crate::manual_rule::ManualRuleKind::default()
            };
        }
        if let Some(target) = self.manual_rule_targets().first() {
            self.manual_rule_target.clone_from(target);
        }
        self.open_manual_rule_dialog(window, cx);
    }

    fn open_manual_rule_editor_for_edit(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.manual_rule_editor_state.is_open() {
            return;
        }
        let Some(rule) = self.manual_rules.get(index).cloned() else {
            return;
        };
        self.manual_rule_editor_state = super::ManualRuleEditorState::Editing(index);
        self.manual_rule_popover = None;
        self.manual_rule_error = None;
        self.manual_rule_condition_count = if rule.is_final() {
            1
        } else {
            rule.conditions().len()
        };
        for (condition_index, editor) in self.manual_rule_conditions.iter_mut().enumerate() {
            if rule.is_final() && condition_index == 0 {
                editor.kind = crate::manual_rule::ManualRuleKind::Final;
                editor
                    .input
                    .update(cx, SubscriptionTextInput::clear_without_event);
            } else if let Some(condition) = rule.conditions().get(condition_index) {
                editor.kind = condition.kind();
                editor.input.update(cx, |input, cx| {
                    input.set_value_without_event(condition.parameter().to_owned(), cx);
                });
            } else {
                editor
                    .input
                    .update(cx, SubscriptionTextInput::clear_without_event);
            }
        }
        let targets = self.manual_rule_targets();
        self.manual_rule_target = if targets.iter().any(|target| target == rule.target()) {
            rule.target().to_owned()
        } else if rule.target() == "Proxy" {
            self.managed_policies
                .groups
                .first()
                .map_or_else(|| "DIRECT".to_owned(), |group| group.name.clone())
        } else {
            targets
                .first()
                .cloned()
                .unwrap_or_else(|| "DIRECT".to_owned())
        };
        self.open_manual_rule_dialog(window, cx);
    }

    fn open_manual_rule_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let app = cx.entity();
        window.open_dialog(cx, move |dialog, window, cx| {
            app.update(cx, |this, cx| {
                let width = window.viewport_size().width.as_f32();
                let size_class = WindowSizeClass::for_width(width);
                let theme = this.theme();
                this.ensure_manual_rule_input(theme, window, cx);
                this.manual_rule_editor_modal(
                    dialog,
                    theme,
                    this.language(),
                    size_class == WindowSizeClass::Compact,
                    window,
                    cx,
                )
            })
        });
        if let Some(condition) = self.manual_rule_conditions.first() {
            condition.input.focus_handle(cx).focus(window, cx);
        }
        cx.notify();
    }

    fn reset_manual_rule_editor_state(&mut self) {
        self.manual_rule_editor_state = super::ManualRuleEditorState::Closed;
        self.manual_rule_popover = None;
        self.manual_rule_error = None;
    }

    fn close_manual_rule_editor(&mut self, cx: &mut Context<Self>) {
        self.reset_manual_rule_editor_state();
        cx.notify();
    }

    pub(super) fn ensure_manual_rule_input(
        &mut self,
        theme: Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self
            .manual_rule_targets()
            .contains(&self.manual_rule_target)
        {
            self.manual_rule_target = self.manual_rule_targets().remove(0);
        }
        if !self.manual_rule_conditions.is_empty() {
            for condition in &self.manual_rule_conditions {
                let placeholder = manual_rule_placeholder(condition.kind, self.language());
                condition.input.update(cx, |input, cx| {
                    input.set_theme(theme, self.dark, cx);
                    input.set_placeholder(placeholder, cx);
                });
            }
            return;
        }
        self.manual_rule_conditions = (0..crate::manual_rule::MAX_CONDITIONS)
            .map(|index| {
                let kind = if index == 1 {
                    crate::manual_rule::ManualRuleKind::DstPort
                } else {
                    crate::manual_rule::ManualRuleKind::default()
                };
                let placeholder = manual_rule_placeholder(kind, self.language());
                let input = cx.new(|cx| {
                    SubscriptionTextInput::new_field(
                        TextInputSpec::new(
                            format!("manual-rule-parameter-{index}"),
                            placeholder,
                            MAX_MANUAL_RULE_INPUT_BYTES,
                            theme,
                            self.dark,
                        ),
                        window,
                        cx,
                    )
                });
                super::ManualRuleConditionEditor { kind, input }
            })
            .collect();
        let Some(store_dir) = self.subscription_store_dir.as_ref() else {
            return;
        };
        match crate::manual_rule::load_manual_rules_in(store_dir) {
            Ok(rules) => {
                self.manual_rules = rules;
                self.sync_routing_rule_group_order();
            }
            Err(error) => {
                self.status = format!(
                    "{}{error}",
                    self.language()
                        .localized(copy::configuration::COULD_NOT_READ_MANUAL_RULES)
                );
            }
        }
        cx.notify();
    }

    fn manual_rule_targets(&self) -> Vec<String> {
        let mut targets = self
            .managed_policies
            .groups
            .iter()
            .map(|group| group.name.clone())
            .collect::<Vec<_>>();
        targets.extend(["DIRECT".to_owned(), "REJECT".to_owned()]);
        targets
    }

    fn set_manual_rule_kind(
        &mut self,
        condition_index: usize,
        kind: crate::manual_rule::ManualRuleKind,
        cx: &mut Context<Self>,
    ) {
        if !kind.supported_by(self.runtime.kind()) {
            self.manual_rule_error = Some(crate::manual_rule::ManualRuleError::UnsupportedByKernel);
            self.manual_rule_popover = None;
            cx.notify();
            return;
        }
        let Some(condition) = self.manual_rule_conditions.get_mut(condition_index) else {
            return;
        };
        condition.kind = kind;
        if kind == crate::manual_rule::ManualRuleKind::Final {
            self.manual_rule_condition_count = 1;
            for condition in &self.manual_rule_conditions {
                condition
                    .input
                    .update(cx, SubscriptionTextInput::clear_without_event);
            }
        }
        self.manual_rule_error = None;
        self.manual_rule_popover = None;
        let placeholder = manual_rule_placeholder(kind, self.language());
        if let Some(condition) = self.manual_rule_conditions.get(condition_index) {
            condition
                .input
                .update(cx, |input, cx| input.set_placeholder(placeholder, cx));
        }
        cx.notify();
    }

    fn add_manual_rule_condition(&mut self, cx: &mut Context<Self>) {
        if self.manual_rule_condition_count >= crate::manual_rule::MAX_CONDITIONS
            || self
                .manual_rule_conditions
                .first()
                .map(|condition| condition.kind)
                == Some(crate::manual_rule::ManualRuleKind::Final)
        {
            return;
        }
        let index = self.manual_rule_condition_count;
        self.manual_rule_condition_count += 1;
        self.manual_rule_popover = None;
        self.manual_rule_error = None;
        if let Some(condition) = self.manual_rule_conditions.get(index) {
            condition
                .input
                .update(cx, SubscriptionTextInput::clear_without_event);
        }
        cx.notify();
    }

    fn remove_manual_rule_condition(&mut self, index: usize, cx: &mut Context<Self>) {
        if index == 0 || index >= self.manual_rule_condition_count {
            return;
        }
        for current in index..self.manual_rule_condition_count - 1 {
            let next_kind = self.manual_rule_conditions[current + 1].kind;
            let value = self.manual_rule_conditions[current + 1]
                .input
                .read(cx)
                .value()
                .to_owned();
            self.manual_rule_conditions[current].kind = next_kind;
            self.manual_rule_conditions[current]
                .input
                .update(cx, |input, cx| input.set_value_without_event(value, cx));
        }
        self.manual_rule_condition_count -= 1;
        if let Some(condition) = self
            .manual_rule_conditions
            .get(self.manual_rule_condition_count)
        {
            condition
                .input
                .update(cx, SubscriptionTextInput::clear_without_event);
        }
        self.manual_rule_popover = None;
        self.manual_rule_error = None;
        cx.notify();
    }

    fn apply_manual_rule_edit(
        &mut self,
        editing_index: Option<usize>,
        rule: crate::manual_rule::ManualRule,
    ) -> Result<(), crate::manual_rule::ManualRuleEditError> {
        if let Some(index) = editing_index {
            crate::manual_rule::replace_manual_rule(&mut self.manual_rules, index, rule)?;
        } else {
            if rule.is_final()
                && self
                    .manual_rules
                    .iter()
                    .any(crate::manual_rule::ManualRule::is_final)
            {
                return Err(crate::manual_rule::ManualRuleEditError::FinalAlreadyExists);
            }
            if self
                .manual_rules
                .iter()
                .any(|existing| existing.same_definition(&rule))
            {
                return Err(crate::manual_rule::ManualRuleEditError::Duplicate);
            }
            self.manual_rules.push(rule);
        }
        Ok(())
    }

    fn submit_manual_rule(&mut self, cx: &mut Context<Self>) -> bool {
        if self.manual_rule_editor_state == super::ManualRuleEditorState::Closed {
            return false;
        }
        if self.manual_rule_conditions[..self.manual_rule_condition_count]
            .iter()
            .any(|condition| !condition.kind.supported_by(self.runtime.kind()))
        {
            self.manual_rule_error = Some(crate::manual_rule::ManualRuleError::UnsupportedByKernel);
            cx.notify();
            return false;
        }
        let conditions = self.manual_rule_conditions[..self.manual_rule_condition_count]
            .iter()
            .map(|condition| (condition.kind, condition.input.read(cx).value().to_owned()))
            .collect::<Vec<_>>();
        let condition_count = conditions.len();
        let rule = match crate::manual_rule::ManualRule::parse_conditions(
            conditions,
            &self.manual_rule_target,
        ) {
            Ok(rule) => rule,
            Err(error) => {
                self.manual_rule_error = Some(error);
                cx.notify();
                return false;
            }
        };
        let editing_index = self.manual_rule_editor_state.editing_index();
        let previous_rules = self.manual_rules.clone();
        match self.apply_manual_rule_edit(editing_index, rule) {
            Ok(()) => {}
            Err(crate::manual_rule::ManualRuleEditError::Duplicate) => {
                self.manual_rule_error = Some(crate::manual_rule::ManualRuleError::Duplicate);
                cx.notify();
                return false;
            }
            Err(crate::manual_rule::ManualRuleEditError::FinalAlreadyExists) => {
                self.manual_rule_error =
                    Some(crate::manual_rule::ManualRuleError::FinalAlreadyExists);
                cx.notify();
                return false;
            }
            Err(crate::manual_rule::ManualRuleEditError::Missing) => {
                self.reset_manual_rule_editor_state();
                cx.notify();
                return false;
            }
        }
        if let Some(index) = self
            .manual_rules
            .iter()
            .position(crate::manual_rule::ManualRule::is_final)
        {
            let final_rule = self.manual_rules.remove(index);
            self.manual_rules.push(final_rule);
        }
        let completion = self
            .language()
            .localized(copy::configuration::MANUAL_RULES_UPDATED)
            .to_owned();
        if !self.persist_manual_rules(completion, previous_rules.clone(), cx) {
            self.manual_rules = previous_rules;
            return false;
        }
        self.manual_rule_error = None;
        self.reset_manual_rule_editor_state();
        record_event(
            LogLevel::Info,
            if editing_index.is_some() {
                "routing.manual_rule.updated"
            } else {
                "routing.manual_rule.added"
            },
            format!(
                "conditions={} target={} total={}",
                condition_count,
                self.manual_rule_target,
                self.manual_rules.len()
            ),
        );
        true
    }

    fn remove_manual_rule(&mut self, index: usize, cx: &mut Context<Self>) {
        if index >= self.manual_rules.len() {
            return;
        }
        let previous_rules = self.manual_rules.clone();
        let removed = self.manual_rules.remove(index);
        let completion = self
            .language()
            .localized(copy::configuration::MANUAL_RULE_REMOVED)
            .to_owned();
        if !self.persist_manual_rules(completion, previous_rules.clone(), cx) {
            self.manual_rules = previous_rules;
            return;
        }
        self.manual_rule_error = None;
        record_event(
            LogLevel::Info,
            "routing.manual_rule.removed",
            format!(
                "conditions={} total={}",
                removed.conditions().len(),
                self.manual_rules.len()
            ),
        );
    }

    fn set_manual_rule_enabled(&mut self, index: usize, enabled: bool, cx: &mut Context<Self>) {
        let previous_rules = self.manual_rules.clone();
        let Some(rule) = self.manual_rules.get_mut(index) else {
            return;
        };
        if rule.is_enabled() == enabled {
            return;
        }
        rule.set_enabled(enabled);
        let completion = self
            .language()
            .localized(if enabled {
                copy::configuration::MANUAL_RULE_ENABLED
            } else {
                copy::configuration::MANUAL_RULE_DISABLED
            })
            .to_owned();
        if !self.persist_manual_rules(completion, previous_rules.clone(), cx) {
            self.manual_rules = previous_rules;
            return;
        }
        record_event(
            LogLevel::Info,
            if enabled {
                "routing.manual_rule.enabled"
            } else {
                "routing.manual_rule.disabled"
            },
            format!("index={index} total={}", self.manual_rules.len()),
        );
        cx.notify();
    }

    fn persist_manual_rules(
        &mut self,
        completion: String,
        previous_rules: Vec<crate::manual_rule::ManualRule>,
        cx: &mut Context<Self>,
    ) -> bool {
        let language = self.language();
        if self.routing_apply_state.is_busy() {
            language
                .message(Message::RoutingApplyBusy)
                .clone_into(&mut self.status);
            cx.notify();
            return false;
        }
        let Some(store_dir) = self.subscription_store_dir.clone() else {
            language
                .message(Message::ManualRulesLocationUnavailable)
                .clone_into(&mut self.status);
            cx.notify();
            return false;
        };
        let store_snapshot = match mihomo::SubscriptionStoreSnapshot::capture(&store_dir) {
            Ok(snapshot) => snapshot,
            Err(_error) => {
                language
                    .message(Message::StoreTransactionUnavailable)
                    .clone_into(&mut self.status);
                cx.notify();
                return false;
            }
        };
        let previous_order = self.rule_sources.group_order.clone();
        self.sync_routing_rule_group_order();
        if mihomo::save_routing_rule_group_order_in(&store_dir, &self.rule_sources.group_order)
            .is_err()
        {
            self.rule_sources.group_order = previous_order;
            let _ = store_snapshot.restore(&store_dir);
            language
                .message(Message::RuleGroupOrderSaveFailed)
                .clone_into(&mut self.status);
            cx.notify();
            return false;
        }
        if let Err(error) = crate::manual_rule::save_manual_rules_in(&store_dir, &self.manual_rules)
        {
            let _ = store_snapshot.clone().restore(&store_dir);
            self.status = format!(
                "{}{error}",
                language.message(Message::ManualRulesSaveFailed)
            );
            cx.notify();
            return false;
        }
        self.start_routing_runtime_apply(
            store_dir,
            completion,
            super::RoutingApplyRollback {
                manual_rules: previous_rules,
                group_order: previous_order,
                store_snapshot,
            },
            cx,
        );
        true
    }

    fn start_routing_runtime_apply(
        &mut self,
        store_dir: std::path::PathBuf,
        completion: String,
        rollback: super::RoutingApplyRollback,
        cx: &mut Context<Self>,
    ) {
        let started = self.routing_apply_state.begin();
        debug_assert!(started, "routing apply must be idle before spawning");
        if !started {
            return;
        }
        self.status = format!(
            "{} · {}",
            completion,
            self.language().message(Message::ApplyingChanges)
        );
        let runtime = self.runtime.clone();
        let executor = cx.background_executor().clone();
        let disk_rollback = rollback.clone();
        cx.spawn(async move |this, cx| {
            let (apply, rollback_error) = executor
                .spawn(async move {
                    let apply =
                        SourceRuntimeApply::from_result(runtime.apply_saved_sources(&store_dir));
                    let rollback_error = if apply.requires_source_rollback() {
                        disk_rollback
                            .store_snapshot
                            .restore(&store_dir)
                            .map_err(|error| error.to_string())
                            .err()
                    } else {
                        None
                    };
                    (apply, rollback_error)
                })
                .await;
            this.update(cx, |this, cx| {
                this.routing_apply_state.finish();
                if apply.requires_source_rollback() {
                    this.manual_rules = rollback.manual_rules;
                    this.rule_sources.group_order = rollback.group_order;
                }
                apply.reconcile_proxy_mode(&mut this.proxy_mode);
                this.status = if let Some(rollback_error) = rollback_error {
                    format!(
                        "{}{} · {}{rollback_error}",
                        completion,
                        apply.status_suffix(this.language()),
                        this.language().localized(
                            copy::configuration::COULD_NOT_RESTORE_THE_PREVIOUS_SAVED_RULES
                        )
                    )
                } else {
                    format!(
                        "{}{}",
                        completion,
                        if apply.requires_source_rollback() {
                            apply.status_suffix_after_source_rollback(this.language())
                        } else {
                            apply.status_suffix(this.language())
                        }
                    )
                };
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn sync_routing_rule_group_order(&mut self) {
        self.rule_sources.group_order = mihomo::normalized_routing_rule_group_order(
            &self.rule_sources.group_order,
            !self.manual_rules.is_empty(),
            &self.rule_sources.sources,
        );
    }

    fn move_routing_rule_group(&mut self, group_id: &str, direction: i8, cx: &mut Context<Self>) {
        if self.routing_apply_state.is_busy() {
            self.language()
                .message(Message::RoutingApplyBusy)
                .clone_into(&mut self.status);
            cx.notify();
            return;
        }
        self.sync_routing_rule_group_order();
        let previous = self.rule_sources.group_order.clone();
        if !mihomo::move_routing_rule_group(&mut self.rule_sources.group_order, group_id, direction)
        {
            return;
        }
        let Some(store_dir) = self.subscription_store_dir.clone() else {
            self.rule_sources.group_order = previous;
            return;
        };
        let store_snapshot = match mihomo::SubscriptionStoreSnapshot::capture(&store_dir) {
            Ok(snapshot) => snapshot,
            Err(_error) => {
                self.rule_sources.group_order = previous;
                self.language()
                    .message(Message::StoreTransactionUnavailable)
                    .clone_into(&mut self.status);
                cx.notify();
                return;
            }
        };
        if mihomo::save_routing_rule_group_order_in(&store_dir, &self.rule_sources.group_order)
            .is_err()
        {
            self.rule_sources.group_order = previous;
            self.language()
                .message(Message::RuleGroupOrderSaveFailed)
                .clone_into(&mut self.status);
            cx.notify();
            return;
        }
        let language = self.language();
        let completion = if direction < 0 {
            language.localized(copy::configuration::RULE_GROUP_MOVED_UP)
        } else {
            language.localized(copy::configuration::RULE_GROUP_MOVED_DOWN)
        }
        .to_owned();
        self.start_routing_runtime_apply(
            store_dir,
            completion,
            super::RoutingApplyRollback {
                manual_rules: self.manual_rules.clone(),
                group_order: previous,
                store_snapshot,
            },
            cx,
        );
    }

    fn manual_rule_kind_menu(
        &self,
        condition_index: usize,
        selected_kind: crate::manual_rule::ManualRuleKind,
        theme: Theme,
        language: Language,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let kernel = self.runtime.kind();
        let editing_index = self.manual_rule_editor_state.editing_index();
        let final_available = !self
            .manual_rules
            .iter()
            .enumerate()
            .any(|(index, rule)| rule.is_final() && Some(index) != editing_index);
        let mut choices = div().id("manual-rule-kind-choices");
        for kind in crate::manual_rule::ManualRuleKind::ALL {
            if kind == crate::manual_rule::ManualRuleKind::Final && condition_index > 0 {
                continue;
            }
            let supported = kind.supported_by(kernel)
                && (kind != crate::manual_rule::ManualRuleKind::Final || final_available);
            let selected = selected_kind == kind;
            let detail = if supported {
                manual_rule_kind_detail(kind, language)
            } else if kind == crate::manual_rule::ManualRuleKind::Final {
                language.localized(copy::configuration::ALREADY_CONFIGURED)
            } else if kind == crate::manual_rule::ManualRuleKind::UserAgent {
                language.localized(copy::configuration::NO_EXACT_KERNEL_EQUIVALENT)
            } else {
                language.localized(copy::configuration::AVAILABLE_WITH_MIHOMO)
            };
            choices = choices.child(
                div()
                    .id(format!(
                        "manual-rule-kind-{condition_index}-{}",
                        kind.storage_key()
                    ))
                    .role(Role::Button)
                    .aria_label(kind.display_label())
                    .tab_stop(supported)
                    .focusable()
                    .when(supported, gpui::Styled::cursor_pointer)
                    .min_h(px(36.0))
                    .px(Space::Md.px())
                    .py(Space::Xs.px())
                    .border_b_1()
                    .border_color(theme.outline_subtle)
                    .bg(if selected {
                        theme.action_soft
                    } else {
                        theme.surface_high
                    })
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .child(
                        div()
                            .text_size(TextRole::Label.size())
                            .line_height(TextRole::Label.line_height())
                            .font_weight(TextRole::Label.weight())
                            .text_color(if supported {
                                theme.text_primary
                            } else {
                                theme.text_tertiary
                            })
                            .child(kind.display_label()),
                    )
                    .child(
                        div()
                            .text_size(TextRole::Metadata.size())
                            .line_height(TextRole::Metadata.line_height())
                            .text_color(theme.text_tertiary)
                            .child(detail),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if supported {
                            this.set_manual_rule_kind(condition_index, kind, cx);
                        }
                    })),
            );
        }
        choices
    }

    fn manual_rule_target_menu(&self, theme: Theme, cx: &mut Context<Self>) -> Stateful<Div> {
        let mut choices = div().id("manual-rule-target-choices");
        for target in self.manual_rule_targets() {
            let selected = self.manual_rule_target == target;
            let row_target = target.clone();
            choices = choices.child(
                div()
                    .id(format!("manual-rule-target-{target}"))
                    .role(Role::Button)
                    .aria_label(format!("Target {target}"))
                    .tab_stop(true)
                    .focusable()
                    .cursor_pointer()
                    .min_h(ControlSize::Standard.min_pointer_target())
                    .px(Space::Md.px())
                    .py(Space::Sm.px())
                    .border_b_1()
                    .border_color(theme.outline_subtle)
                    .bg(if selected {
                        theme.action_soft
                    } else {
                        theme.surface_high
                    })
                    .text_size(TextRole::Label.size())
                    .line_height(TextRole::Label.line_height())
                    .font_weight(TextRole::Label.weight())
                    .text_color(if selected {
                        theme.action_primary
                    } else {
                        theme.text_primary
                    })
                    .child(target)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.manual_rule_target.clone_from(&row_target);
                        this.manual_rule_error = None;
                        this.manual_rule_popover = None;
                        cx.notify();
                    })),
            );
        }
        choices
    }

    fn manual_rule_select(
        id: &str,
        label: &'static str,
        value: String,
        menu: impl gpui::IntoElement,
        open: bool,
        width: f32,
        on_open_change: impl Fn(&bool, &mut gpui::Window, &mut gpui::App) + 'static,
    ) -> AnyElement {
        let trigger = Button::new(id.to_owned())
            .accessibility_label(label)
            .dropdown_caret(true)
            .with_variant(ButtonVariant::Default)
            .with_size(ControlSize::Standard.component_size())
            .h(ControlSize::Standard.height())
            .w_full()
            .child(
                div()
                    .w_full()
                    .min_w(px(0.0))
                    .overflow_x_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_size(TextRole::Label.size())
                    .line_height(TextRole::Label.line_height())
                    .font_weight(TextRole::Label.weight())
                    .child(value),
            );

        crate::components::anchored_popover(format!("{id}-popover"), trigger, menu, width, 360.0)
            .open(open)
            .on_open_change(on_open_change)
            .into_any_element()
    }

    fn manual_rule_condition_editor(
        &self,
        condition_index: usize,
        kind: crate::manual_rule::ManualRuleKind,
        theme: Theme,
        language: Language,
        compact: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let input = self
            .manual_rule_conditions
            .get(condition_index)
            .expect("manual rule condition input is initialized")
            .input
            .clone();
        let kind_width = if compact { 260.0 } else { 240.0 };
        let kind_popover = ManualRulePopover::Kind(condition_index);
        let kind_open = self.manual_rule_popover == Some(kind_popover);
        let kind_menu = self.manual_rule_kind_menu(condition_index, kind, theme, language, cx);
        let select_id = format!("manual-rule-kind-select-{condition_index}");
        let label = language.localized(copy::configuration::CHOOSE_CONDITION_TYPE);
        let app = cx.entity();
        let kind_select = Self::manual_rule_select(
            &select_id,
            label,
            kind.display_label().to_owned(),
            kind_menu,
            kind_open,
            kind_width,
            move |open, _, cx| {
                app.update(cx, |this, cx| {
                    this.manual_rule_popover = open.then_some(kind_popover);
                    cx.notify();
                });
            },
        );
        let is_final = kind == crate::manual_rule::ManualRuleKind::Final;
        let mut row = div()
            .mt_3()
            .child(div().child(field_label(
                if condition_index == 0 {
                    language.localized(copy::configuration::CONDITION_1).to_owned()
                } else {
                    copy::configuration::condition_title(language, condition_index + 1)
                },
                theme,
            )))
            .child(
                div()
                    .flex()
                    .when(compact, gpui::Styled::flex_col)
                    .items_stretch()
                    .gap_2()
                    .child(
                        div()
                            .when(compact, gpui::Styled::w_full)
                            .when(!compact, |item| item.w(px(220.0)))
                            .flex_shrink_0()
                            .child(kind_select),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .when(!is_final, |field| field.child(input))
                            .when(is_final, |field| {
                                field.child(
                                    div()
                                        .h(ControlSize::Standard.height())
                                        .px(Space::Md.px())
                                        .flex()
                                        .items_center()
                                        .rounded(Radius::Control.px())
                                        .bg(theme.surface_low)
                                        .text_size(TextRole::Body.size())
                                        .line_height(TextRole::Body.line_height())
                                        .text_color(theme.text_secondary)
                                        .child(language.localized(copy::configuration::MATCHES_ONLY_AFTER_EVERY_RULE_ABOVE_MISSES)),
                                )
                            }),
                    ),
            );
        if condition_index > 0 {
            row = row.child(
                Button::new(format!("remove-manual-rule-condition-{condition_index}"))
                    .accessibility_label(
                        language.localized(copy::configuration::REMOVE_THIS_CONDITION),
                    )
                    .label(language.localized(copy::configuration::REMOVE_CONDITION))
                    .text()
                    .with_size(ControlSize::Compact.component_size())
                    .h(ControlSize::Compact.height())
                    .mt(Space::Sm.px())
                    .cursor_pointer()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.status_error)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.remove_manual_rule_condition(condition_index, cx);
                    })),
            );
        }
        row
    }

    pub(super) fn manual_rule_editor_modal(
        &self,
        dialog: Dialog,
        theme: Theme,
        language: Language,
        compact: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Dialog {
        let target_width = if compact { 260.0 } else { 240.0 };
        let target_open = self.manual_rule_popover == Some(ManualRulePopover::Target);
        let target_menu = self.manual_rule_target_menu(theme, cx);
        let app = cx.entity();
        let target = Self::manual_rule_select(
            "manual-rule-target-select",
            language.localized(copy::configuration::CHOOSE_TARGET_POLICY),
            self.manual_rule_target.clone(),
            target_menu,
            target_open,
            target_width,
            move |open, _, cx| {
                app.update(cx, |this, cx| {
                    this.manual_rule_popover = open.then_some(ManualRulePopover::Target);
                    cx.notify();
                });
            },
        );

        let editing = self.manual_rule_editor_state.editing_index().is_some();
        let final_selected = self
            .manual_rule_conditions
            .first()
            .map(|condition| condition.kind)
            == Some(crate::manual_rule::ManualRuleKind::Final);
        let conditions =
            self.manual_rule_editor_conditions(final_selected, compact, theme, language, cx);
        let body = self.manual_rule_editor_body(conditions, target, theme, language);
        let footer = self.manual_rule_editor_footer(editing, theme, language, cx);

        let viewport = window.viewport_size();
        let dialog_width = (viewport.width.as_f32() - 32.0).clamp(280.0, 720.0);
        let estimated_height = if compact {
            520.0
        } else {
            match self.manual_rule_condition_count {
                0 | 1 => 368.0,
                2 => 458.0,
                3 => 548.0,
                _ => 638.0,
            }
        };
        let margin_top = ((viewport.height.as_f32() - estimated_height) / 2.0).max(16.0);
        let app = cx.entity();

        dialog
            .width(px(dialog_width))
            .max_h(px((viewport.height.as_f32() - 32.0).max(320.0)))
            .margin_top(px(margin_top))
            .overlay(true)
            .overlay_closable(true)
            .keyboard(true)
            .close_button(false)
            .p_0()
            .rounded_md()
            .bg(theme.surface_high)
            .overflow_hidden()
            .title(Self::manual_rule_editor_title(
                editing,
                final_selected,
                theme,
                language,
            ))
            .child(body)
            .footer(footer)
            .on_close(move |_, _, cx| {
                app.update(cx, ManisApp::close_manual_rule_editor);
            })
    }

    fn manual_rule_editor_conditions(
        &self,
        final_selected: bool,
        compact: bool,
        theme: Theme,
        language: Language,
        cx: &mut Context<Self>,
    ) -> Div {
        let mut conditions = div();
        for index in 0..self.manual_rule_condition_count {
            conditions = conditions.child(self.manual_rule_condition_editor(
                index,
                self.manual_rule_conditions[index].kind,
                theme,
                language,
                compact,
                cx,
            ));
        }
        if final_selected || self.manual_rule_condition_count >= crate::manual_rule::MAX_CONDITIONS
        {
            return conditions;
        }
        conditions.child(
            Button::new("add-manual-rule-condition")
                .accessibility_label(language.localized(copy::configuration::ADD_AN_AND_CONDITION))
                .label(language.localized(copy::configuration::ADD_AND_CONDITION))
                .with_variant(ButtonVariant::Default)
                .with_size(ControlSize::Standard.component_size())
                .h(ControlSize::Standard.height())
                .mt(Space::Md.px())
                .px(Space::Md.px())
                .py(Space::Sm.px())
                .cursor_pointer()
                .border_color(theme.outline_strong)
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme.action_primary)
                .on_click(cx.listener(|this, _, _, cx| this.add_manual_rule_condition(cx))),
        )
    }

    fn manual_rule_editor_body(
        &self,
        conditions: Div,
        target: AnyElement,
        theme: Theme,
        language: Language,
    ) -> Stateful<Div> {
        div()
            .id("manual-rule-modal-body")
            .flex_1()
            .min_h(px(0.0))
            .overflow_y_scroll()
            .px_5()
            .py_4()
            .child(conditions)
            .child(
                div()
                    .mt_4()
                    .child(field_label(
                        language.localized(copy::configuration::POLICY_GROUP_AFTER_MATCH),
                        theme,
                    ))
                    .child(target),
            )
            .when_some(self.manual_rule_error, |body, error| {
                body.child(
                    div()
                        .mt_3()
                        .p_3()
                        .rounded_md()
                        .bg(theme.surface_low)
                        .text_size(TextRole::Body.size())
                        .line_height(TextRole::Body.line_height())
                        .text_color(theme.status_error)
                        .child(manual_rule_error_label(error, language)),
                )
            })
    }

    fn manual_rule_editor_footer(
        &self,
        editing: bool,
        theme: Theme,
        language: Language,
        cx: &mut Context<Self>,
    ) -> Div {
        div()
            .flex_shrink_0()
            .px_5()
            .py_3()
            .border_t_1()
            .border_color(theme.outline_subtle)
            .flex()
            .items_center()
            .justify_end()
            .gap_2()
            .child(
                style_action_button(
                    Button::new("cancel-manual-rule").label(language.message(Message::Cancel)),
                    ActionRole::Secondary,
                    ControlSize::Standard,
                )
                .px(Space::Lg.px())
                .cursor_pointer()
                .on_click(cx.listener(|this, _, window, cx| {
                    this.close_manual_rule_editor(cx);
                    window.close_dialog(cx);
                })),
            )
            .child(
                style_action_button(
                    Button::new("save-manual-rule")
                        .label(if editing {
                            language.message(Message::SaveChanges)
                        } else {
                            language.message(Message::AddRule)
                        })
                        .disabled(self.routing_apply_state.is_busy()),
                    ActionRole::Primary,
                    ControlSize::Standard,
                )
                .px(Space::Lg.px())
                .cursor_pointer()
                .bg(theme.action_primary)
                .text_color(theme.action_on_primary)
                .on_click(cx.listener(|this, _, window, cx| {
                    if this.submit_manual_rule(cx) {
                        window.close_dialog(cx);
                    }
                })),
            )
    }

    fn manual_rule_editor_title(
        editing: bool,
        final_selected: bool,
        theme: Theme,
        language: Language,
    ) -> Stateful<Div> {
        div()
            .id("manual-rule-modal-header")
            .flex_shrink_0()
            .px_5()
            .py_4()
            .border_b_1()
            .border_color(theme.outline_subtle)
            .child(
                div()
                    .text_size(px(17.0))
                    .font_weight(TextRole::SectionTitle.weight())
                    .child(if editing {
                        language.localized(copy::configuration::EDIT_ROUTING_RULE)
                    } else {
                        language.localized(copy::configuration::ADD_ROUTING_RULE)
                    }),
            )
            .child(
                div()
                    .mt_1()
                    .text_size(TextRole::Metadata.size())
                    .text_color(theme.text_secondary)
                    .child(if final_selected {
                        language.localized(copy::configuration::FINAL_IS_ALWAYS_EVALUATED_LAST_AND_HANDLES_UNMATCHED_TRAFFIC)
                    } else {
                        language.localized(copy::configuration::ALL_CONDITIONS_MUST_MATCH_GROUP_ORDER_DETERMINES_RULE_PRIORITY)
                    }),
            )
    }

    fn manual_routing_rule_row(
        &self,
        order: usize,
        index: usize,
        rule: &crate::manual_rule::ManualRule,
        theme: Theme,
        language: Language,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let enabled = rule.is_enabled();
        let target = self.effective_rule_target(rule.target(), language);
        let matchers = Self::manual_rule_matchers(rule, enabled, theme, language);
        let edit_label = copy::configuration::manual_rule_accessibility(language, order);
        let row = div()
            .id(format!("manual-routing-rule-{index}"))
            .mt_1()
            .min_h(px(44.0))
            .px(Space::Md.px())
            .py(Space::Sm.px())
            .rounded(Radius::Row.px())
            .bg(if enabled {
                theme.surface_low
            } else {
                theme.surface_base
            })
            .flex()
            .items_center()
            .gap(Space::Md.px())
            .cursor_pointer()
            .role(Role::Button)
            .tab_stop(true)
            .focusable()
            .aria_label(edit_label)
            .on_click(cx.listener(move |this, _, window, cx| {
                this.open_manual_rule_editor_for_edit(index, window, cx);
            }))
            .on_key_down(cx.listener(move |this, event, window, cx| {
                let Some(action) = manual_rule_keyboard_action(event) else {
                    return;
                };
                cx.stop_propagation();
                match action {
                    ManualRuleKeyboardAction::Edit => {
                        this.open_manual_rule_editor_for_edit(index, window, cx);
                    }
                    ManualRuleKeyboardAction::Toggle => {
                        this.set_manual_rule_enabled(index, !enabled, cx);
                    }
                    ManualRuleKeyboardAction::Delete => {
                        this.remove_manual_rule(index, cx);
                    }
                }
            }))
            .child(
                div()
                    .w(px(42.0))
                    .flex_shrink_0()
                    .text_size(TextRole::Metadata.size())
                    .line_height(TextRole::Metadata.line_height())
                    .text_color(theme.text_tertiary)
                    .child(format!("#{order:03}")),
            )
            .child(matchers)
            .child(
                div()
                    .flex_shrink_0()
                    .text_size(TextRole::Data.size())
                    .line_height(TextRole::Data.line_height())
                    .font_weight(TextRole::Data.weight())
                    .text_color(if enabled {
                        theme.action_primary
                    } else {
                        theme.text_tertiary
                    })
                    .child(target),
            )
            .when(!enabled, |row| {
                row.child(
                    div()
                        .flex_shrink_0()
                        .px_2()
                        .py_1()
                        .rounded(Radius::Control.px())
                        .bg(theme.surface_chrome)
                        .text_size(TextRole::Metadata.size())
                        .line_height(TextRole::Metadata.line_height())
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme.text_tertiary)
                        .child(language.localized(copy::configuration::DISABLED)),
                )
            });
        Self::manual_rule_context_menu(row, index, enabled, language, cx.entity())
    }

    fn manual_rule_context_menu(
        row: Stateful<Div>,
        index: usize,
        enabled: bool,
        language: Language,
        app: Entity<Self>,
    ) -> AnyElement {
        let toggle_label = if enabled {
            language.message(Message::Disable)
        } else {
            language.message(Message::Enable)
        };
        row.context_menu(move |menu, _, _| {
            let toggle_app = app.clone();
            let delete_app = app.clone();
            menu.item(PopupMenuItem::new(toggle_label).on_click(move |_, _, cx| {
                toggle_app.update(cx, |this, cx| {
                    this.set_manual_rule_enabled(index, !enabled, cx);
                });
            }))
            .separator()
            .item(
                PopupMenuItem::new(language.message(Message::Delete)).on_click(move |_, _, cx| {
                    delete_app.update(cx, |this, cx| {
                        this.remove_manual_rule(index, cx);
                    });
                }),
            )
        })
        .into_any_element()
    }

    fn manual_rule_matchers(
        rule: &crate::manual_rule::ManualRule,
        enabled: bool,
        theme: Theme,
        language: Language,
    ) -> Div {
        let primary_text = if enabled {
            theme.text_primary
        } else {
            theme.text_tertiary
        };
        let secondary_text = if enabled {
            theme.text_secondary
        } else {
            theme.text_tertiary
        };
        if rule.is_final() {
            return div()
                .flex_1()
                .min_w(px(0.0))
                .flex()
                .items_center()
                .gap(Space::Sm.px())
                .child(
                    div()
                        .px(Space::Sm.px())
                        .py(Space::Xs.px())
                        .rounded(Radius::Control.px())
                        .bg(theme.surface_high)
                        .text_size(TextRole::Label.size())
                        .line_height(TextRole::Label.line_height())
                        .font_weight(TextRole::Label.weight())
                        .text_color(primary_text)
                        .child("FINAL"),
                )
                .child(
                    div()
                        .text_size(TextRole::Metadata.size())
                        .line_height(TextRole::Metadata.line_height())
                        .text_color(secondary_text)
                        .child(language.localized(copy::configuration::FALLBACK_ALWAYS_LAST)),
                );
        }
        let mut matchers = div()
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .flex_wrap()
            .items_center()
            .gap_1();
        for (condition_index, condition) in rule.conditions().iter().enumerate() {
            if condition_index > 0 {
                matchers = matchers.child(
                    div()
                        .mx_1()
                        .text_size(TextRole::Metadata.size())
                        .line_height(TextRole::Metadata.line_height())
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme.text_tertiary)
                        .child(language.localized(copy::configuration::AND)),
                );
            }
            matchers = matchers.child(
                div()
                    .flex()
                    .items_center()
                    .gap(Space::Sm.px())
                    .px(Space::Sm.px())
                    .py(Space::Xs.px())
                    .rounded(Radius::Control.px())
                    .bg(theme.surface_high)
                    .child(
                        div()
                            .text_size(TextRole::Metadata.size())
                            .line_height(TextRole::Metadata.line_height())
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(secondary_text)
                            .child(condition.kind().display_label()),
                    )
                    .child(
                        div()
                            .text_size(TextRole::Data.size())
                            .line_height(TextRole::Data.line_height())
                            .text_color(primary_text)
                            .child(condition.parameter().to_owned()),
                    ),
            );
        }
        matchers
    }

    fn rule_group_order_controls(
        &self,
        group_id: &str,
        group_name: &str,
        position: (usize, usize),
        theme: Theme,
        language: Language,
        cx: &mut Context<Self>,
    ) -> Div {
        let (position, group_count) = position;
        let up_id = group_id.to_owned();
        let down_id = group_id.to_owned();
        div()
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap(Space::Xs.px())
            .child(
                Button::new(format!("move-rule-group-up-{group_id}"))
                    .accessibility_label(copy::configuration::move_rule_group(
                        language, group_name, true,
                    ))
                    .icon(IconName::ArrowUp)
                    .text()
                    .with_size(ControlSize::Icon.component_size())
                    .text_color(theme.text_secondary)
                    .disabled(position == 0 || self.routing_apply_state.is_busy())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.move_routing_rule_group(&up_id, -1, cx);
                    })),
            )
            .child(
                Button::new(format!("move-rule-group-down-{group_id}"))
                    .accessibility_label(copy::configuration::move_rule_group(
                        language, group_name, false,
                    ))
                    .icon(IconName::ArrowDown)
                    .text()
                    .with_size(ControlSize::Icon.component_size())
                    .text_color(theme.text_secondary)
                    .disabled(position + 1 >= group_count || self.routing_apply_state.is_busy())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.move_routing_rule_group(&down_id, 1, cx);
                    })),
            )
    }

    fn active_rules_panel(
        &self,
        theme: Theme,
        language: Language,
        compact: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let remote_count = self
            .rule_sources
            .sources
            .iter()
            .filter(|source| source.enabled)
            .map(|source| source.rule_count)
            .sum::<usize>();
        let disabled_remote_count = self
            .rule_sources
            .sources
            .iter()
            .filter(|source| !source.enabled)
            .map(|source| source.rule_count)
            .sum::<usize>();
        let enabled_manual_count = self
            .manual_rules
            .iter()
            .filter(|rule| rule.is_enabled())
            .count();
        let disabled_manual_count = self.manual_rules.len() - enabled_manual_count;
        let active_count = enabled_manual_count + remote_count;
        let disabled_count = disabled_manual_count + disabled_remote_count;
        let group_order = mihomo::normalized_routing_rule_group_order(
            &self.rule_sources.group_order,
            !self.manual_rules.is_empty(),
            &self.rule_sources.sources,
        );
        let mut list = Self::active_rules_panel_shell(
            active_count,
            disabled_count,
            compact,
            language,
            theme,
            cx,
        );
        let mut rule_order = 1;
        for (position, group_id) in group_order.iter().enumerate() {
            if group_id == mihomo::MANUAL_ROUTING_RULE_GROUP_ID {
                list = list.child(self.manual_rule_group(
                    disabled_manual_count,
                    &mut rule_order,
                    RuleGroupRenderContext {
                        position,
                        group_count: group_order.len(),
                        compact,
                        language,
                        theme,
                    },
                    cx,
                ));
            } else if let Some((source_index, source)) = self
                .rule_sources
                .sources
                .iter()
                .enumerate()
                .find(|(_, source)| source.id == *group_id)
            {
                list = list.child(self.remote_rule_group(
                    source_index,
                    source,
                    &mut rule_order,
                    RuleGroupRenderContext {
                        position,
                        group_count: group_order.len(),
                        compact,
                        language,
                        theme,
                    },
                    cx,
                ));
            }
        }
        if group_order.is_empty() {
            list = list.child(
                empty_state(
                    language.localized(copy::configuration::NO_ROUTING_RULES_YET),
                    language.localized(copy::configuration::ADD_RULES_TO_SEND_MATCHING_CONNECTIONS_THROUGH_A_POLICY_GROUP),
                    None,
                    theme,
                )
                .mt(Space::Lg.px()),
            );
        }
        list
    }

    fn active_rules_panel_shell(
        active_count: usize,
        disabled_count: usize,
        compact: bool,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let summary =
            copy::configuration::active_rule_summary(language, active_count, disabled_count);
        div()
            .id("active-routing-rules")
            .w_full()
            .flex_1()
            .min_w(px(0.0))
            .p(if compact {
                Space::Md.px()
            } else {
                Space::Lg.px()
            })
            .rounded(Radius::Pane.px())
            .border_1()
            .border_color(theme.outline_subtle)
            .bg(theme.surface_high)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(Space::Md.px())
                    .child(section_heading(
                        language.localized(copy::configuration::ACTIVE_RULES),
                        language.localized(
                            copy::configuration::GROUPS_MATCH_FROM_TOP_TO_BOTTOM_USE_THE_ARROWS_TO,
                        ),
                        None,
                        theme,
                    ))
                    .child(
                        div()
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .gap(Space::Sm.px())
                            .child(status_badge(summary, StatusTone::Route, theme))
                            .child(
                                action_button(
                                    "open-manual-rule-editor",
                                    language.message(Message::AddRule),
                                    ActionRole::Primary,
                                    ControlSize::Compact,
                                )
                                .cursor_pointer()
                                .bg(theme.action_primary)
                                .text_color(theme.action_on_primary)
                                .on_click(cx.listener(
                                    |this, _, window, cx| {
                                        this.open_manual_rule_editor(window, cx);
                                    },
                                )),
                            ),
                    ),
            )
    }

    fn manual_rule_group(
        &self,
        disabled_count: usize,
        rule_order: &mut usize,
        view: RuleGroupRenderContext,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let RuleGroupRenderContext {
            compact,
            language,
            theme,
            ..
        } = view;
        let group_name = language.localized(copy::common::MANUAL_RULES);
        let detail = copy::configuration::manual_group_detail(
            language,
            self.manual_rules.len(),
            disabled_count,
        );
        let title = self.rule_group_title(
            mihomo::MANUAL_ROUTING_RULE_GROUP_ID,
            group_name,
            detail,
            None,
            view,
            cx,
        );
        let mut rules = Self::rule_group_rows(compact, theme);
        for (index, rule) in self.manual_rules.iter().enumerate() {
            rules = rules.child(self.manual_routing_rule_row(
                *rule_order,
                index,
                rule,
                theme,
                language,
                cx,
            ));
            *rule_order += 1;
        }
        let open = rule_group_is_open(&self.node_workspace, MANUAL_RULES_EXPANSION_KEY);
        Accordion::new("routing-manual-rules")
            .bordered(false)
            .with_size(Size::Large)
            .mt(Space::Lg.px())
            .rounded(Radius::Pane.px())
            .overflow_hidden()
            .border_1()
            .border_color(theme.outline_subtle)
            .bg(theme.surface_low)
            .item(|item| {
                item.open(open)
                    .title_style(accordion_title_style(compact))
                    .content_style(accordion_content_style())
                    .title(title)
                    .child(rules)
            })
            .on_toggle_click(cx.listener(|this, open_indices: &[usize], _, cx| {
                Self::sync_rule_group_open(
                    this,
                    MANUAL_RULES_EXPANSION_KEY,
                    open_indices.contains(&0),
                    cx,
                );
            }))
            .into_any_element()
    }

    fn remote_rule_group(
        &self,
        source_index: usize,
        source: &mihomo::StoredQxRuleSource,
        rule_order: &mut usize,
        view: RuleGroupRenderContext,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let RuleGroupRenderContext {
            compact,
            language,
            theme,
            ..
        } = view;
        let parsed = QxRuleList::parse(&source.content);
        let target = self.effective_rule_target(source.target_policy.as_str(), language);
        let name = source.source.subscription_name().unwrap_or_else(|| {
            copy::configuration::numbered_rule_source(language, source_index + 1)
        });
        let detail = Self::remote_rule_group_detail(source, parsed.rules.len(), &target, language);
        let target_select = self.qx_rule_source_target_select(
            source,
            source.enabled && !self.source_refresh_busy(),
            theme,
            cx,
        );
        let title = self.rule_group_title(&source.id, &name, detail, Some(target_select), view, cx);
        let mut rules = Self::rule_group_rows(compact, theme);
        for rule in parsed.rules {
            rules = rules.child(Self::routing_rule_row(
                *rule_order,
                Self::qx_rule_kind_label(rule.kind),
                &rule.value,
                &target,
                theme,
            ));
            *rule_order += 1;
        }
        let expansion_key = rule_source_expansion_key(&source.id);
        let toggle_key = expansion_key.clone();
        let open = rule_group_is_open(&self.node_workspace, &expansion_key);
        Accordion::new(format!("routing-rule-source-{}", source.id))
            .bordered(false)
            .with_size(Size::Large)
            .mt(Space::Lg.px())
            .rounded(Radius::Pane.px())
            .overflow_hidden()
            .border_1()
            .border_color(theme.outline_subtle)
            .bg(theme.surface_low)
            .item(|item| {
                item.open(open)
                    .title_style(accordion_title_style(compact))
                    .content_style(accordion_content_style())
                    .title(title)
                    .child(rules)
            })
            .on_toggle_click(cx.listener(move |this, open_indices: &[usize], _, cx| {
                Self::sync_rule_group_open(this, &toggle_key, open_indices.contains(&0), cx);
            }))
            .into_any_element()
    }

    fn rule_group_title(
        &self,
        group_id: &str,
        name: &str,
        detail: String,
        middle: Option<AnyElement>,
        view: RuleGroupRenderContext,
        cx: &mut Context<Self>,
    ) -> Div {
        let RuleGroupRenderContext {
            position,
            group_count,
            language,
            theme,
            ..
        } = view;
        div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .gap(Space::Sm.px())
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .child(
                        div()
                            .text_size(TextRole::Label.size())
                            .font_weight(TextRole::Label.weight())
                            .child(name.to_owned()),
                    )
                    .child(
                        div()
                            .mt(Space::Xs.px())
                            .overflow_x_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(TextRole::Metadata.size())
                            .text_color(theme.text_tertiary)
                            .child(detail),
                    ),
            )
            .when_some(middle, ParentElement::child)
            .child(self.rule_group_order_controls(
                group_id,
                name,
                (position, group_count),
                theme,
                language,
                cx,
            ))
    }

    fn remote_rule_group_detail(
        source: &mihomo::StoredQxRuleSource,
        rule_count: usize,
        target: &str,
        language: Language,
    ) -> String {
        let update = source_update_label(
            source.last_successful_update_unix_secs,
            mihomo::current_unix_secs(),
            language,
        );
        copy::configuration::remote_group_detail(
            language,
            rule_count,
            source.enabled,
            target,
            &update,
        )
    }

    fn rule_group_rows(compact: bool, theme: Theme) -> Div {
        div()
            .px(if compact {
                Space::Sm.px()
            } else {
                Space::Md.px()
            })
            .pb(Space::Md.px())
            .border_t_1()
            .border_color(theme.outline_subtle)
            .bg(theme.surface_high)
    }

    fn sync_rule_group_open(this: &mut Self, key: &str, open: bool, cx: &mut Context<Self>) {
        let should_collapse = !open;
        if this.node_workspace.is_group_collapsed(key) != should_collapse {
            this.node_workspace.toggle_group(key);
            this.persist_node_workspace();
            cx.notify();
        }
    }

    fn qx_rule_kind_label(kind: QxRuleKind) -> &'static str {
        match kind {
            QxRuleKind::Domain => "DOMAIN",
            QxRuleKind::DomainKeyword => "DOMAIN-KEYWORD",
            QxRuleKind::DomainSuffix => "DOMAIN-SUFFIX",
        }
    }

    fn routing_rule_row(
        order: usize,
        kind: &'static str,
        value: &str,
        target: &str,
        theme: Theme,
    ) -> Div {
        div()
            .mt_1()
            .min_h(px(44.0))
            .px(Space::Md.px())
            .py(Space::Sm.px())
            .rounded(Radius::Row.px())
            .bg(theme.surface_low)
            .flex()
            .items_center()
            .gap(Space::Md.px())
            .child(
                div()
                    .w(px(42.0))
                    .flex_shrink_0()
                    .text_size(TextRole::Metadata.size())
                    .line_height(TextRole::Metadata.line_height())
                    .text_color(theme.text_tertiary)
                    .child(format!("#{order:03}")),
            )
            .child(
                div()
                    .w(px(124.0))
                    .flex_shrink_0()
                    .text_size(TextRole::Metadata.size())
                    .line_height(TextRole::Metadata.line_height())
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.text_secondary)
                    .child(kind),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .overflow_x_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_size(TextRole::Data.size())
                    .line_height(TextRole::Data.line_height())
                    .child(value.to_owned()),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .text_size(TextRole::Data.size())
                    .line_height(TextRole::Data.line_height())
                    .font_weight(TextRole::Data.weight())
                    .text_color(theme.action_primary)
                    .child(target.to_owned()),
            )
    }

    fn qx_rule_import_feedback(&self, theme: Theme, language: Language) -> Div {
        let (message, color) = match &self.rule_sources.feedback {
            QxRuleImportFeedback::Idle => (
                language
                    .localized(copy::configuration::HTTPS_ONLY_UP_TO_1_MIB_INVALID_LINES_ARE_COUNTED)
                    .to_owned(),
                theme.text_secondary,
            ),
            QxRuleImportFeedback::Importing => (
                language
                    .localized(copy::configuration::SECURELY_DOWNLOADING_PARSING_AND_WRITING_LOCALLY)
                    .to_owned(),
                theme.action_primary,
            ),
            QxRuleImportFeedback::Imported {
                rule_count,
                diagnostic_count,
            } => (
                copy::configuration::imported_rules(
                    language,
                    *rule_count,
                    *diagnostic_count,
                ),
                theme.status_success,
            ),
            QxRuleImportFeedback::AlreadyExists {
                rule_count,
                target_policy,
                ..
            } => (
                copy::configuration::duplicate_rule_source(
                    language,
                    *rule_count,
                    target_policy,
                ),
                theme.status_warning,
            ),
            QxRuleImportFeedback::InvalidDocument => (
                language
                    .localized(copy::configuration::FILE_DOWNLOADED_BUT_NO_RECOGNIZABLE_QX_DOMAIN_RULES_WERE_FOUND)
                    .to_owned(),
                theme.status_error,
            ),
            QxRuleImportFeedback::DownloadFailed(error) => (error.to_string(), theme.status_error),
            QxRuleImportFeedback::StoreFailed(error) => (error.to_string(), theme.status_error),
        };
        div()
            .mt_2()
            .text_size(TextRole::Body.size())
            .line_height(TextRole::Body.line_height())
            .text_color(color)
            .child(message)
    }

    fn qx_rule_targets(&self) -> Vec<String> {
        let mut targets = self
            .managed_policies
            .groups
            .iter()
            .map(|group| group.name.clone())
            .collect::<Vec<_>>();
        targets.push("DIRECT".to_owned());
        targets
    }

    fn effective_rule_target(&self, target: &str, language: Language) -> String {
        if target != "Proxy"
            || self
                .managed_policies
                .groups
                .iter()
                .any(|group| group.name == target)
        {
            return target.to_owned();
        }
        self.managed_policies.groups.first().map_or_else(
            || {
                language
                    .localized(copy::configuration::GLOBAL_EXIT)
                    .to_owned()
            },
            |group| group.name.clone(),
        )
    }

    fn submit_qx_rule_import(
        &mut self,
        input: &Entity<SubscriptionTextInput>,
        cx: &mut Context<Self>,
    ) -> bool {
        let url = input.read(cx).value().trim().to_owned();
        let target = self.rule_sources.target_policy.clone();
        let editing_id = self.rule_sources.editor_source_id.clone();
        let refresh_interval = self.rule_sources.editor_refresh_interval;
        let operation_id = begin_operation(
            "configuration.rule_source.save.requested",
            format!(
                "editing={} target={target} known_sources={}",
                editing_id.is_some(),
                self.rule_sources.sources.len()
            ),
        );
        let Ok(parsed_source) = SecretUrl::parse_https(&url) else {
            self.rule_sources.feedback =
                QxRuleImportFeedback::StoreFailed(SubscriptionStoreError::InvalidSource);
            self.language()
                .localized(copy::configuration::ENTER_A_VALID_HTTPS_RULE_URL)
                .clone_into(&mut self.status);
            cx.notify();
            return false;
        };
        let Some(store_dir) = self.subscription_store_dir.clone() else {
            self.rule_sources.feedback =
                QxRuleImportFeedback::StoreFailed(SubscriptionStoreError::DataDirectoryUnavailable);
            self.language()
                .localized(copy::configuration::COULD_NOT_DETERMINE_WHERE_TO_SAVE_RULES)
                .clone_into(&mut self.status);
            record_operation(
                operation_id,
                LogLevel::Error,
                "configuration.rule_source.add.failed",
                "phase=store reason=data_directory_unavailable",
            );
            cx.notify();
            return false;
        };
        if self.reject_duplicate_qx_rule_source(
            &parsed_source,
            editing_id.as_deref(),
            operation_id,
            cx,
        ) {
            return false;
        }
        self.rule_sources.import_generation = self.rule_sources.import_generation.wrapping_add(1);
        let generation = self.rule_sources.import_generation;
        self.rule_sources.feedback = QxRuleImportFeedback::Importing;
        self.language()
            .localized(copy::configuration::DOWNLOADING_AND_PARSING_QX_RULES)
            .clone_into(&mut self.status);
        input.update(cx, |input, cx| input.set_enabled(false, cx));
        let runtime = self.runtime.clone();
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn(async move {
                    save_qx_rule_source(
                        &runtime,
                        &store_dir,
                        QxRuleSaveRequest {
                            url,
                            target,
                            editing_id,
                            refresh_interval,
                        },
                    )
                })
                .await;
            this.update(cx, |this, cx| {
                this.finish_qx_rule_import(generation, operation_id, result, cx);
            })
            .ok();
        })
        .detach();
        cx.notify();
        true
    }

    fn reject_duplicate_qx_rule_source(
        &mut self,
        parsed_source: &SecretUrl,
        editing_id: Option<&str>,
        operation_id: u64,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some((source_id, rule_count, stored_target)) = self
            .rule_sources
            .sources
            .iter()
            .find(|source| {
                source.source == *parsed_source && editing_id != Some(source.id.as_str())
            })
            .map(|source| {
                (
                    source.id.clone(),
                    source.rule_count,
                    source.target_policy.clone(),
                )
            })
        else {
            return false;
        };
        let target_policy = self.effective_rule_target(stored_target.as_str(), self.language());
        self.rule_sources.feedback = QxRuleImportFeedback::AlreadyExists {
            source_id: source_id.clone(),
            rule_count,
            target_policy: target_policy.clone(),
        };
        self.language()
            .localized(copy::configuration::RULE_SOURCE_ALREADY_EXISTS_NO_DUPLICATE_WAS_ADDED)
            .clone_into(&mut self.status);
        record_operation(
            operation_id,
            LogLevel::Warn,
            "configuration.rule_source.add.duplicate",
            format!("existing_id={source_id} rules={rule_count} target={target_policy}"),
        );
        cx.notify();
        true
    }

    fn finish_qx_rule_import(
        &mut self,
        generation: u64,
        operation_id: u64,
        result: super::QxRuleImportResult,
        cx: &mut Context<Self>,
    ) {
        if self.rule_sources.import_generation != generation {
            return;
        }
        if let Some(input) = self.inputs.qx_rule.as_ref() {
            input.update(cx, |input, cx| input.set_enabled(true, cx));
        }
        match result {
            Ok(ImportQxRuleSuccess::Imported { stored, apply }) => {
                self.finish_imported_qx_rule(operation_id, stored, &apply, cx);
            }
            Ok(ImportQxRuleSuccess::AlreadyExists { stored }) => {
                self.finish_existing_qx_rule(operation_id, stored);
            }
            Ok(ImportQxRuleSuccess::RolledBack {
                apply,
                rollback_error,
            }) => {
                self.rule_sources.feedback = QxRuleImportFeedback::Idle;
                self.status = apply
                    .status_suffix_after_rollback_attempt(self.language(), rollback_error.as_ref());
            }
            Err(error) => self.finish_failed_qx_rule_import(operation_id, &error),
        }
        cx.notify();
    }

    fn finish_imported_qx_rule(
        &mut self,
        operation_id: u64,
        stored: mihomo::StoredQxRuleSource,
        apply: &SourceRuntimeApply,
        cx: &mut Context<Self>,
    ) {
        let language = self.language();
        let rule_count = stored.rule_count;
        let diagnostic_count = stored.diagnostic_count;
        let stored_id = stored.id.clone();
        let target_policy = self.effective_rule_target(stored.target_policy.as_str(), language);
        if let Some(existing) = self
            .rule_sources
            .sources
            .iter_mut()
            .find(|source| source.id == stored_id)
        {
            *existing = stored;
        } else {
            self.rule_sources.sources.push(stored);
        }
        self.persist_routing_rule_group_order();
        self.rule_sources.refreshes.remove(&stored_id);
        self.rule_sources.feedback = QxRuleImportFeedback::Imported {
            rule_count,
            diagnostic_count,
        };
        if let Some(input) = self.inputs.qx_rule.as_ref() {
            input.update(cx, SubscriptionTextInput::clear_without_event);
        }
        apply.reconcile_proxy_mode(&mut self.proxy_mode);
        self.status = copy::configuration::qx_rules_applied(
            language,
            copy::configuration::QxRuleAction::Imported,
            rule_count,
            &apply.status_suffix(language),
        );
        record_operation(
            operation_id,
            LogLevel::Info,
            "configuration.rule_source.add.succeeded",
            format!(
                "id={stored_id} rules={rule_count} skipped={diagnostic_count} target={target_policy}"
            ),
        );
    }

    fn finish_existing_qx_rule(&mut self, operation_id: u64, stored: mihomo::StoredQxRuleSource) {
        let target_policy =
            self.effective_rule_target(stored.target_policy.as_str(), self.language());
        let source_id = stored.id.clone();
        let rule_count = stored.rule_count;
        if !self
            .rule_sources
            .sources
            .iter()
            .any(|source| source.id == source_id)
        {
            self.rule_sources.sources.push(stored);
        }
        self.persist_routing_rule_group_order();
        self.rule_sources.feedback = QxRuleImportFeedback::AlreadyExists {
            source_id: source_id.clone(),
            rule_count,
            target_policy: target_policy.clone(),
        };
        self.language()
            .localized(copy::configuration::RULE_SOURCE_ALREADY_EXISTS_NO_DUPLICATE_WAS_ADDED)
            .clone_into(&mut self.status);
        record_operation(
            operation_id,
            LogLevel::Warn,
            "configuration.rule_source.add.duplicate",
            format!("existing_id={source_id} rules={rule_count} target={target_policy}"),
        );
    }

    fn persist_routing_rule_group_order(&mut self) {
        self.sync_routing_rule_group_order();
        if let Some(store_dir) = self.subscription_store_dir.as_ref() {
            let _ =
                mihomo::save_routing_rule_group_order_in(store_dir, &self.rule_sources.group_order);
        }
    }

    fn finish_failed_qx_rule_import(&mut self, operation_id: u64, error: &ImportQxRuleError) {
        match error {
            ImportQxRuleError::Download(error) => {
                self.status = format!(
                    "{}: {error}",
                    self.language()
                        .localized(copy::configuration::QX_RULE_DOWNLOAD_FAILED)
                );
                self.rule_sources.feedback = QxRuleImportFeedback::DownloadFailed(*error);
                record_operation(
                    operation_id,
                    LogLevel::Error,
                    "configuration.rule_source.add.failed",
                    "phase=download",
                );
            }
            ImportQxRuleError::InvalidDocument => {
                self.rule_sources.feedback = QxRuleImportFeedback::InvalidDocument;
                self.language()
                    .localized(
                        copy::configuration::QX_RULES_NOT_IMPORTED_NO_RECOGNIZABLE_DOMAIN_RULES,
                    )
                    .clone_into(&mut self.status);
                record_operation(
                    operation_id,
                    LogLevel::Error,
                    "configuration.rule_source.add.failed",
                    "phase=parse reason=no_recognizable_domain_rules",
                );
            }
            ImportQxRuleError::Store(error) => {
                self.status = format!(
                    "{}: {error}",
                    self.language()
                        .localized(copy::configuration::QX_RULE_SAVE_FAILED)
                );
                self.rule_sources.feedback = QxRuleImportFeedback::StoreFailed(*error);
                record_operation(
                    operation_id,
                    LogLevel::Error,
                    "configuration.rule_source.add.failed",
                    "phase=store",
                );
            }
        }
    }

    fn remove_qx_rule_source(&mut self, id: String, cx: &mut Context<Self>) {
        let Some(store_dir) = self.subscription_store_dir.clone() else {
            return;
        };
        self.rule_sources.import_generation = self.rule_sources.import_generation.wrapping_add(1);
        let generation = self.rule_sources.import_generation;
        self.rule_sources.feedback = QxRuleImportFeedback::Importing;
        self.language()
            .localized(copy::configuration::REMOVING_REMOTE_QX_RULES)
            .clone_into(&mut self.status);
        let runtime = self.runtime.clone();
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn(async move {
                    super::mutate_saved_sources(&runtime, &store_dir, || {
                        mihomo::remove_qx_rule_source_in(&store_dir, &id).map(|()| id.clone())
                    })
                })
                .await;
            this.update(cx, |this, cx| {
                if this.rule_sources.import_generation != generation {
                    return;
                }
                match result {
                    Ok(transaction) if transaction.value.is_some() => {
                        let id = transaction.value.expect("checked committed mutation");
                        this.rule_sources.sources.retain(|source| source.id != id);
                        this.sync_routing_rule_group_order();
                        if let Some(store_dir) = this.subscription_store_dir.as_ref() {
                            let _ = mihomo::save_routing_rule_group_order_in(
                                store_dir,
                                &this.rule_sources.group_order,
                            );
                        }
                        this.rule_sources.refreshes.remove(&id);
                        this.rule_sources
                            .refresh_retry_not_before
                            .remove(&super::DueRemoteSource::QxRule(id.clone()).scheduler_key());
                        this.rule_sources.feedback = QxRuleImportFeedback::Idle;
                        let language = this.language();
                        transaction.apply.reconcile_proxy_mode(&mut this.proxy_mode);
                        this.status = copy::configuration::qx_rules_removed(
                            language,
                            &transaction.apply.status_suffix(language),
                        );
                    }
                    Ok(transaction) => {
                        this.rule_sources.feedback = QxRuleImportFeedback::StoreFailed(
                            SubscriptionStoreError::StoreUnavailable,
                        );
                        this.status = format!(
                            "{}{}",
                            this.language()
                                .localized(copy::configuration::REMOTE_QX_RULE_REMOVAL_FAILED),
                            transaction
                                .apply
                                .status_suffix_after_source_rollback(this.language())
                        );
                    }
                    Err(error) => {
                        this.rule_sources.feedback = QxRuleImportFeedback::StoreFailed(error);
                        this.status = format!(
                            "{}: {error}",
                            this.language()
                                .localized(copy::configuration::REMOTE_QX_RULE_REMOVAL_FAILED)
                        );
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn set_subscription_enabled(&mut self, id: &str, enabled: bool, cx: &mut Context<Self>) {
        if self.source_refresh_busy() {
            return;
        }
        let Some(store_dir) = self.subscription_store_dir.clone() else {
            return;
        };
        let Some(source) = self
            .imported_subscriptions
            .iter_mut()
            .find(|source| source.id == id)
        else {
            return;
        };
        let previous_state = source.state;
        let previous_enabled = source.enabled;
        let kind = super::source_kind(&source.source);
        self.subscription_preview_generation = self.subscription_preview_generation.wrapping_add(1);
        let generation = self.subscription_preview_generation;
        source.generation = generation;
        source.state = ImportedSubscriptionState::Refreshing(kind);
        let completion = SubscriptionToggleCompletion {
            id: id.to_owned(),
            generation,
            kind,
            previous_state,
            previous_enabled,
        };
        self.language()
            .localized(copy::configuration::APPLYING_SUBSCRIPTION_STATE)
            .clone_into(&mut self.status);
        let runtime = self.runtime.clone();
        let executor = cx.background_executor().clone();
        let task_id = id.to_owned();
        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn(async move {
                    super::mutate_saved_sources(&runtime, &store_dir, || {
                        mihomo::update_subscription_source_enabled_in(&store_dir, &task_id, enabled)
                    })
                })
                .await;
            this.update(cx, |this, cx| {
                this.finish_subscription_toggle(completion, result, cx);
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn finish_subscription_toggle(
        &mut self,
        completion: SubscriptionToggleCompletion,
        result: Result<super::SourceMutation<mihomo::StoredSubscription>, SubscriptionStoreError>,
        cx: &mut Context<Self>,
    ) {
        let language = self.language();
        let Some(source) = self
            .imported_subscriptions
            .iter_mut()
            .find(|source| source.id == completion.id)
        else {
            return;
        };
        if source.generation != completion.generation {
            return;
        }
        let refresh_after_enable = match result {
            Ok(transaction) if transaction.value.is_some() => {
                let stored = transaction.value.expect("checked committed mutation");
                source.enabled = stored.enabled;
                source.state = if stored.enabled {
                    ImportedSubscriptionState::Pending(completion.kind)
                } else {
                    ImportedSubscriptionState::None
                };
                transaction.apply.reconcile_proxy_mode(&mut self.proxy_mode);
                self.status = format!(
                    "{}{}",
                    if stored.enabled {
                        language.localized(copy::configuration::SUBSCRIPTION_ENABLED)
                    } else {
                        language.localized(copy::configuration::SUBSCRIPTION_DISABLED)
                    },
                    transaction.apply.status_suffix(language)
                );
                stored.enabled
            }
            Ok(transaction) => {
                source.enabled = completion.previous_enabled;
                source.state = completion.previous_state;
                self.status = format!(
                    "{}{}",
                    language.localized(copy::configuration::FAILED_TO_CHANGE_SUBSCRIPTION_STATE),
                    transaction
                        .apply
                        .status_suffix_after_source_rollback(language)
                );
                false
            }
            Err(error) => {
                source.enabled = completion.previous_enabled;
                source.state = completion.previous_state;
                self.status = format!(
                    "{}: {error}",
                    language.localized(copy::configuration::FAILED_TO_CHANGE_SUBSCRIPTION_STATE)
                );
                false
            }
        };
        if refresh_after_enable {
            self.refresh_imported_subscription(completion.id, cx);
        }
        cx.notify();
    }

    fn set_qx_rule_source_enabled(&mut self, id: String, enabled: bool, cx: &mut Context<Self>) {
        if self.source_refresh_busy() {
            return;
        }
        let Some(store_dir) = self.subscription_store_dir.clone() else {
            return;
        };
        self.rule_sources.import_generation = self.rule_sources.import_generation.wrapping_add(1);
        let generation = self.rule_sources.import_generation;
        self.rule_sources
            .target_updates
            .insert(id.clone(), generation);
        self.language()
            .localized(copy::configuration::APPLYING_RULE_SOURCE_STATE)
            .clone_into(&mut self.status);
        let runtime = self.runtime.clone();
        let executor = cx.background_executor().clone();
        let task_id = id.clone();
        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn(async move {
                    super::mutate_saved_sources(&runtime, &store_dir, || {
                        mihomo::update_qx_rule_source_enabled_in(&store_dir, &task_id, enabled)
                    })
                })
                .await;
            this.update(cx, |this, cx| {
                if this.rule_sources.target_updates.get(&id) != Some(&generation) {
                    return;
                }
                this.rule_sources.target_updates.remove(&id);
                match result {
                    Ok(transaction) if transaction.value.is_some() => {
                        let stored = transaction.value.expect("checked committed mutation");
                        let language = this.language();
                        let enabled = stored.enabled;
                        if let Some(source) = this
                            .rule_sources
                            .sources
                            .iter_mut()
                            .find(|source| source.id == id)
                        {
                            *source = stored;
                        }
                        transaction.apply.reconcile_proxy_mode(&mut this.proxy_mode);
                        this.status = format!(
                            "{}{}",
                            if enabled {
                                language.localized(copy::configuration::RULE_SOURCE_ENABLED)
                            } else {
                                language.localized(copy::configuration::RULE_SOURCE_DISABLED)
                            },
                            transaction.apply.status_suffix(language)
                        );
                    }
                    Ok(transaction) => {
                        this.status = format!(
                            "{}{}",
                            this.language()
                                .localized(copy::configuration::FAILED_TO_CHANGE_RULE_SOURCE_STATE),
                            transaction
                                .apply
                                .status_suffix_after_source_rollback(this.language())
                        );
                    }
                    Err(error) => {
                        this.status = format!(
                            "{}: {error}",
                            this.language()
                                .localized(copy::configuration::FAILED_TO_CHANGE_RULE_SOURCE_STATE)
                        );
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn update_qx_rule_source_target(&mut self, id: String, target: String, cx: &mut Context<Self>) {
        if self.source_refresh_busy() || !self.qx_rule_targets().contains(&target) {
            return;
        }
        let Some(store_dir) = self.subscription_store_dir.clone() else {
            self.language()
                .localized(copy::configuration::COULD_NOT_DETERMINE_WHERE_TO_SAVE_THE_RULE_SOURCE)
                .clone_into(&mut self.status);
            cx.notify();
            return;
        };
        let Some(source) = self
            .rule_sources
            .sources
            .iter()
            .find(|source| source.id == id)
        else {
            return;
        };
        if self.effective_rule_target(source.target_policy.as_str(), self.language()) == target {
            self.rule_sources.target_popover = None;
            cx.notify();
            return;
        }

        self.rule_sources.import_generation = self.rule_sources.import_generation.wrapping_add(1);
        let generation = self.rule_sources.import_generation;
        self.rule_sources
            .target_updates
            .insert(id.clone(), generation);
        self.rule_sources.target_popover = None;
        self.status = format!(
            "{} {target}",
            self.language()
                .localized(copy::configuration::SAVING_RULE_SOURCE_POLICY)
        );
        let runtime = self.runtime.clone();
        let executor = cx.background_executor().clone();
        let task_id = id.clone();
        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn(async move {
                    super::mutate_saved_sources(&runtime, &store_dir, || {
                        mihomo::update_qx_rule_source_target_in(&store_dir, &task_id, &target)
                    })
                })
                .await;
            this.update(cx, |this, cx| {
                this.finish_qx_rule_target_update(&id, generation, result, cx);
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn finish_qx_rule_target_update(
        &mut self,
        id: &str,
        generation: u64,
        result: Result<super::SourceMutation<mihomo::StoredQxRuleSource>, SubscriptionStoreError>,
        cx: &mut Context<Self>,
    ) {
        if self.rule_sources.target_updates.get(id) != Some(&generation) {
            return;
        }
        self.rule_sources.target_updates.remove(id);
        match result {
            Ok(transaction) if transaction.value.is_some() => {
                self.finish_successful_qx_rule_target_update(id, transaction);
            }
            Ok(transaction) => {
                self.status = format!(
                    "{}{}",
                    self.language()
                        .localized(copy::configuration::FAILED_TO_SAVE_RULE_SOURCE_POLICY),
                    transaction
                        .apply
                        .status_suffix_after_source_rollback(self.language())
                );
            }
            Err(error) => {
                self.status = format!(
                    "{}: {error}",
                    self.language()
                        .localized(copy::configuration::FAILED_TO_SAVE_RULE_SOURCE_POLICY)
                );
            }
        }
        cx.notify();
    }

    fn finish_successful_qx_rule_target_update(
        &mut self,
        id: &str,
        mut transaction: super::SourceMutation<mihomo::StoredQxRuleSource>,
    ) {
        let stored = transaction
            .value
            .take()
            .expect("checked committed mutation");
        let language = self.language();
        let target = stored.target_policy.as_str().to_owned();
        if let Some(source) = self
            .rule_sources
            .sources
            .iter_mut()
            .find(|source| source.id == id)
        {
            *source = stored;
        }
        transaction.apply.reconcile_proxy_mode(&mut self.proxy_mode);
        self.status = format!(
            "{} {target}{}",
            language.localized(copy::configuration::RULE_SOURCE_POLICY_SET_TO),
            transaction.apply.status_suffix(language)
        );
        record_event(
            LogLevel::Info,
            "routing.rule_source.target.updated",
            format!("source_id={id} target={target}"),
        );
    }

    pub(super) fn refresh_qx_rule_source(&mut self, id: String, cx: &mut Context<Self>) {
        if self.source_refresh_busy() {
            return;
        }
        let Some(store_dir) = self.subscription_store_dir.clone() else {
            return;
        };
        let Some(source) = self
            .rule_sources
            .sources
            .iter()
            .find(|source| source.id == id)
        else {
            return;
        };
        let url = source.source.clone();
        self.rule_sources.import_generation = self.rule_sources.import_generation.wrapping_add(1);
        let generation = self.rule_sources.import_generation;
        self.rule_sources.refreshes.insert(
            id.clone(),
            QxRuleSourceRefreshState::Refreshing { generation },
        );
        self.language()
            .localized(copy::configuration::UPDATING_REMOTE_QX_RULES)
            .clone_into(&mut self.status);
        let runtime = self.runtime.clone();
        let executor = cx.background_executor().clone();
        let task_id = id.clone();
        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn(async move {
                    let content = download_qx_rule_document_secret(&url)
                        .map_err(ImportQxRuleError::Download)?;
                    let parsed = QxRuleList::parse(&content);
                    if parsed.rules.is_empty() {
                        return Err(ImportQxRuleError::InvalidDocument);
                    }
                    let transaction = super::mutate_saved_sources(&runtime, &store_dir, || {
                        mihomo::replace_qx_rule_source_content_in(
                            &store_dir,
                            &task_id,
                            &content,
                            mihomo::current_unix_secs(),
                        )
                    })
                    .map_err(ImportQxRuleError::Store)?;
                    Ok::<_, ImportQxRuleError>(transaction)
                })
                .await;
            this.update(cx, |this, cx| {
                this.finish_qx_rule_source_refresh(&id, generation, result, cx);
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn finish_qx_rule_source_refresh(
        &mut self,
        id: &str,
        generation: u64,
        result: super::QxRuleRefreshResult,
        cx: &mut Context<Self>,
    ) {
        if !matches!(
            self.rule_sources.refreshes.get(id),
            Some(QxRuleSourceRefreshState::Refreshing { generation: active })
                if *active == generation
        ) {
            return;
        }
        match result {
            Ok(transaction) if transaction.value.is_some() => {
                self.finish_successful_qx_rule_refresh(id, transaction);
            }
            Err(error) => self.finish_failed_qx_rule_refresh(id, generation, &error),
            Ok(transaction) => {
                let message = "runtime apply failed".to_owned();
                self.rule_sources.refreshes.insert(
                    id.to_owned(),
                    QxRuleSourceRefreshState::Failed {
                        generation,
                        message,
                    },
                );
                self.status = format!(
                    "{}{}",
                    self.language()
                        .localized(copy::configuration::REMOTE_QX_RULE_UPDATE_FAILED),
                    transaction
                        .apply
                        .status_suffix_after_source_rollback(self.language())
                );
            }
        }
        cx.notify();
    }

    fn finish_successful_qx_rule_refresh(
        &mut self,
        id: &str,
        mut transaction: super::SourceMutation<mihomo::StoredQxRuleSource>,
    ) {
        let stored = transaction
            .value
            .take()
            .expect("checked committed mutation");
        let language = self.language();
        let rule_count = stored.rule_count;
        if let Some(source) = self
            .rule_sources
            .sources
            .iter_mut()
            .find(|source| source.id == id)
        {
            *source = stored;
        }
        self.rule_sources.refreshes.remove(id);
        self.rule_sources
            .refresh_retry_not_before
            .remove(&super::DueRemoteSource::QxRule(id.to_owned()).scheduler_key());
        transaction.apply.reconcile_proxy_mode(&mut self.proxy_mode);
        self.status = copy::configuration::qx_rules_applied(
            language,
            copy::configuration::QxRuleAction::Updated,
            rule_count,
            &transaction.apply.status_suffix(language),
        );
    }

    fn finish_failed_qx_rule_refresh(
        &mut self,
        id: &str,
        generation: u64,
        error: &ImportQxRuleError,
    ) {
        let message = match error {
            ImportQxRuleError::Download(error) => error.to_string(),
            ImportQxRuleError::InvalidDocument => self
                .language()
                .localized(copy::configuration::NO_RECOGNIZABLE_DOMAIN_RULES)
                .to_owned(),
            ImportQxRuleError::Store(error) => error.to_string(),
        };
        self.rule_sources.refreshes.insert(
            id.to_owned(),
            QxRuleSourceRefreshState::Failed {
                generation,
                message: message.clone(),
            },
        );
        self.status = format!(
            "{}: {message}",
            self.language()
                .localized(copy::configuration::QX_RULE_UPDATE_FAILED)
        );
    }
}

#[cfg(test)]
mod tests {
    use manis_core::NodeWorkspaceState;

    use super::{
        Language, MANUAL_RULES_EXPANSION_KEY, ManualRuleKeyboardAction,
        manual_rule_keyboard_action_for, rule_group_is_open, rule_source_expansion_key,
        source_update_label,
    };

    #[test]
    fn remote_rule_sources_start_open_and_remember_collapse() {
        let mut workspace = NodeWorkspaceState::default();
        let key = rule_source_expansion_key("qx-rule-deadbeef");

        assert!(!workspace.is_group_collapsed(&key));
        assert!(rule_group_is_open(&workspace, &key));
        workspace.toggle_group(&key);
        assert!(workspace.is_group_collapsed(&key));
        assert!(!rule_group_is_open(&workspace, &key));
        assert_eq!(key, "routing-rule-source:qx-rule-deadbeef");
    }

    #[test]
    fn manual_rules_use_the_same_collapsible_group_state() {
        let mut workspace = NodeWorkspaceState::default();

        assert!(!workspace.is_group_collapsed(MANUAL_RULES_EXPANSION_KEY));
        assert!(rule_group_is_open(&workspace, MANUAL_RULES_EXPANSION_KEY));
        workspace.toggle_group(MANUAL_RULES_EXPANSION_KEY);
        assert!(workspace.is_group_collapsed(MANUAL_RULES_EXPANSION_KEY));
        assert!(!rule_group_is_open(&workspace, MANUAL_RULES_EXPANSION_KEY));
    }

    #[test]
    fn manual_rule_rows_expose_keyboard_edit_toggle_and_delete_actions() {
        assert_eq!(
            manual_rule_keyboard_action_for("enter", false, false),
            Some(ManualRuleKeyboardAction::Edit)
        );
        assert_eq!(
            manual_rule_keyboard_action_for("space", false, false),
            Some(ManualRuleKeyboardAction::Toggle)
        );
        assert_eq!(
            manual_rule_keyboard_action_for("delete", false, false),
            Some(ManualRuleKeyboardAction::Delete)
        );
        assert_eq!(manual_rule_keyboard_action_for("enter", true, false), None);
        assert_eq!(manual_rule_keyboard_action_for("delete", false, true), None);
    }

    #[test]
    fn source_update_time_is_compact_and_handles_clock_rollback() {
        assert_eq!(
            source_update_label(0, 10_000, Language::SimplifiedChinese),
            "从未更新"
        );
        assert_eq!(
            source_update_label(9_980, 10_000, Language::SimplifiedChinese),
            "刚刚更新"
        );
        assert_eq!(
            source_update_label(6_400, 10_000, Language::SimplifiedChinese),
            "1 小时前更新"
        );
        assert_eq!(
            source_update_label(10_100, 10_000, Language::SimplifiedChinese),
            "刚刚更新"
        );
        assert_eq!(
            source_update_label(6_400, 10_000, Language::English),
            "Updated 1 hr ago"
        );
    }
}
