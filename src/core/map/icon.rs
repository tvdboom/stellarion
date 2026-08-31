//! Strategic-map icon/objective kinds and their gameplay requirements.

use bevy::prelude::Component;
use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;

use crate::core::map::planet::Planet;
#[cfg(feature = "app")]
use crate::core::ui::systems::Shop;
use crate::core::units::{Description, Unit};

#[derive(
    Component, EnumIter, Copy, Clone, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize,
)]
/// Strategic objective and UI-category icons shared by missions and map panels.
pub enum Icon {
    /// The colonize value.
    Colonize,
    #[default]
    /// The attack value.
    Attack,
    /// The spy value.
    Spy,
    /// The missile strike value.
    MissileStrike,
    /// The destroy value.
    Destroy,
    /// The attacked value.
    Attacked,
    /// The buildings value.
    Buildings,
    /// The fleet value.
    Fleet,
    /// The defenses value.
    Defenses,
    /// The deploy value.
    Deploy,
}

impl Icon {
    /// Rendered size associated with this map object.
    pub const SIZE: f32 = Planet::SIZE * 0.2;

    /// Handles the units interaction.
    pub fn on_units(&self) -> bool {
        matches!(self, Icon::Buildings | Icon::Fleet | Icon::Defenses)
    }

    /// Handles the planet only interaction.
    pub fn on_planet_only(&self) -> bool {
        matches!(self, Icon::Colonize | Icon::MissileStrike)
    }

    /// Returns whether this value mission.
    pub fn is_mission(&self) -> bool {
        matches!(
            self,
            Icon::Deploy
                | Icon::Colonize
                | Icon::Attack
                | Icon::Spy
                | Icon::MissileStrike
                | Icon::Destroy
        )
    }

    /// Returns whether this value hidden.
    pub fn is_hidden(&self) -> bool {
        matches!(self, Icon::Spy | Icon::MissileStrike)
    }

    #[cfg(feature = "app")]
    /// Returns the shop category associated with this map icon.
    pub fn shop(&self) -> Option<Shop> {
        match self {
            Icon::Buildings => Some(Shop::Buildings),
            Icon::Fleet => Some(Shop::Fleet),
            Icon::Defenses => Some(Shop::Defenses),
            _ => None,
        }
    }

    /// Returns the stable resolution priority of this objective.
    pub fn priority(&self) -> Option<usize> {
        match self {
            Icon::Colonize => Some(2),
            Icon::Attack => Some(1),
            Icon::Spy => Some(4),
            Icon::MissileStrike => Some(5),
            Icon::Destroy => Some(3),
            Icon::Deploy => Some(0),
            _ => None,
        }
    }

    /// Returns mission objectives available for the selected origin and destination.
    pub fn objectives(to_owned_planet: bool, to_controlled_planet: bool) -> Vec<Icon> {
        if to_owned_planet {
            vec![Icon::Deploy]
        } else if to_controlled_planet {
            vec![Icon::Colonize, Icon::Deploy]
        } else {
            vec![Icon::Colonize, Icon::Attack, Icon::Spy, Icon::MissileStrike, Icon::Destroy]
        }
    }

    /// Returns whether this objective is currently allowed for the selected mission.
    pub fn condition(&self, origin: &Planet) -> bool {
        match self {
            Icon::Buildings => origin.has_buildings(),
            Icon::Fleet => origin.has_fleet(),
            Icon::Defenses => origin.has_defense(),
            Icon::Colonize => origin.has(&Unit::colony_ship()),
            Icon::Attack => origin.army.iter().any(|(u, c)| *c > 0 && u.is_combat_ship()),
            Icon::Spy => origin.has(&Unit::probe()),
            Icon::MissileStrike => origin.has(&Unit::interplanetary_missile()),
            Icon::Destroy => origin.has(&Unit::war_sun()),
            Icon::Deploy => origin.has_fleet(),
            Icon::Attacked => false,
        }
    }

    /// Returns a user-facing explanation when this objective is unavailable.
    pub fn requirement(&self) -> &str {
        match self {
            Icon::Colonize => {
                "No Colony Ship on the origin planet, maximum number of colonized planets \
                reached or destination is a moon."
            },
            Icon::Attack => "No combat ships on the origin planet.",
            Icon::Spy => "No Probes on the origin planet.",
            Icon::MissileStrike => {
                "No Interplanetary Missiles on the origin planet or destination is a moon."
            },
            Icon::Destroy => "No War Suns on the origin planet.",
            Icon::Deploy => "No ships on the origin planet.",
            _ => "This icon is not a mission objective.",
        }
    }
}

impl Description for Icon {
    /// Returns the user-facing description of this gameplay value.
    fn description(&self) -> &str {
        match self {
            Icon::Colonize => {
                "A successful mission that contains at least one Colony Ship, colonizes the target \
                planet (the player gains ownership). The Colony Ship is consumed in the process. \
                If the planet is empty, a level 1 Metal Mine, Crystal Mine and Deuterium Synthesizer \
                are automatically built. An owned planet produces resources and can be developed \
                with buildings."
            },
            Icon::Attack => {
                "Attack a planet with your combat ships. If the attack is successful, the ships \
                remain on the conquered planet, gaining control, but not ownership over it. If \
                the planet was owned by another player, they lose ownership. Buildings on the \
                target planet remain."
            },
            Icon::Spy => {
                "Send only Probes to gather intelligence on an enemy planet. Probes leave combat \
                after the first round, and report on the enemy units. The more Probes return, the \
                better the intelligence. Spying missions aren't detected by the Sensor Phalanx \
                and don't reveal the planet of origin."
            },
            Icon::MissileStrike => {
                "Launch an Interplanetary Missile strike against an enemy planet. Missiles can \
                not be accompanied by any other ships. Interplanetary Missiles ignore any ships \
                and the Planetary Shield at the target planet, directly hitting any defenses. \
                At the end of combat, all surviving missiles are destroyed. Once launched, a \
                missile strike always hits the destination planet, even if it has been colonized \
                by the player. Missile Strikes don't report any intelligence about the enemy \
                units. They cannot be detected by the Sensor Phalanx and don't reveal the planet \
                of origin."
            },
            Icon::Destroy => {
                "Attack a planet with your combat ships. After every round of the attack, and only \
                if there are no enemy ships left, every War Sun tries to destroy the target planet \
                with a 10-15% chance (depending on the planet's size), decreased with 1% for every \
                round afterwards (long battles reduce the destruction chance to zero). Regardless \
                of the result, the fleet returns after combat. A destroyed planet can't be \
                colonized again."
            },
            Icon::Deploy => "Send a fleet to another planet you control.",
            _ => "This icon selects a local map or shop category.",
        }
    }
}
