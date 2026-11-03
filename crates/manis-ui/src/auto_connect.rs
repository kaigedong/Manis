//! Remembers whether opening Manis should start the managed kernel.
//!
//! The choice lives in `config.toml` next to the other startup preferences, so a user who prefers
//! a manual start keeps it across updates and a fresh install gets the familiar client behaviour:
//! open the app and the proxy is already running.

use std::path::{Path, PathBuf};

use crate::diagnostics::{LogLevel, record_event};

const PREFERENCE_FILE: &str = "startup.auto_connect";
const MAX_PREFERENCE_BYTES: u64 = 16;
const DEFAULT_AUTO_CONNECT: bool = true;
const ENABLED_VALUE: &str = "enabled";
const DISABLED_VALUE: &str = "disabled";

/// Reads the saved choice, defaulting to enabled on first use.
#[must_use]
pub(crate) fn load_preference(directory: Option<&Path>) -> bool {
    directory.map_or(DEFAULT_AUTO_CONNECT, load_preference_in)
}

fn load_preference_in(directory: &Path) -> bool {
    match crate::config_toml::read_entry(directory, PREFERENCE_FILE, MAX_PREFERENCE_BYTES) {
        Ok(Some(value)) => match value.trim() {
            ENABLED_VALUE => true,
            DISABLED_VALUE => false,
            _ => {
                record_event(
                    LogLevel::Warn,
                    "auto_connect.preference.invalid",
                    "saved startup value is not recognized",
                );
                DEFAULT_AUTO_CONNECT
            }
        },
        Ok(None) => DEFAULT_AUTO_CONNECT,
        Err(error) => {
            record_event(
                LogLevel::Warn,
                "auto_connect.preference.read_failed",
                error.to_string(),
            );
            DEFAULT_AUTO_CONNECT
        }
    }
}

pub(crate) fn save_preference_in(
    directory: &Path,
    enabled: bool,
) -> Result<PathBuf, crate::config_toml::ConfigTomlError> {
    let value = if enabled {
        ENABLED_VALUE
    } else {
        DISABLED_VALUE
    };
    crate::config_toml::write_entry(directory, PREFERENCE_FILE, &format!("{value}\n"))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{load_preference_in, save_preference_in};

    fn scratch(name: &str) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("manis-auto-connect-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("scratch dir");
        path
    }

    #[test]
    fn round_trips_the_saved_choice() {
        let directory = scratch("round-trip");
        assert!(load_preference_in(&directory), "default is enabled");

        save_preference_in(&directory, false).expect("save disabled");
        assert!(!load_preference_in(&directory));

        save_preference_in(&directory, true).expect("save enabled");
        assert!(load_preference_in(&directory));
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn falls_back_to_enabled_when_the_value_is_unknown() {
        let directory = scratch("unknown");
        crate::config_toml::write_entry(&directory, "startup.auto_connect", "7\n")
            .expect("write entry");
        assert!(load_preference_in(&directory));
        let _ = std::fs::remove_dir_all(&directory);
    }
}
