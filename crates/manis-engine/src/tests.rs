use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
#[cfg(unix)]
use std::time::Instant;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use super::{
    CHILD_ENV_ALLOWLIST, CommandSpec, ControllerEndpoint, EngineManager, EngineState, ManagedChild,
    ManagedEngineConfig, ProbeStatus, ProcessExit, ProcessSpawner, ReadinessPolicy, ReadinessProbe,
    resolved_command,
};
use manis_core::KernelKind;

static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);

struct TempLayout {
    root: PathBuf,
    binary: PathBuf,
    config: PathBuf,
    data_dir: PathBuf,
}

impl TempLayout {
    fn new() -> Self {
        let id = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
        let root =
            std::env::temp_dir().join(format!("manis-engine-test-{}-{id}", std::process::id()));
        let binary = root.join("mihomo-fixture");
        let config = root.join("config.yaml");
        let data_dir = root.join("runtime");
        fs::create_dir_all(&root).expect("create test root");
        fs::write(&binary, b"fixture").expect("write fake binary");
        fs::write(&config, b"mixed-port: 0\n").expect("write fake config");
        Self {
            root,
            binary,
            config,
            data_dir,
        }
    }

    fn config(&self) -> ManagedEngineConfig {
        ManagedEngineConfig::new(
            self.binary.clone(),
            self.config.clone(),
            self.data_dir.clone(),
            ControllerEndpoint::UnixSocket(self.data_dir.join("controller.sock")),
        )
    }
}

impl Drop for TempLayout {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[derive(Default)]
struct FakeState {
    validations: usize,
    spawns: usize,
    terminates: usize,
    validation_succeeds: bool,
    exits_early: bool,
    terminate_failures_remaining: usize,
}

struct FakeSpawner {
    state: Arc<Mutex<FakeState>>,
}

struct FakeChild {
    state: Arc<Mutex<FakeState>>,
}

impl ManagedChild for FakeChild {
    fn id(&self) -> u32 {
        4242
    }

    fn try_wait(&mut self) -> std::io::Result<Option<ProcessExit>> {
        let state = self.state.lock().expect("fake state");
        Ok(state.exits_early.then(ProcessExit::failure))
    }

    fn terminate(&mut self) -> std::io::Result<ProcessExit> {
        let mut state = self.state.lock().expect("fake state");
        state.terminates += 1;
        if state.terminate_failures_remaining > 0 {
            state.terminate_failures_remaining -= 1;
            return Err(std::io::Error::other("injected terminate failure"));
        }
        Ok(ProcessExit::success())
    }
}

impl ProcessSpawner for FakeSpawner {
    fn validate(
        &mut self,
        _spec: &CommandSpec,
        _timeout: Duration,
    ) -> std::io::Result<ProcessExit> {
        let mut state = self.state.lock().expect("fake state");
        state.validations += 1;
        Ok(if state.validation_succeeds {
            ProcessExit::success()
        } else {
            ProcessExit::failure()
        })
    }

    fn spawn(&mut self, _spec: &CommandSpec) -> std::io::Result<Box<dyn ManagedChild>> {
        self.state.lock().expect("fake state").spawns += 1;
        Ok(Box::new(FakeChild {
            state: Arc::clone(&self.state),
        }))
    }
}

struct ScriptedProbe {
    results: Vec<ProbeStatus>,
    cursor: usize,
}

impl ReadinessProbe for ScriptedProbe {
    fn check(&mut self, _endpoint: &ControllerEndpoint) -> ProbeStatus {
        let result = self
            .results
            .get(self.cursor)
            .copied()
            .unwrap_or(ProbeStatus::Pending);
        self.cursor += 1;
        result
    }
}

fn manager(
    config: ManagedEngineConfig,
    state: Arc<Mutex<FakeState>>,
    results: Vec<ProbeStatus>,
    attempts: usize,
) -> EngineManager {
    EngineManager::with_adapters(
        config,
        ReadinessPolicy::new(attempts, Duration::ZERO).expect("valid readiness policy"),
        Box::new(FakeSpawner { state }),
        Box::new(ScriptedProbe { results, cursor: 0 }),
    )
}

#[test]
fn builds_validation_and_launch_commands_from_isolated_paths() {
    let layout = TempLayout::new();
    let config = layout.config();
    let validation = config.validation_command();
    let launch = config.launch_command();

    assert_eq!(validation.program(), layout.binary.as_path());
    assert_eq!(
        validation.args(),
        &[
            OsString::from("-t"),
            OsString::from("-d"),
            layout.data_dir.clone().into_os_string(),
            OsString::from("-f"),
            layout.config.clone().into_os_string(),
        ]
    );
    assert_eq!(
        launch.args(),
        &[
            OsString::from("-d"),
            layout.data_dir.clone().into_os_string(),
            OsString::from("-f"),
            layout.config.clone().into_os_string(),
            OsString::from("-ext-ctl-unix"),
            layout.data_dir.join("controller.sock").into_os_string(),
        ]
    );
}

#[test]
fn builds_sing_box_check_and_run_commands_with_a_secured_loopback_api() {
    let layout = TempLayout::new();
    let config = ManagedEngineConfig::new_sing_box(
        layout.binary.clone(),
        layout.config.clone(),
        layout.data_dir.clone(),
        ControllerEndpoint::Tcp("127.0.0.1:19090".parse().expect("loopback address")),
        true,
    );

    assert_eq!(config.kernel(), KernelKind::SingBox);
    assert!(config.validate().is_ok());
    assert_eq!(
        config.validation_command().args(),
        &[
            OsString::from("check"),
            OsString::from("-c"),
            layout.config.clone().into_os_string(),
            OsString::from("-D"),
            layout.data_dir.clone().into_os_string(),
        ]
    );
    assert_eq!(
        config.launch_command().args(),
        &[
            OsString::from("run"),
            OsString::from("-c"),
            layout.config.clone().into_os_string(),
            OsString::from("-D"),
            layout.data_dir.clone().into_os_string(),
        ]
    );
}

#[test]
fn sing_box_rejects_an_unauthenticated_or_non_loopback_controller() {
    let layout = TempLayout::new();
    let unauthenticated = ManagedEngineConfig::new_sing_box(
        layout.binary.clone(),
        layout.config.clone(),
        layout.data_dir.clone(),
        ControllerEndpoint::Tcp("127.0.0.1:19090".parse().expect("loopback address")),
        false,
    );
    let remote = ManagedEngineConfig::new_sing_box(
        layout.binary.clone(),
        layout.config.clone(),
        layout.data_dir.clone(),
        ControllerEndpoint::Tcp("192.0.2.10:19090".parse().expect("remote address")),
        true,
    );

    assert!(unauthenticated.validate().is_err());
    assert!(remote.validate().is_err());
}

#[test]
fn child_commands_inherit_only_the_minimum_environment_allowlist() {
    let layout = TempLayout::new();
    let command = resolved_command(&layout.config().launch_command());
    let inherited = command
        .get_envs()
        .filter_map(|(name, value)| value.map(|_| name))
        .collect::<Vec<_>>();

    for variable in inherited {
        assert!(
            CHILD_ENV_ALLOWLIST
                .iter()
                .any(|allowed| variable == std::ffi::OsStr::new(allowed))
        );
    }
}

#[test]
fn rejects_relative_and_non_loopback_runtime_boundaries() {
    let layout = TempLayout::new();
    let relative = ManagedEngineConfig::new(
        PathBuf::from("mihomo"),
        layout.config.clone(),
        layout.data_dir.clone(),
        ControllerEndpoint::UnixSocket(layout.data_dir.join("controller.sock")),
    );
    assert!(relative.validate().is_err());

    let remote = ManagedEngineConfig::new(
        layout.binary.clone(),
        layout.config.clone(),
        layout.data_dir.clone(),
        ControllerEndpoint::Tcp("192.0.2.10:9090".parse().expect("socket address")),
    );
    assert!(remote.validate().is_err());

    let loopback = ManagedEngineConfig::new(
        layout.binary.clone(),
        layout.config.clone(),
        layout.data_dir.clone(),
        ControllerEndpoint::Tcp("127.0.0.1:19090".parse().expect("socket address")),
    );
    assert!(loopback.validate().is_err());
}

#[cfg(unix)]
#[test]
fn start_creates_a_private_runtime_directory() {
    let layout = TempLayout::new();
    let state = Arc::new(Mutex::new(FakeState {
        validation_succeeds: true,
        ..FakeState::default()
    }));
    let mut manager = manager(layout.config(), state, vec![ProbeStatus::Ready], 1);

    manager.start().expect("engine becomes ready");

    let mode = fs::metadata(&layout.data_dir)
        .expect("runtime metadata")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o700);
}

#[cfg(unix)]
#[test]
fn start_rejects_a_controller_nested_below_the_private_runtime() {
    let layout = TempLayout::new();
    let data_dir = layout.root.join("private-runtime");
    let config = ManagedEngineConfig::new(
        layout.binary.clone(),
        layout.config.clone(),
        data_dir.clone(),
        ControllerEndpoint::UnixSocket(data_dir.join("nested/controller.sock")),
    );
    let state = Arc::new(Mutex::new(FakeState {
        validation_succeeds: true,
        ..FakeState::default()
    }));
    let mut manager = manager(config, state, vec![ProbeStatus::Ready], 1);

    assert!(manager.start().is_err());
}

#[test]
fn starts_only_after_validation_and_readiness() {
    let layout = TempLayout::new();
    let state = Arc::new(Mutex::new(FakeState {
        validation_succeeds: true,
        ..FakeState::default()
    }));
    let mut manager = manager(
        layout.config(),
        Arc::clone(&state),
        vec![ProbeStatus::Pending, ProbeStatus::Ready],
        3,
    );

    let endpoint = manager.start().expect("engine becomes ready");

    assert_eq!(endpoint, layout.config().controller().clone());
    assert!(matches!(
        manager.state(),
        EngineState::Ready { pid: 4242, .. }
    ));
    let state = state.lock().expect("fake state");
    assert_eq!(state.validations, 1);
    assert_eq!(state.spawns, 1);
    assert_eq!(state.terminates, 0);
}

#[test]
fn running_endpoint_detects_a_child_that_crashed_after_readiness() {
    let layout = TempLayout::new();
    let state = Arc::new(Mutex::new(FakeState {
        validation_succeeds: true,
        ..FakeState::default()
    }));
    let mut manager = manager(
        layout.config(),
        Arc::clone(&state),
        vec![ProbeStatus::Ready, ProbeStatus::Ready],
        1,
    );
    manager.start().expect("engine becomes ready");
    state.lock().expect("fake state").exits_early = true;

    assert!(manager.running_endpoint().is_err());
    assert!(matches!(manager.state(), EngineState::Failed { .. }));

    state.lock().expect("fake state").exits_early = false;
    assert!(manager.start().is_ok());
}

#[test]
fn timeout_terminates_only_the_owned_child() {
    let layout = TempLayout::new();
    let state = Arc::new(Mutex::new(FakeState {
        validation_succeeds: true,
        ..FakeState::default()
    }));
    let mut manager = manager(
        layout.config(),
        Arc::clone(&state),
        vec![ProbeStatus::Pending, ProbeStatus::Pending],
        2,
    );

    assert!(manager.start().is_err());
    assert!(matches!(manager.state(), EngineState::Failed { .. }));
    assert_eq!(state.lock().expect("fake state").terminates, 1);
}

#[test]
fn early_exit_is_reported_without_terminating_an_unowned_pid() {
    let layout = TempLayout::new();
    let state = Arc::new(Mutex::new(FakeState {
        validation_succeeds: true,
        exits_early: true,
        ..FakeState::default()
    }));
    let mut manager = manager(
        layout.config(),
        Arc::clone(&state),
        vec![ProbeStatus::Pending],
        1,
    );

    assert!(manager.start().is_err());
    assert_eq!(state.lock().expect("fake state").terminates, 0);
}

#[test]
fn stop_is_idempotent_and_drop_cleans_up_a_ready_child() {
    let layout = TempLayout::new();
    let state = Arc::new(Mutex::new(FakeState {
        validation_succeeds: true,
        ..FakeState::default()
    }));
    {
        let mut manager = manager(
            layout.config(),
            Arc::clone(&state),
            vec![ProbeStatus::Ready],
            1,
        );
        manager.start().expect("engine becomes ready");
        manager.stop().expect("first stop");
        manager.stop().expect("second stop");
        assert_eq!(*manager.state(), EngineState::Stopped);
    }
    assert_eq!(state.lock().expect("fake state").terminates, 1);

    {
        let mut manager = manager(
            layout.config(),
            Arc::clone(&state),
            vec![ProbeStatus::Ready],
            1,
        );
        manager.start().expect("engine becomes ready");
    }
    assert_eq!(state.lock().expect("fake state").terminates, 2);
}

#[test]
fn failed_stop_retains_the_owned_child_for_drop_cleanup() {
    let layout = TempLayout::new();
    let state = Arc::new(Mutex::new(FakeState {
        validation_succeeds: true,
        terminate_failures_remaining: 1,
        ..FakeState::default()
    }));
    {
        let mut manager = manager(
            layout.config(),
            Arc::clone(&state),
            vec![ProbeStatus::Ready],
            1,
        );
        manager.start().expect("engine becomes ready");
        assert!(manager.stop().is_err());
        assert_eq!(state.lock().expect("fake state").terminates, 1);
    }
    assert_eq!(state.lock().expect("fake state").terminates, 2);
}

#[cfg(unix)]
#[test]
fn standard_adapter_runs_validation_and_owns_a_real_fixture_process() {
    let layout = TempLayout::new();
    fs::write(
        &layout.binary,
        b"#!/bin/sh\nif [ \"$1\" = \"-t\" ]; then exit 0; fi\nwhile :; do sleep 60; done\n",
    )
    .expect("write executable fixture");
    let mut permissions = fs::metadata(&layout.binary)
        .expect("fixture metadata")
        .permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(&layout.binary, permissions).expect("make fixture executable");

    let mut manager = EngineManager::new(
        layout.config(),
        ReadinessPolicy::new(1, Duration::ZERO).expect("readiness policy"),
        Box::new(ScriptedProbe {
            results: vec![ProbeStatus::Ready],
            cursor: 0,
        }),
    );

    manager.start().expect("fixture process starts");
    assert!(matches!(manager.state(), EngineState::Ready { .. }));
    manager.stop().expect("fixture process stops");
    assert_eq!(*manager.state(), EngineState::Stopped);
}

#[cfg(unix)]
#[test]
fn standard_validation_times_out_and_reaps_a_hung_fixture() {
    let layout = TempLayout::new();
    fs::write(
        &layout.binary,
        b"#!/bin/sh\nif [ \"$1\" = \"-t\" ]; then while :; do :; done; fi\nexit 0\n",
    )
    .expect("write executable fixture");
    let mut permissions = fs::metadata(&layout.binary)
        .expect("fixture metadata")
        .permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(&layout.binary, permissions).expect("make fixture executable");
    let mut manager = EngineManager::new(
        layout.config(),
        ReadinessPolicy::default(),
        Box::new(ScriptedProbe {
            results: vec![ProbeStatus::Ready],
            cursor: 0,
        }),
    )
    .with_validation_timeout(Duration::from_millis(50))
    .expect("validation timeout");

    let started = Instant::now();
    let error = manager.start().expect_err("validation must time out");

    assert!(started.elapsed() < Duration::from_secs(2));
    assert!(matches!(
        error,
        super::EngineError::Io { source, .. }
            if source.kind() == std::io::ErrorKind::TimedOut
    ));
}
