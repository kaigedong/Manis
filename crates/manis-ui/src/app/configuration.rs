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
    AppUpdateState, ConfigurationSection, ImportQxRuleError, ImportQxRuleSuccess,
    ImportedSubscriptionState, ManisApp, ManualRulePopover, MihomoCoreUpdateState,
    ProxySourceEditorKind, QxRuleImportFeedback, QxRuleList, QxRuleSourceRefreshState,
    SourceRuntimeApply, SubscriptionFeedback, proxy_mode_label, routing_mode_label,
};
use crate::{
    app_update,
    components::{
        ActionRole, StatusTone, action_button, dialog_footer_surface, dialog_header_surface,
        empty_state, page_heading, section_heading, status_badge, style_action_button,
        surface_dialog,
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
    name: String,
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
    request: &QxRuleSaveRequest,
) -> super::QxRuleImportResult {
    let existing = if let Some(id) = request.editing_id.as_ref() {
        mihomo::load_qx_rule_sources_in(store_dir)
            .map_err(ImportQxRuleError::Store)?
            .into_iter()
            .find(|source| &source.id == id)
    } else {
        None
    };
    if let Some(source) = existing.as_ref()
        && source.source.expose_to(|url| url == request.url)
        && source.target_policy.as_str() == request.target
        && source.refresh_interval == request.refresh_interval
    {
        let stored = mihomo::update_qx_rule_source_name_in(store_dir, &source.id, &request.name)
            .map_err(ImportQxRuleError::Store)?;
        return Ok(ImportQxRuleSuccess::Imported {
            stored,
            apply: SourceRuntimeApply::MetadataOnly,
        });
    }
    // Editing metadata must also work offline and must not count as a rule refresh.
    let (content, last_success) = match existing {
        Some(source) if source.source.expose_to(|url| url == request.url) => {
            (source.content, source.last_successful_update_unix_secs)
        }
        _ => (
            download_qx_rule_document(&request.url).map_err(ImportQxRuleError::Download)?,
            mihomo::current_unix_secs(),
        ),
    };
    if QxRuleList::parse(&content).rules.is_empty() {
        return Err(ImportQxRuleError::InvalidDocument);
    }
    if let Some(editing_id) = request.editing_id.as_ref() {
        return replace_qx_rule_source(
            runtime,
            store_dir,
            editing_id,
            request,
            &content,
            last_success,
        );
    }
    create_qx_rule_source(
        runtime,
        store_dir,
        &request.url,
        &request.name,
        &request.target,
        &content,
        request.refresh_interval,
    )
}

fn replace_qx_rule_source(
    runtime: &super::KernelRuntime,
    store_dir: &Path,
    id: &str,
    request: &QxRuleSaveRequest,
    content: &str,
    last_success: u64,
) -> super::QxRuleImportResult {
    let transaction = super::mutate_saved_sources(runtime, store_dir, || {
        mihomo::replace_qx_rule_source_definition_in(
            store_dir,
            id,
            &request.name,
            &request.url,
            &request.target,
            content,
            request.refresh_interval,
            last_success,
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
    name: &str,
    target: &str,
    content: &str,
    refresh_interval: RemoteSourceRefreshInterval,
) -> super::QxRuleImportResult {
    let transaction = super::mutate_saved_sources(runtime, store_dir, || {
        let outcome = mihomo::save_named_qx_rule_source_in(store_dir, url, name, target, content)?;
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

fn accordion_title_style(compact: bool, open: bool, theme: Theme) -> StyleRefinement {
    let mut style = StyleRefinement::default()
        .bg(theme.surface_low)
        .rounded_tl(Radius::Pane.px())
        .rounded_tr(Radius::Pane.px());
    if !open {
        style = style
            .rounded_bl(Radius::Pane.px())
            .rounded_br(Radius::Pane.px());
    }
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
        let sections = [
            div()
                .flex()
                .flex_col()
                .gap(Space::Lg.px())
                .child(self.language_panel(theme, compact, cx))
                .child(self.app_update_panel(theme, compact, cx))
                .into_any_element(),
            self.kernel_panel(theme, compact, cx).into_any_element(),
            self.source_panel(theme, compact, cx).into_any_element(),
            self.rule_source_manager(rule_input, rule_busy, theme, compact, cx)
                .into_any_element(),
            self.advanced_configuration_panel(theme, compact)
                .into_any_element(),
        ];
        let navigation = self.configuration_navigation(theme, compact, cx);
        let content = div()
            .id("configuration-detail-scroll")
            .flex_1()
            .min_w(px(0.0))
            .min_h(px(0.0))
            .overflow_y_scroll()
            .track_scroll(&self.configuration_scroll)
            .px(if compact { px(12.0) } else { px(24.0) })
            .pb(px(56.0))
            .children(ConfigurationSection::ALL.into_iter().zip(sections).map(
                |(section, detail)| {
                    div()
                        .id(format!("configuration-section-{}", section.key()))
                        .w_full()
                        .max_w(px(900.0))
                        .mx_auto()
                        .pt(if compact { px(12.0) } else { px(24.0) })
                        .child(detail)
                },
            ))
            .on_scroll_wheel(cx.listener(|_, _, window, cx| {
                // Read the offset after GPUI applies the wheel event, including momentum.
                cx.defer_in(window, |this, _, cx| {
                    this.sync_configuration_directory(cx);
                });
            }));
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
}
include!("configuration/settings.rs");
include!("configuration/proxy_sources.rs");
include!("configuration/rule_sources.rs");
include!("configuration/manual_rules.rs");
include!("configuration/source_mutations.rs");

#[cfg(test)]
mod tests {
    use manis_core::NodeWorkspaceState;

    use super::{
        ConfigurationSection, Language, MANUAL_RULES_EXPANSION_KEY, ManualRuleKeyboardAction,
        manual_rule_keyboard_action_for, rule_group_is_open, rule_source_expansion_key,
        source_update_label,
    };

    #[test]
    fn configuration_starts_at_the_first_directory_section() {
        assert_eq!(
            ConfigurationSection::default(),
            ConfigurationSection::ALL[0]
        );
    }

    #[test]
    fn rule_source_rename_saves_offline_and_survives_restart() {
        let store =
            std::env::temp_dir().join(format!("manis-rule-source-rename-{}", std::process::id()));
        let url = "https://127.0.0.1:1/rules.list";
        let source = crate::mihomo::save_qx_rule_source_in(
            &store,
            url,
            "DIRECT",
            "DOMAIN-SUFFIX,example.com,DIRECT\n",
        )
        .expect("save source fixture")
        .into_source();
        let app = super::ManisApp::with_fixture_controller_and_subscription_store(
            "http://127.0.0.1:1",
            store.clone(),
        );
        let outcome = super::save_qx_rule_source(
            &app.runtime,
            &store,
            &super::QxRuleSaveRequest {
                url: url.to_owned(),
                name: "  工作规则  ".to_owned(),
                target: "DIRECT".to_owned(),
                editing_id: Some(source.id.clone()),
                refresh_interval: source.refresh_interval,
            },
        )
        .unwrap_or_else(|_| {
            panic!("renaming cached rules must not access the closed network port")
        });
        let super::ImportQxRuleSuccess::Imported { stored, .. } = outcome else {
            panic!("rename must save successfully");
        };
        assert_eq!(stored.name.as_deref(), Some("工作规则"));
        assert_eq!(stored.id, source.id);
        assert_eq!(stored.content, source.content);
        assert_eq!(
            stored.last_successful_update_unix_secs,
            source.last_successful_update_unix_secs
        );
        let restored = super::ManisApp::with_fixture_controller_and_subscription_store(
            "http://127.0.0.1:1",
            store.clone(),
        );
        assert_eq!(restored.rule_sources.sources, vec![stored.clone()]);
        assert_eq!(
            super::ManisApp::qx_rule_source_name(&stored, 0, Language::SimplifiedChinese),
            "工作规则"
        );
        std::fs::remove_dir_all(store).expect("remove fixture");
    }

    #[gpui::test]
    fn rule_source_name_editor_prefills_and_discards_unsaved_changes(
        cx: &mut gpui::TestAppContext,
    ) {
        use gpui::AppContext as _;
        let store =
            std::env::temp_dir().join(format!("manis-rule-name-editor-{}", std::process::id()));
        let source = crate::mihomo::save_qx_rule_source_in(
            &store,
            "https://127.0.0.1:1/rules.list",
            "DIRECT",
            "DOMAIN,example.com,DIRECT\n",
        )
        .expect("save fixture")
        .into_source();
        crate::mihomo::update_qx_rule_source_name_in(&store, &source.id, "我的规则")
            .expect("name fixture");
        cx.update(crate::init);
        let mut app = None;
        let (_, window_cx) = cx.add_window_view(|window, cx| {
            let entity = cx.new(|_| {
                super::ManisApp::with_fixture_controller_and_subscription_store(
                    "http://127.0.0.1:1",
                    store.clone(),
                )
            });
            app = Some(entity.clone());
            crate::root(entity, window, cx)
        });
        let app = app.expect("fixture app");
        window_cx.update(|window, cx| {
            window.draw(cx).clear(cx);
            app.update(cx, |app, cx| {
                app.open_qx_rule_editor(source.id.clone(), cx);
                let name = app.inputs.qx_rule_name.clone().expect("name input");
                assert_eq!(name.read(cx).value(), "我的规则");
                name.update(cx, |input, cx| {
                    input.set_value_without_event("未保存名称", cx);
                });
                app.close_qx_rule_editor(cx);
                app.open_qx_rule_editor(source.id.clone(), cx);
                assert_eq!(name.read(cx).value(), "我的规则");
                app.open_new_qx_rule_editor(cx);
                assert_eq!(name.read(cx).value(), "");
                assert_eq!(
                    app.rule_sources.sources[0].name.as_deref(),
                    Some("我的规则")
                );
            });
        });
        std::fs::remove_dir_all(store).expect("remove fixture");
    }

    #[test]
    fn configuration_directory_follows_variable_height_sections_in_both_directions() {
        use super::configuration_section_at_scroll;
        use gpui::px;

        let tops = [120.0, 480.0, 790.0, 1_450.0, 1_900.0].map(px);
        for (offset, expected) in [
            (120.0, ConfigurationSection::General),
            (478.0, ConfigurationSection::General),
            (480.0, ConfigurationSection::Runtime),
            (1_200.0, ConfigurationSection::ProxySources),
            (1_500.0, ConfigurationSection::RuleSources),
            (1_900.0, ConfigurationSection::Advanced),
            (500.0, ConfigurationSection::Runtime),
            (120.0, ConfigurationSection::General),
        ] {
            assert_eq!(
                configuration_section_at_scroll(&tops, px(offset), false),
                expected
            );
        }
        assert_eq!(
            configuration_section_at_scroll(&tops, px(1_600.0), true),
            ConfigurationSection::Advanced,
            "the last short section is active when the viewport reaches the bottom",
        );
    }

    #[gpui::test]
    fn configuration_renders_one_scroll_document_and_directory_jumps_to_sections(
        cx: &mut gpui::TestAppContext,
    ) {
        use super::ManisApp;
        use gpui::{AppContext as _, ScrollDelta, ScrollWheelEvent, point, px};
        use manis_core::PrimaryWorkspace;

        cx.update(crate::init);
        let mut app = None;
        let (_, window_cx) = cx.add_window_view(|window, cx| {
            let entity = cx.new(|_| ManisApp::with_fixture_controller("http://127.0.0.1:9090"));
            app = Some(entity.clone());
            crate::root(entity, window, cx)
        });
        let app = app.expect("fixture app");
        window_cx.update(|window, cx| {
            app.update(cx, |app, cx| {
                assert_eq!(app.primary_workspace, PrimaryWorkspace::Nodes);
                app.primary_workspace = PrimaryWorkspace::Configuration;
                cx.notify();
            });
            window.draw(cx).clear(cx);
            app.read_with(cx, |app, _| {
                assert_eq!(app.configuration_section, ConfigurationSection::General);
                assert_eq!(app.configuration_scroll.children_count(), 5);
                assert_eq!(app.configuration_scroll.offset().y, px(0.0));
            });

            app.update(cx, |app, cx| {
                app.scroll_to_configuration_section(ConfigurationSection::Runtime, cx);
            });
            window.draw(cx).clear(cx);
            app.read_with(cx, |app, _| {
                let scroll = &app.configuration_scroll;
                let runtime = scroll.bounds_for_item(1).expect("runtime anchor");
                assert!(
                    (runtime.top() + scroll.offset().y - scroll.bounds().top()).abs() <= px(1.0)
                );
            });

            app.update(cx, |app, cx| {
                app.configuration_scroll
                    .set_offset(point(px(0.0), -app.configuration_scroll.max_offset().y));
                app.sync_configuration_directory(cx);
                assert_eq!(app.configuration_section, ConfigurationSection::Advanced);
                app.scroll_to_configuration_section(ConfigurationSection::General, cx);
            });
            window.draw(cx).clear(cx);
            app.read_with(cx, |app, _| {
                assert_eq!(app.configuration_scroll.offset().y, px(0.0));
                assert_eq!(app.configuration_scroll.children_count(), 5);
            });
        });

        let position = window_cx
            .update(|_, cx| app.read_with(cx, |app, _| app.configuration_scroll.bounds().center()));
        for (delta, expected) in [
            (-10_000.0, ConfigurationSection::Advanced),
            (10_000.0, ConfigurationSection::General),
        ] {
            window_cx.simulate_event(ScrollWheelEvent {
                position,
                delta: ScrollDelta::Pixels(point(px(0.0), px(delta))),
                ..Default::default()
            });
            window_cx.update(|window, cx| {
                window.draw(cx).clear(cx);
                app.read_with(cx, |app, _| assert_eq!(app.configuration_section, expected));
            });
        }
    }

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
