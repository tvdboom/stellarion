use crate::core::map::planet::PlanetId;
use crate::core::units::defense::Battery;
use crate::core::units::ships::Fleet;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Default, PartialEq, Serialize, Deserialize)]
pub enum Objective {
    #[default]
    Colonize,
    Attack,
    Spy,
    Strike,
    Destroy,
    Transport,
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Mission {
    pub fleet: Fleet,
    pub missiles: Battery,
    pub origin: PlanetId,
    pub destination: PlanetId,
    pub position: Vec2,
    pub objective: Objective,
}
