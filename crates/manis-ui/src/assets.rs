use std::borrow::Cow;

use gpui::{AssetSource, Result, SharedString};

pub(crate) const BRAND_MARK_PATH: &str = "brand/manis-mark.svg";

pub(crate) const BRAND_MARK: &[u8] = include_bytes!("../../../assets/brand/manis-mark.svg");
#[cfg(any(target_os = "macos", test))]
pub(crate) const TRAY_TEMPLATE: &[u8] =
    include_bytes!("../../../assets/brand/manis-tray-template.svg");

/// Serves Manis-owned assets alongside the icons bundled by gpui-component.
pub struct Assets;

// The default component bundle omits the full catalog. Embed these extra Lucide icons.
gpui_component_assets::icon_assets!(
    ExtraIcons,
    [GripVertical, LayoutList, Network, Pin, Workflow]
);

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if path == BRAND_MARK_PATH {
            return Ok(Some(Cow::Borrowed(BRAND_MARK)));
        }
        if let Some(bytes) = ExtraIcons.load(path)? {
            return Ok(Some(bytes));
        }
        gpui_component_assets::Assets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut assets = gpui_component_assets::Assets.list(path)?;
        assets.extend(ExtraIcons.list(path)?);
        if BRAND_MARK_PATH.starts_with(path) {
            assets.push(BRAND_MARK_PATH.into());
        }
        Ok(assets)
    }
}

#[cfg(test)]
mod tests {
    use gpui::AssetSource as _;

    use super::{Assets, BRAND_MARK_PATH, TRAY_TEMPLATE};

    #[test]
    fn rule_drag_handle_is_embedded_alongside_the_component_icons() {
        for icon in [
            gpui_component_assets::IconName::GripVertical,
            gpui_component_assets::IconName::LayoutList,
            gpui_component_assets::IconName::Network,
            gpui_component_assets::IconName::Pin,
            gpui_component_assets::IconName::Workflow,
        ] {
            let path = icon.path();
            assert!(Assets.load(&path).unwrap().is_some(), "missing {path}");
            assert!(
                Assets.list("icons/").unwrap().contains(&path),
                "unlisted {path}"
            );
        }
    }

    fn assert_safe_embedded_svg(bytes: &[u8]) {
        let svg = std::str::from_utf8(bytes).expect("brand asset is UTF-8 SVG");

        assert!(svg.contains("<svg"));
        assert!(!svg.contains("<script"));
        assert!(!svg.contains("<foreignObject"));
        assert!(!svg.contains("href=\"http://") && !svg.contains("href=\"https://"));
    }

    #[test]
    fn brand_mark_is_embedded_without_remote_or_executable_content() {
        let bytes = Assets
            .load(BRAND_MARK_PATH)
            .expect("load embedded brand mark")
            .expect("brand mark exists");
        assert_safe_embedded_svg(&bytes);
        assert_safe_embedded_svg(TRAY_TEMPLATE);
    }
}
