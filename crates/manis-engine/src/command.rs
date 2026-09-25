use std::{
    ffi::OsString,
    fmt,
    path::{Path, PathBuf},
};
#[cfg(target_os = "android")]
use std::{
    os::fd::{AsFd, AsRawFd, OwnedFd},
    sync::Arc,
};

/// A fully resolved process command without shell interpolation.
#[derive(Clone)]
pub struct CommandSpec {
    program: PathBuf,
    args: Vec<OsString>,
    current_dir: PathBuf,
    #[cfg(target_os = "android")]
    inherited_fd: Option<Arc<OwnedFd>>,
}

impl PartialEq for CommandSpec {
    fn eq(&self, other: &Self) -> bool {
        self.program == other.program
            && self.args == other.args
            && self.current_dir == other.current_dir
            && {
                #[cfg(target_os = "android")]
                {
                    self.inherited_fd.as_ref().map(|fd| fd.as_fd().as_raw_fd())
                        == other.inherited_fd.as_ref().map(|fd| fd.as_fd().as_raw_fd())
                }
                #[cfg(not(target_os = "android"))]
                {
                    true
                }
            }
    }
}

impl Eq for CommandSpec {}

impl fmt::Debug for CommandSpec {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut redact_next = false;
        let args = self
            .args
            .iter()
            .map(|argument| {
                if redact_next {
                    redact_next = false;
                    return "<redacted>".to_owned();
                }
                let argument = argument.to_string_lossy().into_owned();
                redact_next = argument == "-secret";
                argument
            })
            .collect::<Vec<_>>();
        formatter
            .debug_struct("CommandSpec")
            .field("program", &self.program)
            .field("args", &args)
            .field("current_dir", &self.current_dir)
            .field("has_inherited_fd", &{
                #[cfg(target_os = "android")]
                {
                    self.inherited_fd.is_some()
                }
                #[cfg(not(target_os = "android"))]
                {
                    false
                }
            })
            .finish()
    }
}

impl CommandSpec {
    pub(crate) fn new(program: PathBuf, args: Vec<OsString>, current_dir: PathBuf) -> Self {
        Self {
            program,
            args,
            current_dir,
            #[cfg(target_os = "android")]
            inherited_fd: None,
        }
    }

    #[cfg(target_os = "android")]
    pub(crate) fn with_inherited_fd(mut self, fd: Arc<OwnedFd>) -> Self {
        self.inherited_fd = Some(fd);
        self
    }

    #[cfg(target_os = "android")]
    pub(crate) fn inherited_fd(&self) -> Option<&Arc<OwnedFd>> {
        self.inherited_fd.as_ref()
    }

    /// Returns the executable path.
    #[must_use]
    pub fn program(&self) -> &Path {
        &self.program
    }

    /// Returns arguments passed directly to the executable.
    #[must_use]
    pub fn args(&self) -> &[OsString] {
        &self.args
    }

    /// Returns the private working directory used for this command.
    #[must_use]
    pub fn current_dir(&self) -> &Path {
        &self.current_dir
    }
}
