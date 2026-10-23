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

/// Renders a profile for the Android `VpnService` interface Mihomo inherits.
///
/// The Android host owns the interface, addresses, routes and DNS server; Mihomo only reads the
/// inherited descriptor. See [`crate::render_mihomo_yaml_with_tun`] for the desktop variant.
///
/// # Errors
/// Returns a redacted validation error, or `InvalidValue` when the descriptor is negative. A
/// successful result contains the subscription URL and must be treated as secret material.
pub fn render_mihomo_yaml_for_android(
    profile: &Profile,
    file_descriptor: i32,
) -> Result<String, ProfileError> {
    profile.validate()?;
    render::mihomo_android(profile, file_descriptor)
}
