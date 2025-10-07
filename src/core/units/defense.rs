use crate::core::units::Description;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;

#[derive(Clone, Serialize, Deserialize)]
pub struct Battery(pub Vec<(Defense, usize)>);

#[derive(Component, EnumIter, Clone, Debug, Serialize, Deserialize)]
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
                "The light laser is the second Defensive Structure that most players can build. It's used as fodder at all stages of the game, much like the Rocket Launcher, but is better in several ways. All the ships with Rapid Fire against the Light Laser are relatively slow. Light Lasers also have a higher weapon power than Rocket Launchers. Although the weapon power is only 25% higher, fodder is built in much larger numbers than anything else, and the difference quickly becomes rather significant."
            },
            Defense::HeavyLaser => "More powerful laser with longer range, effective against medium ships.".into(),
            Defense::GaussCannon => "Projectile weapon that fires metal slugs at high velocity, effective against large ships.".into(),
            Defense::IonCannon => "Energy weapon that disables ship systems, effective against shields and electronics.".into(),
            Defense::PlasmaTurret => "High-energy plasma weapon that causes significant damage, effective against all ship types.".into(),
            Defense::AntiballisticMissile => "Missile system designed to intercept and destroy incoming ballistic missiles.".into(),
            Defense::InterplanetaryMissile => "Long-range missile capable of striking targets on other planets or moons.".into(),
        }
    }
}
