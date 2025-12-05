use bevy_renet::renet::ClientId;
use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;

use crate::core::combat::combat::CombatUnit;
use crate::core::map::icon::Icon;
use crate::core::map::planet::Planet;
use crate::core::missions::Mission;
use crate::core::player::Player;
use crate::core::units::{Army, Unit};

#[derive(Clone, Serialize, Deserialize)]
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
    pub destination_owned: Option<ClientId>,

    /// Controller of the planet after mission resolution
    pub destination_controlled: Option<ClientId>,

    /// Combat report (if combat took place)
    pub combat_report: Option<CombatReport>,

    /// Whether to show this report in the report mission tab
    pub hidden: bool,
}

impl MissionReport {
    pub fn winner(&self) -> Option<ClientId> {
        match self.mission.objective {
            Icon::Spy if self.scout_probes > 0 => None,
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

    pub fn image(&self, player: &Player) -> &str {
        match self.mission.objective {
            Icon::MissileStrike => "missile",
            Icon::Spy if self.scout_probes > 0 => "eye",
            _ if self.winner() == Some(player.id) => "won",
            _ => "lost",
        }
    }

    pub fn can_see(&self, side: &Side, player_id: ClientId) -> bool {
        match side {
            Side::Attacker => {
                self.mission.owner == player_id
                    || self.planet.owned == Some(player_id)
                    || self.winner() == Some(player_id)
                    || self.mission.objective == Icon::Spy // Spy winner returns None
            },
            Side::Defender => {
                self.planet.controlled == Some(player_id) || self.winner() == Some(player_id)
            },
        }
    }
}

pub type ReportId = u64;

#[derive(EnumIter, Clone, Debug, PartialEq)]
pub enum Side {
    Attacker,
    Defender,
}

impl Side {
    pub fn opposite(&self) -> Side {
        match self {
            Side::Attacker => Side::Defender,
            Side::Defender => Side::Attacker,
        }
    }
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct CombatReport {
    pub rounds: Vec<RoundReport>,
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct RoundReport {
    pub attacker: Vec<CombatUnit>,
    pub defender: Vec<CombatUnit>,
    pub planetary_shield: usize,
    pub antiballistic_fired: usize,
    pub buildings: Army,
    pub destroy_probability: f32,
}

impl RoundReport {
    pub fn units(&self, side: &Side) -> &Vec<CombatUnit> {
        match side {
            Side::Attacker => &self.attacker,
            Side::Defender => &self.defender,
        }
    }
}
