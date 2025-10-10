use crate::core::map::planet::PlanetId;
use crate::core::units::ships::Fleet;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub enum Objective {
    Colonize,
    Attack,
    Spy,
    Strike,
    Destroy,
    Transport,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Mission {
    pub fleet: Fleet,
    pub origin: PlanetId,
    pub destination: PlanetId,
    pub position: Vec2,
    pub objective: Objective,
}
