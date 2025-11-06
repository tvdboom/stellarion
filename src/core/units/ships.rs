use std::collections::HashMap;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;

use crate::core::resources::Resources;
use crate::core::units::defense::Defense;
use crate::core::units::{Combat, Description, Price, Unit};

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
    pub fn production(&self) -> usize {
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
}

impl Description for Ship {
    fn description(&self) -> &str {
        match self {
            Ship::Probe => {
                "The Probe is an espionage craft, used to analyze enemy defenses. By default, \
                this ship only takes part in the first round of any attack (this behavior can be \
                changed when sending a mission). After the first round, it reports on the enemy \
                units (prior to any combat) and returns to the planet of origin. The more Probes \
                survive, the better the intelligence. Probes don't have damage, but can be used \
                as fodder in combat. Probes are the fastest ships in the game."
            },
            Ship::ColonyShip => {
                "This ship is used to colonize (gain ownership) planets. The Colony Ship does \
                not take part in any combat and is automatically destroyed if the fight is lost. \
                Upon colonizing a planet, the ship is deconstructed. Colony ships are very slow \
                and consume a lot of fuel."
            },
            Ship::LightFighter => {
                "Given their relatively low armor and simple weapons systems, Light Fighters \
                serve best as support ships in battle. Their agility and speed, paired with \
                the number in which they often appear, can provide a shield-like buffer for \
                bigger ships that are not quite as maneuverable. Light Fighters are often used \
                as fodder."
            },
            Ship::HeavyFighter => {
                "The Heavy Fighter is a more powerful version of the Light Fighter. Even though \
                it is not as effective as the Cruiser, the Heavy Fighter can still cause a \
                reasonable amount of damage when launched in significant numbers."
            },
            Ship::Destroyer => {
                "With their Rapid Fire capabilities, Destroyers are extremely effective at \
                eliminating the Light Fighter and Rocket Launcher fodder. In addition they're \
                speed make them excellent as fast strike ships."
            },
            Ship::Cruiser => {
                "Cruisers are the backbone of any military fleet. Heavy armor, powerful weapon
                systems, and a high speed make this ship a tough opponent to fight against."
            },
            Ship::Bomber => {
                "The Bomber is used primarily to destroy planetary defense. Its high Rapid Fire \
                against most defensive structures makes it effective for planetary assaults. It's \
                the only ship with Rapid Fire against the Plasma Turret."
            },
            Ship::Battleship => {
                "The Battleship is the mean between the Cruiser and the Dreadnought. Its Rapid \
                Fire capabilities makes him highly effective against medium-sized ships."
            },
            Ship::Dreadnought => {
                "Dreadnoughts are the largest and most powerful ships, second only to the War Sun. \
                They are relatively slow, and require a lot of fuel to move, but have incredibly \
                high damage. Due to its Rapid Fire capabilities, it's highly specialized in the \
                interception of hostile heavy ships."
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
    fn hull(&self) -> usize {
        match self {
            Ship::Probe => 10,
            Ship::ColonyShip => 0,
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
            Ship::ColonyShip => 0,
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
            Ship::ColonyShip => 0,
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

    fn rapid_fire(&self) -> HashMap<Unit, usize> {
        match self {
            Ship::Probe | Ship::ColonyShip => HashMap::new(),
            Ship::LightFighter | Ship::HeavyFighter => {
                HashMap::from([(Unit::Ship(Ship::Probe), 80)])
            },
            Ship::Destroyer => HashMap::from([
                (Unit::Ship(Ship::Probe), 80),
                (Unit::Ship(Ship::LightFighter), 70),
                (Unit::Defense(Defense::RocketLauncher), 70),
            ]),
            Ship::Cruiser => HashMap::from([(Unit::Ship(Ship::Probe), 80)]),
            Ship::Bomber => HashMap::from([
                (Unit::Ship(Ship::Probe), 80),
                (Unit::Defense(Defense::RocketLauncher), 80),
                (Unit::Defense(Defense::LightLaser), 80),
                (Unit::Defense(Defense::HeavyLaser), 60),
                (Unit::Defense(Defense::GaussCannon), 60),
                (Unit::Defense(Defense::IonCannon), 40),
                (Unit::Defense(Defense::PlasmaTurret), 40),
            ]),
            Ship::Battleship => HashMap::from([
                (Unit::Ship(Ship::Probe), 80),
                (Unit::Ship(Ship::HeavyFighter), 70),
                (Unit::Ship(Ship::Destroyer), 60),
                (Unit::Ship(Ship::Cruiser), 50),
            ]),
            Ship::Dreadnought => HashMap::from([
                (Unit::Ship(Ship::Probe), 80),
                (Unit::Ship(Ship::Bomber), 40),
                (Unit::Ship(Ship::Battleship), 40),
                (Unit::Ship(Ship::Dreadnought), 30),
            ]),
            Ship::WarSun => HashMap::from([
                (Unit::Ship(Ship::Probe), 80),
                (Unit::Ship(Ship::LightFighter), 80),
                (Unit::Ship(Ship::HeavyFighter), 80),
                (Unit::Ship(Ship::Destroyer), 70),
                (Unit::Ship(Ship::Cruiser), 60),
                (Unit::Ship(Ship::Bomber), 50),
                (Unit::Ship(Ship::Battleship), 50),
                (Unit::Ship(Ship::Dreadnought), 40),
                (Unit::Defense(Defense::RocketLauncher), 80),
                (Unit::Defense(Defense::LightLaser), 80),
                (Unit::Defense(Defense::HeavyLaser), 60),
                (Unit::Defense(Defense::GaussCannon), 60),
                (Unit::Defense(Defense::IonCannon), 40),
            ]),
        }
    }

    fn speed(&self) -> f32 {
        match self {
            Ship::Probe => 1.8,
            Ship::ColonyShip => 1.0,
            Ship::LightFighter => 1.6,
            Ship::HeavyFighter => 1.6,
            Ship::Destroyer => 1.5,
            Ship::Cruiser => 1.4,
            Ship::Bomber => 1.2,
            Ship::Battleship => 1.3,
            Ship::Dreadnought => 1.2,
            Ship::WarSun => 1.1,
        }
    }

    fn fuel_consumption(&self) -> usize {
        match self {
            Ship::Probe => 1,
            Ship::ColonyShip => 10,
            Ship::LightFighter => 2,
            Ship::HeavyFighter => 3,
            Ship::Destroyer => 3,
            Ship::Cruiser => 6,
            Ship::Bomber => 7,
            Ship::Battleship => 8,
            Ship::Dreadnought => 9,
            Ship::WarSun => 12,
        }
    }
}
