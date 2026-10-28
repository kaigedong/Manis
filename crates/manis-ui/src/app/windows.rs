//! Native utility windows share the application model, but own their host and focus.
use std::collections::{HashMap, HashSet};

use gpui::{
    AnyElement, AnyWindowHandle, App, AppContext as _, Bounds, Context, Entity, EntityId,
    FocusHandle, Focusable, Global, IntoElement, ParentElement, Render, Styled, Subscription,
    TitlebarOptions, Window, WindowBackgroundAppearance, WindowBounds, WindowOptions, actions, div,
    prelude::*, px, size,
};
use gpui_component::{Root, WindowExt as _};
use manis_core::WindowSizeClass;

use super::{ManisApp, QxRuleImportFeedback};
use crate::{
    localization::{Message, copy},
    theme::TextRole,
};

actions!(manis_windows, [ShowSettings, CloseAuxiliary]);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum AuxiliaryKind {
    ProxySource,
    RuleSource,
    ManualRule,
    Policy,
}

type WindowKey = (EntityId, AuxiliaryKind);

#[derive(Default)]
struct AuxiliaryWindows {
    handles: HashMap<WindowKey, AnyWindowHandle>,
    pending: HashSet<WindowKey>,
}
impl Global for AuxiliaryWindows {}

pub(crate) fn init(cx: &mut App) {
    cx.bind_keys([
        gpui::KeyBinding::new("cmd-,", ShowSettings, Some("ManisWindow")),
        gpui::KeyBinding::new("ctrl-,", ShowSettings, Some("ManisWindow")),
        gpui::KeyBinding::new("cmd-w", CloseAuxiliary, Some("ManisAuxiliary")),
        gpui::KeyBinding::new("ctrl-w", CloseAuxiliary, Some("ManisAuxiliary")),
        gpui::KeyBinding::new("escape", CloseAuxiliary, Some("ManisEditor")),
    ]);
}

pub(super) fn has_editors(id: EntityId, cx: &App) -> bool {
    cx.try_global::<AuxiliaryWindows>().is_some_and(|registry| {
        registry
            .handles
            .keys()
            .chain(registry.pending.iter())
            .any(|(owner, _kind)| *owner == id)
    })
}

pub(crate) fn is_auxiliary(handle: AnyWindowHandle, cx: &App) -> bool {
    cx.try_global::<AuxiliaryWindows>().is_some_and(|registry| {
        registry
            .handles
            .values()
            .any(|entry| entry.window_id() == handle.window_id())
    })
}

/// Bring an existing draft to the front before the caller can overwrite it.
pub(super) fn activate(id: EntityId, kind: AuxiliaryKind, cx: &mut App) -> bool {
    let key = (id, kind);
    let registry = cx.default_global::<AuxiliaryWindows>();
    if registry.pending.contains(&key) {
        return true;
    }
    let handle = registry.handles.get(&key).copied();
    if let Some(handle) = handle {
        if handle
            .update(cx, |_, window, _| window.activate_window())
            .is_ok()
        {
            cx.activate(true);
            return true;
        }
        cx.default_global::<AuxiliaryWindows>().handles.remove(&key);
    }
    false
}

pub(crate) fn open(app: Entity<ManisApp>, kind: AuxiliaryKind, cx: &mut App) {
    let key = (app.entity_id(), kind);
    if activate(key.0, kind, cx) {
        return;
    }
    cx.default_global::<AuxiliaryWindows>().pending.insert(key);
    // Opening is deferred because the launching event already borrows ManisApp.
    cx.defer(move |cx| {
        let title = app.read(cx).auxiliary_title(kind);
        let options = options(title, cx);
        let retained = app.clone();
        let result = cx.open_window(options, move |window, cx| {
            let closing = retained.downgrade();
            window.on_window_should_close(cx, move |_, cx| {
                let allowed = closing
                    .update(cx, |app, cx| app.cancel_auxiliary(kind, cx))
                    .unwrap_or(true);
                if allowed {
                    cx.default_global::<AuxiliaryWindows>().handles.remove(&key);
                }
                allowed
            });
            build_root(retained, kind, window, cx)
        });
        cx.default_global::<AuxiliaryWindows>().pending.remove(&key);
        match result {
            Ok(handle) => {
                cx.default_global::<AuxiliaryWindows>()
                    .handles
                    .insert(key, handle.into());
                cx.activate(true);
            }
            Err(error) => {
                app.update(cx, |app, cx| {
                    app.cancel_auxiliary(kind, cx);
                    app.status = format!(
                        "{}: {error}",
                        app.language().message(Message::Configuration)
                    );
                    cx.notify();
                });
                crate::diagnostics::record_event(
                    crate::diagnostics::LogLevel::Error,
                    "window.utility.open_failed",
                    error.to_string(),
                );
            }
        }
    });
}

fn build_root(
    retained: Entity<ManisApp>,
    kind: AuxiliaryKind,
    window: &mut Window,
    cx: &mut App,
) -> Entity<Root> {
    retained.update(cx, |app, cx| {
        let theme = app.theme();
        crate::theme::sync_component_theme(theme, app.dark, Some(window), cx);
        app.prepare_auxiliary(kind, window, cx);
    });
    let view = cx.new(|cx| {
        let focus = cx.focus_handle();
        let observation = cx.observe_in(&retained, window, |_, _, window, cx| {
            window.refresh();
            cx.notify();
        });
        AuxiliaryView {
            app: retained,
            kind,
            focus,
            _observation: observation,
        }
    });
    let focus = view.read(cx).focus.clone();
    focus.focus(window, cx);
    // Give text fields the initial focus after the window's own focus scope exists.
    let app = view.read(cx).app.clone();
    app.update(cx, |app, cx| app.focus_auxiliary(kind, window, cx));
    cx.new(|cx| Root::new(view, window, cx))
}

/// Render the production utility view in an offscreen window using synthetic data.
#[cfg(feature = "snapshot-fixtures")]
#[doc(hidden)]
pub fn fixture_root(
    app: Entity<ManisApp>,
    kind: AuxiliaryKind,
    dark: bool,
    window: &mut Window,
    cx: &mut App,
) -> Entity<Root> {
    app.update(cx, |app, cx| {
        app.dark = dark;
        app.prepare_auxiliary(kind, window, cx);
        match kind {
            AuxiliaryKind::ProxySource => app.open_new_subscription_editor(cx),
            AuxiliaryKind::Policy => app.start_managed_policy_create(cx),
            AuxiliaryKind::ManualRule => {
                app.manual_rule_editor_state = super::ManualRuleEditorState::Creating;
                app.manual_rule_condition_count = 1;
            }
            _ => {}
        }
    });
    build_root(app, kind, window, cx)
}

fn options(title: String, cx: &App) -> WindowOptions {
    let (width, height) = (720., 660.);
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
            None,
            size(px(width), px(height)),
            cx,
        ))),
        window_min_size: Some(size(px(480.), px(480.))),
        titlebar: Some(TitlebarOptions {
            title: Some(title.into()),
            appears_transparent: false,
            ..Default::default()
        }),
        window_background: WindowBackgroundAppearance::Opaque,
        ..Default::default()
    }
}

/// Footer actions already own success/cancel state. Only remove their presentation host.
pub(super) fn close_host(window: &mut Window, cx: &mut App) {
    let id = window.window_handle().window_id();
    let key = cx
        .default_global::<AuxiliaryWindows>()
        .handles
        .iter()
        .find_map(|(key, handle)| (handle.window_id() == id).then_some(*key));
    if let Some(key) = key {
        cx.default_global::<AuxiliaryWindows>().handles.remove(&key);
        window.remove_window();
    } else {
        window.close_dialog(cx);
    }
}

pub(super) struct EditorSections {
    pub header: AnyElement,
    pub body: AnyElement,
    pub footer: AnyElement,
}
impl EditorSections {
    pub fn new(header: impl IntoElement, body: impl IntoElement, footer: impl IntoElement) -> Self {
        Self {
            header: header.into_any_element(),
            body: body.into_any_element(),
            footer: footer.into_any_element(),
        }
    }
    fn panel(self) -> AnyElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(self.header)
            .child(self.body)
            .child(self.footer)
            .into_any_element()
    }
}

struct AuxiliaryView {
    app: Entity<ManisApp>,
    kind: AuxiliaryKind,
    focus: FocusHandle,
    _observation: Subscription,
}
impl Render for AuxiliaryView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let kind = self.kind;
        let app = self.app.clone();
        let (theme, content, title) = app.update(cx, |app, cx| {
            let theme = app.theme();
            let compact = WindowSizeClass::for_width(window.viewport_size().width.as_f32())
                == WindowSizeClass::Compact;
            let content = match kind {
                AuxiliaryKind::ProxySource => app
                    .proxy_source_editor_sections(theme, app.language(), window, cx)
                    .panel(),
                AuxiliaryKind::RuleSource => app
                    .qx_rule_source_editor_sections(theme, app.language(), window, cx)
                    .panel(),
                AuxiliaryKind::ManualRule => app
                    .manual_rule_editor_sections(theme, app.language(), compact, cx)
                    .panel(),
                AuxiliaryKind::Policy => {
                    app.set_policy_group_editor_enabled(
                        !app.managed_policies.mutation_state.is_busy(),
                        cx,
                    );
                    app.managed_policy_editor_sections(compact, app.language(), theme, cx)
                        .panel()
                }
            };
            (theme, content, app.auxiliary_title(kind))
        });
        window.set_window_title(&title);
        div()
            .id("manis-auxiliary-window")
            .debug_selector(|| "manis-auxiliary-window".into())
            .key_context("ManisWindow ManisAuxiliary ManisEditor")
            .track_focus(&self.focus)
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.surface_base)
            .text_color(theme.text_primary)
            .text_size(TextRole::Body.size())
            .line_height(TextRole::Body.line_height())
            .on_action(cx.listener(move |this, _: &CloseAuxiliary, window, cx| {
                if window.has_active_dialog(cx) {
                    if this.app.read(cx).configuration_transfer.active {
                        return;
                    }
                    window.close_dialog(cx);
                    return;
                }
                if this
                    .app
                    .update(cx, |app, cx| app.cancel_auxiliary(kind, cx))
                {
                    close_host(window, cx);
                }
            }))
            .child(div().flex_1().min_h(px(0.0)).child(content))
    }
}

impl ManisApp {
    fn auxiliary_title(&self, kind: AuxiliaryKind) -> String {
        let label = match kind {
            AuxiliaryKind::ProxySource => self
                .language()
                .localized(copy::configuration::PROXY_SOURCES),
            AuxiliaryKind::RuleSource => {
                self.language().localized(copy::configuration::RULE_SOURCES)
            }
            AuxiliaryKind::ManualRule => self.language().localized(copy::common::MANUAL_RULES),
            AuxiliaryKind::Policy => self.language().localized(copy::common::EDIT_POLICY_GROUP),
        };
        format!("{label} · Manis")
    }
    fn prepare_auxiliary(
        &mut self,
        kind: AuxiliaryKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let theme = self.theme();
        // Inputs subscribe once; never replace the main window's resize subscription.
        self.sync_window_inputs(window, cx);
        match kind {
            AuxiliaryKind::ManualRule => self.ensure_manual_rule_input(theme, window, cx),
            AuxiliaryKind::Policy => self.ensure_policy_group_inputs(theme, window, cx),
            _ => {}
        }
    }
    fn focus_auxiliary(&self, kind: AuxiliaryKind, window: &mut Window, cx: &mut Context<Self>) {
        let input = match kind {
            AuxiliaryKind::ProxySource => self.proxy_source_editor.name_input.as_ref(),
            AuxiliaryKind::RuleSource => self.inputs.qx_rule_name.as_ref(),
            AuxiliaryKind::ManualRule => self
                .manual_rule_conditions
                .first()
                .map(|condition| &condition.input),
            AuxiliaryKind::Policy => self.inputs.policy_group_name.as_ref(),
        };
        if let Some(input) = input {
            input.focus_handle(cx).focus(window, cx);
        }
    }
    fn cancel_auxiliary(&mut self, kind: AuxiliaryKind, cx: &mut Context<Self>) -> bool {
        match kind {
            AuxiliaryKind::ProxySource if self.proxy_source_editor.is_importing() => return false,
            AuxiliaryKind::RuleSource
                if self.rule_sources.feedback == QxRuleImportFeedback::Importing =>
            {
                return false;
            }
            AuxiliaryKind::Policy if self.managed_policies.mutation_state.is_busy() => {
                return false;
            }
            AuxiliaryKind::ProxySource => self.close_subscription_editor(cx),
            AuxiliaryKind::RuleSource => self.close_qx_rule_editor(cx),
            AuxiliaryKind::ManualRule => self.close_manual_rule_editor(cx),
            AuxiliaryKind::Policy => self.close_managed_policy_editor(cx),
        }
        true
    }
}

#[cfg(test)]
mod tests;
