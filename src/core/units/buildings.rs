use bevy::prelude::Component;
use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;

#[derive(Component, EnumIter, Clone, Debug, Serialize, Deserialize)]
pub enum Building {
    Shipyard(u8),
    Factory(u8),
    Silo(u8),
    PlanetShield(u8),
    JumpGate(u8),
    SensorPhalanx(u8),
}
