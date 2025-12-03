use bevy::color::Color;

/// General
pub const WIDTH: f32 = 1600.;
pub const HEIGHT: f32 = 900.;
pub const MESSAGE_DURATION: u64 = 5;

/// Menu
pub const SUBTITLE_TEXT_SIZE: f32 = 10.;
pub const TITLE_TEXT_SIZE: f32 = 15.;
pub const BUTTON_TEXT_SIZE: f32 = 20.;
pub const NORMAL_BUTTON_COLOR: Color = Color::srgb(0.15, 0.15, 0.15);
pub const HOVERED_BUTTON_COLOR: Color = Color::srgb_u8(59, 66, 82);
pub const PRESSED_BUTTON_COLOR: Color = Color::srgb_u8(95, 131, 175);
pub const DISABLED_BUTTON_COLOR: Color = Color::srgb(0.8, 0.5, 0.5);

/// Colors
pub const BG_COLOR: Color = Color::srgb_u8(12, 16, 20);
pub const SHIELD_COLOR: Color = Color::srgb_u8(0, 255, 255);
pub const OWN_COLOR: Color = Color::srgb_u8(102, 128, 255);
pub const ENEMY_COLOR: Color = Color::srgb_u8(255, 64, 32);

/// Camera
pub const MIN_ZOOM: f32 = 0.5;
pub const MAX_ZOOM: f32 = 1.4;
pub const ZOOM_FACTOR: f32 = 1.1;
pub const LERP_FACTOR: f32 = 0.05;

/// GAME
pub const SHIPYARD_PRODUCTION_FACTOR: usize = 5;
pub const FACTORY_PRODUCTION_FACTOR: usize = 5;
pub const SILO_CAPACITY_FACTOR: usize = 10;
pub const PROBES_PER_PRODUCTION_LEVEL: usize = 5;
pub const PS_SHIELD_PER_LEVEL: usize = 100;
pub const NEXUS_FACTOR: f32 = 0.1;
pub const PHALANX_DISTANCE: f32 = 0.8;
pub const RADAR_DISTANCE: f32 = 1.0;
pub const CRAWLER_HEALING_PER_ROUND: usize = 30;

/// Map
pub const BACKGROUND_Z: f32 = 0.;
pub const VORONOI_Z: f32 = 1.;
pub const PLANET_Z: f32 = 2.;
pub const MISSION_Z: f32 = 3.;
pub const EXPLOSION_Z: f32 = 4.;
pub const COMBAT_BACKGROUND_Z: f32 = 10.;
pub const COMBAT_SHIP_Z: f32 = 11.;

pub const PLANET_NAMES: [&str; 80] = [
    "Aegis", "Arcadia", "Arctur", "Avalon", "Bellax", "Boreal", "Calypso", "Ceryn", "Cindra",
    "Cydon", "Daedal", "Dione", "Drakar", "Elysia", "Eos", "Erebus", "Eryos", "Faelor", "Fomir",
    "Fortis", "Fornax", "Galix", "Ganymede", "Harrow", "Helion", "Hesper", "Hyperion", "Icarus",
    "Ilios", "Io", "Janus", "Jareth", "Juno", "Keplar", "Kestrel", "Korren", "Lacara", "Lyra",
    "Lyris", "Marduk", "Meris", "Morpheus", "Nereid", "Novan", "Noxus", "Nyx", "Oberon", "Orion",
    "Orpheus", "Othra", "Pegas", "Perra", "Phaen", "Pyrron", "Quasar", "Quill", "Qor", "Ragnar",
    "Rhea", "Riven", "Sable", "Selar", "Solar", "Styga", "Tethys", "Thalos", "Titan", "Umbra",
    "Umbril", "Ulyss", "Vela", "Vesper", "Vortan", "Wyvern", "Xandar", "Xyra", "Yalen", "Ythra",
    "Zaurak", "Zephyr",
];
