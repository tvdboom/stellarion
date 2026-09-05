//! Combat statistic labels used by report and tooltip interfaces.

use strum_macros::EnumIter;

use crate::core::units::Description;

#[derive(EnumIter, Debug, PartialEq)]
/// Statistics available in unit and combat information panels.
pub enum CombatStats {
    /// The hull value.
    Hull,
    /// The shield value.
    Shield,
    /// The damage value.
    Damage,
    /// The production value.
    Production,
    /// The speed value.
    Speed,
    /// The fuel consumption value.
    FuelConsumption,
    /// The rapid fire value.
    RapidFire,
}

impl Description for CombatStats {
    /// Returns the user-facing description of this gameplay value.
    fn description(&self) -> &str {
        match self {
            CombatStats::Production => {
                "\
                Production cost of the unit. The production cost also determines the minimum level \
                of the building required to build it, as well as the minimum level a Sensor \
                Phalanx must have to see it, and the jump cost it has through a Jump Gate."
            },
            CombatStats::Hull => "The amount of damage a unit can take before being destroyed.",
            CombatStats::Shield => {
                "\
                The amount of damage a unit absorbs before it starts taking hull damage. The \
                shield is regenerated every round."
            },
            CombatStats::Damage => "The amount of damage a unit deals per round.",
            CombatStats::Speed => "Movement rating: AU covered on the first turn. Each later turn \
                adds two thirds of this rating to the distance covered. Fleets use their slowest unit.",
            CombatStats::FuelConsumption => {
                "The amount of deuterium a unit requires to travel 1 AU."
            },
            CombatStats::RapidFire => {
                "The percentage probability to fire again this round when targeting specific units."
            },
        }
    }
}
