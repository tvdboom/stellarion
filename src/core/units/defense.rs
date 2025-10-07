use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;

#[derive(Clone, Serialize, Deserialize)]
pub struct Battery(pub Vec<(Defense, usize)>);

#[derive(EnumIter, Clone, Debug, Serialize, Deserialize)]
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
    pub fn requires_level(&self) -> usize {
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
