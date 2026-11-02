//! Headless Manis engine.
//!
//! The NSH shell is a pure client: it never owns the Mihomo process. This
//! module starts the managed kernel from the saved workspace, keeps the saved
//! remote sources fresh, and applies a refreshed profile to the running
//! controller. It is the gpui-free replacement for `manis-ui --headless`.

use std::fmt;
use std::path::Path;
use std::thread;
use std::time::Duration;

use crate::kernel::KernelRuntime;
use crate::mihomo;
use crate::{core_update, diagnostics, rule_source};

/// How often the engine checks saved remote sources for a due refresh.
const REFRESH_TICK: Duration = Duration::from_secs(60);

/// Failure that stops the headless engine before it serves a controller.
#[derive(Debug)]
pub struct EngineError(String);

impl fmt::Display for EngineError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for EngineError {}

/// Starts the managed kernel and serves it until the process is stopped.
///
/// # Errors
/// Returns an error when the saved profile is invalid or the kernel cannot be
/// started, so the calling shell can report why no controller appeared.
pub fn run() -> Result<(), EngineError> {
    let store = mihomo::imported_subscription_store_dir().ok();
    diagnostics::initialize(store.as_deref().and_then(Path::parent));
    match core_update::install_bundled_seed_if_missing() {
        Ok(core_update::SeedInstallOutcome::Installed(path)) => diagnostics::record_event(
            diagnostics::LogLevel::Info,
            "core.seed.installed",
            format!("path={}", path.display()),
        ),
        Ok(
            core_update::SeedInstallOutcome::AlreadyPresent(_)
            | core_update::SeedInstallOutcome::MissingSeed { .. },
        ) => {}
        Err(error) => diagnostics::record_event(
            diagnostics::LogLevel::Warn,
            "core.seed.failed",
            error.to_string(),
        ),
    }

    let runtime = KernelRuntime::configured(store.as_deref());
    runtime
        .connect()
        .map_err(|error| EngineError(error.to_string()))?;
    diagnostics::record_event(
        diagnostics::LogLevel::Info,
        "engine.started",
        runtime.endpoint_label(),
    );

    let Some(store) = store else {
        return Err(EngineError("找不到 Manis 数据目录".to_owned()));
    };
    loop {
        thread::sleep(REFRESH_TICK);
        refresh_due_sources(&runtime, &store);
    }
}

/// Refreshes every saved remote source whose interval elapsed and re-applies
/// the compiled profile once. A failed download stays due for the next tick.
fn refresh_due_sources(runtime: &KernelRuntime, store: &Path) {
    let now = mihomo::current_unix_secs();
    let mut changed = false;

    if let Ok(subscriptions) = mihomo::load_subscription_sources_in(store) {
        for source in subscriptions {
            if !source.enabled
                || !source
                    .refresh_interval
                    .is_due(source.last_successful_update_unix_secs, now)
            {
                continue;
            }
            let nameservers = mihomo::discover_subscription_proxy_nameservers(&source.source);
            if mihomo::preview_imported_subscription(&source.source).is_err() {
                continue;
            }
            if mihomo::mark_subscription_source_update_success_in(store, &source.id, now).is_err() {
                continue;
            }
            if !nameservers.is_empty() {
                let _ = mihomo::update_subscription_source_proxy_nameservers_in(
                    store,
                    &source.id,
                    &nameservers,
                );
            }
            changed = true;
        }
    }

    if let Ok(sources) = mihomo::load_qx_rule_sources_in(store) {
        for source in sources {
            if !source.enabled
                || !source
                    .refresh_interval
                    .is_due(source.last_successful_update_unix_secs, now)
            {
                continue;
            }
            let Ok(content) = rule_source::download_qx_rule_document_secret(&source.source) else {
                continue;
            };
            if manis_profile::QxRuleList::parse(&content).rules.is_empty() {
                continue;
            }
            if mihomo::replace_qx_rule_source_content_in(store, &source.id, &content, now).is_ok() {
                changed = true;
            }
        }
    }

    if changed {
        match runtime.apply_saved_sources(store) {
            Ok(_) => diagnostics::record_event(
                diagnostics::LogLevel::Info,
                "engine.sources.refreshed",
                "profile re-applied",
            ),
            Err(error) => diagnostics::record_event(
                diagnostics::LogLevel::Warn,
                "engine.sources.refresh_failed",
                error.to_string(),
            ),
        }
    }
}
