//! Persisted combat outcomes, rounds, visibility, and attacker/defender views.

use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;

use crate::core::combat::resolution::CombatUnit;
use crate::core::identity::PlayerId;
use crate::core::map::icon::Icon;
use crate::core::map::planet::{Garrison, Planet, PlanetId};
use crate::core::missions::Mission;
use crate::core::player::Player;
use crate::core::resources::Resources;
use crate::core::units::{Amount, Army, Combat, Price, Unit};

/// Match the production-weighted ship strength used by fleet withdrawal and mission UI.
pub(crate) fn combat_fleet_strength(army: &Army) -> u128 {
    army.iter().fold(0_u128, |strength, (unit, count)| {
        if unit.is_ship() {
            strength.saturating_add((*count as u128).saturating_mul(unit.production() as u128))
        } else {
            strength
        }
    })
}

/// Cumulative ranges keep adjoining player colors flush and fill the final pixel exactly.
pub(crate) fn combat_strength_ranges(strengths: &[u128]) -> Vec<(f32, f32)> {
    let total = strengths.iter().map(|strength| *strength as f64).sum::<f64>();
    if total == 0.0 {
        return strengths
            .iter()
            .enumerate()
            .map(|(index, _)| {
                if index == 0 {
                    (0.0, 1.0)
                } else {
                    (1.0, 1.0)
                }
            })
            .collect();
    }

    let last = strengths.iter().rposition(|strength| *strength > 0);
    let mut consumed = 0.0_f64;
    strengths
        .iter()
        .enumerate()
        .map(|(index, strength)| {
            let start = (consumed / total) as f32;
            consumed += *strength as f64;
            let end = if Some(index) == last {
                1.0
            } else {
                (consumed / total) as f32
            };
            (start, end)
        })
        .collect()
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// Persisted outcome and visibility data produced when one mission resolves.
pub struct MissionReport {
    /// Unique identifier for the report
    pub id: ReportId,

    /// Turn the report was generated
    pub turn: usize,

    /// Mission that created the report
    pub mission: Mission,

    /// Planet as it was before the mission resolution
    pub planet: Planet,

    /// Probes returning with intelligence after reconnaissance, with or without combat.
    pub scout_probes: usize,

    /// Surviving units from the attacker
    pub surviving_attacker: Army,

    /// Surviving defending forces, retaining the commander of every unit.
    pub surviving_defender: Garrison,

    /// Whether the planet was colonized
    pub planet_colonized: bool,

    /// Whether the planet was destroyed
    pub planet_destroyed: bool,

    /// Owner of the planet after mission resolution
    #[serde(deserialize_with = "crate::serialization::required_option")]
    pub destination_owned: Option<PlayerId>,

    /// Controller of the planet after mission resolution
    #[serde(deserialize_with = "crate::serialization::required_option")]
    pub destination_controlled: Option<PlayerId>,

    /// Combat report (if combat took place)
    #[serde(deserialize_with = "crate::serialization::required_option")]
    pub combat_report: Option<CombatReport>,

    /// Whether to show this report in the report mission tab
    pub hidden: bool,
}

impl MissionReport {
    /// Returns whether this report records combat against neutral space fauna.
    pub fn is_space_fauna_encounter(&self) -> bool {
        self.planet.army.combined().keys().any(Unit::is_fauna)
    }

    /// Returns the encounter formation's generated title.
    pub fn space_fauna_name(&self) -> Option<&str> {
        self.is_space_fauna_encounter().then_some(self.planet.name.as_str())
    }

    /// Initial hull used by this report, including the defending Space Dock's mode.
    pub fn unit_hull(&self, unit: Unit, side: &Side) -> usize {
        if *side == Side::Defender {
            self.planet.unit_hull(unit)
        } else {
            unit.hull()
        }
    }

    /// Full regenerating shield used by this report.
    pub fn unit_shield(&self, unit: Unit, side: &Side) -> usize {
        if *side == Side::Defender {
            self.planet.unit_shield(unit)
        } else {
            unit.shield()
        }
    }

    /// Returns the attacking commander first, followed by the other fleet owners.
    pub fn attacker_players(&self) -> Vec<PlayerId> {
        if let Some(attack) =
            self.mission.joint_attack.as_ref().filter(|attack| !attack.attackers.is_empty())
        {
            let mut players = attack.attackers.keys().copied().collect::<Vec<_>>();
            if let Some(index) = players.iter().position(|id| *id == self.mission.owner) {
                players.remove(index);
                players.insert(0, self.mission.owner);
            }
            return players;
        }
        vec![self.mission.owner]
    }

    /// Returns whether this player participated in the attack recorded by the report.
    pub fn is_attacker(&self, player_id: PlayerId) -> bool {
        self.mission
            .joint_attack
            .as_ref()
            .filter(|attack| !attack.attackers.is_empty())
            .map_or(self.mission.owner == player_id, |attack| {
                attack.attackers.contains_key(&player_id)
            })
    }

    /// Returns the defending controller first, followed by protection-fleet owners.
    pub fn defender_players(&self) -> Vec<PlayerId> {
        let controller = self.planet.controlled.or(self.planet.owned);
        let mut players = controller.into_iter().collect::<Vec<_>>();
        players.extend(self.planet.army.protector_ids().filter(|id| Some(*id) != controller));
        players
    }

    /// Returns one participant's production-weighted ships at the start of combat.
    pub(crate) fn participant_fleet_strength(&self, side: &Side, player_id: PlayerId) -> u128 {
        match side {
            Side::Attacker => {
                let participant_army = self
                    .mission
                    .joint_attack
                    .as_ref()
                    .filter(|attack| !attack.attackers.is_empty())
                    .and_then(|attack| attack.attackers.get(&player_id));
                if let Some(army) = participant_army {
                    combat_fleet_strength(army)
                } else if player_id == self.mission.owner {
                    combat_fleet_strength(&self.mission.army)
                } else {
                    0
                }
            },
            Side::Defender => {
                let controller = self.planet.controlled.or(self.planet.owned);
                if Some(player_id) == controller {
                    combat_fleet_strength(self.planet.army.controller())
                } else {
                    self.planet.army.protector(player_id).map_or(0, combat_fleet_strength)
                }
            },
        }
    }

    /// Returns whether this player participated in the defense recorded by the report.
    pub fn is_defender(&self, player_id: PlayerId) -> bool {
        self.planet.controlled == Some(player_id)
            || self.planet.owned == Some(player_id)
            || self.planet.army.protector(player_id).is_some()
    }

    /// Returns the exact shared shield strength at the start of recorded combat.
    ///
    /// The first snapshot stores the strength remaining after that round. Adding its recorded
    /// absorbed damage recovers the energy-scaled, overload-adjusted starting value without
    /// requiring playback to reconstruct the defender's turn-start energy grid.
    pub fn initial_planetary_shield(&self) -> usize {
        let Some(first) = self.combat_report.as_ref().and_then(|combat| combat.rounds.first())
        else {
            return 0;
        };
        first
            .attacker
            .iter()
            .flat_map(|unit| &unit.shots)
            .fold(first.planetary_shield, |shield, shot| {
                shield.saturating_add(shot.planetary_shield_damage)
            })
    }

    /// Returns whether this report contains an outcome worth replaying as combat animation.
    ///
    /// A lone Colony Ship evacuated before combat still creates its recorded return mission, but
    /// never enters combat presentation because Colony Ships are intentionally not shown there.
    pub fn has_combat_playback(&self) -> bool {
        let Some(combat) = self.combat_report.as_ref() else {
            return false;
        };
        if combat.rounds.is_empty() {
            return false;
        }
        let colony_only_immediate_retreat =
            combat.defender_retreat.as_ref().is_some_and(|retreat| {
                retreat.after_round.is_none()
                    && retreat.ships.amount(&Unit::colony_ship()) > 0
                    && retreat
                        .ships
                        .iter()
                        .all(|(unit, count)| *count == 0 || *unit == Unit::colony_ship())
            });
        if !colony_only_immediate_retreat {
            return true;
        }

        combat.rounds.iter().any(|round| {
            round.antiballistic_fired > 0
                || round.destroy_probability > 0.0
                || round.attacker.iter().chain(&round.defender).any(|unit| {
                    !unit.shots.is_empty() || unit.repairs.iter().any(|amount| *amount > 0)
                })
        })
    }

    /// Returns escaped defenders of this kind; these are survivors, not battlefield losses.
    pub fn escaped_defenders(&self, unit: &Unit) -> usize {
        self.combat_report
            .as_ref()
            .and_then(|combat| combat.defender_retreat.as_ref())
            .map_or(0, |retreat| retreat.ships.amount(unit))
    }

    /// Returns resources recovered from destroyed ground defenses by surviving Crawlers.
    ///
    /// Salvage is paid only after a defender victory. Each survivor recovers one percent of
    /// every resource component, with the combined recovery capped at half the destroyed cost.
    pub fn defender_salvage(&self) -> Resources {
        let Some(defender) = self.planet.controlled else {
            return Resources::default();
        };
        if self.winner() != Some(defender) {
            return Resources::default();
        }

        let percent = self.surviving_defender.amount(&Unit::crawler()).min(50);
        if percent == 0 {
            return Resources::default();
        }

        let destroyed_cost = self
            .planet
            .army
            .iter()
            .filter(|(unit, _)| unit.is_defense() && !unit.is_missile() && !unit.is_orbital())
            .map(|(unit, initial)| {
                unit.price() * initial.saturating_sub(self.surviving_defender.amount(unit))
            })
            .sum::<Resources>();

        destroyed_cost.scaled_percent(percent)
    }

    /// Returns the winning combat side when the report is decisive.
    pub fn winner(&self) -> Option<PlayerId> {
        if self.is_space_fauna_encounter() {
            let attacker_survives = self.surviving_attacker.has_army();
            let fauna_survives = self.surviving_defender.has_army();
            return (attacker_survives && !fauna_survives).then_some(self.mission.owner);
        }
        match self.mission.objective {
            Icon::Spy if self.scout_probes > 0 => None,
            Icon::MissileStrike => {
                let round = self.combat_report.as_ref()?.rounds.first()?;
                if round.missiles_shot() >= round.n_missiles() {
                    self.planet.controlled
                } else {
                    None
                }
            },
            _ if self.is_stalemate() => None,
            _ => {
                if self.surviving_attacker.iter().any(|(u, c)| {
                    if *u == Unit::probe() {
                        *c > self.scout_probes
                    } else {
                        *c > 0
                    }
                }) {
                    Some(self.mission.owner)
                } else {
                    self.planet.controlled
                }
            },
        }
    }

    /// Returns whether this player fought on the winning side of the battle.
    pub fn won_by(&self, player_id: PlayerId) -> bool {
        self.winner().is_some_and(|winner| {
            (self.is_attacker(winner) && self.is_attacker(player_id))
                || (self.is_defender(winner) && self.is_defender(player_id))
        })
    }

    /// Both combat armies remain after the bounded round limit; neither side conquered the world.
    pub fn is_stalemate(&self) -> bool {
        (self.is_space_fauna_encounter()
            || matches!(self.mission.objective, Icon::Attack | Icon::Colonize | Icon::Destroy))
            && self.surviving_attacker.iter().any(|(unit, count)| {
                *unit != Unit::colony_ship()
                    && *count
                        > if *unit == Unit::probe() {
                            self.scout_probes
                        } else {
                            0
                        }
            })
            && self.surviving_defender.combined().iter().any(|(unit, count)| {
                *count > 0
                    && !unit.is_building()
                    && !unit.is_missile()
                    && *unit != Unit::colony_ship()
            })
    }

    /// Returns the user-facing status of this combat side.
    pub fn status(&self, player: &Player) -> &'static str {
        if self.is_space_fauna_encounter() && !self.surviving_attacker.has_army() {
            "defeat"
        } else if self.winner().is_none() {
            "draw"
        } else if self.won_by(player.id) {
            "victory"
        } else {
            "defeat"
        }
    }

    /// Returns the runtime image key for this value.
    pub fn image(&self, player: &Player) -> &'static str {
        match self.mission.objective {
            Icon::MissileStrike => "missile",
            Icon::Spy if self.scout_probes > 0 => "eye",
            _ if self.won_by(player.id) => "won",
            _ => "lost",
        }
    }

    /// Returns whether the current state can see.
    pub fn can_see(&self, side: &Side, player_id: PlayerId) -> bool {
        match side {
            Side::Attacker => {
                self.is_attacker(player_id)
                    || self.is_defender(player_id)
                    || self.won_by(player_id)
                    || matches!(self.mission.objective, Icon::Spy | Icon::MissileStrike)
            },
            Side::Defender => {
                if self.is_space_fauna_encounter() {
                    self.is_attacker(player_id)
                } else {
                    self.is_defender(player_id) || self.won_by(player_id)
                }
            },
        }
    }
}

/// Stable identifier of a mission report.
pub type ReportId = u64;

#[derive(EnumIter, Clone, Debug, PartialEq)]
/// Attacker or defender perspective within a combat report.
pub enum Side {
    /// The attacker value.
    Attacker,
    /// The defender value.
    Defender,
}

impl Side {
    /// Returns the opposing combat side.
    pub fn opposite(&self) -> Side {
        match self {
            Side::Attacker => Side::Defender,
            Side::Defender => Side::Attacker,
        }
    }
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// Complete ordered round history for one resolved combat.
pub struct CombatReport {
    /// Combat rounds in deterministic playback order.
    pub rounds: Vec<RoundReport>,
    /// Ships that left this battle, separate from the defenders still holding the world.
    #[serde(deserialize_with = "crate::serialization::required_option")]
    pub defender_retreat: Option<DefenderRetreat>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// A recorded withdrawal used both to launch a Deploy mission and to replay ships flying away.
pub struct DefenderRetreat {
    /// Zero-based round after which ships leave, or `None` for a level-five immediate retreat.
    #[serde(deserialize_with = "crate::serialization::required_option")]
    pub after_round: Option<usize>,
    /// The defender's homeworld at the time of departure.
    pub home_planet: PlanetId,
    /// All surviving ships evacuated from the battle, excluding stationary defenses.
    pub ships: Army,
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// Unit, shield, interception, and bombing state captured for one combat round.
pub struct RoundReport {
    /// Attacking unit states captured for this round.
    pub attacker: Vec<CombatUnit>,
    /// Defending unit states captured for this round.
    pub defender: Vec<CombatUnit>,
    /// Shared planetary-shield strength remaining in this round.
    pub planetary_shield: usize,
    /// Number of antiballistic missiles fired during interception.
    pub antiballistic_fired: usize,
    /// Defending buildings exposed to bombing after fleet combat.
    pub buildings: Army,
    /// Bounded probability that a War Sun destroys the planet.
    pub destroy_probability: f32,
}

impl RoundReport {
    /// Returns the unit states visible for this combat side.
    pub fn units(&self, side: &Side) -> &Vec<CombatUnit> {
        match side {
            Side::Attacker => &self.attacker,
            Side::Defender => &self.defender,
        }
    }

    /// Counts interplanetary missiles present at the start of this combat side.
    pub fn n_missiles(&self) -> usize {
        self.attacker.iter().filter(|cu| cu.unit == Unit::interplanetary_missile()).count()
    }

    /// Counts antiballistic missiles present at the start of this combat side.
    pub fn n_antiballistic(&self) -> usize {
        self.defender.iter().filter(|cu| cu.unit == Unit::antiballistic_missile()).count()
    }

    /// Returns the number of offensive missiles consumed during the round.
    pub fn missiles_shot(&self) -> usize {
        self.defender
            .iter()
            .filter(|cu| {
                cu.unit == Unit::antiballistic_missile() && cu.shots.iter().any(|s| s.killed)
            })
            .count()
    }
}
