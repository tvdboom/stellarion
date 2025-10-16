use crate::core::map::planet::Planet;
use crate::core::ui::systems::Shop;
use crate::core::units::defense::Defense;
use crate::core::units::ships::Ship;
use crate::core::units::{Description, Unit};
use bevy::prelude::Component;
use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;

#[derive(Component, EnumIter, Copy, Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum Icon {
    #[default]
    Colonize,
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

    pub fn objectives(to_own_planet: bool) -> Vec<Icon> {
        if to_own_planet {
            vec![Icon::Deploy]
        } else {
            vec![Icon::Colonize, Icon::Attack, Icon::Spy, Icon::MissileStrike, Icon::Destroy]
        }
    }

    pub fn condition(&self, planet: &Planet) -> bool {
        match self {
            Icon::Buildings => !planet.complex.is_empty(),
            Icon::Fleet => planet.has_fleet(),
            Icon::Defenses => planet.has_battery(),
            Icon::Deploy => planet.has_fleet(),
            Icon::Colonize => planet.has(&Unit::Ship(Ship::ColonyShip)),
            Icon::Attack => planet.fleet.iter().any(|(s, c)| s.is_combat() && *c > 0),
            Icon::Spy => planet.has(&Unit::Ship(Ship::Probe)),
            Icon::MissileStrike => planet.has(&Unit::Defense(Defense::InterplanetaryMissile)),
            Icon::Destroy => planet.has(&Unit::Ship(Ship::WarSun)),
            _ => unreachable!(),
        }
    }

    pub fn requirement(&self) -> &str {
        match self {
            Icon::Colonize => "No Colony Ship on the origin planet.",
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
                "After a successful attack that contains at least one Colony Ship will colonize \
                the target planet. The Colony Ship will be consumed in the process. If the planet \
                is empty, a level 1 mine will be built automatically. A colonized planet produces \
                resources and can be developed with buildings."
            },
            Icon::Attack => {
                "Attack a planet with your combat ships. If the attack is successful, the ships \
                remain on the conquered planet, but do not colonize it. If the planet was owned \
                by another player, they will lose control of it. All buildings will remain."
            },
            Icon::Spy => {
                "Send only Probes to gather intelligence on an enemy planet. Probes return to the \
                origin planet after one round of combat. The more Probes you send, the more \
                accurate the returned information will be."
            },
            Icon::MissileStrike => {
                "Launch an interplanetary missile strike against an enemy planet. Missiles can \
                not be accompanied by any other ships. Interplanetary missiles ignore any ships \
                and the planetary shield at the target planet, and directly hit the defenses."
            },
            Icon::Destroy => {
                "After a successful attack, every surviving War Sun will try to obliterate the \
                target planet with a 20% chance. If the planet is destroyed, the fleet will return \
                to the origin planet. A destroyed planet can not be colonized again."
            },
            Icon::Deploy => "Send a fleet to another one of your planets.",
            _ => unreachable!(),
        }
    }
}
