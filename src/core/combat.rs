use crate::core::units::Description;
use strum_macros::EnumIter;

#[derive(EnumIter, Debug, PartialEq)]
pub enum CombatStats {
    Hull,
    Shield,
    Damage,
    RapidFire,
    Speed,
    FuelConsumption,
}

impl Description for CombatStats {
    fn description(&self) -> &str {
        match self {
            CombatStats::Hull => "The amount of damage a unit can take before being destroyed.",
            CombatStats::Shield => "The amount of damage a unit absorbs before it starts taking hull damage. The shield is regenerated every round",
            CombatStats::Damage => "The amount of damage a unit deals per round.",
            CombatStats::RapidFire => "The chance to fire again this round when targeting specific units.",
            CombatStats::Speed => "The speed at which a unit travels through space (in AU / turn).",
            CombatStats::FuelConsumption => "The amount of deuterium a unit requires to travel 1 AU.",
        }
    }
}
