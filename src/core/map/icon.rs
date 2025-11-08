use bevy::prelude::Component;
use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;

use crate::core::map::planet::Planet;
use crate::core::ui::systems::Shop;
use crate::core::units::defense::Defense;
use crate::core::units::ships::Ship;
use crate::core::units::{Description, Unit};

#[derive(
    Component, EnumIter, Copy, Clone, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize,
)]
pub enum Icon {
    Colonize,
    #[default]
    Attack,
    Spy,
    MissileStrike,
    Destroy,
    Attacked,
    Buildings,
    Fleet,
    Defenses,
    Deploy,
}

impl Icon {
    pub const SIZE: f32 = Planet::SIZE * 0.2;

    pub fn on_units(&self) -> bool {
        matches!(self, Icon::Buildings | Icon::Fleet | Icon::Defenses)
    }

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

    pub fn shop(&self) -> Shop {
        match self {
            Icon::Buildings => Shop::Buildings,
            Icon::Fleet => Shop::Fleet,
            Icon::Defenses => Shop::Defenses,
            _ => unreachable!(),
        }
    }

    pub fn priority(&self) -> usize {
        match self {
            Icon::Colonize => 2,
            Icon::Attack => 1,
            Icon::Spy => 4,
            Icon::MissileStrike => 5,
            Icon::Destroy => 3,
            Icon::Deploy => 0,
            _ => unreachable!(),
        }
    }

    pub fn objectives(to_owned_planet: bool, to_controlled_planet: bool) -> Vec<Icon> {
        if to_owned_planet {
            vec![Icon::Deploy]
        } else if to_controlled_planet {
            vec![Icon::Colonize, Icon::Deploy]
        } else {
            vec![Icon::Colonize, Icon::Attack, Icon::Spy, Icon::MissileStrike, Icon::Destroy]
        }
    }

    pub fn condition(&self, origin: &Planet) -> bool {
        match self {
            Icon::Buildings => origin.has_buildings(),
            Icon::Fleet => origin.has_fleet(),
            Icon::Defenses => origin.has_defense(),
            Icon::Colonize => origin.has(&Unit::Ship(Ship::ColonyShip)),
            Icon::Attack => origin.army.iter().any(|(u, c)| *c > 0 && u.is_combat_ship()),
            Icon::Spy => origin.has(&Unit::Ship(Ship::Probe)),
            Icon::MissileStrike => origin.has(&Unit::Defense(Defense::InterplanetaryMissile)),
            Icon::Destroy => origin.has(&Unit::Ship(Ship::WarSun)),
            Icon::Deploy => origin.has_fleet(),
            _ => unreachable!(),
        }
    }

    pub fn requirement(&self) -> &str {
        match self {
            Icon::Colonize => {
                "No Colony Ship on the origin planet or maximum number of colonized planets reached."
            },
            Icon::Attack => "No combat ships on the origin planet.",
            Icon::Spy => "No Probes on the origin planet.",
            Icon::MissileStrike => "No Interplanetary Missiles on the origin planet.",
            Icon::Destroy => "No War Suns on the origin planet.",
            Icon::Deploy => "No ships on the origin planet.",
            _ => unreachable!(),
        }
    }
}

impl Description for Icon {
    fn description(&self) -> &str {
        match self {
            Icon::Colonize => {
                "A successful mission that contains at least one Colony Ship, colonizes the target \
                planet (the player gains ownership). The Colony Ship is consumed in the process. \
                If the planet is empty, a level 1 Mine is automatically built. An owned planet \
                produces resources and can be developed with buildings."
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
                better the intelligence. Spying missions aren't detected by the Sensor Phalanx."
            },
            Icon::MissileStrike => {
                "Launch an Interplanetary Missile strike against an enemy planet. Missiles can \
                not be accompanied by any other ships. Interplanetary Missiles ignore any ships \
                and the Planetary Shield at the target planet, directly hitting any defenses. \
                At the end of combat, all surviving missiles are destroyed. Once launched, a \
                missile strike always hits the destination planet, even if it has been colonized \
                by the player. Missile Strikes don't report any intelligence about the enemy units."
            },
            Icon::Destroy => {
                "Attack a planet with your combat ships. After every round of the attack, and only \
                if there are no enemy ships left, every War Sun tries to destroy the target planet \
                with a 10% chance, decreased with 1% for every round afterwards. Regardless of the \
                result, the fleet returns after combat. A destroyed planet can't be colonized again."
            },
            Icon::Deploy => "Send a fleet to another one of your planets.",
            _ => unreachable!(),
        }
    }
}
