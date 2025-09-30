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
pub const PRESSED_BUTTON_COLOR: Color = Color::srgb(0.35, 0.65, 0.35);
pub const DISABLED_BUTTON_COLOR: Color = Color::srgb(0.8, 0.5, 0.5);

/// Camera
pub const MIN_ZOOM: f32 = 0.2;
pub const MAX_ZOOM: f32 = 1.;
pub const ZOOM_FACTOR: f32 = 1.1;
pub const LERP_FACTOR: f32 = 0.05;
