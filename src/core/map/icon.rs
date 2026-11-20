//! Strategic-map icon/objective kinds and their gameplay requirements.

use bevy::prelude::Component;
use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;

use crate::core::constants::MIN_SPY_PROBES;
use crate::core::map::planet::Planet;
#[cfg(feature = "app")]
use crate::core::ui::systems::Shop;
use crate::core::units::{Amount, Army, Description, Unit};
#[cfg(feature = "app")]
use crate::utils::NameFromEnum;

#[derive(
    Component, EnumIter, Copy, Clone, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize,
)]
/// Strategic objective and UI-category icons shared by missions and map panels.
pub enum Icon {
    /// The colonize value.
    Colonize,
    /// Stations a player-owned fleet at another player's world with the controller's permission.
    Protect,
    #[default]
    /// The attack value.
    Attack,
    /// Fights for resources and returns without changing ownership or control.
    Raid,
    /// Searches a fixed area of space and attacks every detected foreign fleet.
    Patrol,
    /// Explores a deep-space landmark and returns with resources or recovered ships.
    Expedition,
    /// Ambushes one visible travelling enemy fleet before its arrival.
    Intercept,
    /// The spy value.
    Spy,
    /// Probes temporarily disable a known orbital structure.
    Sabotage,
    /// The missile strike value.
    MissileStrike,
    /// The destroy value.
    Destroy,
    /// Commits every in-range owned Orbital Railgun against this world.
    RailgunStrike,
    /// A coordinated attack whose participating fleets are visible to the local player.
    AlliedAttack,
    /// An incoming hostile fleet whose exact objective is unknown to the local player.
    EnemyFleet,
    /// The buildings value.
    Buildings,
    /// The orbitals value.
    Orbitals,
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

    /// Returns the runtime image key used for this objective.
    ///
    /// Returns the dedicated mission/action artwork for every objective.
    #[cfg(feature = "app")]
    pub fn asset_key(&self) -> String {
        self.to_lowername()
    }

    /// Handles the units interaction.
    pub fn on_units(&self) -> bool {
        matches!(self, Icon::Buildings | Icon::Orbitals | Icon::Fleet | Icon::Defenses)
    }

    /// Handles the planet only interaction.
    pub fn on_planet_only(&self) -> bool {
        matches!(self, Icon::Colonize | Icon::Raid | Icon::MissileStrike | Icon::Sabotage)
    }

    /// Returns whether this value mission.
    pub fn is_mission(&self) -> bool {
        matches!(
            self,
            Icon::Deploy
                | Icon::Colonize
                | Icon::Attack
                | Icon::Raid
                | Icon::Patrol
                | Icon::Expedition
                | Icon::Intercept
                | Icon::Spy
                | Icon::Sabotage
                | Icon::MissileStrike
                | Icon::Destroy
                | Icon::Protect
        )
    }

    /// Returns whether an active mission with this objective may be recalled after launch.
    pub fn is_recallable(self) -> bool {
        matches!(
            self,
            Icon::Deploy
                | Icon::Colonize
                | Icon::Attack
                | Icon::Raid
                | Icon::Patrol
                | Icon::Expedition
                | Icon::Intercept
                | Icon::Spy
                | Icon::Sabotage
                | Icon::Destroy
                | Icon::Protect
        )
    }

    /// Probe-only reconnaissance and sabotage share concealment and first-round withdrawal.
    pub const fn is_probe_mission(self) -> bool {
        matches!(self, Self::Spy | Self::Sabotage)
    }

    /// Returns whether this value hidden.
    pub fn is_hidden(&self) -> bool {
        matches!(self, Icon::Spy | Icon::Sabotage | Icon::MissileStrike)
    }

    /// Returns whether this action is hostile toward its destination.
    pub fn is_hostile_action(self) -> bool {
        matches!(
            self,
            Icon::Colonize
                | Icon::Attack
                | Icon::Raid
                | Icon::Spy
                | Icon::Sabotage
                | Icon::MissileStrike
                | Icon::Destroy
                | Icon::RailgunStrike
        )
    }

    #[cfg(feature = "app")]
    /// Returns the shop category associated with this map icon.
    pub fn shop(&self) -> Option<Shop> {
        match self {
            Icon::Buildings => Some(Shop::Buildings),
            Icon::Orbitals => Some(Shop::Orbitals),
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
            Icon::Raid => Some(1),
            Icon::Spy | Icon::Sabotage => Some(4),
            Icon::MissileStrike => Some(5),
            Icon::Destroy => Some(3),
            Icon::Deploy | Icon::Protect => Some(0),
            _ => None,
        }
    }

    /// Returns mission objectives available for the selected origin and destination.
    pub fn objectives(
        to_owned_planet: bool,
        to_controlled_planet: bool,
        protection_allowed: bool,
        already_protecting: bool,
    ) -> Vec<Icon> {
        if to_owned_planet {
            vec![Icon::Deploy]
        } else if to_controlled_planet {
            vec![Icon::Colonize, Icon::Deploy]
        } else if already_protecting {
            vec![Icon::Protect]
        } else {
            let mut objectives = vec![
                Icon::Colonize,
                Icon::Attack,
                Icon::Raid,
                Icon::Spy,
                Icon::Sabotage,
                Icon::MissileStrike,
                Icon::Destroy,
            ];
            if protection_allowed {
                objectives.insert(1, Icon::Protect);
            }
            objectives
        }
    }

    /// Returns whether this objective is currently allowed for the selected mission.
    pub fn condition(&self, origin: &Planet) -> bool {
        match self {
            Icon::Buildings => origin.has_buildings(),
            Icon::Orbitals => {
                origin.army.iter().any(|(unit, count)| unit.is_orbital() && *count > 0)
            },
            Icon::Fleet | Icon::Deploy | Icon::Protect => origin.has_fleet(),
            Icon::Defenses => origin.has_defense(),
            Icon::Colonize | Icon::Expedition => origin.has(&Unit::colony_ship()),
            Icon::Attack | Icon::Raid | Icon::Patrol | Icon::Intercept => {
                origin.army.iter().any(|(u, c)| *c > 0 && u.is_combat_ship())
            },
            Icon::Spy | Icon::Sabotage => origin.army.amount(&Unit::probe()) >= MIN_SPY_PROBES,
            Icon::MissileStrike => origin.has(&Unit::interplanetary_missile()),
            Icon::Destroy => origin.has(&Unit::war_sun()),
            Icon::RailgunStrike | Icon::AlliedAttack | Icon::EnemyFleet => false,
        }
    }

    /// Returns whether this objective can be launched from the supplied player-owned unit pool.
    pub fn condition_for_army(&self, army: &Army) -> bool {
        match self {
            Icon::Colonize | Icon::Expedition => army.amount(&Unit::colony_ship()) > 0,
            Icon::Attack | Icon::Raid | Icon::Patrol | Icon::Intercept => {
                army.iter().any(|(unit, count)| *count > 0 && unit.is_combat_ship())
            },
            Icon::Spy | Icon::Sabotage => army.amount(&Unit::probe()) >= MIN_SPY_PROBES,
            Icon::MissileStrike => army.amount(&Unit::interplanetary_missile()) > 0,
            Icon::Destroy => army.amount(&Unit::war_sun()) > 0,
            Icon::Deploy | Icon::Protect | Icon::Fleet => {
                army.iter().any(|(unit, count)| unit.is_ship() && *count > 0)
            },
            Icon::Buildings => army.iter().any(|(unit, count)| unit.is_building() && *count > 0),
            Icon::Orbitals => army.iter().any(|(unit, count)| unit.is_orbital() && *count > 0),
            Icon::Defenses => army.iter().any(|(unit, count)| unit.is_defense() && *count > 0),
            Icon::RailgunStrike | Icon::AlliedAttack | Icon::EnemyFleet => false,
        }
    }

    /// Checks the actual dispatched units, ignoring empty entries retained by the UI.
    pub fn accepts_army(self, army: &Army) -> bool {
        self.fleet_unavailable_reason(army).is_none()
    }

    /// Explains a missing required unit or a disallowed unit in the dispatched fleet.
    pub(crate) fn fleet_unavailable_reason(self, army: &Army) -> Option<&'static str> {
        if !self.condition_for_army(army) {
            return Some(match self {
                Self::Colonize => "Colonization requires at least one Colony Ship.",
                Self::Expedition => "An Expedition requires at least one Colony Ship.",
                Self::Attack => "An Attack requires at least one combat ship.",
                Self::Raid => "A Raid requires at least one combat ship.",
                Self::Patrol => "A Patrol requires at least one combat ship. Probes can travel alone to reinforce your active patrol.",
                Self::Intercept => "Select at least one combat ship for interception.",
                Self::Spy | Self::Sabotage => "Spy and Sabotage missions require at least 5 Probes.",
                Self::MissileStrike => "A Missile Strike requires at least one Interplanetary Missile.",
                Self::Destroy => "A Destroy mission requires at least one War Sun.",
                Self::Deploy | Self::Protect => "Select at least one ship for this mission.",
                _ => "Select a mission objective.",
            });
        }
        let has_disallowed_unit = |allowed: fn(&Unit) -> bool| {
            army.iter().any(|(unit, count)| *count > 0 && !allowed(unit))
        };
        match self {
            Self::Spy | Self::Sabotage => has_disallowed_unit(|unit| *unit == Unit::probe())
                .then_some("Only Probes can join Spy and Sabotage missions."),
            Self::MissileStrike => {
                has_disallowed_unit(|unit| *unit == Unit::interplanetary_missile())
                    .then_some("Only Interplanetary Missiles can join a Missile Strike.")
            },
            Self::Deploy
            | Self::Protect
            | Self::Colonize
            | Self::Expedition
            | Self::Destroy
            | Self::Attack
            | Self::Raid
            | Self::Patrol
            | Self::Intercept => {
                has_disallowed_unit(Unit::is_ship).then_some("Only ships can join this mission.")
            },
            _ => Some("Select a mission objective."),
        }
    }

    /// Returns the explanation when this objective lacks its required origin units.
    pub fn requirement(&self) -> &'static str {
        match self {
            Icon::Colonize => "No Colony Ship on the origin planet.",
            Icon::Attack | Icon::Raid => "No combat ships on the origin planet.",
            Icon::Intercept => {
                "Select combat ships and a visible enemy mission with time to intercept."
            },
            Icon::Patrol => "Select combat ships and a patrol zone in empty space.",
            Icon::Expedition => {
                "Select at least one Colony Ship and a deep-space landmark destination."
            },
            Icon::Spy => "A minimum of 5 Probes are required for a Spy mission.",
            Icon::Sabotage => "A minimum of 5 Probes and a known orbital target are required.",
            Icon::MissileStrike => "No Interplanetary Missiles on the origin planet.",
            Icon::Destroy => "No War Suns on the origin planet.",
            Icon::Deploy => "No ships on the origin planet.",
            Icon::Protect => "No ships on the origin planet.",
            Icon::RailgunStrike => {
                "At least one owned Orbital Railgun must have this world in range."
            },
            _ => "This icon is not a mission objective.",
        }
    }
}

impl Description for Icon {
    /// Returns the user-facing description of this gameplay value.
    fn description(&self) -> &str {
        match self {
            Icon::Expedition => {
                "Explore a point on the map with at least one Colony Ship. Most completed \
                expeditions find some salvage. Sites farther from the nearest planet or moon you \
                control offer better odds of larger hauls and recovered ships. The fleet returns \
                automatically and delivers its \
                rewards on docking. Recalling before exploration earns no rewards."
            },
            Icon::Intercept => {
                "Ambush a visible enemy fleet along its flight path, including its final \
                approach before landing. A recall can make the committed rendezvous miss. \
                Surviving targets continue their scheduled movement."
            },
            Icon::Patrol => {
                "Patrol a fixed area of space and attack detected foreign fleets. Hover the \
                protection icon on an active patrol to choose friendly players: their detected \
                fleets are reported without combat. Their patrols may still attack yours. \
                Detection improves with route exposure and combined ship production. Insufficient \
                fuel automatically sends the mission back to the planet of origin."
            },
            Icon::Colonize => {
                "A successful mission that contains at least one Colony Ship, colonizes the target \
                planet (the player gains ownership). The Colony Ship is consumed in the process. \
                If the planet is empty, a level 1 Metal Mine, Crystal Mine and Deuterium Synthesizer \
                are automatically built. An owned planet produces resources and can be developed \
                with buildings. In games with Independent Populations, first contact can instead \
                reveal a neutral garrison that must be defeated in normal combat. Colony Ships \
                travelling alone or only with Probes automatically lose space-fauna encounters."
            },
            Icon::Attack => {
                "Attack a planet with your combat ships. If the attack is successful, the ships \
                remain on the conquered planet, gaining control, but not ownership over it. If \
                the planet was owned by another player, they lose ownership. Buildings on the \
                target planet remain. Unclaimed planets can reveal Independent Populations when \
                that game setting is enabled."
            },
            Icon::Raid => {
                "Fight normal combat, steal resources on victory, and return without taking \
                ownership or control. Moons cannot be raided. Resources raided are chosen \
                randomly, limited by the colony owner's shared stockpile."
            },
            Icon::Sabotage => {
                "Send at least 5 Probes to disable a known Sensor Phalanx, Jump Gate, or Orbital \
                Railgun for one turn. Probes withdraw after the first combat round. At least one \
                Probe must survive, and the orbital must still be there on arrival."
            },
            Icon::Spy => {
                "Send at least 5 Probes to gather intelligence on a target planet. Probes leave \
                combat after the first round and report on enemy or independent units. Every 5 returning probes \
                reveals one more level of intelligence. Spy missions aren't detected by the \
                Sensor Phalanx and don't reveal the planet of origin. If they encounter space \
                fauna en route, surviving Probes withdraw after the first round and continue."
            },
            Icon::MissileStrike => {
                "Launch an Interplanetary Missile strike against an enemy planet. Missiles can \
                not be accompanied by any other ships. Interplanetary Missiles ignore any ships \
                and the Planetary Shield at the target planet, directly hitting any defenses. \
                Once launched, a missile strike cannot be recalled and hits the destination planet \
                even if it has been colonized by the player. At the end of combat, all surviving \
                missiles are destroyed. Missile Strikes don't report any intelligence about the \
                enemy units. They cannot be detected by the Sensor Phalanx and don't reveal the \
                planet of origin. Missile Strikes never encounter space fauna."
            },
            Icon::Destroy => {
                "Attack a planet with your combat ships. After every round of the attack, and only \
                if there are no enemy ships or Space Dock left, every War Sun tries to destroy the \
                target planet with an initial 8-12% chance (10% plus the planet's size modifier), \
                decreased by 1% in each later combat round (long battles reduce the chance to zero). \
                Regardless of the result, the fleet returns after combat. A destroyed planet can't \
                be colonized again."
            },
            Icon::RailgunStrike => {
                "Commit every owned Orbital Railgun that can reach this world. All participating \
                Railguns charge separately, converge their fire, and strike together at the start \
                of the next turn. Each participating Railgun adds to the resource cost and the \
                combined destruction chance. Smaller worlds are easier to destroy, while every \
                Planetary Shield level reduces the chance and an overloaded Shield reduces it \
                twice as much."
            },
            Icon::Deploy => "Send a fleet to another planet you control.",
            Icon::Protect => {
                "Send your fleet to a world whose controller has granted you protection access. \
                The fleet remains yours and joins that world's defense. Select that world as a \
                mission origin to send your stationed units elsewhere, or recall the entire fleet \
                home. You cannot attack that world while your fleet is stationed there with active \
                protection access. A revoked fleet departs before incoming attacks resolve."
            },
            _ => "This icon selects a local map or shop category.",
        }
    }
}
