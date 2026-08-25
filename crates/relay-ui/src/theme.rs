#![allow(clippy::unreadable_literal)]

use gpui::{Rgba, rgb};

#[derive(Clone, Copy)]
pub(crate) struct Theme {
    pub surface_base: Rgba,
    pub surface_low: Rgba,
    pub surface_high: Rgba,
    pub surface_chrome: Rgba,
    pub text_primary: Rgba,
    pub text_secondary: Rgba,
    pub text_tertiary: Rgba,
    pub outline_subtle: Rgba,
    pub outline_strong: Rgba,
    pub action_primary: Rgba,
    pub action_on_primary: Rgba,
    pub action_soft: Rgba,
    pub route_trace: Rgba,
    pub route_soft: Rgba,
    pub status_success: Rgba,
    pub status_error: Rgba,
}

impl Theme {
    pub(crate) fn light() -> Self {
        Self {
            surface_base: rgb(0xf4f7f5),
            surface_low: rgb(0xedf2ef),
            surface_high: rgb(0xffffff),
            surface_chrome: rgb(0xe7eeea),
            text_primary: rgb(0x152321),
            text_secondary: rgb(0x5f6e69),
            text_tertiary: rgb(0x84918d),
            outline_subtle: rgb(0xcbd6d2),
            outline_strong: rgb(0x9fafa9),
            action_primary: rgb(0x176c62),
            action_on_primary: rgb(0xffffff),
            action_soft: rgb(0xd5ebe6),
            route_trace: rgb(0xd46642),
            route_soft: rgb(0xf8e5dc),
            status_success: rgb(0x24795f),
            status_error: rgb(0xb54f49),
        }
    }

    pub(crate) fn dark() -> Self {
        Self {
            surface_base: rgb(0x0e1715),
            surface_low: rgb(0x111d1a),
            surface_high: rgb(0x172521),
            surface_chrome: rgb(0x13211e),
            text_primary: rgb(0xe3eeea),
            text_secondary: rgb(0xa4b4ae),
            text_tertiary: rgb(0x7d8e88),
            outline_subtle: rgb(0x2b3d37),
            outline_strong: rgb(0x435851),
            action_primary: rgb(0x79d7c6),
            action_on_primary: rgb(0x082a24),
            action_soft: rgb(0x1b4038),
            route_trace: rgb(0xf39b75),
            route_soft: rgb(0x402820),
            status_success: rgb(0x79d7b0),
            status_error: rgb(0xef8c84),
        }
    }
}
