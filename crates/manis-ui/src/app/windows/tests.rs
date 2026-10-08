use super::*;
use gpui::VisualTestContext;
use manis_core::PrimaryWorkspace;

fn fixture(cx: &mut gpui::TestAppContext) -> (Entity<ManisApp>, AnyWindowHandle) {
    cx.update(crate::init);
    let app = cx.new(|_| ManisApp::with_fixture_controller("http://127.0.0.1:1"));
    let retained = app.clone();
    let window = cx.add_window(|window, cx| crate::root(retained, window, cx));
    (app, window.into())
}

#[gpui::test]
fn settings_is_singleton_and_closes_without_changing_the_main_workspace(
    cx: &mut gpui::TestAppContext,
) {
    let (app, main) = fixture(cx);
    let mut main_context = VisualTestContext::from_window(main, cx);
    main_context.update(|window, cx| window.draw(cx).clear(cx));
    main_context.simulate_keystrokes("cmd-,");
    cx.update(|cx| {
        open(app.clone(), AuxiliaryKind::Settings, cx);
        open(app.clone(), AuxiliaryKind::Settings, cx);
    });
    cx.run_until_parked();
    let handle = cx.read(|cx| {
        cx.global::<AuxiliaryWindows>().handles[&(app.entity_id(), AuxiliaryKind::Settings)]
    });
    assert_eq!(cx.windows().len(), 2);
    assert!(cx.read(|cx| is_auxiliary(handle, cx)));
    assert!(!cx.read(|cx| is_auxiliary(main, cx)));
    assert_eq!(
        app.read_with(cx, |app, _| app.primary_workspace),
        PrimaryWorkspace::Nodes
    );
    let mut window = VisualTestContext::from_window(handle, cx);
    window.update(|window, cx| window.draw(cx).clear(cx));
    assert!(window.debug_bounds("manis-auxiliary-window").is_some());
    // The native close button must preserve the configuration dialog's host.
    app.update(cx, |app, _| app.configuration_transfer.active = true);
    assert!(!window.simulate_close());
    window.simulate_keystrokes("cmd-w");
    assert_eq!(cx.windows().len(), 2);
    app.update(cx, |app, _| app.configuration_transfer.active = false);
    window.simulate_keystrokes("cmd-w");
    cx.run_until_parked();
    assert_eq!(cx.windows().len(), 1);
    assert_eq!(cx.windows()[0].window_id(), main.window_id());
    cx.update(|cx| open(app.clone(), AuxiliaryKind::Settings, cx));
    cx.run_until_parked();
    assert_eq!(cx.windows().len(), 2);
}

#[gpui::test]
fn native_editor_reactivation_preserves_draft_and_cancel_discards_it(
    cx: &mut gpui::TestAppContext,
) {
    let (app, _) = fixture(cx);
    app.update(cx, ManisApp::open_new_subscription_editor);
    cx.update(|cx| open(app.clone(), AuxiliaryKind::ProxySource, cx));
    cx.run_until_parked();
    let name = app.read_with(cx, |app, _| {
        app.proxy_source_editor.name_input.clone().unwrap()
    });
    name.update(cx, |input, cx| {
        input.set_value_without_event("未保存的来源", cx);
    });
    app.update(cx, ManisApp::open_new_subscription_editor);
    assert_eq!(
        name.read_with(cx, |input, _| input.value().to_owned()),
        "未保存的来源"
    );
    assert_eq!(cx.windows().len(), 2);
    let handle = cx.read(|cx| {
        cx.global::<AuxiliaryWindows>().handles[&(app.entity_id(), AuxiliaryKind::ProxySource)]
    });
    let mut window = VisualTestContext::from_window(handle, cx);
    window.update(|window, cx| window.draw(cx).clear(cx));
    let cancel = window
        .debug_bounds("cancel-proxy-source")
        .expect("native footer renders");
    window.simulate_click(cancel.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    assert_eq!(cx.windows().len(), 1);
    assert_eq!(name.read_with(cx, |input, _| input.value().to_owned()), "");
}

#[gpui::test]
fn native_long_editors_render_and_close_with_keyboard(cx: &mut gpui::TestAppContext) {
    let (app, _) = fixture(cx);
    for kind in [
        AuxiliaryKind::RuleSource,
        AuxiliaryKind::ManualRule,
        AuxiliaryKind::Policy,
    ] {
        app.update(cx, |app, cx| {
            if kind == AuxiliaryKind::Policy {
                app.start_managed_policy_create(cx);
            }
            if kind == AuxiliaryKind::ManualRule {
                app.manual_rule_editor_state = super::super::ManualRuleEditorState::Creating;
            }
        });
        cx.update(|cx| open(app.clone(), kind, cx));
        cx.run_until_parked();
        let handle =
            cx.read(|cx| cx.global::<AuxiliaryWindows>().handles[&(app.entity_id(), kind)]);
        let mut window = VisualTestContext::from_window(handle, cx);
        window.update(|window, cx| window.draw(cx).clear(cx));
        let footer = match kind {
            AuxiliaryKind::RuleSource => "save-qx-rule-source",
            AuxiliaryKind::ManualRule => "save-manual-rule",
            AuxiliaryKind::Policy => "save-managed-policy-dialog",
            _ => unreachable!(),
        };
        let bounds = window.debug_bounds(footer).expect(footer);
        window.update(|window, _| assert!(bounds.bottom() <= window.viewport_size().height));
        window.dispatch_action(CloseAuxiliary);
        cx.run_until_parked();
        assert_eq!(cx.windows().len(), 1);
    }
}

#[gpui::test]
fn import_in_progress_prevents_native_editor_cancellation(cx: &mut gpui::TestAppContext) {
    let (app, _) = fixture(cx);
    app.update(cx, |app, cx| {
        app.proxy_source_editor.feedback = super::super::SubscriptionFeedback::Importing(
            crate::subscription::SourceKind::HttpsSubscription,
        );
        assert!(!app.cancel_auxiliary(AuxiliaryKind::ProxySource, cx));
        app.rule_sources.feedback = QxRuleImportFeedback::Importing;
        assert!(!app.cancel_auxiliary(AuxiliaryKind::RuleSource, cx));
        app.close_qx_rule_editor(cx);
        assert_eq!(app.rule_sources.feedback, QxRuleImportFeedback::Importing);
    });
}

#[gpui::test]
fn native_close_button_preserves_an_inflight_import(cx: &mut gpui::TestAppContext) {
    let (app, _) = fixture(cx);
    cx.update(|cx| open(app.clone(), AuxiliaryKind::ProxySource, cx));
    cx.run_until_parked();
    let handle = cx.read(|cx| {
        cx.global::<AuxiliaryWindows>().handles[&(app.entity_id(), AuxiliaryKind::ProxySource)]
    });
    let mut window = VisualTestContext::from_window(handle, cx);
    window.update(|window, cx| window.draw(cx).clear(cx));
    app.update(cx, |app, _| {
        app.proxy_source_editor.feedback = super::super::SubscriptionFeedback::Importing(
            crate::subscription::SourceKind::HttpsSubscription,
        );
    });
    assert!(!window.simulate_close());
    window.simulate_keystrokes("cmd-w");
    assert_eq!(cx.windows().len(), 2);
    app.update(cx, |app, _| {
        app.proxy_source_editor.feedback = super::super::SubscriptionFeedback::Idle;
    });
    assert!(window.simulate_close());
    assert!(!cx.read(|cx| has_editors(app.entity_id(), cx)));
}
