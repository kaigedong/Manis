use std::{
    ffi::OsString,
    fmt,
    path::{Path, PathBuf},
};

/// A fully resolved process command without shell interpolation.
#[derive(Clone, PartialEq, Eq)]
pub struct CommandSpec {
    program: PathBuf,
    args: Vec<OsString>,
    current_dir: PathBuf,
}

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
            .finish()
    }
}

impl CommandSpec {
    pub(crate) fn new(program: PathBuf, args: Vec<OsString>, current_dir: PathBuf) -> Self {
        Self {
            program,
            args,
            current_dir,
        }
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
