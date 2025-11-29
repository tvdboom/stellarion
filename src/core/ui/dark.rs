use bevy_egui::egui::{Color32, Vec2};

use crate::core::constants::OWN_COLOR;
use crate::core::ui::aesthetics::Aesthetics;
use crate::utils::ToColor32;

pub struct NordDark;

impl Aesthetics for NordDark {
    fn name(&self) -> &str {
        "Nord Dark"
    }

    fn primary_accent_color_visuals(&self) -> Color32 {
        OWN_COLOR.to_color32()
    }

    fn bg_primary_color_visuals(&self) -> Color32 {
        Color32::from_rgb(14, 21, 26)
    }

    fn bg_secondary_color_visuals(&self) -> Color32 {
        Color32::from_rgb(59, 66, 82)
    }

    fn bg_triage_color_visuals(&self) -> Color32 {
        Color32::from_rgb(59, 66, 82)
    }

    fn bg_auxiliary_color_visuals(&self) -> Color32 {
        Color32::from_rgb(59, 66, 82)
    }

    fn bg_contrast_color_visuals(&self) -> Color32 {
        Color32::from_rgb(59, 66, 82)
    }

    fn fg_primary_text_color_visuals(&self) -> Option<Color32> {
        Some(Color32::from_rgb(216, 222, 233))
    }

    fn fg_warn_text_color_visuals(&self) -> Color32 {
        Color32::from_rgb(255, 215, 64)
    }

    fn fg_error_text_color_visuals(&self) -> Color32 {
        Color32::from_rgb(255, 121, 121)
    }

    fn dark_mode_visuals(&self) -> bool {
        true
    }

    fn margin_style(&self) -> i8 {
        12
    }

    fn button_padding(&self) -> Vec2 {
        Vec2 {
            x: 12.0,
            y: 10.0,
        }
    }

    fn item_spacing_style(&self) -> f32 {
        18.0
    }

    fn rounding_visuals(&self) -> u8 {
        6
    }
}
