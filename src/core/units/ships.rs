use crate::core::resources::Resources;
use crate::core::units::{Combat, Description, Price};
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use strum_macros::EnumIter;

pub type Fleet = HashMap<Ship, usize>;

#[derive(Component, EnumIter, Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum Ship {
    Probe,
    ColonyShip,
    LightFighter,
    HeavyFighter,
    Destroyer,
    Cruiser,
    Bomber,
    Battleship,
    Dreadnought,
    WarSun,
}

impl Ship {
    /// Minim level of the shipyard to build this ship
    pub fn level(&self) -> usize {
        match self {
            Ship::Probe => 1,
            Ship::ColonyShip => 2,
            Ship::LightFighter => 1,
            Ship::HeavyFighter => 1,
            Ship::Destroyer => 2,
            Ship::Cruiser => 3,
            Ship::Bomber => 3,
            Ship::Battleship => 4,
            Ship::Dreadnought => 4,
            Ship::WarSun => 5,
        }
    }

    pub fn is_combat(&self) -> bool {
        match self {
            Ship::Probe | Ship::ColonyShip => false,
            _ => true,
        }
    }
}

impl Description for Ship {
    fn description(&self) -> &str {
        match self {
            Ship::Probe => {
                "The probe is an espionage craft, used to analyze enemy defenses. This ship \
                is very likely to be destroyed in any conflict. They are the fastest ships \
                in the game."
            },
            Ship::ColonyShip => {
                "This ship is used to colonize planets. During a fight, the colony ship \
                is always focused last. Upon colonizing a planet, the ship is deconstructed."
            },
            Ship::LightFighter => {
                "Given their relatively low armor and simple weapons systems, light fighters \
                serve best as support ships in battle. Their agility and speed, paired with \
                the number in which they often appear, can provide a shield-like buffer for \
                bigger ships that are not quite as maneuverable. Light Fighters are often used \
                as fodder."
            },
            Ship::HeavyFighter => {
                "The Heavy Fighter is a more powerful version of the light fighter. Even though \
                it is not as effective as the cruiser, the heavy fighter can still cause a \
                reasonable amount of damage when launched in significant numbers."
            },
            Ship::Destroyer => {
                "With their rapid fire capabilities, destroyers are extremely effective at \
                eliminating the light fighter and rocket launcher fodder. In addition they're \
                speed make them excellent as fast strike ships."
            },
            Ship::Cruiser => {
                "Cruisers are the backbone of any military fleet. Heavy armor, powerful weapon
                systems, and a high speed make this ship a tough opponent to fight against."
            },
            Ship::Bomber => {
                "The bomber is used primarily to destroy planetary defense. Its high rapid fire \
                against most defensive structures makes it effective for planetary assaults."
            },
            Ship::Battleship => {
                "The battleship is the mean between the cruiser and the dreadnought. Due to its \
                rapid fire capabilities, it's highly specialized in the interception of hostile \
                heavy ships."
            },
            Ship::Dreadnought => {
                "Dreadnoughts are the largest and most powerful ships, second only to the War Sun. \
                They are relatively slow, and require a lot of fuel to move, but have incredibly \
                high damage."
            },
            Ship::WarSun => {
                "The War Sun is the most advanced ship in the game. It has the highest damage, \
                shield strength, and health of all ships, but what makes it unique, is that it \
                can destroy entire planets. Some consider that building a War Sun the ultimate \
                achievement in the universe."
            },
        }
    }
}

impl Price for Ship {
    fn price(&self) -> Resources {
        match self {
            Ship::Probe => Resources::new(0, 10, 0),
            Ship::ColonyShip => Resources::new(100, 200, 100),
            Ship::LightFighter => Resources::new(30, 10, 0),
            Ship::HeavyFighter => Resources::new(60, 40, 0),
            Ship::Destroyer => Resources::new(60, 70, 20),
            Ship::Cruiser => Resources::new(100, 100, 0),
            Ship::Bomber => Resources::new(100, 200, 35),
            Ship::Battleship => Resources::new(150, 200, 100),
            Ship::Dreadnought => Resources::new(250, 200, 150),
            Ship::WarSun => Resources::new(1000, 500, 250),
        }
    }
}

impl Combat for Ship {
    fn health(&self) -> usize {
        match self {
            Ship::Probe => 10,
            Ship::ColonyShip => 300,
            Ship::LightFighter => 40,
            Ship::HeavyFighter => 100,
            Ship::Destroyer => 270,
            Ship::Cruiser => 350,
            Ship::Bomber => 350,
            Ship::Battleship => 500,
            Ship::Dreadnought => 700,
            Ship::WarSun => 1000,
        }
    }

    fn shield(&self) -> usize {
        match self {
            Ship::Probe => 0,
            Ship::ColonyShip => 10,
            Ship::LightFighter => 1,
            Ship::HeavyFighter => 3,
            Ship::Destroyer => 10,
            Ship::Cruiser => 20,
            Ship::Bomber => 40,
            Ship::Battleship => 40,
            Ship::Dreadnought => 50,
            Ship::WarSun => 100,
        }
    }

    fn damage(&self) -> usize {
        match self {
            Ship::Probe => 0,
            Ship::ColonyShip => 5,
            Ship::LightFighter => 5,
            Ship::HeavyFighter => 15,
            Ship::Destroyer => 40,
            Ship::Cruiser => 70,
            Ship::Bomber => 60,
            Ship::Battleship => 80,
            Ship::Dreadnought => 100,
            Ship::WarSun => 250,
        }
    }

    fn speed(&self) -> f32 {
        match self {
            Ship::Probe => 4.,
            Ship::ColonyShip => 1.5,
            Ship::LightFighter => 3.5,
            Ship::HeavyFighter => 2.5,
            Ship::Destroyer => 3.,
            Ship::Cruiser => 2.5,
            Ship::Bomber => 1.5,
            Ship::Battleship => 2.,
            Ship::Dreadnought => 1.5,
            Ship::WarSun => 1.,
        }
    }

    fn fuel_consumption(&self) -> usize {
        match self {
            Ship::Probe => 1,
            Ship::ColonyShip => 10,
            Ship::LightFighter => 2,
            Ship::HeavyFighter => 3,
            Ship::Destroyer => 5,
            Ship::Cruiser => 6,
            Ship::Bomber => 7,
            Ship::Battleship => 8,
            Ship::Dreadnought => 9,
            Ship::WarSun => 12,
        }
    }
}
