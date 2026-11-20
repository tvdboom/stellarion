//! Probe sabotage, intelligence-gated targets, and one-planning-turn infrastructure outages.

use serde::{Deserialize, Serialize};

use crate::core::combat::report::Side;
use crate::core::map::icon::Icon;
use crate::core::map::planet::Planet;
use crate::core::missions::{BombingRaid, Mission};
use crate::core::player::Player;
use crate::core::units::buildings::Building;
use crate::core::units::{Amount, Unit};

/// One orbital system selected before dispatch; undisclosed infrastructure cannot be targeted.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum SabotageTarget {
    /// Suppresses this world's Sensor Phalanx coverage.
    SensorPhalanx,
    /// Prevents this world's Orbital Railgun from committing a shot.
    OrbitalRailgun,
    /// Prevents both launches and arrivals through this world's Jump Gate.
    JumpGate,
}

impl SabotageTarget {
    /// Stable display order for the mission's target picker.
    pub const ALL: [Self; 3] = [Self::SensorPhalanx, Self::OrbitalRailgun, Self::JumpGate];

    /// Infrastructure affected by this target.
    pub const fn building(self) -> Building {
        match self {
            Self::SensorPhalanx => Building::SensorPhalanx,
            Self::OrbitalRailgun => Building::OrbitalRailgun,
            Self::JumpGate => Building::JumpGate,
        }
    }

    /// User-facing name, independent of serialization spelling.
    pub const fn name(self) -> &'static str {
        match self {
            Self::SensorPhalanx => "Sensor Phalanx",
            Self::OrbitalRailgun => "Orbital Railgun",
            Self::JumpGate => "Jump Gate",
        }
    }
}

/// Outages last through the planning turn immediately following the arrival.
/// Orders are validated while these flags are set. The next real resolution clears them
/// before processing new sabotage arrivals; draft previews never clear an outage.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SabotageState {
    /// Whether Sensor Phalanx coverage is unavailable this planning turn.
    pub sensor_phalanx: bool,
    /// Whether Orbital Railgun firing is unavailable this planning turn.
    pub orbital_railgun: bool,
    /// Whether this endpoint's Jump Gate is unavailable this planning turn.
    pub jump_gate: bool,
}

impl SabotageState {
    /// Records an outage without destroying or downgrading the structure.
    pub fn disable(&mut self, target: SabotageTarget) {
        match target {
            SabotageTarget::SensorPhalanx => self.sensor_phalanx = true,
            SabotageTarget::OrbitalRailgun => self.orbital_railgun = true,
            SabotageTarget::JumpGate => self.jump_gate = true,
        }
    }

    /// Whether the selected building is disabled; unrelated infrastructure remains operational.
    pub const fn is_disabled(self, building: Building) -> bool {
        match building {
            Building::SensorPhalanx => self.sensor_phalanx,
            Building::OrbitalRailgun => self.orbital_railgun,
            Building::JumpGate => self.jump_gate,
            _ => false,
        }
    }
}

/// The selected target and the resolved outcome carried by reports and returning probes.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sabotage {
    /// Infrastructure selected from the attacker's known intelligence.
    pub target: SabotageTarget,
    /// True only when a planetary arrival actually disabled the selected structure.
    pub succeeded: bool,
}

/// Offers only observed levels, or the publicly disclosed existence of a Railgun.
/// Stale intelligence may authorize a launch, but the structure must still exist on arrival.
pub fn known_targets(
    player: &Player,
    planet: &Planet,
    missions: &[Mission],
) -> Vec<SabotageTarget> {
    if player.spectator
        || planet.is_destroyed
        || planet.is_moon()
        || player.owns(planet)
        || player.controls(planet)
    {
        return Vec::new();
    }
    let intel = player.last_info(planet, missions);
    SabotageTarget::ALL
        .into_iter()
        .filter(|target| {
            let unit = Unit::Building(target.building());
            let observed = player
                .reports
                .iter()
                .filter_map(|report| {
                    if report.mission.destination != planet.id || report.is_space_encounter() {
                        return None;
                    }
                    let amount =
                        if report.mission.sabotage.is_some_and(|sabotage| {
                            sabotage.succeeded && sabotage.target == *target
                        }) {
                            1
                        } else if report.can_see(&Side::Defender, player.id) {
                            report.surviving_defender.combined_amount(&unit)
                        } else if report.mission.owner == player.id
                            && unit.revealed_by_probes_on_world(report.scout_probes, false)
                        {
                            report.planet.army.amount(&unit)
                        } else {
                            return None;
                        };
                    Some((report.turn, amount))
                })
                .max_by_key(|(turn, _)| *turn);
            (target == &SabotageTarget::OrbitalRailgun && planet.has(&unit))
                || observed.map_or_else(
                    || intel.as_ref().is_some_and(|info| info.army.amount(&unit) > 0),
                    |(_, amount)| amount > 0,
                )
        })
        .collect()
}

impl Planet {
    /// Completed usable levels, preserving the actual building and its intelligence when disabled.
    pub fn operational_level(&self, building: Building) -> usize {
        if self.is_destroyed || self.sabotage.is_disabled(building) {
            0
        } else {
            self.army.amount(&Unit::Building(building))
        }
    }
}

impl Mission {
    /// Selects a sabotage target while clearing any previous resolution result.
    pub fn with_sabotage_target(mut self, target: Option<SabotageTarget>) -> Self {
        self.sabotage = target.map(|target| Sabotage {
            target,
            succeeded: false,
        });
        self
    }

    /// Checks target metadata in both live missions and historical reports.
    pub(crate) fn valid_sabotage(&self) -> bool {
        let Some(_) = self.sabotage else {
            return self.objective != Icon::Sabotage
                && self.return_objective != Some(Icon::Sabotage);
        };
        (self.objective == Icon::Sabotage && self.return_objective.is_none()
            || self.return_objective == Some(Icon::Sabotage))
            && self.patrol.is_none()
            && self.intercept.is_none()
            && self.expedition.is_none()
            && !self.jump_gate
            && self.joint_attack.is_none()
            && self.bombing == BombingRaid::None
            && self.army.iter().all(|(unit, count)| *count == 0 || *unit == Unit::probe())
    }
}

#[cfg(test)]
#[path = "../../tests/core/sabotage.rs"]
mod tests;
