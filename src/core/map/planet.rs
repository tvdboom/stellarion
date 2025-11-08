use std::collections::HashMap;
use std::ops::Range;

use bevy::math::Vec2;
use bevy_renet::renet::ClientId;
use rand::prelude::IteratorRandom;
use rand::{rng, Rng};
use serde::{Deserialize, Serialize};

use crate::core::constants::{
    FACTORY_PRODUCTION_FACTOR, SHIPYARD_PRODUCTION_FACTOR, SILO_CAPACITY_FACTOR,
};
use crate::core::resources::Resources;
use crate::core::units::buildings::Building;
use crate::core::units::{Amount, Army, Unit};

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
    pub jump_gate: usize,
    pub is_destroyed: bool,

    // Ownership and units
    pub owned: Option<ClientId>,
    pub controlled: Option<ClientId>,
    pub army: Army,
    pub buy: Vec<Unit>,
}

impl Planet {
    // Pixel size of a planet on the screen
    pub const SIZE: f32 = 100.;

    pub fn new(id: PlanetId, name: String, position: Vec2) -> Self {
        let low = 1..3;
        let medium = 2..4;
        let high = 3..5;

        let configs: &[(PlanetKind, [&Range<usize>; 3])] = &[
            (PlanetKind::Desert, [&high, &low, &low]),
            (PlanetKind::Gas, [&low, &low, &high]),
            (PlanetKind::Ice, [&low, &high, &low]),
            (PlanetKind::Normal, [&medium, &medium, &low]),
        ];

        let (kind, ranges) = configs.iter().choose(&mut rng()).unwrap();

        let resources = Resources::new(
            rng().random_range(ranges[0].clone()) * 100,
            rng().random_range(ranges[1].clone()) * 70,
            rng().random_range(ranges[2].clone()) * 50,
        );

        Self {
            id,
            name,
            kind: *kind,
            image: *kind.indices().iter().choose(&mut rng()).unwrap(),
            position,
            resources,
            jump_gate: 0,
            is_destroyed: false,
            owned: None,
            controlled: None,
            army: Army::new(),
            buy: vec![],
        }
    }

    pub fn make_home_planet(&mut self, client_id: ClientId) {
        self.colonize(client_id);
        self.army = Army::from([
            (Unit::Building(Building::Mine), 1),
            (Unit::Building(Building::Shipyard), 1),
            (Unit::Building(Building::Factory), 1),
        ]);
    }

    pub fn clean(&mut self) {
        self.owned = None;
        self.controlled = None;
        self.army = Army::new();
        self.buy = Vec::new();
    }

    pub fn colonize(&mut self, client_id: ClientId) {
        self.owned = Some(client_id);
        self.controlled = Some(client_id);
    }

    pub fn control(&mut self, client_id: ClientId) {
        self.controlled = Some(client_id);
        if self.owned != Some(client_id) {
            self.owned = None;
        }
    }

    /// Resources and production ===================================== >>

    pub fn produce(&mut self) {
        for unit in self.buy.drain(..) {
            *self.army.entry(unit).or_default() += 1;
        }
    }

    pub fn resource_production(&self) -> Resources {
        self.resources * self.army.amount(&Unit::Building(Building::Mine))
    }

    pub fn fleet_production(&self) -> usize {
        self.buy.iter().filter_map(|u| u.is_ship().then_some(u.production())).sum()
    }

    pub fn max_fleet_production(&self) -> usize {
        SHIPYARD_PRODUCTION_FACTOR * self.army.amount(&Unit::Building(Building::Shipyard))
    }

    pub fn battery_production(&self) -> usize {
        self.buy.iter().filter_map(|u| u.is_defense().then_some(u.production())).sum()
    }

    pub fn max_battery_production(&self) -> usize {
        FACTORY_PRODUCTION_FACTOR * self.army.amount(&Unit::Building(Building::Factory))
    }

    pub fn missile_capacity(&self) -> usize {
        self.army
            .iter()
            .filter_map(|(u, c)| matches!(u, Unit::Defense(d) if d.is_missile()).then_some(c))
            .sum()
    }

    pub fn max_missile_capacity(&self) -> usize {
        SILO_CAPACITY_FACTOR * self.army.amount(&Unit::Building(Building::MissileSilo))
    }

    pub fn max_jump_capacity(&self) -> usize {
        FACTORY_PRODUCTION_FACTOR * self.army.amount(&Unit::Building(Building::JumpGate))
    }

    /// Units and combat ============================================= >>

    pub fn has(&self, unit: &Unit) -> bool {
        self.army.amount(unit) > 0
    }

    pub fn has_buildings(&self) -> bool {
        self.army.iter().any(|(u, c)| u.is_building() && *c > 0)
    }

    pub fn has_fleet(&self) -> bool {
        self.army.iter().any(|(u, c)| u.is_ship() && *c > 0)
    }

    pub fn has_defense(&self) -> bool {
        self.army.iter().any(|(u, c)| u.is_defense() && *c > 0)
    }

    /// Merge a fleet into the planet's fleet
    pub fn dock(&mut self, army: Army) {
        for (unit, count) in army {
            *self.army.entry(unit).or_default() += count;
        }
    }

    /// Destroy this planet
    pub fn destroy(&mut self) {
        self.image = 0;
        self.owned = None;
        self.controlled = None;
        self.army = HashMap::new();
        self.buy = Vec::new();
        self.is_destroyed = true;
    }
}
