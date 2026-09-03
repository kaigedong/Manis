use std::ops::Deref;
use std::path::Path;

use crate::localization::{Language, Message, copy};
use crate::mihomo::{self, ControllerRuntime, ControllerState, LoadError, RuntimeSnapshot};
use manis_core::KernelKind;

/// The UI-facing runtime boundary for the Manis-managed Mihomo process.
#[derive(Clone)]
pub(crate) struct KernelRuntime {
    kind: KernelKind,
    controller: ControllerRuntime,
}

impl KernelRuntime {
    #[must_use]
    #[cfg(any(test, feature = "snapshot-fixtures"))]
    pub(crate) fn mihomo(controller: ControllerRuntime) -> Self {
        Self {
            kind: KernelKind::Mihomo,
            controller,
        }
    }

    #[must_use]
    pub(crate) fn configured(store_dir: Option<&Path>, _language: Language) -> Self {
        Self::prepare_with_language(store_dir)
    }

    pub(crate) fn prepare_with_language(store_dir: Option<&Path>) -> Self {
        Self {
            kind: KernelKind::Mihomo,
            controller: mihomo::configured_runtime(store_dir),
        }
    }

    #[must_use]
    pub(crate) const fn kind(&self) -> KernelKind {
        self.kind
    }

    pub(crate) fn stop_managed(&self) -> Result<(), String> {
        self.controller
            .stop_managed()
            .map_err(|error| error.to_string())
    }

    pub(crate) fn connect(&self) -> Result<RuntimeSnapshot, LoadError> {
        self.controller.connect()
    }

    #[must_use]
    pub(crate) fn initial_status_in(&self, language: Language) -> String {
        match &self.controller {
            #[cfg(any(test, feature = "snapshot-fixtures"))]
            ControllerRuntime::Fixture { .. } => format!(
                "{} {}",
                language.localized(copy::kernel::NOT_CONNECTED_TO),
                self.kind.display_name()
            ),
            ControllerRuntime::Managed { .. } => format!(
                "{} {}",
                language.localized(copy::kernel::MANAGED_KERNEL_CONFIGURED_CLICK_TO_START),
                self.kind.display_name()
            ),
            ControllerRuntime::Invalid { message } => format!(
                "{} {}: {message}",
                self.kind.display_name(),
                language.localized(copy::kernel::CONFIGURATION_IS_INVALID)
            ),
        }
    }

    #[must_use]
    pub(crate) fn button_label_in(
        &self,
        state: &ControllerState,
        language: Language,
    ) -> &'static str {
        match state {
            ControllerState::Connecting { .. } => language.localized(copy::kernel::CONNECTING),
            ControllerState::Connected { .. } => language.localized(copy::kernel::REFRESH),
            ControllerState::Disconnected | ControllerState::Failed { .. } => {
                match &self.controller {
                    ControllerRuntime::Managed { .. } => {
                        language.localized(copy::kernel::START_MIHOMO)
                    }
                    #[cfg(any(test, feature = "snapshot-fixtures"))]
                    ControllerRuntime::Fixture { .. } | ControllerRuntime::Invalid { .. } => {
                        language.message(Message::ConnectMihomo)
                    }
                    #[cfg(not(any(test, feature = "snapshot-fixtures")))]
                    ControllerRuntime::Invalid { .. } => language.message(Message::ConnectMihomo),
                }
            }
        }
    }
}

impl Deref for KernelRuntime {
    type Target = ControllerRuntime;

    fn deref(&self) -> &Self::Target {
        &self.controller
    }
}
