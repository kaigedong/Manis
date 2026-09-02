use std::ffi::OsString;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

pub(crate) const PRODUCT_NAME: &str = "Manis";

pub(crate) fn config_dir() -> Option<PathBuf> {
    platform_config_dir()
}

pub(crate) fn data_dir() -> Option<PathBuf> {
    platform_data_dirs().and_then(|(manis, legacy)| {
        let selected = select_data_dir(&manis, &legacy)?;
        if selected == manis {
            migrate_legacy_artifacts(&selected);
        }
        Some(selected)
    })
}

pub(crate) fn env_var_os(primary: &str, legacy: &str) -> Option<OsString> {
    std::env::var_os(primary).or_else(|| std::env::var_os(legacy))
}

pub(crate) fn env_var(primary: &str, legacy: &str) -> Option<String> {
    std::env::var(primary)
        .ok()
        .or_else(|| std::env::var(legacy).ok())
}

fn select_data_dir(manis: &Path, legacy: &Path) -> Option<PathBuf> {
    match fs::symlink_metadata(manis) {
        Ok(metadata) => {
            return (metadata.is_dir() && !metadata.file_type().is_symlink())
                .then(|| manis.to_owned());
        }
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(_) => return None,
    }

    let metadata = match fs::symlink_metadata(legacy) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Some(manis.to_owned()),
        Err(_) => return None,
    };
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Some(manis.to_owned());
    }
    match fs::rename(legacy, manis) {
        Ok(()) => Some(manis.to_owned()),
        Err(_) => match fs::symlink_metadata(manis) {
            Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {
                Some(manis.to_owned())
            }
            Err(error) if error.kind() == ErrorKind::NotFound => {
                let legacy_metadata = fs::symlink_metadata(legacy).ok()?;
                (legacy_metadata.is_dir() && !legacy_metadata.file_type().is_symlink())
                    .then(|| legacy.to_owned())
            }
            Ok(_) | Err(_) => None,
        },
    }
}

fn migrate_legacy_artifacts(root: &Path) {
    const ARTIFACTS: [(&str, &str); 7] = [
        ("logs/relay-events.log", "logs/manis-events.pre-brand.log"),
        ("mihomo/relay-core.log", "mihomo/manis-core.pre-brand.log"),
        (
            "mihomo/relay-privileged-core.log",
            "mihomo/manis-privileged-core.pre-brand.log",
        ),
        (
            "mihomo/relay-generated.yaml",
            "mihomo/manis-generated.pre-brand.yaml",
        ),
        (
            "mihomo/relay-generated.candidate.yaml",
            "mihomo/manis-generated.pre-brand.candidate.yaml",
        ),
        (
            "mihomo/relay-generated.json",
            "mihomo/manis-generated.pre-brand.json",
        ),
        (
            "mihomo/relay-generated.candidate.json",
            "mihomo/manis-generated.pre-brand.candidate.json",
        ),
    ];

    let Ok(root_metadata) = fs::symlink_metadata(root) else {
        return;
    };
    if !root_metadata.is_dir() || root_metadata.file_type().is_symlink() {
        return;
    }

    for (legacy, manis) in ARTIFACTS {
        let legacy = root.join(legacy);
        let manis = root.join(manis);
        if manis.exists() {
            continue;
        }
        let Ok(metadata) = fs::symlink_metadata(&legacy) else {
            continue;
        };
        if metadata.is_file() && !metadata.file_type().is_symlink() {
            let _ = fs::rename(legacy, manis);
        }
    }
}

#[cfg(target_os = "macos")]
fn platform_data_dirs() -> Option<(PathBuf, PathBuf)> {
    std::env::var_os("HOME").map(PathBuf::from).map(|home| {
        (
            home.join("Library/Application Support/Manis"),
            home.join("Library/Application Support/Relay"),
        )
    })
}

#[cfg(target_os = "macos")]
fn platform_config_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join("Library/Application Support/Manis"))
}

#[cfg(windows)]
fn platform_data_dirs() -> Option<(PathBuf, PathBuf)> {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .map(|root| (root.join("Manis"), root.join("Relay")))
}

#[cfg(windows)]
fn platform_config_dir() -> Option<PathBuf> {
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .map(|root| root.join("Manis"))
}

#[cfg(all(unix, not(target_os = "macos")))]
fn platform_data_dirs() -> Option<(PathBuf, PathBuf)> {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .map(|root| (root.join("manis"), root.join("relay")))
        .or_else(|| {
            std::env::var_os("HOME").map(PathBuf::from).map(|home| {
                (
                    home.join(".local/share/manis"),
                    home.join(".local/share/relay"),
                )
            })
        })
}

#[cfg(all(unix, not(target_os = "macos")))]
fn platform_config_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .map(|root| root.join("manis"))
        .or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .map(|home| home.join(".config/manis"))
        })
}

#[cfg(not(any(unix, windows)))]
fn platform_data_dirs() -> Option<(PathBuf, PathBuf)> {
    None
}

#[cfg(not(any(unix, windows)))]
fn platform_config_dir() -> Option<PathBuf> {
    None
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    fn fixture(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("manis-brand-{name}-{}", std::process::id()))
    }

    #[test]
    fn legacy_data_directory_moves_to_manis_without_losing_files() {
        let root = fixture("migration");
        let legacy = root.join("Relay");
        let manis = root.join("Manis");
        fs::create_dir_all(legacy.join("subscriptions")).expect("create legacy fixture");
        fs::write(legacy.join("subscriptions/source"), b"saved").expect("write legacy fixture");

        assert_eq!(super::select_data_dir(&manis, &legacy), Some(manis.clone()));
        assert_eq!(
            fs::read(manis.join("subscriptions/source")).expect("read migrated fixture"),
            b"saved"
        );
        assert!(!legacy.exists());
        fs::remove_dir_all(root).expect("remove migration fixture");
    }

    #[test]
    fn existing_manis_directory_wins_without_merging_legacy_data() {
        let root = fixture("precedence");
        let legacy = root.join("Relay");
        let manis = root.join("Manis");
        fs::create_dir_all(&legacy).expect("create legacy fixture");
        fs::create_dir_all(&manis).expect("create Manis fixture");
        fs::write(legacy.join("legacy"), b"legacy").expect("write legacy fixture");
        fs::write(manis.join("current"), b"current").expect("write Manis fixture");

        assert_eq!(super::select_data_dir(&manis, &legacy), Some(manis.clone()));
        assert!(legacy.join("legacy").exists());
        assert!(manis.join("current").exists());
        fs::remove_dir_all(root).expect("remove precedence fixture");
    }

    #[test]
    fn legacy_named_runtime_artifacts_are_preserved_under_manis_names() {
        let root = fixture("artifacts");
        let logs = root.join("logs");
        let runtime = root.join("mihomo");
        fs::create_dir_all(&logs).expect("create logs fixture");
        fs::create_dir_all(&runtime).expect("create runtime fixture");
        fs::write(logs.join("relay-events.log"), b"event").expect("write legacy event log");
        fs::write(runtime.join("relay-generated.yaml"), b"profile").expect("write legacy profile");

        super::migrate_legacy_artifacts(&root);

        assert_eq!(
            fs::read(logs.join("manis-events.pre-brand.log")).expect("read migrated event log"),
            b"event"
        );
        assert_eq!(
            fs::read(runtime.join("manis-generated.pre-brand.yaml"))
                .expect("read migrated profile"),
            b"profile"
        );
        assert!(!logs.join("relay-events.log").exists());
        assert!(!runtime.join("relay-generated.yaml").exists());
        fs::remove_dir_all(root).expect("remove artifact fixture");
    }

    #[cfg(unix)]
    #[test]
    fn legacy_artifacts_are_not_migrated_through_a_manis_symlink() {
        use std::os::unix::fs::symlink;

        let root = fixture("artifact-symlink");
        let outside = root.join("outside");
        let manis = root.join("Manis");
        fs::create_dir_all(outside.join("logs")).expect("create outside logs fixture");
        fs::write(outside.join("logs/relay-events.log"), b"event")
            .expect("write outside legacy log");
        symlink(&outside, &manis).expect("create Manis symlink");

        super::migrate_legacy_artifacts(&manis);

        assert!(outside.join("logs/relay-events.log").exists());
        assert!(!outside.join("logs/manis-events.pre-brand.log").exists());
        fs::remove_file(manis).expect("remove Manis symlink");
        fs::remove_dir_all(root).expect("remove artifact symlink fixture");
    }

    #[cfg(unix)]
    #[test]
    fn legacy_symlink_is_never_migrated() {
        use std::os::unix::fs::symlink;

        let root = fixture("symlink");
        let outside = root.join("outside");
        let legacy = root.join("Relay");
        let manis = root.join("Manis");
        fs::create_dir_all(&outside).expect("create outside fixture");
        symlink(&outside, &legacy).expect("create legacy symlink");

        assert_eq!(super::select_data_dir(&manis, &legacy), Some(manis.clone()));
        assert!(legacy.is_symlink());
        assert!(!manis.exists());
        fs::remove_file(legacy).expect("remove legacy symlink");
        fs::remove_dir_all(root).expect("remove symlink fixture");
    }

    #[cfg(unix)]
    #[test]
    fn manis_symlink_is_rejected_as_a_data_directory() {
        use std::os::unix::fs::symlink;

        let root = fixture("manis-symlink");
        let outside = root.join("outside");
        let legacy = root.join("Relay");
        let manis = root.join("Manis");
        fs::create_dir_all(&outside).expect("create outside fixture");
        symlink(&outside, &manis).expect("create Manis symlink");

        assert_eq!(super::select_data_dir(&manis, &legacy), None);
        assert!(manis.is_symlink());
        assert!(!legacy.exists());
        fs::remove_file(manis).expect("remove Manis symlink");
        fs::remove_dir_all(root).expect("remove Manis symlink fixture");
    }
}
