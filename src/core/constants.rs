use bevy::color::Color;

/// Window
pub const WIDTH: f32 = 1600.;
pub const HEIGHT: f32 = 900.;

/// Menu
pub const SUBTITLE_TEXT_SIZE: f32 = 10.;
pub const TITLE_TEXT_SIZE: f32 = 15.;
pub const BUTTON_TEXT_SIZE: f32 = 20.;
pub const NORMAL_BUTTON_COLOR: Color = Color::srgb(0.15, 0.15, 0.15);
pub const HOVERED_BUTTON_COLOR: Color = Color::srgb_u8(59, 66, 82);
pub const PRESSED_BUTTON_COLOR: Color = Color::srgb_u8(95, 131, 175);
pub const DISABLED_BUTTON_COLOR: Color = Color::srgb(0.8, 0.5, 0.5);

/// Camera
pub const MIN_ZOOM: f32 = 0.5;
pub const MAX_ZOOM: f32 = 1.2;
pub const ZOOM_FACTOR: f32 = 1.1;
pub const LERP_FACTOR: f32 = 0.05;

/// Settings
pub const MIN_PLANETS: u32 = 10;
pub const MAX_PLANETS: u32 = 50;
pub const MESSAGE_DURATION: u64 = 5;

/// GAME
pub const SHIPYARD_PRODUCTION_FACTOR: usize = 5;
pub const FACTORY_PRODUCTION_FACTOR: usize = 5;
pub const SILO_CAPACITY_FACTOR: usize = 10;
pub const PROBES_PER_PRODUCTION_LEVEL: usize = 10;

/// Map
pub const BACKGROUND_Z: f32 = 0.;
pub const VORONOI_Z: f32 = 1.;
pub const MISSION_Z: f32 = 2.;
pub const PLANET_Z: f32 = 3.;

pub const PLANET_NAMES: [&str; 80] = [
    "Aegis",
    "Arcadia",
    "Arcturus",
    "Avalon",
    "Bellatrix",
    "Borealis",
    "Calypso",
    "Cerulea",
    "Cimmeria",
    "Cydonia",
    "Daedalus",
    "Dione",
    "Draconis",
    "Elysium",
    "Eos",
    "Erebus",
    "Erythra",
    "Faeloria",
    "Fomalhaut",
    "Fortuna",
    "Fornax",
    "Galatea",
    "Ganymede",
    "Harrow",
    "Helion",
    "Hesperus",
    "Hyperion",
    "Icarus",
    "Ilios",
    "Io",
    "Janus",
    "Jareth",
    "Juno",
    "Kepleron",
    "Kestrel",
    "Korran",
    "Laconia",
    "Lyra",
    "Lyris",
    "Marduk",
    "Meridian",
    "Morpheus",
    "Nereus",
    "Novara",
    "Noxus",
    "Nyx",
    "Oberon",
    "Orionis",
    "Orpheus",
    "Othrys",
    "Pegasus",
    "Persephone",
    "Phaeton",
    "Pyrrhos",
    "Quasar",
    "Quillon",
    "Quora",
    "Ragnarok",
    "Rhea",
    "Riven",
    "Sable",
    "Selene",
    "Solaris",
    "Stygia",
    "Tethys",
    "Thalassa",
    "Titania",
    "Umbra",
    "Umbriel",
    "Ulysses",
    "Vela",
    "Vespera",
    "Vortiga",
    "Wyvern",
    "Xandar",
    "Xylos",
    "Yalara",
    "Ythril",
    "Zaurak",
    "Zephyria",
];
