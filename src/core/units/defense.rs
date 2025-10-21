use std::collections::HashMap;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;

use crate::core::resources::Resources;
use crate::core::units::{Combat, Description, Price};

pub type Battery = HashMap<Defense, usize>;

#[derive(Component, EnumIter, Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum Defense {
    RocketLauncher,
    LightLaser,
    HeavyLaser,
    GaussCannon,
    IonCannon,
    PlasmaTurret,
    AntiballisticMissile,
    InterplanetaryMissile,
}

impl Defense {
    /// Minimum level of the factory/silo to build this defense
    pub fn production(&self) -> usize {
        match self {
            Defense::RocketLauncher => 1,
            Defense::LightLaser => 1,
            Defense::HeavyLaser => 2,
            Defense::GaussCannon => 3,
            Defense::IonCannon => 4,
            Defense::PlasmaTurret => 5,
            Defense::AntiballisticMissile => 1,
            Defense::InterplanetaryMissile => 2,
        }
    }

    pub fn is_missile(&self) -> bool {
        matches!(self, Defense::AntiballisticMissile | Defense::InterplanetaryMissile)
    }
}

impl Description for Defense {
    fn description(&self) -> &str {
        match self {
            Defense::RocketLauncher => {
                "The rocket launcher is the weakest defense you can build. They are used as \
                fodder to protect the better, more expensive, defences."
            },
            Defense::LightLaser => {
                "The light laser is much like the rocket launcher, but is better in several ways. \
                All the ships with Rapid Fire against the light laser are relatively slow. Light \
                Lasers also have a higher weapon power than rocket launchers. Although the weapon \
                power is only slightly higher, fodder is built in much larger numbers than anything \
                else, and the difference quickly becomes rather significant."
            },
            Defense::HeavyLaser => {
                "The heavy laser is an improvement in sheer power over the light Laser. However, \
                it has less overall power-per-resource and similar things have rapid fire against \
                it."
            },
            Defense::GaussCannon => {
                "Gauss cannons are an effective defence due to their high shield and weapon power, \
                making them capable of holding their own against Cruiser-based fleets, when backed \
                by large amounts of fodder."
            },
            Defense::IonCannon => {
                "The ion cannon is well known for its relatively high shield power. A reason to \
                build them is because only the bomber and the War Sun have rapid fire against it. \
                This makes them useful against Cruisers and Destroyer-dominated fleets."
            },
            Defense::PlasmaTurret => {
                "The plasma turret is the most powerful defense in the game. It is fairly \
                expensive, but well worth its price. The bomber is the only ship with rapid fire \
                against it. "
            },
            Defense::AntiballisticMissile => {
                "Antiballistic missiles are the only way to destroy attacking interplanetary \
                missiles. Each anti-ballistic missile has a 50% chance of destroying one incoming \
                interplanetary missile. Antiballistic missiles are launched automatically whenever \
                an approaching interplanetary missile is detected. Otherwise, they do not take \
                part in any attacks."
            },
            Defense::InterplanetaryMissile => {
                "Interplanetary missiles are designed to destroy enemy defenses. Before a missile \
                can hit the defense itself, all the enemy's antiballistic missiles must be \
                destroyed. Interplanetary missiles ignore enemy ships and the planetary shield. \
                They don't consume fuel."
            },
        }
    }
}

impl Price for Defense {
    fn price(&self) -> Resources {
        match self {
            Defense::RocketLauncher => Resources::new(20, 0, 0),
            Defense::LightLaser => Resources::new(15, 5, 0),
            Defense::HeavyLaser => Resources::new(50, 20, 0),
            Defense::GaussCannon => Resources::new(100, 100, 0),
            Defense::IonCannon => Resources::new(150, 150, 100),
            Defense::PlasmaTurret => Resources::new(250, 150, 150),
            Defense::AntiballisticMissile => Resources::new(80, 0, 20),
            Defense::InterplanetaryMissile => Resources::new(125, 25, 100),
        }
    }
}

impl Combat for Defense {
    fn hull(&self) -> usize {
        match self {
            Defense::RocketLauncher => 80,
            Defense::LightLaser => 100,
            Defense::HeavyLaser => 180,
            Defense::GaussCannon => 350,
            Defense::IonCannon => 500,
            Defense::PlasmaTurret => 600,
            Defense::AntiballisticMissile => 0,
            Defense::InterplanetaryMissile => 150,
        }
    }

    fn shield(&self) -> usize {
        match self {
            Defense::RocketLauncher => 2,
            Defense::LightLaser => 2,
            Defense::HeavyLaser => 3,
            Defense::GaussCannon => 25,
            Defense::IonCannon => 50,
            Defense::PlasmaTurret => 70,
            Defense::AntiballisticMissile => 0,
            Defense::InterplanetaryMissile => 0,
        }
    }

    fn damage(&self) -> usize {
        match self {
            Defense::RocketLauncher => 8,
            Defense::LightLaser => 10,
            Defense::HeavyLaser => 20,
            Defense::GaussCannon => 80,
            Defense::IonCannon => 100,
            Defense::PlasmaTurret => 120,
            Defense::AntiballisticMissile => 0,
            Defense::InterplanetaryMissile => 120,
        }
    }

    fn speed(&self) -> f32 {
        match self {
            Defense::InterplanetaryMissile => 2.,
            _ => 0.,
        }
    }
}
