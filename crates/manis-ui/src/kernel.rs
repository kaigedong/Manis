use std::fs;
use std::io::Read as _;
use std::ops::Deref;
use std::path::{Path, PathBuf};

use manis_core::{KernelCapabilities, KernelKind};
use manis_profile::write_private_atomic;

use crate::localization::{Language, Message, copy};
use crate::mihomo::{self, ControllerRuntime, ControllerState, LoadError, RuntimeSnapshot};

const KERNEL_SELECTION_FILE: &str = "kernel.kind";
const MAX_KERNEL_SELECTION_BYTES: u64 = 32;

/// The UI-facing runtime boundary. Mihomo remains the default adapter while sing-box support is
/// introduced behind the same product-level capability surface.
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
    pub(crate) fn configured(store_dir: Option<&Path>, language: Language) -> Self {
        let kind = match store_dir {
            Some(directory) => match load_kernel_kind_in(directory) {
                Ok(kind) => kind,
                Err(error) => {
                    return Self {
                        kind: KernelKind::default(),
                        controller: ControllerRuntime::Invalid {
                            message: error.message(language).to_owned(),
                        },
                    };
                }
            },
            None => KernelKind::default(),
        };
        match Self::prepare_with_language(kind, store_dir, language) {
            Ok(runtime) => runtime,
            Err(message) => Self {
                kind,
                controller: ControllerRuntime::Invalid { message },
            },
        }
    }

    pub(crate) fn prepare_with_language(
        kind: KernelKind,
        store_dir: Option<&Path>,
        language: Language,
    ) -> Result<Self, String> {
        let controller = match kind {
            KernelKind::Mihomo => mihomo::configured_runtime(store_dir),
            KernelKind::SingBox => {
                let store_dir = store_dir.ok_or_else(|| {
                    language
                        .localized(copy::kernel::CANNOT_PREPARE_SING_BOX_WITHOUT_A_SOURCE_DIRECTORY)
                        .to_owned()
                })?;
                mihomo::configured_sing_box_runtime(store_dir)?
            }
        };
        Ok(Self { kind, controller })
    }

    #[must_use]
    pub(crate) const fn kind(&self) -> KernelKind {
        self.kind
    }

    #[must_use]
    pub(crate) const fn capabilities(&self) -> KernelCapabilities {
        self.kind.capabilities()
    }

    pub(crate) fn stop_managed(&self) -> Result<(), String> {
        self.controller
            .stop_managed()
            .map_err(|error| error.to_string())
    }

    pub(crate) fn connect(&self) -> Result<RuntimeSnapshot, LoadError> {
        match self.kind {
            KernelKind::Mihomo => self.controller.connect(),
            KernelKind::SingBox => self.controller.connect_sing_box(),
        }
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
                match (self.kind, &self.controller) {
                    (KernelKind::Mihomo, ControllerRuntime::Managed { .. }) => {
                        language.localized(copy::kernel::START_MIHOMO)
                    }
                    (KernelKind::SingBox, ControllerRuntime::Managed { .. }) => {
                        language.localized(copy::kernel::START_SING_BOX)
                    }
                    #[cfg(any(test, feature = "snapshot-fixtures"))]
                    (
                        KernelKind::Mihomo,
                        ControllerRuntime::Fixture { .. } | ControllerRuntime::Invalid { .. },
                    ) => language.message(Message::ConnectMihomo),
                    #[cfg(not(any(test, feature = "snapshot-fixtures")))]
                    (KernelKind::Mihomo, ControllerRuntime::Invalid { .. }) => {
                        language.message(Message::ConnectMihomo)
                    }
                    #[cfg(any(test, feature = "snapshot-fixtures"))]
                    (
                        KernelKind::SingBox,
                        ControllerRuntime::Fixture { .. } | ControllerRuntime::Invalid { .. },
                    ) => language.localized(copy::kernel::CONNECT_SING_BOX),
                    #[cfg(not(any(test, feature = "snapshot-fixtures")))]
                    (KernelKind::SingBox, ControllerRuntime::Invalid { .. }) => {
                        language.localized(copy::kernel::CONNECT_SING_BOX)
                    }
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

pub(crate) fn load_kernel_kind_in(directory: &Path) -> Result<KernelKind, KernelSelectionError> {
    let path = directory.join(KERNEL_SELECTION_FILE);
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(KernelKind::Mihomo);
        }
        Err(_error) => return Err(KernelSelectionError::Unavailable),
    };
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.len() > MAX_KERNEL_SELECTION_BYTES
    {
        return Err(KernelSelectionError::UnsafeFile);
    }
    let file = fs::File::open(&path).map_err(|_error| KernelSelectionError::Unavailable)?;
    let opened = file
        .metadata()
        .map_err(|_error| KernelSelectionError::Unavailable)?;
    if !opened.is_file()
        || opened.len() > MAX_KERNEL_SELECTION_BYTES
        || !same_file(&metadata, &opened)
    {
        return Err(KernelSelectionError::UnsafeFile);
    }
    let mut value = String::new();
    file.take(MAX_KERNEL_SELECTION_BYTES + 1)
        .read_to_string(&mut value)
        .map_err(|_error| KernelSelectionError::Unavailable)?;
    if value.len() as u64 > MAX_KERNEL_SELECTION_BYTES {
        return Err(KernelSelectionError::UnsafeFile);
    }
    KernelKind::parse(value.trim_end_matches(['\r', '\n']))
        .ok_or(KernelSelectionError::InvalidValue)
}

#[cfg(unix)]
fn same_file(expected: &fs::Metadata, opened: &fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt as _;

    expected.dev() == opened.dev() && expected.ino() == opened.ino()
}

#[cfg(not(unix))]
fn same_file(_expected: &fs::Metadata, _opened: &fs::Metadata) -> bool {
    true
}

pub(crate) fn save_kernel_kind_in(
    directory: &Path,
    kind: KernelKind,
) -> Result<PathBuf, KernelSelectionError> {
    let contents = format!("{}\n", kind.persistence_key());
    write_private_atomic(directory, KERNEL_SELECTION_FILE, contents.as_bytes())
        .map_err(|_error| KernelSelectionError::Unavailable)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum KernelSelectionError {
    Unavailable,
    UnsafeFile,
    InvalidValue,
}

impl std::fmt::Display for KernelSelectionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.message(Language::English))
    }
}

impl std::error::Error for KernelSelectionError {}

impl KernelSelectionError {
    #[must_use]
    pub(crate) fn message(self, language: Language) -> &'static str {
        match self {
            Self::Unavailable => {
                language.localized(copy::kernel::KERNEL_SELECTION_COULD_NOT_BE_READ_OR_SAVED)
            }
            Self::UnsafeFile => {
                language.localized(copy::kernel::KERNEL_SELECTION_IS_NOT_A_SAFE_REGULAR_FILE)
            }
            Self::InvalidValue => {
                language.localized(copy::kernel::KERNEL_SELECTION_CONTAINS_AN_UNKNOWN_VALUE)
            }
        }
    }
}

#[cfg(all(test, not(windows)))]
mod tests {
    use std::fs;

    use manis_core::KernelKind;

    use super::{load_kernel_kind_in, save_kernel_kind_in};

    #[test]
    fn kernel_selection_defaults_to_mihomo_and_round_trips() {
        let root =
            std::env::temp_dir().join(format!("manis-kernel-selection-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);

        assert_eq!(
            load_kernel_kind_in(&root).expect("missing selection uses compatibility default"),
            KernelKind::Mihomo
        );
        save_kernel_kind_in(&root, KernelKind::SingBox).expect("selection should persist");
        assert_eq!(
            load_kernel_kind_in(&root).expect("selection should load"),
            KernelKind::SingBox
        );

        fs::remove_dir_all(root).expect("remove fixture");
    }

    #[test]
    fn configured_runtime_surfaces_an_invalid_saved_selection() {
        let root = std::env::temp_dir().join(format!(
            "manis-kernel-selection-invalid-{}",
            std::process::id()
        ));
        if root.exists() {
            fs::remove_dir_all(&root).expect("remove stale fixture");
        }
        fs::create_dir_all(&root).expect("create fixture directory");
        fs::write(root.join(super::KERNEL_SELECTION_FILE), "unknown\n")
            .expect("write invalid selection");

        let runtime =
            super::KernelRuntime::configured(Some(&root), crate::localization::Language::English);

        assert!(matches!(
            runtime.controller,
            crate::mihomo::ControllerRuntime::Invalid { .. }
        ));
        fs::remove_dir_all(root).expect("remove fixture");
    }
}
