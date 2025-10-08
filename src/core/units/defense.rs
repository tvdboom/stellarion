use crate::core::units::Description;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use strum_macros::EnumIter;

#[derive(Clone, Serialize, Deserialize)]
pub struct Battery(pub HashMap<Defense, usize>);

impl Battery {
    pub fn get(&self, defense: &Defense) -> usize {
        *self.0.get(defense).unwrap_or(&0)
    }
}

#[derive(Component, EnumIter, Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
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
    pub fn level(&self) -> usize {
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
                making them capable of holding their own against cruiser-based fleets, when backed \
                by large amounts of fodder."
            },
            Defense::IonCannon => {
                "The ion cannon is well known for its relatively high shield power. A reason to \
                build them is because only the bomber and the war sun have rapid fire against it. \
                This makes them useful against cruisers and destroyer-dominated fleets."
            },
            Defense::PlasmaTurret => {
                "The plasma turret is the most powerful defense in the game. It is fairly \
                expensive, but well worth its price. Bomber is the only ship with rapid fire \
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
                can hit the defense itself, all the enemy's antiballistic missiles must be destroyed."
            },
        }
    }
}
