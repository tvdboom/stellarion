use std::collections::HashMap;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;

use crate::core::resources::Resources;
use crate::core::units::{Army, Combat, Description, Price, Unit};

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
                "The Rocket Launcher is the weakest defense you can build. They are used as \
                fodder to protect the better, more expensive, defences."
            },
            Defense::LightLaser => {
                "The Light Laser is much like the Rocket Launcher, but is better in several ways. \
                All the ships with Rapid Fire against the Light Laser are relatively slow. Light \
                Lasers also have a higher weapon power than Rocket Launchers. Although the weapon \
                power is only slightly higher, fodder is built in much larger numbers than anything \
                else, and the difference quickly becomes rather significant."
            },
            Defense::HeavyLaser => {
                "The Heavy Laser is an improvement in sheer power over the Light Laser. However, \
                it has less overall power-per-resource and similar things have Rapid Fire against \
                it."
            },
            Defense::GaussCannon => {
                "Gauss Cannons are an effective defence due to their high shield and weapon power, \
                making them capable of holding their own against Cruiser-based fleets, when backed \
                by large amounts of fodder."
            },
            Defense::IonCannon => {
                "The Ion Cannon is well known for its relatively high shield power. A reason to \
                build them is because only the bomber and the War Sun have Rapid Fire against it. \
                This makes them useful against Cruisers and Destroyer-dominated fleets."
            },
            Defense::PlasmaTurret => {
                "The Plasma Turret is the most powerful defense in the game. It is fairly \
                expensive, but well worth its price. The bomber is the only ship with Rapid Fire \
                against it. "
            },
            Defense::AntiballisticMissile => {
                "The purpose of Antiballistic Missiles is to intercept Interplanetary Missiles and \
                destroy them prior to impact. Each Antiballistic Missile has a 50% chance of \
                destroying one incoming Interplanetary Missile. Antiballistic Missiles are launched \
                automatically whenever an approaching enemy missile is detected. Otherwise, they \
                do not take part in any combat. Antiballistic Missiles are much cheaper than \
                Interplanetary Missiles."
            },
            Defense::InterplanetaryMissile => {
                "Interplanetary Missiles are designed to destroy enemy defenses. They ignore enemy \
                ships and the Planetary Shield. All the enemy's Antiballistic Missiles are launched \
                and resolved before defenses are hit. Interplanetary Missiles have a very good \
                price-to-stat ratio and don't consume fuel. They don't have Hull points, meaning \
                the combat is always resolved in one round, but their Damage is capable of \
                destroying every defense unit in one round and their Rapid Fire capabilities \
                ensures the total inflicted destruction can be huge."
            },
        }
    }
}

impl Price for Defense {
    fn price(&self) -> Resources {
        match self {
            Defense::RocketLauncher => Resources::new(25, 0, 0),
            Defense::LightLaser => Resources::new(30, 5, 0),
            Defense::HeavyLaser => Resources::new(45, 10, 0),
            Defense::GaussCannon => Resources::new(80, 80, 0),
            Defense::IonCannon => Resources::new(130, 130, 80),
            Defense::PlasmaTurret => Resources::new(220, 140, 140),
            Defense::AntiballisticMissile => Resources::new(50, 0, 20),
            Defense::InterplanetaryMissile => Resources::new(105, 20, 100),
        }
    }
}

impl Combat for Defense {
    fn hull(&self) -> usize {
        match self {
            Defense::RocketLauncher => 80,
            Defense::LightLaser => 100,
            Defense::HeavyLaser => 180,
            Defense::GaussCannon => 370,
            Defense::IonCannon => 500,
            Defense::PlasmaTurret => 630,
            Defense::AntiballisticMissile => 0,
            Defense::InterplanetaryMissile => 0,
        }
    }

    fn shield(&self) -> usize {
        match self {
            Defense::RocketLauncher => 2,
            Defense::LightLaser => 6,
            Defense::HeavyLaser => 10,
            Defense::GaussCannon => 20,
            Defense::IonCannon => 40,
            Defense::PlasmaTurret => 60,
            Defense::AntiballisticMissile => 0,
            Defense::InterplanetaryMissile => 0,
        }
    }

    fn damage(&self) -> usize {
        match self {
            Defense::RocketLauncher => 8,
            Defense::LightLaser => 14,
            Defense::HeavyLaser => 20,
            Defense::GaussCannon => 80,
            Defense::IonCannon => 100,
            Defense::PlasmaTurret => 120,
            Defense::AntiballisticMissile => 0,
            Defense::InterplanetaryMissile => 800,
        }
    }

    fn rapid_fire(&self) -> HashMap<Unit, usize> {
        match self {
            Defense::InterplanetaryMissile => HashMap::from([
                (Unit::Defense(Defense::RocketLauncher), 90),
                (Unit::Defense(Defense::LightLaser), 80),
                (Unit::Defense(Defense::HeavyLaser), 70),
                (Unit::Defense(Defense::GaussCannon), 60),
                (Unit::Defense(Defense::IonCannon), 50),
                (Unit::Defense(Defense::PlasmaTurret), 40),
            ]),
            _ => Army::new(),
        }
    }

    fn speed(&self) -> f32 {
        match self {
            Defense::InterplanetaryMissile => 3.,
            _ => 0.,
        }
    }
}
