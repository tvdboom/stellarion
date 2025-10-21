use strum_macros::EnumIter;

use crate::core::units::Description;

#[derive(EnumIter, Debug, PartialEq)]
pub enum CombatStats {
    Hull,
    Shield,
    Damage,
    Production,
    Speed,
    FuelConsumption,
    RapidFire,
}

impl Description for CombatStats {
    fn description(&self) -> &str {
        match self {
            CombatStats::Production => "Production cost of the unit, and minimum level of the building required to build it.",
            CombatStats::Hull => "The amount of damage a unit can take before being destroyed.",
            CombatStats::Shield => "The amount of damage a unit absorbs before it starts taking hull damage. The shield is regenerated every round.",
            CombatStats::Damage => "The amount of damage a unit deals per round.",
            CombatStats::Speed => "The speed at which a unit travels through space (in AU / turn).",
            CombatStats::FuelConsumption => "The amount of deuterium a unit requires to travel 1 AU.",
            CombatStats::RapidFire => "The chance to fire again this round when targeting specific units.",
        }
    }
}
