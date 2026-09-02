//! Persisted combat outcomes, rounds, visibility, and attacker/defender views.

use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;

use crate::core::combat::resolution::CombatUnit;
use crate::core::identity::PlayerId;
use crate::core::map::icon::Icon;
use crate::core::map::planet::Planet;
use crate::core::missions::Mission;
use crate::core::player::Player;
use crate::core::units::{Army, Unit};

#[derive(Clone, Serialize, Deserialize)]
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

    /// Surviving units from the defender
    pub surviving_defender: Army,

    /// Whether the planet was colonized
    pub planet_colonized: bool,

    /// Whether the planet was destroyed
    pub planet_destroyed: bool,

    /// Owner of the planet after mission resolution
    pub destination_owned: Option<PlayerId>,

    /// Controller of the planet after mission resolution
    pub destination_controlled: Option<PlayerId>,

    /// Combat report (if combat took place)
    pub combat_report: Option<CombatReport>,

    /// Whether to show this report in the report mission tab
    pub hidden: bool,
}

impl MissionReport {
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
            && self.surviving_defender.iter().any(|(unit, count)| {
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
            _ => "defeat",
        }
    }

    /// Returns the runtime image key for this value.
    pub fn image(&self, player: &Player) -> &'static str {
        match self.mission.objective {
            Icon::MissileStrike => "missile",
            Icon::Spy if self.scout_probes > 0 => "eye",
            _ if self.winner() == Some(player.id) => "won",
            _ => "lost",
        }
    }

    /// Returns whether the current state can see.
    pub fn can_see(&self, side: &Side, player_id: PlayerId) -> bool {
        match side {
            Side::Attacker => {
                self.mission.owner == player_id
                    || self.planet.owned == Some(player_id)
                    || self.winner() == Some(player_id)
                    || matches!(self.mission.objective, Icon::Spy | Icon::MissileStrike)
            },
            Side::Defender => {
                self.planet.controlled == Some(player_id) || self.winner() == Some(player_id)
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
/// Complete ordered round history for one resolved combat.
pub struct CombatReport {
    /// Combat rounds in deterministic playback order.
    pub rounds: Vec<RoundReport>,
}

#[derive(Clone, Default, Serialize, Deserialize)]
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
