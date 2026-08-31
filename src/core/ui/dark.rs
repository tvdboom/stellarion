//! Stellarion's dark egui theme palette and spacing configuration.

use bevy_egui::egui::{Color32, Vec2};

use crate::core::constants::OWN_COLOR;
use crate::core::ui::aesthetics::Aesthetics;
use crate::utils::ToColor32;

/// Factory for the dark Nord-inspired egui palette.
pub struct NordDark;

impl Aesthetics for NordDark {
    /// Returns the stable identifier for this visual theme.
    fn name(&self) -> &str {
        "Nord Dark"
    }

    /// Returns the primary interactive accent color.
    fn primary_accent_color_visuals(&self) -> Color32 {
        OWN_COLOR.to_color32()
    }

    /// Returns the primary background color.
    fn bg_primary_color_visuals(&self) -> Color32 {
        Color32::from_rgb(14, 21, 26)
    }

    /// Returns the secondary panel background color.
    fn bg_secondary_color_visuals(&self) -> Color32 {
        Color32::from_rgb(59, 66, 82)
    }

    /// Returns the tertiary panel background color.
    fn bg_triage_color_visuals(&self) -> Color32 {
        Color32::from_rgb(59, 66, 82)
    }

    /// Returns the auxiliary background color.
    fn bg_auxiliary_color_visuals(&self) -> Color32 {
        Color32::from_rgb(59, 66, 82)
    }

    /// Returns the high-contrast background color.
    fn bg_contrast_color_visuals(&self) -> Color32 {
        Color32::from_rgb(59, 66, 82)
    }

    /// Returns the normal foreground text color.
    fn fg_primary_text_color_visuals(&self) -> Option<Color32> {
        Some(Color32::from_rgb(216, 222, 233))
    }

    /// Returns the warning foreground text color.
    fn fg_warn_text_color_visuals(&self) -> Color32 {
        Color32::from_rgb(255, 215, 64)
    }

    /// Returns the error foreground text color.
    fn fg_error_text_color_visuals(&self) -> Color32 {
        Color32::from_rgb(255, 121, 121)
    }

    /// Builds the complete dark egui visual palette.
    fn dark_mode_visuals(&self) -> bool {
        true
    }

    /// Returns the standard panel content margin.
    fn margin_style(&self) -> i8 {
        12
    }

    /// Returns the standard button padding.
    fn button_padding(&self) -> Vec2 {
        Vec2 {
            x: 12.0,
            y: 10.0,
        }
    }

    /// Returns standard spacing between adjacent widgets.
    fn item_spacing_style(&self) -> f32 {
        18.0
    }

    /// Returns standard widget corner rounding.
    fn rounding_visuals(&self) -> u8 {
        6
    }
}
