use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::io::Read as _;
use std::path::{Component, Path, PathBuf};
use std::str::FromStr as _;

use toml_edit::{DocumentMut, Item, Table, value};

const CONFIG_FILE_NAME: &str = "config.toml";
const CONFIG_SCHEMA_VERSION: i64 = 1;
const CONFIG_TABLE: &str = "configuration";
const MAX_CONFIG_BYTES: u64 = 64 * 1024 * 1024;

static CONFIG_WRITE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

const EMPTY_DOCUMENT: &str = r"schema_version = 1

# Manis keeps user configuration in this file. Comments and formatting outside
# values changed by the application are preserved.
[configuration]
";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ConfigTomlError {
    Unavailable,
    UnsafePath,
    InvalidFormat,
    Oversized,
}

impl fmt::Display for ConfigTomlError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Unavailable => "configuration is unavailable",
            Self::UnsafePath => "configuration path is unsafe",
            Self::InvalidFormat => "configuration TOML is invalid",
            Self::Oversized => "configuration is too large",
        })
    }
}

impl std::error::Error for ConfigTomlError {}

pub(crate) fn config_path(directory: &Path) -> Result<PathBuf, ConfigTomlError> {
    require_clean_absolute_directory(directory)?;
    Ok(directory.join(CONFIG_FILE_NAME))
}

pub(crate) fn read_source(directory: &Path) -> Result<String, ConfigTomlError> {
    let _guard = config_lock()?;
    let (source, _) = load_or_create_document(directory)?;
    Ok(source)
}

pub(crate) fn replace_source(directory: &Path, source: &str) -> Result<(), ConfigTomlError> {
    if source.len() as u64 > MAX_CONFIG_BYTES {
        return Err(ConfigTomlError::Oversized);
    }
    parse_document(source)?;
    let _guard = config_lock()?;
    write_source(directory, source)
}

pub(crate) fn entries(directory: &Path) -> Result<BTreeMap<String, String>, ConfigTomlError> {
    let _guard = config_lock()?;
    let (_, document) = load_or_create_document(directory)?;
    entries_from_document(&document)
}

pub(crate) fn entries_from_source(
    source: &str,
) -> Result<BTreeMap<String, String>, ConfigTomlError> {
    if source.len() as u64 > MAX_CONFIG_BYTES {
        return Err(ConfigTomlError::Oversized);
    }
    entries_from_document(&parse_document(source)?)
}

pub(crate) fn read_entry(
    directory: &Path,
    name: &str,
    max_bytes: u64,
) -> Result<Option<String>, ConfigTomlError> {
    if !is_config_entry_name(name) {
        return Err(ConfigTomlError::InvalidFormat);
    }
    let value = entries(directory)?.remove(name);
    if value
        .as_ref()
        .is_some_and(|contents| contents.len() as u64 > max_bytes)
    {
        return Err(ConfigTomlError::Oversized);
    }
    Ok(value)
}

pub(crate) fn write_entry(
    directory: &Path,
    name: &str,
    contents: &str,
) -> Result<PathBuf, ConfigTomlError> {
    if !is_config_entry_name(name) {
        return Err(ConfigTomlError::InvalidFormat);
    }
    let _guard = config_lock()?;
    let (_, mut document) = load_or_create_document(directory)?;
    let table = configuration_table_mut(&mut document)?;
    let preserved_decor = table
        .get(name)
        .and_then(Item::as_value)
        .map(|value| value.decor().clone());
    let mut replacement = value(contents);
    if let (Some(decor), Some(value)) = (preserved_decor, replacement.as_value_mut()) {
        *value.decor_mut() = decor;
    }
    table.insert(name, replacement);
    let source = document.to_string();
    if source.len() as u64 > MAX_CONFIG_BYTES {
        return Err(ConfigTomlError::Oversized);
    }
    write_source(directory, &source)?;
    config_path(directory)
}

pub(crate) fn remove_entry(directory: &Path, name: &str) -> Result<(), ConfigTomlError> {
    if !is_config_entry_name(name) {
        return Err(ConfigTomlError::InvalidFormat);
    }
    let _guard = config_lock()?;
    let (_, mut document) = load_or_create_document(directory)?;
    configuration_table_mut(&mut document)?.remove(name);
    write_source(directory, &document.to_string())
}

pub(crate) fn replace_entry_if_unchanged(
    directory: &Path,
    name: &str,
    expected: Option<&str>,
    replacement: Option<&str>,
) -> Result<bool, ConfigTomlError> {
    if !is_config_entry_name(name) {
        return Err(ConfigTomlError::InvalidFormat);
    }
    let _guard = config_lock()?;
    let (_, mut document) = load_or_create_document(directory)?;
    let table = configuration_table_mut(&mut document)?;
    let current = table
        .get(name)
        .and_then(Item::as_value)
        .and_then(|value| value.as_str());
    if current != expected {
        return Ok(false);
    }
    match replacement {
        Some(contents) => {
            let preserved_decor = table
                .get(name)
                .and_then(Item::as_value)
                .map(|value| value.decor().clone());
            let mut replacement = value(contents);
            if let (Some(decor), Some(value)) = (preserved_decor, replacement.as_value_mut()) {
                *value.decor_mut() = decor;
            }
            table.insert(name, replacement);
        }
        None => {
            table.remove(name);
        }
    }
    write_source(directory, &document.to_string())?;
    Ok(true)
}

pub(crate) fn migrate_legacy_directory(
    directory: &Path,
    legacy: &Path,
) -> Result<bool, ConfigTomlError> {
    require_clean_absolute_directory(directory)?;
    require_clean_absolute_directory(legacy)?;
    let _guard = config_lock()?;
    if config_path(directory)?.exists() {
        return Ok(false);
    }
    let entries = read_legacy_entries(legacy)?;
    if entries.is_empty() {
        return Ok(false);
    }
    let mut document = parse_document(EMPTY_DOCUMENT)?;
    let table = configuration_table_mut(&mut document)?;
    for (name, contents) in &entries {
        table.insert(name, value(contents));
    }
    write_source(directory, &document.to_string())?;
    for name in entries.keys() {
        let path = legacy.join(name);
        if let Ok(metadata) = fs::symlink_metadata(&path)
            && metadata.is_file()
            && !metadata.file_type().is_symlink()
        {
            fs::remove_file(path).map_err(|_| ConfigTomlError::Unavailable)?;
        }
    }
    Ok(true)
}

pub(crate) fn is_config_entry_name(name: &str) -> bool {
    matches!(
        name,
        "subscription.url"
            | "routing-rule-group-order.state"
            | "workspace.state"
            | "routing.mode"
            | "node-selection.state"
            | "manual-routing-rules.state"
            | "direct-rules.state"
            | "kernel.kind"
            | "language.preference"
    ) || prefixed_name(name, "source-", ".url")
        || prefixed_name(name, "saved-", ".vless")
        || prefixed_name(name, "qx-rule-", ".qxrules")
        || prefixed_name(name, "policy-", ".policy")
        || prefixed_name(name, "group-", ".policy")
        || prefixed_name(name, "group-", ".group")
        || prefixed_name(name, "policy-", ".group")
}

fn prefixed_name(name: &str, prefix: &str, suffix: &str) -> bool {
    name.strip_suffix(suffix)
        .and_then(|stem| stem.strip_prefix(prefix))
        .is_some_and(|id| {
            !id.is_empty()
                && id
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit() || byte == b'-')
        })
}

fn config_lock() -> Result<std::sync::MutexGuard<'static, ()>, ConfigTomlError> {
    CONFIG_WRITE_LOCK
        .lock()
        .map_err(|_| ConfigTomlError::Unavailable)
}

fn load_or_create_document(directory: &Path) -> Result<(String, DocumentMut), ConfigTomlError> {
    let path = config_path(directory)?;
    match fs::symlink_metadata(&path) {
        Ok(metadata) => {
            let source = read_private_text(&path, &metadata, MAX_CONFIG_BYTES)?;
            let document = parse_document(&source)?;
            Ok((source, document))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let legacy_entries = read_legacy_entries(directory)?;
            if legacy_entries.is_empty() {
                let document = parse_document(EMPTY_DOCUMENT)?;
                return Ok((EMPTY_DOCUMENT.to_owned(), document));
            }
            let mut document = parse_document(EMPTY_DOCUMENT)?;
            let table = configuration_table_mut(&mut document)?;
            for (name, contents) in &legacy_entries {
                table.insert(name, value(contents));
            }
            let source = document.to_string();
            write_source(directory, &source)?;
            remove_legacy_entries(directory, legacy_entries.keys())?;
            Ok((source, document))
        }
        Err(_) => Err(ConfigTomlError::Unavailable),
    }
}

fn remove_legacy_entries<'a>(
    directory: &Path,
    names: impl Iterator<Item = &'a String>,
) -> Result<(), ConfigTomlError> {
    for name in names {
        let path = directory.join(name);
        if let Ok(metadata) = fs::symlink_metadata(&path)
            && metadata.is_file()
            && !metadata.file_type().is_symlink()
        {
            fs::remove_file(path).map_err(|_| ConfigTomlError::Unavailable)?;
        }
    }
    Ok(())
}

fn parse_document(source: &str) -> Result<DocumentMut, ConfigTomlError> {
    let document = DocumentMut::from_str(source).map_err(|_| ConfigTomlError::InvalidFormat)?;
    if document.get("schema_version").and_then(Item::as_integer) != Some(CONFIG_SCHEMA_VERSION) {
        return Err(ConfigTomlError::InvalidFormat);
    }
    let table = document
        .get(CONFIG_TABLE)
        .and_then(Item::as_table)
        .ok_or(ConfigTomlError::InvalidFormat)?;
    if table.iter().any(|(name, item)| {
        !is_config_entry_name(name) || item.as_value().and_then(|value| value.as_str()).is_none()
    }) {
        return Err(ConfigTomlError::InvalidFormat);
    }
    Ok(document)
}

fn entries_from_document(
    document: &DocumentMut,
) -> Result<BTreeMap<String, String>, ConfigTomlError> {
    document
        .get(CONFIG_TABLE)
        .and_then(Item::as_table)
        .ok_or(ConfigTomlError::InvalidFormat)?
        .iter()
        .map(|(name, item)| {
            item.as_value()
                .and_then(|value| value.as_str())
                .map(|contents| (name.to_owned(), contents.to_owned()))
                .ok_or(ConfigTomlError::InvalidFormat)
        })
        .collect()
}

fn configuration_table_mut(document: &mut DocumentMut) -> Result<&mut Table, ConfigTomlError> {
    document
        .get_mut(CONFIG_TABLE)
        .and_then(Item::as_table_mut)
        .ok_or(ConfigTomlError::InvalidFormat)
}

fn write_source(directory: &Path, source: &str) -> Result<(), ConfigTomlError> {
    require_clean_absolute_directory(directory)?;
    manis_profile::write_private_atomic(directory, CONFIG_FILE_NAME, source.as_bytes())
        .map(|_| ())
        .map_err(|_| ConfigTomlError::Unavailable)
}

fn read_legacy_entries(legacy: &Path) -> Result<BTreeMap<String, String>, ConfigTomlError> {
    let iterator = match fs::read_dir(legacy) {
        Ok(iterator) => iterator,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(_) => return Err(ConfigTomlError::Unavailable),
    };
    let mut entries = BTreeMap::new();
    for entry in iterator {
        let path = entry.map_err(|_| ConfigTomlError::Unavailable)?.path();
        let Some(name) = path
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_owned)
        else {
            continue;
        };
        if !is_config_entry_name(&name) {
            continue;
        }
        let metadata = fs::symlink_metadata(&path).map_err(|_| ConfigTomlError::Unavailable)?;
        let contents = read_private_text(&path, &metadata, MAX_CONFIG_BYTES)?;
        entries.insert(name, contents);
    }
    Ok(entries)
}

fn read_private_text(
    path: &Path,
    expected: &fs::Metadata,
    max_bytes: u64,
) -> Result<String, ConfigTomlError> {
    if expected.file_type().is_symlink() || !expected.is_file() {
        return Err(ConfigTomlError::UnsafePath);
    }
    if expected.len() > max_bytes {
        return Err(ConfigTomlError::Oversized);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        if expected.permissions().mode() & 0o077 != 0 {
            return Err(ConfigTomlError::UnsafePath);
        }
    }
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.custom_flags(0o00400000);
    }
    #[cfg(target_os = "macos")]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.custom_flags(0x0100);
    }
    let file = options
        .open(path)
        .map_err(|_| ConfigTomlError::Unavailable)?;
    let opened = file.metadata().map_err(|_| ConfigTomlError::Unavailable)?;
    if !opened.is_file() || !same_file(expected, &opened) {
        return Err(ConfigTomlError::UnsafePath);
    }
    let mut source = String::new();
    file.take(max_bytes + 1)
        .read_to_string(&mut source)
        .map_err(|_| ConfigTomlError::Unavailable)?;
    if source.len() as u64 > max_bytes {
        return Err(ConfigTomlError::Oversized);
    }
    Ok(source)
}

#[cfg(unix)]
fn same_file(expected: &fs::Metadata, opened: &fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt as _;

    expected.dev() == opened.dev() && expected.ino() == opened.ino()
}

#[cfg(not(unix))]
fn same_file(_: &fs::Metadata, _: &fs::Metadata) -> bool {
    true
}

fn require_clean_absolute_directory(directory: &Path) -> Result<(), ConfigTomlError> {
    if !directory.is_absolute()
        || !directory.components().all(|component| {
            matches!(
                component,
                Component::Prefix(_) | Component::RootDir | Component::Normal(_)
            )
        })
    {
        return Err(ConfigTomlError::UnsafePath);
    }
    match fs::symlink_metadata(directory) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            Err(ConfigTomlError::UnsafePath)
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(ConfigTomlError::Unavailable),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "manis-config-toml-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ))
    }

    #[test]
    fn targeted_updates_preserve_comments_and_unrelated_formatting() {
        let directory = fixture("comments");
        fs::create_dir_all(&directory).expect("fixture");
        let source = r#"schema_version = 1

# 用户自己的说明
[configuration]
"routing.mode" = "rule" # 保留行尾注释

# 不相关配置的说明
"language.preference" = "zh-CN"
"#;
        replace_source(&directory, source).expect("initial source");
        write_entry(&directory, "routing.mode", "global").expect("update");
        let updated = read_source(&directory).expect("updated source");
        assert!(updated.contains("# 用户自己的说明"));
        assert!(updated.contains("# 保留行尾注释"));
        assert!(updated.contains("# 不相关配置的说明\n\"language.preference\" = \"zh-CN\""));
        assert_eq!(
            read_entry(&directory, "routing.mode", 32).unwrap(),
            Some("global".into())
        );
        fs::remove_dir_all(directory).expect("cleanup");
    }

    #[test]
    fn legacy_files_migrate_once_into_one_private_toml_file() {
        let root = fixture("migration");
        let directory = root.join("config");
        let legacy = root.join("subscriptions");
        fs::create_dir_all(&legacy).expect("legacy");
        fs::write(legacy.join("routing.mode"), "direct").expect("legacy mode");
        fs::write(legacy.join("benchmarks.state"), "cache").expect("cache");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            fs::set_permissions(
                legacy.join("routing.mode"),
                fs::Permissions::from_mode(0o600),
            )
            .expect("private legacy mode");
        }
        assert!(migrate_legacy_directory(&directory, &legacy).expect("migration"));
        assert_eq!(
            read_entry(&directory, "routing.mode", 32).unwrap(),
            Some("direct".into())
        );
        assert!(!legacy.join("routing.mode").exists());
        assert!(legacy.join("benchmarks.state").exists());
        assert!(!migrate_legacy_directory(&directory, &legacy).expect("second migration"));
        fs::remove_dir_all(root).expect("cleanup");
    }
}
