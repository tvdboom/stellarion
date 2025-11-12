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
    Dry,
    Gas,
    Ice,
    Water,
}

impl PlanetKind {
    pub fn indices(self) -> Vec<usize> {
        match self {
            PlanetKind::Dry => vec![2, 3, 5, 8, 9, 12, 13, 15, 16, 19, 20, 21],
            PlanetKind::Gas => vec![7, 10, 11, 14, 18, 37, 43, 45],
            PlanetKind::Ice => vec![1, 4, 6, 17, 22, 23, 26, 27, 28, 35, 36, 38, 40],
            PlanetKind::Water => vec![25, 32, 34, 52, 62],
        }
    }

    pub fn diameter(&self) -> usize {
        let mut rng = rng();
        let value = match self {
            PlanetKind::Dry | PlanetKind::Water => rng.random_range(6000..17000),
            PlanetKind::Gas => rng.random_range(17000..140000),
            PlanetKind::Ice => rng.random_range(4000..10000),
        };

        (value / 100) * 100
    }

    pub fn temperature(&self) -> (i16, i16) {
        let mut rng = rng();
        match self {
            PlanetKind::Dry => {
                let low = rng.random_range(80..240);
                let high = rng.random_range(low..=240);
                (low, high)
            },
            PlanetKind::Gas => {
                let low = rng.random_range(-110..-60);
                let high = rng.random_range(low..=-60);
                (low, high)
            },
            PlanetKind::Ice => {
                let low = rng.random_range(-260..-130);
                let high = rng.random_range(low..=-130);
                (low, high)
            },
            PlanetKind::Water => {
                let low = rng.random_range(-10..40);
                let high = rng.random_range(low..=40);
                (low, high)
            },
        }
    }

    pub fn description(&self) -> &str {
        match self {
            PlanetKind::Dry => {
                "Arid desert world with scorching days and cold nights. Dry planets often \
                produce high quantities of metal, but have scarcity of other resources."
            },
            PlanetKind::Water => {
                "Habitable planet covered by oceans and continents. Water worlds have \
                balanced resource reserves."
            },
            PlanetKind::Gas => {
                "Massive gas giant with thick clouds and strong storms. Produce few metal \
                and crystal but have often large reservers of deuterium."
            },
            PlanetKind::Ice => {
                "Frozen world with glaciers, snowfields, and icy terrain. Tend to contain \
                high quantities of crystal, but have scarcity of other resources."
            },
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
    pub diameter: usize,
    pub temperature: (i16, i16),
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
            (PlanetKind::Dry, [&high, &low, &low]),
            (PlanetKind::Gas, [&low, &low, &high]),
            (PlanetKind::Ice, [&low, &high, &low]),
            (PlanetKind::Water, [&medium, &medium, &low]),
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
            diameter: kind.diameter(),
            temperature: kind.temperature(),
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
            (Unit::Building(Building::MetalMine), 1),
            (Unit::Building(Building::CrystalMine), 1),
            (Unit::Building(Building::DeuteriumSynthesizer), 1),
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

    pub fn abandon(&mut self) {
        self.owned = None;
        if !self.has_fleet() {
            self.controlled = None;
        }
    }

    pub fn destroy_probability(&self) -> f32 {
        match self.diameter {
            4000..6000 => 0.15,
            6000..9000 => 0.14,
            9000..13000 => 0.13,
            13000..20000 => 0.12,
            20000..100000 => 0.11,
            _ => 0.10,
        }
    }

    /// Resources and production ===================================== >>

    pub fn produce(&mut self) {
        for unit in self.buy.drain(..) {
            *self.army.entry(unit).or_default() += 1;
        }
    }

    pub fn resource_production(&self) -> Resources {
        Resources::new(
            self.resources.metal * self.army.amount(&Unit::Building(Building::MetalMine)),
            self.resources.crystal * self.army.amount(&Unit::Building(Building::CrystalMine)),
            self.resources.deuterium
                * self.army.amount(&Unit::Building(Building::DeuteriumSynthesizer)),
        )
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
