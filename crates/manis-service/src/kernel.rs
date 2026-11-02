use std::ops::Deref;
use std::path::Path;

use crate::mihomo::{self, ControllerRuntime, LoadError, RuntimeSnapshot};
use manis_core::KernelKind;

/// The engine-facing runtime boundary for the Manis-managed Mihomo process.
#[derive(Clone)]
pub(crate) struct KernelRuntime {
    kind: KernelKind,
    controller: ControllerRuntime,
}

impl KernelRuntime {
    #[must_use]
    pub(crate) fn configured(store_dir: Option<&Path>) -> Self {
        Self {
            kind: KernelKind::Mihomo,
            controller: mihomo::configured_runtime(store_dir),
        }
    }

    pub(crate) fn connect(&self) -> Result<RuntimeSnapshot, LoadError> {
        self.controller.connect()
    }
}

impl Deref for KernelRuntime {
    type Target = ControllerRuntime;

    fn deref(&self) -> &Self::Target {
        &self.controller
    }
}
