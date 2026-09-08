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
/// Shared hull/health color; player identity is presented separately.
pub const HEALTH_COLOR: Color = Color::srgb_u8(102, 224, 170);
/// Map color used for the local player's ownership.
pub const OWN_COLOR: Color = Color::srgb_u8(102, 128, 255);
/// Accent for the local player's home planet in map and HUD labels.
pub const HOME_PLANET_COLOR: Color = Color::srgb_u8(235, 199, 120);
/// Crown silhouette in normalized coordinates, with Y pointing up.
#[cfg(feature = "app")]
pub(crate) const HOME_CROWN_VERTICES: [[f32; 2]; 7] =
    [[0.12, 0.0], [0.88, 0.0], [1.0, 1.0], [0.75, 0.45], [0.5, 1.0], [0.25, 0.45], [0.0, 1.0]];
/// Triangulation shared by the map and HUD crown renderers.
#[cfg(feature = "app")]
pub(crate) const HOME_CROWN_INDICES: [u32; 15] = [0, 1, 3, 1, 2, 3, 0, 3, 5, 3, 4, 5, 0, 5, 6];
/// Map color used for enemy ownership.
pub const ENEMY_COLOR: Color = Color::srgb_u8(255, 64, 32);

/// Camera
pub const MIN_ZOOM: f32 = 0.5;
/// Largest orthographic zoom scale allowed by the strategic camera.
pub const MAX_ZOOM: f32 = 1.6;
/// Multiplicative step applied to wheel zoom.
pub const ZOOM_FACTOR: f32 = 1.1;
/// Default interpolation fraction for smooth presentation movement.
pub const LERP_FACTOR: f32 = 0.05;

/// GAME
pub const SHIPYARD_PRODUCTION_FACTOR: usize = 5;
/// Ship-production capacity granted by a stationed Space Dock.
pub const SPACE_DOCK_FLEET_PRODUCTION: usize = 5;
/// Jump-gate transport capacity granted per completed Jump Gate level.
pub const JUMP_GATE_CAPACITY_PER_LEVEL: usize = 5;
/// Energy supplied by each Reactor level while retaining its fuel-reduction effect.
pub const REACTOR_ENERGY_PER_LEVEL: usize = 3;
/// Energy supplied by each lunar Tidal Generator level.
pub const TIDAL_GENERATOR_ENERGY_PER_LEVEL: usize = 5;
/// Fully powered Planetary Shield strength granted per building level.
pub const PS_SHIELD_PER_LEVEL: usize = 300;
/// Additional empire-wide Energy demand while one Planetary Shield is overloaded.
pub const PS_OVERLOAD_ENERGY_COST: usize = 3;
/// Shield-strength bonus granted per completed level while overloaded.
pub const PS_OVERLOAD_BONUS_PERCENT_PER_LEVEL: usize = 10;
/// Defense-production capacity granted per factory level.
pub const FACTORY_PRODUCTION_FACTOR: usize = 5;
/// Focused-resource production bonus granted per Terraformer level.
pub const TERRAFORMER_FOCUS_BONUS_PERCENT_PER_LEVEL: usize = 10;
/// Non-focused-resource production penalty applied per Terraformer level.
pub const TERRAFORMER_OTHER_PENALTY_PERCENT_PER_LEVEL: usize = 10;
/// Missile capacity granted per silo level.
pub const SILO_CAPACITY_FACTOR: usize = 10;
/// Probe capacity granted per shipyard production level.
pub const PROBES_PER_PRODUCTION_LEVEL: usize = 5;
/// Smallest probe group capable of performing a dedicated Spy mission.
pub const MIN_SPY_PROBES: usize = 5;
/// Fleet-fuel reduction granted by each Reactor level.
pub const REACTOR_FUEL_REDUCTION_FACTOR: f32 = 0.1;
/// Sensor-phalanx range granted per building level, measured in AU (planet-size units).
pub const PHALANX_DISTANCE: f32 = 1.0;
/// Orbital-radar range granted per building level, measured in AU (planet-size units).
pub const RADAR_DISTANCE: f32 = 1.2;
/// Orbital-railgun reach granted per completed level, measured in AU.
pub const ORBITAL_RAILGUN_RANGE_PER_LEVEL: f32 = 2.0;
/// Deuterium consumed by one synchronized Orbital Railgun strike.
pub const ORBITAL_RAILGUN_FIRE_DEUTERIUM_COST: usize = 1_000;
/// Free empire-grid capacity required to fire an Orbital Railgun strike.
pub const ORBITAL_RAILGUN_FIRE_ENERGY_COST: usize = 10;
/// Empire-grid demand added by every completed Orbital Railgun level.
pub const ORBITAL_RAILGUN_ENERGY_PER_LEVEL: usize = 4;
/// Hull points repaired by one Repair Truck after each round.
pub const REPAIR_TRUCK_HEALING_PER_ROUND: usize = 50;

/// Combat
pub const SETUP_TIME: u64 = 2;
/// Rendered combat-unit sprite size in pixels.
pub const UNIT_SIZE: f32 = 120.;
/// Planetary-shield bar width spanning ten combat cards at the standard 1.2-card spacing.
pub const PS_WIDTH: f32 = 11.8;
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
/// Travelling missions sit above planets and orbital structures, below the map's icon overlays.
pub const MISSION_Z: f32 = PLANET_Z + 0.4;
/// Render layer of strategic map explosions.
pub const EXPLOSION_Z: f32 = 4.;
/// Rendered diameter of the large solar landmark used as the solar-band origin.
pub const SOLAR_STAR_SIZE: f32 = 1_440.0;

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
