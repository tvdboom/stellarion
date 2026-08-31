//! Shared gameplay, rendering, map, and UI constants.

use bevy::color::Color;

/// General
pub const WIDTH: f32 = 1600.;
/// Logical game-window height in pixels.
pub const HEIGHT: f32 = 900.;
/// Seconds a transient UI notification remains visible.
pub const MESSAGE_DURATION: u64 = 5;

/// Menu
pub const SUBTITLE_TEXT_SIZE: f32 = 10.;
/// Default title font size.
pub const TITLE_TEXT_SIZE: f32 = 15.;
/// Default menu-button font size.
pub const BUTTON_TEXT_SIZE: f32 = 20.;
/// Default idle menu-button color.
pub const NORMAL_BUTTON_COLOR: Color = Color::srgb(0.15, 0.15, 0.15);
/// Menu-button color while hovered.
pub const HOVERED_BUTTON_COLOR: Color = Color::srgb_u8(59, 66, 82);
/// Menu-button color while pressed.
pub const PRESSED_BUTTON_COLOR: Color = Color::srgb_u8(95, 131, 175);
/// Menu-button color while unavailable.
pub const DISABLED_BUTTON_COLOR: Color = Color::srgb(0.8, 0.5, 0.5);

/// Colors
pub const BG_COLOR: Color = Color::srgb_u8(12, 16, 20);
/// Secondary dark background color.
pub const BG2_COLOR: Color = Color::srgb_u8(40, 40, 40);
/// Combat shield indicator color.
pub const SHIELD_COLOR: Color = Color::srgb_u8(0, 255, 255);
/// Map color used for the local player's ownership.
pub const OWN_COLOR: Color = Color::srgb_u8(102, 128, 255);
/// Map color used for enemy ownership.
pub const ENEMY_COLOR: Color = Color::srgb_u8(255, 64, 32);

/// Camera
pub const MIN_ZOOM: f32 = 0.5;
/// Largest orthographic zoom scale allowed by the strategic camera.
pub const MAX_ZOOM: f32 = 1.4;
/// Multiplicative step applied to wheel zoom.
pub const ZOOM_FACTOR: f32 = 1.1;
/// Default interpolation fraction for smooth presentation movement.
pub const LERP_FACTOR: f32 = 0.05;

/// GAME
pub const SHIPYARD_PRODUCTION_FACTOR: usize = 5;
/// Defense-production capacity granted per factory level.
pub const FACTORY_PRODUCTION_FACTOR: usize = 5;
/// Missile capacity granted per silo level.
pub const SILO_CAPACITY_FACTOR: usize = 10;
/// Probe capacity granted per shipyard production level.
pub const PROBES_PER_PRODUCTION_LEVEL: usize = 5;
/// Planetary-shield strength granted per building level.
pub const PS_SHIELD_PER_LEVEL: usize = 100;
/// Fraction of owned structures removable per demolition-nexus level.
pub const NEXUS_FACTOR: f32 = 0.1;
/// Sensor-phalanx range measured in planet-size units.
pub const PHALANX_DISTANCE: f32 = 0.8;
/// Orbital-radar range measured in planet-size units.
pub const RADAR_DISTANCE: f32 = 1.0;
/// Hull points repaired by one crawler after each round.
pub const CRAWLER_HEALING_PER_ROUND: usize = 50;

/// Combat
pub const SETUP_TIME: u64 = 2;
/// Rendered combat-unit sprite size in pixels.
pub const UNIT_SIZE: f32 = 120.;
/// Rendered width of the planetary-shield arc.
pub const PS_WIDTH: f32 = 11.;
/// Render layer of the combat backdrop.
pub const COMBAT_BACKGROUND_Z: f32 = 10.;
/// Render layer of combat unit sprites.
pub const COMBAT_SHIP_Z: f32 = 11.;
/// Render layer of combat effects.
pub const COMBAT_EXPLOSION_Z: f32 = 12.;

/// Map
pub const BACKGROUND_Z: f32 = 0.;
/// Render layer of strategic ownership cells.
pub const VORONOI_Z: f32 = 1.;
/// Render layer of strategic planet sprites.
pub const PLANET_Z: f32 = 2.;
/// Render layer of travelling mission sprites.
pub const MISSION_Z: f32 = 3.;
/// Render layer of strategic map explosions.
pub const EXPLOSION_Z: f32 = 4.;

/// Unique names sampled for generated planets and moons.
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
