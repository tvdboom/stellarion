use crate::core::map::icon::Icon;
use crate::core::map::planet::PlanetId;
use crate::core::units::Army;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Mission {
    pub origin: PlanetId,
    pub destination: PlanetId,
    pub position: Vec2,
    pub objective: Icon,
    pub army: Army,
}
