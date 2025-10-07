use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Building {
    Shipyard(u8),
    Factory(u8),
    Silo(u8),
    PlanetShield(u8),
    JumpGate(u8),
    SensorPhalanx(u8),
}
