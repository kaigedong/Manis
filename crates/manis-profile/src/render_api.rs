use std::fmt;

use crate::{Profile, ProfileError, render};

/// Renders the small Manis profile schema as deterministic Mihomo YAML.
///
/// # Errors
/// Returns a redacted validation error. A successful result contains the subscription URL and
/// must itself be treated as secret material.
pub fn render_mihomo_yaml(profile: &Profile) -> Result<String, ProfileError> {
    render_mihomo_yaml_with_tun(profile, false)
}

/// Renders a deterministic Mihomo runtime profile with the requested TUN state.
///
/// This is used when the managed controller applies a complete configuration reload. Keeping the
/// flag in the renderer ensures the enabled and disabled configurations differ only at the owned
/// `tun.enable` field.
///
/// # Errors
/// Returns a redacted validation error. A successful result contains the subscription URL and
/// must itself be treated as secret material.
pub fn render_mihomo_yaml_with_tun(
    profile: &Profile,
    tun_enabled: bool,
) -> Result<String, ProfileError> {
    profile.validate()?;
    render::mihomo(profile, tun_enabled)
}

/// Private loopback Clash API settings embedded in a generated sing-box configuration.
#[derive(Clone, Eq, PartialEq)]
pub struct SingBoxOptions {
    pub(crate) controller: String,
    pub(crate) secret: String,
}

impl SingBoxOptions {
    #[must_use]
    pub fn new(controller: impl Into<String>, secret: impl Into<String>) -> Self {
        Self {
            controller: controller.into(),
            secret: secret.into(),
        }
    }

    fn validate(&self) -> Result<(), ProfileError> {
        let address = self
            .controller
            .parse::<std::net::SocketAddr>()
            .map_err(|_error| ProfileError::InvalidValue("sing-box controller"))?;
        if !address.ip().is_loopback() {
            return Err(ProfileError::InvalidValue("sing-box controller"));
        }
        if self.secret.is_empty()
            || self.secret.len() > 1024
            || self.secret.chars().any(char::is_control)
        {
            return Err(ProfileError::InvalidValue("sing-box controller secret"));
        }
        Ok(())
    }
}

impl fmt::Debug for SingBoxOptions {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SingBoxOptions")
            .field("controller", &self.controller)
            .field("secret", &"<redacted>")
            .finish()
    }
}

/// Renders the kernel-neutral Manis profile subset supported by sing-box 1.13+.
///
/// Subscription providers are rejected because sing-box cannot consume Mihomo provider files.
/// The caller must keep the returned JSON private because it contains node credentials and the
/// local controller secret.
///
/// # Errors
/// Returns a redacted error when the profile needs a feature that cannot be translated exactly.
pub fn render_sing_box_json(
    profile: &Profile,
    options: &SingBoxOptions,
) -> Result<String, ProfileError> {
    profile.validate()?;
    options.validate()?;
    render::sing_box(profile, options)
}
