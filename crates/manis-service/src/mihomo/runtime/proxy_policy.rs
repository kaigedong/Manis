use std::sync::{Arc, Mutex};

use manis_core::KernelKind;
use manis_engine::EngineManager;

use crate::diagnostics::{LogLevel, record_event};

use super::super::{
    ControllerRuntime, LoadError, ManagedGeneratedProfile, compile_managed_generated_profile,
    reload_mihomo_config, render_generated_profile_with_tun, running_managed_endpoint,
};

impl ControllerRuntime {
    pub(crate) fn set_tun_enabled(&self, enabled: bool) -> Result<(), LoadError> {
        record_event(
            LogLevel::Info,
            "controller.tun.requested",
            format!(
                "enabled={enabled} ownership={}",
                if matches!(self, Self::Managed { .. }) {
                    "managed"
                } else if self.is_fixture() {
                    "fixture"
                } else {
                    "invalid"
                }
            ),
        );
        let (manager, spec) = self.managed_mihomo_tun_parts()?;
        let profile = compile_managed_generated_profile(spec)?;
        let payload = render_generated_profile_with_tun(spec, &profile, enabled)?;
        if enabled {
            self.prepare_tun_activation()?;
        }
        let controller_secret = self.controller_secret();
        let endpoint = running_managed_endpoint(manager)?;
        record_event(
            LogLevel::Info,
            "controller.tun.config_reload.requested",
            format!(
                "enabled={enabled} method=PUT endpoint=/configs?force=true bytes={}",
                payload.len()
            ),
        );
        let result = reload_mihomo_config(&endpoint, &payload, enabled, controller_secret)
            .map_err(LoadError::from);
        if result.is_ok() {
            record_event(
                LogLevel::Info,
                "controller.tun.config_reload.succeeded",
                format!("enabled={enabled} rebuild=general,dns,listeners,tun,providers"),
            );
        }
        #[cfg(target_os = "macos")]
        let result = match result {
            Ok(()) if !enabled => self.release_macos_tun_route(),
            result => result,
        };
        #[cfg(target_os = "macos")]
        if enabled && result.is_ok() {
            match crate::macos_privileged::existing_tun_route() {
                Ok(Some(route)) => {
                    record_event(LogLevel::Info, "controller.tun.route_confirmed", route);
                }
                Ok(None) => record_event(
                    LogLevel::Warn,
                    "controller.tun.route_missing",
                    "Mihomo accepted TUN enable but the expected split-default route was not found",
                ),
                Err(error) => record_event(
                    LogLevel::Warn,
                    "controller.tun.route_probe_failed",
                    error.to_string(),
                ),
            }
        }
        match &result {
            Ok(()) => record_event(
                LogLevel::Info,
                "controller.tun.succeeded",
                format!("enabled={enabled} endpoint={endpoint}"),
            ),
            Err(error) => record_event(
                LogLevel::Error,
                "controller.tun.failed",
                format!("enabled={enabled} error={error}"),
            ),
        }
        result
    }

    fn prepare_tun_activation(&self) -> Result<(), LoadError> {
        #[cfg(target_os = "macos")]
        {
            if let Some(conflict) =
                crate::macos_privileged::existing_tun_route().map_err(|error| {
                    LoadError::Runtime(format!("macOS TUN routes could not be inspected: {error}"))
                })?
            {
                record_event(LogLevel::Error, "controller.tun.conflict", conflict.clone());
                return Err(LoadError::Runtime(format!(
                    "another TUN is using the system proxy route ({conflict}); turn off TUN mode in other proxy applications first"
                )));
            }
            if let Err(error) = self.ensure_privileged_mihomo() {
                record_event(
                    LogLevel::Error,
                    "controller.tun.failed",
                    format!("enabled=true phase=privilege_promotion error={error}"),
                );
                return Err(error);
            }
            record_event(
                LogLevel::Info,
                "controller.tun.interface_selection",
                "strategy=auto-detect-interface",
            );
        }
        #[cfg(target_os = "linux")]
        if let Err(error) = self.ensure_linux_tun_capabilities() {
            record_event(
                LogLevel::Error,
                "controller.tun.failed",
                format!("enabled=true phase=capability_promotion error={error}"),
            );
            return Err(error);
        }
        Ok(())
    }

    fn managed_mihomo_tun_parts(
        &self,
    ) -> Result<(&Arc<Mutex<EngineManager>>, &ManagedGeneratedProfile), LoadError> {
        let Self::Managed {
            manager,
            generated_profile: Some(spec),
            ..
        } = self
        else {
            return Err(LoadError::Runtime(match self {
                #[cfg(test)]
                Self::Fixture { .. } => "test snapshots cannot enable TUN mode".to_owned(),
                Self::Invalid { message } => message.clone(),
                Self::Managed { .. } => {
                    "TUN mode requires a Mihomo configuration generated from saved Manis sources"
                        .to_owned()
                }
            }));
        };
        if spec.kernel != KernelKind::Mihomo {
            return Err(LoadError::Runtime(
                "the TUN configuration reload path supports only Mihomo".to_owned(),
            ));
        }
        Ok((manager, spec))
    }
}
