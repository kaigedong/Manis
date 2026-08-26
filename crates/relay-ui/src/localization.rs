use std::fs;
use std::path::{Path, PathBuf};
#[cfg(any(target_os = "macos", target_os = "windows"))]
use std::process::Command;

use relay_profile::write_private_atomic;

const LANGUAGE_PREFERENCE_FILE: &str = "language.preference";
const MAX_LANGUAGE_PREFERENCE_BYTES: u64 = 32;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum LanguagePreference {
    #[default]
    FollowSystem,
    English,
    SimplifiedChinese,
}

impl LanguagePreference {
    #[must_use]
    pub(crate) const fn persistence_key(self) -> &'static str {
        match self {
            Self::FollowSystem => "system",
            Self::English => "en",
            Self::SimplifiedChinese => "zh-CN",
        }
    }

    #[must_use]
    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "system" => Some(Self::FollowSystem),
            "en" => Some(Self::English),
            "zh-CN" => Some(Self::SimplifiedChinese),
            _ => None,
        }
    }

    #[must_use]
    pub(crate) fn resolve(self, system_locale: Option<&str>) -> Language {
        match self {
            Self::English => Language::English,
            Self::SimplifiedChinese => Language::SimplifiedChinese,
            Self::FollowSystem => Language::from_locale(system_locale),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum Language {
    #[default]
    English,
    SimplifiedChinese,
}

impl Language {
    #[must_use]
    pub(crate) fn from_locale(locale: Option<&str>) -> Self {
        let Some(locale) = locale else {
            return Self::English;
        };
        let language = locale
            .trim()
            .split(['-', '_', '.', '@'])
            .next()
            .unwrap_or_default();
        if language.eq_ignore_ascii_case("zh") {
            Self::SimplifiedChinese
        } else {
            Self::English
        }
    }

    #[must_use]
    pub(crate) const fn text(self, english: &'static str, chinese: &'static str) -> &'static str {
        match self {
            Self::English => english,
            Self::SimplifiedChinese => chinese,
        }
    }

    #[must_use]
    pub(crate) const fn display_name(self) -> &'static str {
        match self {
            Self::English => "English",
            Self::SimplifiedChinese => "中文",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Localizer {
    preference: LanguagePreference,
    system_locale: Option<String>,
}

impl Localizer {
    #[must_use]
    pub(crate) fn load(directory: Option<&Path>) -> Self {
        let preference = directory
            .and_then(|directory| load_language_preference_in(directory).ok())
            .unwrap_or_default();
        Self {
            preference,
            system_locale: detect_system_locale(),
        }
    }

    #[must_use]
    pub(crate) const fn preference(&self) -> LanguagePreference {
        self.preference
    }

    #[must_use]
    pub(crate) fn language(&self) -> Language {
        self.preference.resolve(self.system_locale.as_deref())
    }

    pub(crate) fn set_preference(&mut self, preference: LanguagePreference) {
        self.preference = preference;
    }
}

#[must_use]
pub(crate) fn detect_system_locale() -> Option<String> {
    platform_locale().or_else(environment_locale)
}

fn environment_locale() -> Option<String> {
    ["LANGUAGE", "LC_ALL", "LC_MESSAGES", "LANG"]
        .into_iter()
        .filter_map(|key| std::env::var(key).ok())
        .flat_map(|value| value.split(':').map(str::to_owned).collect::<Vec<_>>())
        .find(|value| !value.trim().is_empty() && !is_neutral_locale(value))
}

fn is_neutral_locale(locale: &str) -> bool {
    matches!(
        locale
            .trim()
            .split(['.', '@'])
            .next()
            .unwrap_or_default()
            .to_ascii_uppercase()
            .as_str(),
        "C" | "POSIX"
    )
}

#[cfg(target_os = "macos")]
fn platform_locale() -> Option<String> {
    let output = Command::new("/usr/bin/defaults")
        .args(["read", "-g", "AppleLanguages"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    parse_apple_languages(&String::from_utf8(output.stdout).ok()?)
}

#[cfg(target_os = "macos")]
fn parse_apple_languages(value: &str) -> Option<String> {
    value
        .split(|character: char| {
            character.is_whitespace() || matches!(character, '(' | ')' | ',' | '"' | '\'')
        })
        .map(str::trim)
        .find(|candidate| {
            !candidate.is_empty()
                && candidate.chars().any(char::is_alphabetic)
                && !matches!(*candidate, "(" | ")")
        })
        .map(str::to_owned)
}

#[cfg(target_os = "windows")]
fn platform_locale() -> Option<String> {
    use std::os::windows::process::CommandExt as _;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let output = Command::new("powershell.exe")
        .args(["-NoProfile", "-Command", "(Get-UICulture).Name"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout)
        .ok()
        .map(|locale| locale.trim().to_owned())
        .filter(|locale| !locale.is_empty())
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn platform_locale() -> Option<String> {
    None
}

pub(crate) fn load_language_preference_in(
    directory: &Path,
) -> Result<LanguagePreference, LanguagePreferenceError> {
    let path = directory.join(LANGUAGE_PREFERENCE_FILE);
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(LanguagePreference::FollowSystem);
        }
        Err(_error) => return Err(LanguagePreferenceError::Unavailable),
    };
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.len() > MAX_LANGUAGE_PREFERENCE_BYTES
    {
        return Err(LanguagePreferenceError::UnsafeFile);
    }
    let value = fs::read_to_string(path).map_err(|_error| LanguagePreferenceError::Unavailable)?;
    LanguagePreference::parse(value.trim_end_matches(['\r', '\n']))
        .ok_or(LanguagePreferenceError::InvalidValue)
}

pub(crate) fn save_language_preference_in(
    directory: &Path,
    preference: LanguagePreference,
) -> Result<PathBuf, LanguagePreferenceError> {
    let contents = format!("{}\n", preference.persistence_key());
    write_private_atomic(directory, LANGUAGE_PREFERENCE_FILE, contents.as_bytes())
        .map_err(|_error| LanguagePreferenceError::Unavailable)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LanguagePreferenceError {
    Unavailable,
    UnsafeFile,
    InvalidValue,
}

impl std::fmt::Display for LanguagePreferenceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Unavailable => "language preference could not be read or saved",
            Self::UnsafeFile => "language preference is not a safe regular file",
            Self::InvalidValue => "language preference contains an unknown value",
        })
    }
}

impl std::error::Error for LanguagePreferenceError {}

#[cfg(all(test, not(windows)))]
mod tests {
    use std::fs;

    use super::{
        Language, LanguagePreference, load_language_preference_in, save_language_preference_in,
    };

    #[test]
    fn follow_system_uses_chinese_only_for_chinese_locales() {
        assert_eq!(
            LanguagePreference::FollowSystem.resolve(Some("zh-Hans-CN")),
            Language::SimplifiedChinese
        );
        assert_eq!(
            LanguagePreference::FollowSystem.resolve(Some("zh_TW.UTF-8")),
            Language::SimplifiedChinese
        );
        assert_eq!(
            LanguagePreference::FollowSystem.resolve(Some("ja-JP")),
            Language::English
        );
        assert_eq!(
            LanguagePreference::FollowSystem.resolve(None),
            Language::English
        );
    }

    #[test]
    fn explicit_language_overrides_the_system_locale() {
        assert_eq!(
            LanguagePreference::English.resolve(Some("zh-CN")),
            Language::English
        );
        assert_eq!(
            LanguagePreference::SimplifiedChinese.resolve(Some("en-US")),
            Language::SimplifiedChinese
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn parses_the_first_macos_preferred_language() {
        assert_eq!(
            super::parse_apple_languages("(\n    \"zh-Hans-CN\",\n    \"en-US\"\n)"),
            Some("zh-Hans-CN".to_owned())
        );
    }

    #[test]
    fn preference_defaults_to_system_and_round_trips_privately() {
        let root =
            std::env::temp_dir().join(format!("relay-language-preference-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);

        assert_eq!(
            load_language_preference_in(&root).expect("missing preference follows system"),
            LanguagePreference::FollowSystem
        );
        let path = save_language_preference_in(&root, LanguagePreference::SimplifiedChinese)
            .expect("preference should persist");
        assert_eq!(
            load_language_preference_in(&root).expect("preference should load"),
            LanguagePreference::SimplifiedChinese
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            assert_eq!(
                fs::metadata(path)
                    .expect("preference metadata")
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }

        fs::remove_dir_all(root).expect("remove fixture");
    }
}
