//! Shared result artwork sizing and entrance timing for both replay renderers.

pub(crate) const ENTER_SECONDS: f32 = 1.5;
pub(crate) const BAR_HEIGHT_FRACTION: f32 = 0.18;
pub(crate) const BAR_MIN_HEIGHT: f32 = 64.0;
pub(crate) const BAR_MAX_HEIGHT: f32 = 140.0;
pub(crate) const BAR_ALPHA: u8 = 190;

pub(crate) struct ResultArtwork {
    pub width_fraction: f32,
    pub max_width: f32,
    /// The original square artwork has its lettering slightly above its center.
    pub center_offset: f32,
}

pub(crate) fn artwork(status: &str) -> ResultArtwork {
    match status {
        "draw" => ResultArtwork {
            width_fraction: 0.28,
            max_width: 330.0,
            center_offset: 0.029,
        },
        "defeat" => ResultArtwork {
            width_fraction: 0.41,
            max_width: 490.0,
            center_offset: 0.038,
        },
        _ => ResultArtwork {
            width_fraction: 0.38,
            max_width: 450.0,
            center_offset: 0.050,
        },
    }
}

/// Fades the full-sized artwork and belt together on the paused/scaled replay clock.
pub(crate) fn entrance_opacity(elapsed: f32) -> f32 {
    let t = (elapsed / ENTER_SECONDS).clamp(0.0, 1.0);
    if t < 0.5 {
        2.0 * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(2) / 2.0
    }
}

/// Changes only opacity so the schematic lettering never grows or moves during its entrance.
pub(crate) struct ImageFadeLens;

impl bevy_tweening::Lens<bevy::prelude::ImageNode> for ImageFadeLens {
    fn lerp(&mut self, mut target: bevy::prelude::Mut<bevy::prelude::ImageNode>, ratio: f32) {
        use bevy::prelude::Alpha;
        target.color.set_alpha(ratio);
    }
}
