use crate::core::constants::{
    FACTORY_PRODUCTION_FACTOR, SHIPYARD_PRODUCTION_FACTOR, SILO_CAPACITY_FACTOR,
};
use crate::core::resources::Resources;
use crate::core::units::buildings::{Building, Complex};
use crate::core::units::defense::Battery;
use crate::core::units::ships::Fleet;
use crate::core::units::Unit;
use bevy::math::Vec2;
use bevy_renet::renet::ClientId;
use rand::prelude::IteratorRandom;
use rand::{rng, Rng};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::ops::Range;

pub type PlanetId = usize;

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum PlanetKind {
    Desert,
    Gas,
    Ice,
    Normal,
}

impl PlanetKind {
    pub fn indices(self) -> Vec<usize> {
        match self {
            PlanetKind::Desert => vec![2, 3, 5, 8, 9, 12, 13, 15, 16, 19, 20, 21],
            PlanetKind::Gas => vec![7, 10, 11, 14, 18, 37, 43, 45],
            PlanetKind::Ice => vec![1, 4, 6, 17, 22, 23, 26, 27, 28, 35, 36, 38, 40],
            PlanetKind::Normal => vec![25, 32, 34, 52, 62],
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Planet {
    // Planet characteristics
    pub id: PlanetId,
    pub name: String,
    pub kind: PlanetKind,
    pub image: usize,
    pub position: Vec2,
    pub resources: Resources,
    pub is_destroyed: bool,

    // Ownership and units
    pub owner: Option<ClientId>,
    pub complex: Complex,
    pub battery: Battery,
    pub fleet: Fleet,
    pub buy: Vec<Unit>,
}

impl Planet {
    // Pixel size of a planet on the screen
    pub const SIZE: f32 = 100.;

    pub fn new(id: PlanetId, name: String, position: Vec2) -> Self {
        let low = 0..3;
        let medium = 2..4;
        let high = 2..5;

        let configs: &[(PlanetKind, [&Range<usize>; 3])] = &[
            (PlanetKind::Desert, [&high, &low, &low]),
            (PlanetKind::Gas, [&low, &low, &high]),
            (PlanetKind::Ice, [&low, &high, &low]),
            (PlanetKind::Normal, [&medium, &medium, &low]),
        ];

        let (kind, ranges) = configs.iter().choose(&mut rng()).unwrap();

        let resources = Resources::new(
            rng().random_range(ranges[0].clone()) * 100,
            rng().random_range(ranges[1].clone()) * 100,
            rng().random_range(ranges[2].clone()) * 100,
        );

        Self {
            id,
            name,
            kind: *kind,
            image: *kind.indices().iter().choose(&mut rng()).unwrap(),
            position,
            resources,
            is_destroyed: false,
            owner: None,
            complex: HashMap::new(),
            battery: HashMap::new(),
            fleet: HashMap::new(),
            buy: vec![],
        }
    }

    pub fn make_home_planet(&mut self, client_id: ClientId) {
        self.resources = Resources::new(200, 200, 100);
        self.owner = Some(client_id);
        self.complex =
            HashMap::from([(Building::Mine, 1), (Building::Shipyard, 1), (Building::Factory, 1)]);
        self.fleet = HashMap::from([(crate::core::units::ships::Ship::LightFighter, 5)]);
    }

    pub fn get(&self, unit: &Unit) -> usize {
        match unit {
            Unit::Building(building) => *self.complex.get(building).unwrap_or(&0),
            Unit::Defense(defense) => *self.battery.get(defense).unwrap_or(&0),
            Unit::Ship(ship) => *self.fleet.get(ship).unwrap_or(&0),
        }
    }

    /// Produce the units bought during the turn
    pub fn produce(&mut self) {
        for unit in self.buy.drain(..) {
            match unit {
                Unit::Building(b) => {
                    *self.complex.entry(b).or_default() += 1;
                },
                Unit::Ship(s) => {
                    *self.fleet.entry(s).or_default() += 1;
                },
                Unit::Defense(d) => {
                    *self.battery.entry(d).or_default() += 1;
                },
            }
        }
    }

    pub fn resource_production(&self) -> Resources {
        self.resources * self.get(&Unit::Building(Building::Mine))
    }

    pub fn fleet_production(&self) -> usize {
        self.buy.iter().filter_map(|u| u.is_ship().then_some(u.production())).sum()
    }

    pub fn max_fleet_production(&self) -> usize {
        SHIPYARD_PRODUCTION_FACTOR * self.get(&Unit::Building(Building::Shipyard))
    }

    pub fn battery_production(&self) -> usize {
        self.buy.iter().filter_map(|u| u.is_defense().then_some(u.production())).sum()
    }

    pub fn max_battery_production(&self) -> usize {
        FACTORY_PRODUCTION_FACTOR * self.get(&Unit::Building(Building::Factory))
    }

    pub fn missile_capacity(&self) -> usize {
        self.battery.iter().filter_map(|(d, c)| d.is_missile().then_some(c)).sum()
    }

    pub fn max_missile_capacity(&self) -> usize {
        SILO_CAPACITY_FACTOR * self.get(&Unit::Building(Building::MissileSilo))
    }
}
