use std::borrow::Cow;

use gpui::{AssetSource, Result, SharedString};

pub(crate) const BRAND_MARK_PATH: &str = "brand/manis-mark.svg";

const BRAND_MARK: &[u8] = include_bytes!("../../../assets/brand/manis-mark.svg");

/// Serves Manis-owned assets alongside the icons bundled by gpui-component.
pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if path == BRAND_MARK_PATH {
            return Ok(Some(Cow::Borrowed(BRAND_MARK)));
        }
        gpui_component_assets::Assets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut assets = gpui_component_assets::Assets.list(path)?;
        if BRAND_MARK_PATH.starts_with(path) {
            assets.push(BRAND_MARK_PATH.into());
        }
        Ok(assets)
    }
}

#[cfg(test)]
mod tests {
    use gpui::AssetSource as _;

    use super::{Assets, BRAND_MARK_PATH};

    #[test]
    fn brand_mark_is_embedded_without_remote_or_executable_content() {
        let bytes = Assets
            .load(BRAND_MARK_PATH)
            .expect("load embedded brand mark")
            .expect("brand mark exists");
        let svg = std::str::from_utf8(&bytes).expect("brand mark is UTF-8 SVG");

        assert!(svg.contains("<svg"));
        assert!(!svg.contains("<script"));
        assert!(!svg.contains("<foreignObject"));
        assert!(!svg.contains("href=\"http://") && !svg.contains("href=\"https://"));
    }
}
