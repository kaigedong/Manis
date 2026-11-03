#[cfg(any(target_os = "macos", target_os = "linux"))]
use super::delete_tun_dns_recovery_snapshot;
#[cfg(target_os = "linux")]
use super::linux;
#[cfg(target_os = "macos")]
use super::macos;
#[cfg(target_os = "windows")]
use super::windows;
use super::{Language, ProxyPorts, SystemProxyError, copy, delete_recovery_snapshot};

#[derive(Debug, Default)]
pub(crate) struct SystemProxySession {
    #[cfg(target_os = "macos")]
    previous: Vec<macos::ServiceSnapshot>,
    #[cfg(target_os = "linux")]
    previous: Option<linux::GnomeSnapshot>,
    #[cfg(target_os = "windows")]
    previous: Option<windows::WinInetSnapshot>,
    applied: bool,
}

impl SystemProxySession {
    #[must_use]
    pub(crate) const fn is_applied(&self) -> bool {
        self.applied
    }

    #[cfg(not(test))]
    pub(crate) fn recover_stale_with_language(
        &mut self,
        language: Language,
    ) -> Result<(), SystemProxyError> {
        #[cfg(target_os = "macos")]
        macos::recover_stale(language)?;
        #[cfg(target_os = "linux")]
        linux::recover_stale(language)?;
        #[cfg(target_os = "windows")]
        windows::recover_stale(language)?;

        self.applied = false;
        Ok(())
    }

    pub(crate) fn enable_with_language(
        &mut self,
        ports: ProxyPorts,
        language: Language,
    ) -> Result<(), SystemProxyError> {
        let ports = ports.usable_with_language(language)?;
        if self.applied {
            self.disable_with_language(language)?;
        }

        #[cfg(target_os = "macos")]
        {
            self.previous = macos::enable(ports, language)?;
        }
        #[cfg(target_os = "linux")]
        {
            self.previous = Some(linux::enable(ports, language)?);
        }
        #[cfg(target_os = "windows")]
        {
            self.previous = Some(windows::enable(ports, language)?);
        }
        #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
        {
            let _ = ports;
            return Err(SystemProxyError::Unavailable(
                language
                    .localized(copy::system_proxy::MANIS_CANNOT_CONFIGURE_THE_SYSTEM_PROXY_ON_THIS_DESKTOP_YET)
                    .to_owned(),
            ));
        }

        self.applied = true;
        Ok(())
    }

    pub(crate) fn shutdown_with_language(
        &mut self,
        language: Language,
    ) -> Result<(), SystemProxyError> {
        self.disable_with_language(language)
    }

    pub(crate) fn disable_with_language(
        &mut self,
        language: Language,
    ) -> Result<(), SystemProxyError> {
        if !self.applied {
            return Ok(());
        }

        #[cfg(target_os = "macos")]
        {
            macos::restore(&self.previous, language)?;
            delete_recovery_snapshot(language)?;
        }
        #[cfg(target_os = "linux")]
        if let Some(previous) = self.previous.as_ref() {
            linux::restore(previous, language)?;
            delete_recovery_snapshot(language)?;
        }
        #[cfg(target_os = "windows")]
        if let Some(previous) = self.previous.as_ref() {
            windows::restore(previous, language)?;
            delete_recovery_snapshot(language)?;
        }

        self.applied = false;
        Ok(())
    }
}

#[derive(Debug, Default)]
pub(crate) struct TunDnsSession {
    #[cfg(target_os = "macos")]
    previous: Option<macos::DnsSnapshot>,
    #[cfg(target_os = "linux")]
    previous: Option<linux::DnsSnapshot>,
    prepared: bool,
    applied: bool,
}

impl TunDnsSession {
    #[cfg(not(test))]
    pub(crate) fn recover_stale_with_language(
        &mut self,
        language: Language,
    ) -> Result<(), SystemProxyError> {
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        let _ = language;
        #[cfg(target_os = "macos")]
        macos::recover_stale_tun_dns(language)?;
        #[cfg(target_os = "linux")]
        linux::recover_stale_tun_dns(language)?;

        self.prepared = false;
        self.applied = false;
        Ok(())
    }

    pub(crate) fn prepare_with_language(
        &mut self,
        language: Language,
    ) -> Result<(), SystemProxyError> {
        if self.prepared || self.applied {
            self.disable_with_language(language)?;
        }

        #[cfg(target_os = "macos")]
        {
            self.previous = Some(macos::prepare_tun_dns(language)?);
        }
        #[cfg(target_os = "linux")]
        {
            self.previous = Some(linux::prepare_tun_dns(language)?);
        }
        self.prepared = true;
        Ok(())
    }

    pub(crate) fn activate_with_language(
        &mut self,
        language: Language,
    ) -> Result<(), SystemProxyError> {
        if !self.prepared {
            return Err(SystemProxyError::Unavailable(
                language
                    .localized(copy::system_proxy::TUN_DNS_WAS_NOT_PREPARED_BEFORE_ACTIVATION)
                    .to_owned(),
            ));
        }

        self.applied = true;
        #[cfg(target_os = "macos")]
        if let Some(previous) = self.previous.as_ref() {
            macos::apply_tun_dns(previous, language)?;
        }
        #[cfg(target_os = "linux")]
        if let Some(previous) = self.previous.as_ref() {
            linux::apply_tun_dns(previous, language)?;
        }
        Ok(())
    }

    pub(crate) fn disable_with_language(
        &mut self,
        language: Language,
    ) -> Result<(), SystemProxyError> {
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        let _ = language;
        if !self.prepared && !self.applied {
            return Ok(());
        }

        #[cfg(target_os = "macos")]
        if let Some(previous) = self.previous.as_ref() {
            if self.applied {
                macos::restore_tun_dns(previous, language)?;
            }
            delete_tun_dns_recovery_snapshot(language)?;
            self.previous = None;
        }
        #[cfg(target_os = "linux")]
        if let Some(previous) = self.previous.as_ref() {
            if self.applied {
                linux::restore_tun_dns(previous, language)?;
            }
            delete_tun_dns_recovery_snapshot(language)?;
            self.previous = None;
        }

        self.prepared = false;
        self.applied = false;
        Ok(())
    }

    pub(crate) fn shutdown_with_language(
        &mut self,
        language: Language,
    ) -> Result<(), SystemProxyError> {
        self.disable_with_language(language)
    }
}
