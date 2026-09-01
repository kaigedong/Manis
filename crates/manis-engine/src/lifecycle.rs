use std::{thread, time::Duration};

use crate::{
    ControllerEndpoint, EngineError, ManagedChild, ManagedEngineConfig, ProcessSpawner,
    StdProcessSpawner, config::prepare_data_dir,
};

const DEFAULT_VALIDATION_TIMEOUT: Duration = Duration::from_secs(10);

/// Result of one controller readiness probe.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProbeStatus {
    /// The controller accepted and answered a health request.
    Ready,
    /// The child may still be starting.
    Pending,
}

/// Readiness adapter, normally backed by a read-only controller `/version` request.
pub trait ReadinessProbe: Send {
    /// Checks the controller without changing its configuration.
    fn check(&mut self, endpoint: &ControllerEndpoint) -> ProbeStatus;
}

/// Bounded readiness attempts and delay between them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReadinessPolicy {
    attempts: usize,
    delay: Duration,
}

impl ReadinessPolicy {
    /// Creates a bounded readiness policy.
    ///
    /// # Errors
    ///
    /// Returns an error when `attempts` is zero.
    pub fn new(attempts: usize, delay: Duration) -> Result<Self, EngineError> {
        if attempts == 0 {
            return Err(EngineError::InvalidConfig(
                "readiness attempts must be greater than zero".to_owned(),
            ));
        }
        Ok(Self { attempts, delay })
    }
}

impl Default for ReadinessPolicy {
    fn default() -> Self {
        Self {
            attempts: 50,
            delay: Duration::from_millis(100),
        }
    }
}

/// Observable lifecycle state for a managed core.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EngineState {
    /// No child has been started.
    Idle,
    /// Runtime paths are being checked and created.
    Preparing,
    /// The selected kernel is checking the supplied configuration.
    Validating,
    /// The child exists but its controller is not ready yet.
    Starting,
    /// The controller answered and the owned child is running.
    Ready {
        /// Display-only child identifier. It is never used to terminate a process.
        pid: u32,
        /// Private controller endpoint for this child.
        endpoint: ControllerEndpoint,
    },
    /// The exact owned child is being terminated.
    Stopping,
    /// No owned child remains.
    Stopped,
    /// Start or stop failed; the message contains no subscription or API secret.
    Failed {
        /// Safe lifecycle diagnostic.
        message: String,
    },
}

/// Owns at most one child spawned from an isolated managed configuration.
pub struct EngineManager {
    config: ManagedEngineConfig,
    readiness: ReadinessPolicy,
    validation_timeout: Duration,
    state: EngineState,
    child: Option<Box<dyn ManagedChild>>,
    spawner: Box<dyn ProcessSpawner>,
    probe: Box<dyn ReadinessProbe>,
}

impl EngineManager {
    /// Creates a manager using the standard-library process adapter.
    #[must_use]
    pub fn new(
        config: ManagedEngineConfig,
        readiness: ReadinessPolicy,
        probe: Box<dyn ReadinessProbe>,
    ) -> Self {
        Self::with_adapters(
            config,
            readiness,
            Box::<StdProcessSpawner>::default(),
            probe,
        )
    }

    /// Creates a manager with explicit process and probe adapters.
    ///
    /// This is primarily useful for deterministic lifecycle tests.
    #[must_use]
    pub fn with_adapters(
        config: ManagedEngineConfig,
        readiness: ReadinessPolicy,
        spawner: Box<dyn ProcessSpawner>,
        probe: Box<dyn ReadinessProbe>,
    ) -> Self {
        Self {
            config,
            readiness,
            validation_timeout: DEFAULT_VALIDATION_TIMEOUT,
            state: EngineState::Idle,
            child: None,
            spawner,
            probe,
        }
    }

    /// Returns the current managed lifecycle state.
    #[must_use]
    pub const fn state(&self) -> &EngineState {
        &self.state
    }

    /// Overrides the bounded kernel validation timeout.
    ///
    /// # Errors
    ///
    /// Returns an error when `timeout` is zero.
    pub fn with_validation_timeout(mut self, timeout: Duration) -> Result<Self, EngineError> {
        if timeout.is_zero() {
            return Err(EngineError::InvalidConfig(
                "validation timeout must be greater than zero".to_owned(),
            ));
        }
        self.validation_timeout = timeout;
        Ok(self)
    }

    /// Returns the endpoint only while the exact owned child is still running.
    ///
    /// A detected exit is reaped and changes the state to [`EngineState::Failed`], allowing the
    /// next call to [`Self::start`] to create a fresh child.
    ///
    /// # Errors
    ///
    /// Returns an error when the operating system cannot query the owned child or when the child
    /// has exited since its controller became ready.
    pub fn running_endpoint(&mut self) -> Result<Option<ControllerEndpoint>, EngineError> {
        let Some(child) = self.child.as_mut() else {
            return Ok(None);
        };
        let exit = child.try_wait().map_err(|source| EngineError::Io {
            operation: "poll managed kernel",
            source,
        })?;
        if let Some(exit) = exit {
            self.child = None;
            return self.fail(EngineError::Exited(exit));
        }
        match &self.state {
            EngineState::Ready { endpoint, .. } => Ok(Some(endpoint.clone())),
            _ => Ok(None),
        }
    }

    /// Validates, starts, and waits for the owned controller to become ready.
    ///
    /// # Errors
    ///
    /// Returns a structured error when validation, spawn, readiness, or cleanup fails.
    pub fn start(&mut self) -> Result<ControllerEndpoint, EngineError> {
        if self.child.is_some() {
            return self.fail(EngineError::AlreadyRunning);
        }
        self.state = EngineState::Preparing;
        if let Err(error) = self.config.validate() {
            return self.fail(error);
        }
        if let Err(error) = prepare_data_dir(self.config.data_dir()) {
            return self.fail(error);
        }

        self.state = EngineState::Validating;
        let validation = match self
            .spawner
            .validate(&self.config.validation_command(), self.validation_timeout)
        {
            Ok(exit) => exit,
            Err(source) => {
                return self.fail(EngineError::Io {
                    operation: "run kernel config validation",
                    source,
                });
            }
        };
        if !validation.is_success() {
            return self.fail(EngineError::ValidationFailed(validation));
        }

        self.state = EngineState::Starting;
        let child = match self.spawner.spawn(&self.config.launch_command()) {
            Ok(child) => child,
            Err(source) => {
                return self.fail(EngineError::Io {
                    operation: "spawn managed kernel",
                    source,
                });
            }
        };
        let pid = child.id();
        self.child = Some(child);

        for attempt in 0..self.readiness.attempts {
            let Some(child) = self.child.as_mut() else {
                return self.fail(EngineError::InvalidConfig(
                    "managed child ownership was lost during startup".to_owned(),
                ));
            };
            let exit = match child.try_wait() {
                Ok(exit) => exit,
                Err(source) => {
                    let error = EngineError::Io {
                        operation: "poll managed kernel",
                        source,
                    };
                    return self.fail_after_cleanup(error);
                }
            };
            if let Some(exit) = exit {
                self.child = None;
                return self.fail(EngineError::ExitedEarly(exit));
            }
            if self.probe.check(self.config.controller()) == ProbeStatus::Ready {
                let endpoint = self.config.controller().clone();
                self.state = EngineState::Ready {
                    pid,
                    endpoint: endpoint.clone(),
                };
                return Ok(endpoint);
            }
            if attempt + 1 < self.readiness.attempts && !self.readiness.delay.is_zero() {
                thread::sleep(self.readiness.delay);
            }
        }

        self.fail_after_cleanup(EngineError::ReadinessTimeout {
            attempts: self.readiness.attempts,
        })
    }

    /// Terminates and reaps only the exact child owned by this manager.
    ///
    /// Calling this method without an owned child is safe and idempotent.
    ///
    /// # Errors
    ///
    /// Returns an error when the operating system cannot terminate or reap the owned child.
    pub fn stop(&mut self) -> Result<(), EngineError> {
        let Some(mut child) = self.child.take() else {
            self.state = EngineState::Stopped;
            return Ok(());
        };
        self.state = EngineState::Stopping;
        match child.terminate() {
            Ok(_) => {
                self.state = EngineState::Stopped;
                Ok(())
            }
            Err(source) => {
                self.child = Some(child);
                self.fail(EngineError::Io {
                    operation: "terminate managed kernel",
                    source,
                })
            }
        }
    }

    fn fail<T>(&mut self, error: EngineError) -> Result<T, EngineError> {
        self.state = EngineState::Failed {
            message: error.to_string(),
        };
        Err(error)
    }

    fn fail_after_cleanup<T>(&mut self, error: EngineError) -> Result<T, EngineError> {
        if let Some(mut child) = self.child.take()
            && let Err(source) = child.terminate()
        {
            self.child = Some(child);
            return self.fail(EngineError::Io {
                operation: "clean up managed kernel after failed start",
                source,
            });
        }
        self.fail(error)
    }
}

impl Drop for EngineManager {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.terminate();
        }
    }
}
