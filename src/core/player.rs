//! Persisted player economy, reports, home world, and spectator state.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::core::combat::report::{MissionReport, Side};
use crate::core::constants::PROBES_PER_PRODUCTION_LEVEL;
use crate::core::identity::PlayerId;
use crate::core::map::icon::Icon;
use crate::core::map::model::Map;
use crate::core::map::planet::{Planet, PlanetId};
use crate::core::missions::Mission;
use crate::core::resources::Resources;
use crate::core::settings::Settings;
use crate::core::units::{Amount, Army, Unit};

/// Maximum number of resolved mission reports retained for one player.
pub const MAX_REPORTS_PER_PLAYER: usize = 512;

/// Distinct empire colors available in multiplayer lobbies.
pub const PLAYER_COLOR_PALETTE: [PlayerColor; 6] = [
    PlayerColor(0),
    PlayerColor(1),
    PlayerColor(2),
    PlayerColor(3),
    PlayerColor(4),
    PlayerColor(5),
];

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
/// Stable palette entry used for player identity and strategic-map ownership.
pub struct PlayerColor(u8);

impl PlayerColor {
    /// Creates a palette entry when the supplied index is supported.
    pub fn new(index: u8) -> Option<Self> {
        (usize::from(index) < PLAYER_COLOR_PALETTE.len()).then_some(Self(index))
    }

    /// Chooses a deterministic color for a stable player slot.
    pub fn for_player(player_id: PlayerId) -> Self {
        let index = player_id.saturating_sub(1) as usize % PLAYER_COLOR_PALETTE.len();
        PLAYER_COLOR_PALETTE[index]
    }

    /// Returns the stored palette index.
    pub fn index(self) -> u8 {
        self.0
    }

    /// Returns whether this value names a supported palette entry.
    pub fn is_valid(self) -> bool {
        usize::from(self.0) < PLAYER_COLOR_PALETTE.len()
    }

    /// Returns this entry's display and rendering color.
    pub fn rgb(self) -> [u8; 3] {
        match self.0 {
            0 => [102, 128, 255], // azure
            1 => [255, 112, 72],  // orange
            2 => [54, 211, 180],  // teal
            3 => [232, 96, 190],  // magenta
            4 => [244, 197, 66],  // gold
            5 => [164, 118, 255], // violet
            _ => [190, 198, 210],
        }
    }

    /// Converts this stable value into Bevy's render color.
    pub fn color(self) -> Color {
        let [red, green, blue] = self.rgb();
        Color::srgb_u8(red, green, blue)
    }
}

#[derive(Clone)]
/// Historical intelligence snapshot visible to one player.
pub struct PlanetInfo {
    /// Turn this information was valid
    pub turn: usize,

    /// Last known controller of the planet, when one was observed.
    pub controlled: Option<PlayerId>,

    /// The army present on the planet this turn
    pub army: Army,
}

#[derive(Resource, Clone, Serialize, Deserialize)]
/// Persisted player slot, economy, home world, reports, and elimination state.
pub struct Player {
    /// Stable identifier used to cross-reference this value.
    pub id: PlayerId,
    /// Stable home world whose loss eliminates this player.
    pub home_planet: PlanetId,
    /// Resource production for a world or stockpile for a player.
    pub resources: Resources,
    /// Resolved mission reports visible to this player.
    pub reports: Vec<MissionReport>,
    /// Whether this player is eliminated and no longer submits turns.
    pub spectator: bool,
    /// Lobby-selected identity color; absent only in snapshots created before color selection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<PlayerColor>,
}

impl Default for Player {
    /// Constructs the default value and its gameplay-safe initial state.
    fn default() -> Self {
        Self {
            id: 0,
            home_planet: 0,
            resources: Resources {
                metal: 1500,
                crystal: 1200,
                deuterium: 1000,
            },
            reports: Vec::new(),
            spectator: false,
            color: None,
        }
    }
}

impl Player {
    /// Creates a new value from the supplied state.
    pub fn new(id: PlayerId, home_planet: PlanetId) -> Self {
        Self {
            id,
            home_planet,
            color: Some(PlayerColor::for_player(id)),
            ..default()
        }
    }

    /// Returns the selected color or a deterministic fallback for older snapshots.
    pub fn color(&self) -> PlayerColor {
        self.color
            .filter(|color| color.is_valid())
            .unwrap_or_else(|| PlayerColor::for_player(self.id))
    }

    /// Appends a report while pruning the oldest entries from the bounded history.
    pub fn push_report(&mut self, mut report: MissionReport) {
        if report.id == 0 || self.reports.iter().any(|existing| existing.id == report.id) {
            if let Some(replacement) = (1..=self.reports.len() as u64 + 1)
                .find(|candidate| self.reports.iter().all(|existing| existing.id != *candidate))
            {
                report.id = replacement;
            }
        }
        self.reports.push(report);
        let excess = self.reports.len().saturating_sub(MAX_REPORTS_PER_PLAYER);
        if excess > 0 {
            self.reports.drain(..excess);
        }
    }

    /// Returns whether this player owns the supplied planet.
    pub fn owns(&self, planet: &Planet) -> bool {
        planet.owned == Some(self.id)
    }

    /// Returns whether this player controls the supplied planet.
    pub fn controls(&self, planet: &Planet) -> bool {
        planet.controlled == Some(self.id)
    }

    /// Computes resource production for the current owned worlds.
    pub fn resource_production(&self, planets: &[Planet]) -> Resources {
        planets.iter().filter(|p| p.owned == Some(self.id)).map(|p| p.resource_production()).sum()
    }

    /// Counts non-moon planets currently owned by this player.
    pub fn planets_owned(&self, map: &Map, settings: &Settings) -> (usize, usize) {
        let n_owned = map.planets().iter().filter(|p| p.owned == Some(self.id)).count();
        let n_max =
            (map.planets().len() as f32 * settings.p_colonizable as f32 / 100.).ceil() as usize;

        (n_owned, n_max)
    }

    /// Returns the most recent information report for a planet when present.
    pub fn last_info(&self, planet: &Planet, missions: &[Mission]) -> Option<PlanetInfo> {
        let mut reports = vec![];

        if planet.is_destroyed {
            return None;
        }

        for r in self.reports.iter() {
            if r.mission.origin == planet.id {
                if r.mission.owner == self.id {
                    // Ignore returning probes or from destroy mission
                    if r.mission.origin_controlled == Some(self.id) {
                        // Own mission send from this planet (and it's no longer controlled)
                        reports.push(PlanetInfo {
                            turn: r.mission.send,
                            controlled: None,
                            army: Unit::all()
                                .iter()
                                .flatten()
                                .map(|u| {
                                    (
                                        *u,
                                        r.mission
                                            .origin_army
                                            .amount(u)
                                            .saturating_sub(r.mission.army.amount(u)),
                                    )
                                })
                                .collect(),
                        });
                    }
                } else if !r.mission.objective.is_hidden() {
                    // Enemy mission send from this planet
                    reports.push(PlanetInfo {
                        turn: r.mission.send,
                        controlled: Some(r.mission.owner),
                        army: Army::new(),
                    });
                }
            } else if r.mission.destination == planet.id
                && r.mission.objective != Icon::MissileStrike
            {
                // Mission arrived at this planet
                let can_see = r.can_see(&Side::Defender, self.id);
                reports.push(PlanetInfo {
                    turn: r.turn,
                    controlled: r.destination_controlled,
                    army: Unit::all()
                        .iter()
                        .flatten()
                        .filter_map(|u| {
                            if can_see {
                                if r.winner() == r.planet.controlled
                                    || r.mission.objective == Icon::Destroy
                                {
                                    Some((*u, r.surviving_defender.amount(u)))
                                } else {
                                    Some((
                                        *u,
                                        if u.is_building() {
                                            r.surviving_defender.amount(u)
                                        } else if *u == Unit::probe() {
                                            r.surviving_attacker
                                                .amount(u)
                                                .saturating_sub(r.scout_probes)
                                        } else {
                                            r.surviving_attacker.amount(u)
                                        },
                                    ))
                                }
                            } else if r.mission.owner == self.id
                                && r.scout_probes
                                    > u.production()
                                        .saturating_sub(1)
                                        .saturating_mul(PROBES_PER_PRODUCTION_LEVEL)
                            {
                                Some((*u, r.planet.army.amount(u)))
                            } else {
                                None
                            }
                        })
                        .collect(),
                });
            }
        }

        // Add missions that haven't arrived yet
        for m in missions {
            if m.origin == planet.id {
                if m.owner == self.id {
                    // Ignore returning probes or from destroy mission
                    if m.origin_controlled == Some(self.id) {
                        // Own mission send from this planet (and it's no longer controlled)
                        let army: Army = Unit::all()
                            .iter()
                            .flatten()
                            .map(|u| (*u, m.origin_army.amount(u).saturating_sub(m.army.amount(u))))
                            .collect();

                        reports.push(PlanetInfo {
                            turn: m.send,
                            controlled: None, // It's no longer controlled or we wouldn't need last_info
                            army,
                        });
                    }
                } else if !m.objective.is_hidden() {
                    // Enemy mission
                    reports.push(PlanetInfo {
                        turn: m.send,
                        controlled: Some(m.owner),
                        army: Army::new(),
                    });
                }
            }
        }

        // Select the latest report and take the highest building level from every report
        reports.iter().max_by_key(|r| r.turn).cloned().map(|mut best| {
            for building in Unit::buildings() {
                if let Some(highest) =
                    reports.iter().map(|r| r.army.amount(&building)).filter(|a| *a > 0).max()
                {
                    best.army.insert(building, highest);
                }
            }

            best
        })
    }
}
