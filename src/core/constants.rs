use bevy::color::Color;

/// Window
pub const WIDTH: f32 = 1600.;
pub const HEIGHT: f32 = 900.;

/// Menu
pub const LABEL_TEXT_SIZE: f32 = 10.;
pub const BUTTON_TEXT_SIZE: f32 = 20.;
pub const SUBTITLE_TEXT_SIZE: f32 = 15.;
pub const TITLE_TEXT_SIZE: f32 = 25.;
pub const NORMAL_BUTTON_COLOR: Color = Color::srgb(0.15, 0.15, 0.15);
pub const HOVERED_BUTTON_COLOR: Color = Color::srgb(0.25, 0.25, 0.25);
pub const PRESSED_BUTTON_COLOR: Color = Color::srgb(0.55, 0.55, 0.65);
pub const DISABLED_BUTTON_COLOR: Color = Color::srgb(0.5, 0.5, 0.8);

/// Camera
pub const MIN_ZOOM: f32 = 0.5;
pub const MAX_ZOOM: f32 = 1.2;
pub const ZOOM_FACTOR: f32 = 1.1;
pub const LERP_FACTOR: f32 = 0.05;

/// Settings
pub const MIN_PLANETS: u32 = 10;
pub const MAX_PLANETS: u32 = 50;

/// Map
pub const BACKGROUND_Z: f32 = 0.;
pub const PLANET_Z: f32 = 1.;
