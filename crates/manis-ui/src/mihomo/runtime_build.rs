use super::{
    Arc, AtomicBool, Component, ControllerEndpoint, ControllerRuntime, DATA_DIR_ENV,
    DEFAULT_MANAGED_MIXED_PORT, EngineManager, GENERATED_PROFILE_FILE, KernelKind,
    LEGACY_RELAY_DATA_DIR_ENV, LEGACY_RELAY_MIXED_PORT_ENV, MIXED_PORT_ENV,
    ManagedGeneratedProfile, Mutex, Path, PathBuf, ReadinessPolicy, RuntimeProfileSource,
    UNSUPPORTED_MIHOMO_RUNTIME_ENV, brand, canonical_binary, compile_saved_profile, core_update,
    env, managed_engine_config, readiness_probe, render_generated_profile,
    sync_single_node_provider_files, validate_managed_config, write_private_atomic,
};
#[cfg(debug_assertions)]
use super::{BINARY_ENV, LEGACY_RELAY_BINARY_ENV};

pub(crate) fn configured_runtime(store_dir: Option<&Path>) -> ControllerRuntime {
    if let Some(variable) = first_unsupported_runtime_override(|name| env::var_os(name).is_some()) {
        return ControllerRuntime::Invalid {
            message: format!(
                "{variable} is no longer supported; Mihomo configuration and controller settings are managed only by Manis"
            ),
        };
    }
    let Some(store_dir) = store_dir else {
        return ControllerRuntime::Invalid {
            message: "Manis source directory could not be determined".to_owned(),
        };
    };
    #[cfg(debug_assertions)]
    let binary = brand::env_var_os(BINARY_ENV, LEGACY_RELAY_BINARY_ENV).map_or_else(
        discover_mihomo_binary,
        |binary| {
            canonical_binary(Path::new(&binary))
                .map_err(|_error| format!("{BINARY_ENV} does not point to an executable file"))
        },
    );
    #[cfg(not(debug_assertions))]
    let binary = discover_mihomo_binary();
    binary
        .and_then(|binary| build_saved_sources_mihomo_runtime_with_binary(store_dir, &binary))
        .unwrap_or_else(|message| ControllerRuntime::Invalid { message })
}

pub(super) fn first_unsupported_runtime_override(
    is_set: impl Fn(&str) -> bool,
) -> Option<&'static str> {
    UNSUPPORTED_MIHOMO_RUNTIME_ENV
        .into_iter()
        .find(|name| is_set(name))
}

fn build_saved_sources_mihomo_runtime_with_binary(
    store_dir: &Path,
    binary: &Path,
) -> Result<ControllerRuntime, String> {
    let data_dir = configured_data_dir()?;
    #[cfg(unix)]
    let controller = configured_managed_controller(&data_dir);
    #[cfg(not(unix))]
    let controller = configured_managed_controller(&data_dir)?;
    build_saved_sources_mihomo_runtime_in(store_dir, binary, &data_dir, &controller)
}

pub(super) fn build_saved_sources_mihomo_runtime_in(
    store_dir: &Path,
    binary: &Path,
    data_dir: &Path,
    controller: &ControllerEndpoint,
) -> Result<ControllerRuntime, String> {
    sync_single_node_provider_files(store_dir, data_dir).map_err(|error| error.to_string())?;
    let profile = compile_saved_profile(store_dir, None).map_err(|error| error.to_string())?;
    let spec = ManagedGeneratedProfile {
        kernel: KernelKind::Mihomo,
        binary: binary.to_path_buf(),
        data_dir: data_dir.to_path_buf(),
        controller: controller.clone(),
        expected_mixed_port: Some(profile.mixed_port),
        profile_store_dir: Some(store_dir.to_path_buf()),
        controller_secret: None,
    };
    let rendered = render_generated_profile(&spec, &profile).map_err(|error| error.to_string())?;
    let config_file =
        write_private_atomic(data_dir, GENERATED_PROFILE_FILE, rendered.as_bytes())
            .map_err(|_error| "private Mihomo configuration could not be written".to_owned())?;
    let config = managed_engine_config(&spec, config_file);
    validate_managed_config(&config).map_err(|error| error.to_string())?;
    let manager = EngineManager::new(config, ReadinessPolicy::default(), readiness_probe(&spec));
    Ok(ControllerRuntime::Managed {
        manager: Arc::new(Mutex::new(manager)),
        apply_lock: Arc::new(Mutex::new(())),
        profile_source: RuntimeProfileSource::SavedSources,
        generated_profile: Some(spec),
        privileged: Arc::new(AtomicBool::new(false)),
    })
}

fn discover_mihomo_binary() -> Result<PathBuf, String> {
    core_update::managed_core_binary_path()
        .map_err(|error| error.to_string())
        .and_then(|path| {
            canonical_binary(&path).map_err(|_error| {
                "Manis-managed Mihomo is not installed; download the stable core in Runtime settings"
                    .to_owned()
            })
        })
}

pub(super) fn has_only_clean_components(path: &Path) -> bool {
    path.components().all(|component| {
        matches!(
            component,
            Component::Prefix(_) | Component::RootDir | Component::Normal(_)
        )
    })
}

pub(super) fn configured_mixed_port() -> Result<u16, String> {
    match brand::env_var_os(MIXED_PORT_ENV, LEGACY_RELAY_MIXED_PORT_ENV) {
        Some(value) => value
            .to_str()
            .ok_or_else(|| format!("{MIXED_PORT_ENV} must be valid Unicode"))?
            .parse::<u16>()
            .ok()
            .filter(|port| *port != 0)
            .ok_or_else(|| format!("{MIXED_PORT_ENV} must be a port from 1 to 65535")),
        None => Ok(DEFAULT_MANAGED_MIXED_PORT),
    }
}

fn configured_data_dir() -> Result<PathBuf, String> {
    brand::env_var_os(DATA_DIR_ENV, LEGACY_RELAY_DATA_DIR_ENV)
        .map(PathBuf::from)
        .or_else(default_data_dir)
        .ok_or_else(|| format!("data directory could not be determined; set {DATA_DIR_ENV}"))
}

#[cfg(unix)]
fn configured_managed_controller(data_dir: &Path) -> ControllerEndpoint {
    default_managed_endpoint(data_dir)
}

#[cfg(not(unix))]
fn configured_managed_controller(data_dir: &Path) -> Result<ControllerEndpoint, String> {
    default_managed_endpoint(data_dir)
}

#[cfg(unix)]
fn default_managed_endpoint(data_dir: &Path) -> ControllerEndpoint {
    ControllerEndpoint::UnixSocket(data_dir.join("controller.sock"))
}

#[cfg(windows)]
fn default_managed_endpoint(_data_dir: &Path) -> Result<ControllerEndpoint, String> {
    Err("managed Mihomo controller transport is not implemented on Windows".to_owned())
}

#[cfg(not(any(unix, windows)))]
fn default_managed_endpoint(_data_dir: &Path) -> Result<ControllerEndpoint, String> {
    Err("this platform has no default Mihomo controller transport".to_owned())
}

fn default_data_dir() -> Option<PathBuf> {
    brand::data_dir().map(|directory| directory.join("mihomo"))
}
