use crate::core::map::map::PlanetId;
use crate::core::units::ships::Ship;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct Fleet(pub Vec<(Ship, usize)>);

#[derive(Clone, Serialize, Deserialize)]
pub struct Mission {
    pub fleet: Fleet,
    pub origin: PlanetId,
    pub destination: PlanetId,
    pub position: Vec2,
}
