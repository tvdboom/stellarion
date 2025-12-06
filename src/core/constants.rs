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
pub const BG2_COLOR: Color = Color::srgb_u8(40, 40, 40);
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
pub const CRAWLER_HEALING_PER_ROUND: usize = 50;

/// Combat
pub const SETUP_TIME: u64 = 2;
pub const UNIT_SIZE: f32 = 120.;
pub const PS_WIDTH: f32 = 11.;
pub const COMBAT_BACKGROUND_Z: f32 = 10.;
pub const COMBAT_SHIP_Z: f32 = 11.;
pub const COMBAT_EXPLOSION_Z: f32 = 12.;

/// Map
pub const BACKGROUND_Z: f32 = 0.;
pub const VORONOI_Z: f32 = 1.;
pub const PLANET_Z: f32 = 2.;
pub const MISSION_Z: f32 = 3.;
pub const EXPLOSION_Z: f32 = 4.;

pub const PLANET_NAMES: [&str; 162] = [
    "Abrax", "Aegis", "Aether", "Aleron", "Andros", "Arcadia", "Arctur", "Arvend", "Astrix",
    "Atreon", "Avalon", "Auralis", "Bastor", "Belion", "Bellax", "Boreal", "Brelix", "Caelum",
    "Calypso", "Caldor", "Cenrix", "Ceryn", "Cerion", "Cindra", "Cindor", "Cydon", "Cyrex",
    "Cyther", "Daedal", "Dalian", "Darian", "Dione", "Drakar", "Dravos", "Drexis", "Eldros",
    "Elios", "Elysia", "Elion", "Embris", "Enyra", "Eos", "Erebus", "Eriath", "Erndor", "Erynd",
    "Faelor", "Falix", "Ferros", "Fomir", "Fortis", "Fornax", "Fynar", "Galix", "Galdor",
    "Ganymede", "Ganyr", "Ghorin", "Glyra", "Hadron", "Harrow", "Helion", "Helyx", "Hesper",
    "Horian", "Hyperion", "Hydra", "Icarus", "Ilios", "Ilmar", "Ilyon", "Inara", "Io", "Isyra",
    "Jadex", "Janus", "Jareth", "Jorun", "Juno", "Kaelis", "Keplar", "Keldor", "Kestrel", "Korren",
    "Kyros", "Lacara", "Lorian", "Lunex", "Lyra", "Lystr", "Lyris", "Maelis", "Marduk", "Marix",
    "Melyra", "Meris", "Morpheus", "Mydor", "Naelis", "Naryn", "Nereid", "Novan", "Noxus", "Nydon",
    "Nyx", "Oberon", "Olaris", "Onyx", "Ordan", "Orion", "Orpheus", "Oryth", "Othra", "Pelion",
    "Pegas", "Perra", "Phaen", "Pylar", "Pyrron", "Qimar", "Qor", "Quasar", "Quill", "Quorin",
    "Ragnar", "Ravon", "Relis", "Rhea", "Riven", "Rylar", "Sable", "Selar", "Selion", "Solar",
    "Styga", "Syron", "Taryn", "Tethys", "Thalos", "Theron", "Titan", "Torix", "Umbra", "Umbril",
    "Ularis", "Ulmar", "Ulyss", "Valen", "Vela", "Vesper", "Vortan", "Voryn", "Wyvern", "Xandar",
    "Xelra", "Xyra", "Yalen", "Ylros", "Ythra", "Zaryn", "Zaurak", "Zephyr",
];
