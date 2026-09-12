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
use crate::core::units::{Amount, Army, Price, Unit};

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

    /// Number of attacking probes that left after one round of combat
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
    /// Returns every player whose fleet began this report on the attacking side.
    pub fn attacker_players(&self) -> Vec<PlayerId> {
        if let Some(attack) =
            self.mission.joint_attack.as_ref().filter(|attack| !attack.attackers.is_empty())
        {
            return attack.attackers.keys().copied().collect();
        }
        vec![self.mission.owner]
    }

    /// Returns whether this player participated in the attack recorded by the report.
    pub fn is_attacker(&self, player_id: PlayerId) -> bool {
        self.attacker_players().contains(&player_id)
    }

    /// Returns every player whose forces began this report on the defending side.
    pub fn defender_players(&self) -> Vec<PlayerId> {
        let mut players = self.planet.army.protector_ids().collect::<Vec<_>>();
        if let Some(controller) = self.planet.controlled.or(self.planet.owned) {
            players.push(controller);
        }
        players.sort_unstable();
        players.dedup();
        players
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

    /// Both combat armies remain after the bounded round limit; neither side conquered the world.
    pub fn is_stalemate(&self) -> bool {
        matches!(self.mission.objective, Icon::Attack | Icon::Colonize | Icon::Destroy)
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
        match self.winner() {
            None => "draw",
            Some(id) if id == player.id => "victory",
            Some(id) if self.is_defender(player.id) && self.is_defender(id) => "victory",
            _ => "defeat",
        }
    }

    /// Returns the runtime image key for this value.
    pub fn image(&self, player: &Player) -> &'static str {
        match self.mission.objective {
            Icon::MissileStrike => "missile",
            Icon::Spy if self.scout_probes > 0 => "eye",
            _ if self.winner() == Some(player.id) => "won",
            _ if self
                .winner()
                .is_some_and(|winner| self.is_defender(player.id) && self.is_defender(winner)) =>
            {
                "won"
            },
            _ => "lost",
        }
    }

    /// Returns whether the current state can see.
    pub fn can_see(&self, side: &Side, player_id: PlayerId) -> bool {
        match side {
            Side::Attacker => {
                self.mission.owner == player_id
                    || self.is_defender(player_id)
                    || self.winner() == Some(player_id)
                    || matches!(self.mission.objective, Icon::Spy | Icon::MissileStrike)
            },
            Side::Defender => self.is_defender(player_id) || self.winner() == Some(player_id),
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
