//! Persisted planet/moon state, production, ownership, and capacity rules.

use std::ops::Range;

use bevy::math::Vec2;
use rand::prelude::IteratorRandom;
use rand::{rng, Rng, RngExt};
use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;

use crate::core::constants::{
    FACTORY_PRODUCTION_FACTOR, SHIPYARD_PRODUCTION_FACTOR, SILO_CAPACITY_FACTOR,
};
use crate::core::identity::PlayerId;
use crate::core::resources::Resources;
use crate::core::units::buildings::Building;
use crate::core::units::{Amount, Army, Unit};

/// Stable index identifying a planet inside one persisted map.
pub type PlanetId = usize;

#[derive(EnumIter, Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
/// Generation and visual profile for a planet or moon.
pub enum PlanetKind {
    // Planets
    /// The dry value.
    Dry,
    /// The gas value.
    Gas,
    /// The ice value.
    Ice,
    /// The metallic value.
    Metallic,
    /// The water value.
    Water,

    // Moons
    /// The blue value.
    Blue,
    /// The brown value.
    Brown,
    /// The gray value.
    Gray,
    /// The red value.
    Red,
    /// The yellow value.
    Yellow,
}

impl PlanetKind {
    /// Returns every lunar visual kind.
    pub fn moons() -> &'static [Self] {
        &[
            PlanetKind::Blue,
            PlanetKind::Brown,
            PlanetKind::Gray,
            PlanetKind::Red,
            PlanetKind::Yellow,
        ]
    }

    /// Returns source-art indices available for this planet visual kind.
    pub fn indices(self) -> &'static [usize] {
        match self {
            PlanetKind::Dry => &[2, 3, 5, 8, 9, 12, 13, 15, 16, 19, 20, 21],
            PlanetKind::Gas => &[7, 10, 11, 14, 18, 37, 43, 45],
            PlanetKind::Ice => &[1, 4, 6, 17, 22, 23, 26, 27, 28, 35, 36, 38, 40, 50],
            PlanetKind::Metallic => &[53, 54, 55, 56, 57, 58, 60, 61, 63],
            PlanetKind::Water => &[25, 32, 34, 52, 62],
            PlanetKind::Blue => &[1],
            PlanetKind::Brown => &[2],
            PlanetKind::Gray => &[3],
            PlanetKind::Red => &[4],
            PlanetKind::Yellow => &[5],
        }
    }

    /// Generates a plausible rounded diameter for this planet kind.
    pub fn diameter(&self) -> usize {
        self.diameter_with_rng(&mut rng())
    }

    /// Generates a diameter from the supplied deterministic stream.
    pub fn diameter_with_rng<R: Rng + ?Sized>(&self, rng: &mut R) -> usize {
        let value = match self {
            PlanetKind::Dry | PlanetKind::Water => rng.random_range(6000..17000),
            PlanetKind::Gas => rng.random_range(17000..140000),
            PlanetKind::Ice | PlanetKind::Metallic => rng.random_range(4000..10000),
            _ => rng.random_range(1000..5000),
        };

        (value / 100) * 100
    }

    /// Generates the surface-temperature range for this planet kind.
    pub fn temperature(&self) -> (i16, i16) {
        self.temperature_with_rng(&mut rng())
    }

    /// Generates a temperature range from the supplied deterministic stream.
    pub fn temperature_with_rng<R: Rng + ?Sized>(&self, rng: &mut R) -> (i16, i16) {
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
            PlanetKind::Metallic => {
                let low = rng.random_range(-70..10);
                let high = rng.random_range(low..=10);
                (low, high)
            },
            PlanetKind::Water => {
                let low = rng.random_range(-10..40);
                let high = rng.random_range(low..=40);
                (low, high)
            },
            _ => {
                let low = rng.random_range(-170..-30);
                let high = rng.random_range(low..=-30);
                (low, high)
            },
        }
    }

    /// Returns an icon summarizing the generated surface temperature.
    pub fn temperature_emoji(&self) -> &str {
        match self {
            PlanetKind::Dry => "🔥",
            PlanetKind::Water => "☀",
            _ => "❄",
        }
    }

    /// Returns the user-facing description of this gameplay value.
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
            PlanetKind::Metallic => {
                "Dense, metal-rich world with exposed ore veins and reflective plains. \
                Metallic planets yield large amounts of refined metals but offer very \
                little other resources."
            },
            PlanetKind::Ice => {
                "Frozen world with glaciers, snowfields, and icy terrain. Tend to contain \
                high quantities of crystal, but have scarcity of other resources."
            },
            _ => {
                "Moons are small natural satellites. Their low gravity and limited atmospheres \
                make them unfit for colonization. Moons produce no resources, can only build a \
                limited number of buildings, and cannot be bombed."
            },
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Complete persisted world state including ownership, economy, queues, and stationed units.
pub struct Planet {
    // Planet characteristics
    /// Stable identifier used to cross-reference this value.
    pub id: PlanetId,
    /// User-facing generated name of this world.
    pub name: String,
    /// Planet/moon generation and visual profile.
    pub kind: PlanetKind,
    /// Index of the source visual selected for this world.
    pub image: usize,
    /// Generated physical diameter used for flavor and rendering scale.
    pub diameter: usize,
    /// Generated minimum and maximum surface temperature.
    pub temperature: (i16, i16),
    /// Current world-space position.
    pub position: Vec2,
    /// Resource production for a world or stockpile for a player.
    pub resources: Resources,
    /// Jump-gate capacity consumed during the current turn.
    pub jump_gate: usize,
    /// Whether this planet has been permanently destroyed.
    pub is_destroyed: bool,

    // Ownership and units
    /// Owning player when colonized, or an ownership flag in presentation state.
    pub owned: Option<PlayerId>,
    /// Player with current military control, if any.
    pub controlled: Option<PlayerId>,
    /// Units stationed on this world or travelling with this mission.
    pub army: Army,
    /// Units queued for production at the next turn transition.
    pub buy: Vec<Unit>,
}

impl Planet {
    // Pixel size of a planet on the screen
    /// Rendered size associated with this map object.
    pub const SIZE: f32 = 100.;

    /// Creates a new value from the supplied state.
    pub fn new(
        id: PlanetId,
        name: String,
        position: Vec2,
        is_moon: bool,
        resource_factor: f32,
    ) -> Self {
        Self::new_with_rng(id, name, position, is_moon, resource_factor, &mut rng())
    }

    /// Creates a planet using the supplied deterministic random stream.
    pub fn new_with_rng<R: Rng + ?Sized>(
        id: PlanetId,
        name: String,
        position: Vec2,
        is_moon: bool,
        resource_factor: f32,
        rng: &mut R,
    ) -> Self {
        let (kind, resources) = if !is_moon {
            let low = 10.0..20.0;
            let medium = 20.0..30.0;
            let high = 30.0..40.0;

            let configs: &[(PlanetKind, [&Range<f32>; 3])] = &[
                (PlanetKind::Dry, [&high, &low, &low]),
                (PlanetKind::Gas, [&low, &low, &high]),
                (PlanetKind::Ice, [&low, &high, &low]),
                (PlanetKind::Metallic, [&high, &low, &low]),
                (PlanetKind::Water, [&medium, &medium, &low]),
            ];

            let (kind, ranges) = &configs[rng.random_range(0..configs.len())];

            let resources = Resources::new(
                (rng.random_range(ranges[0].clone()) * resource_factor).round() as usize * 10,
                (rng.random_range(ranges[1].clone()) * resource_factor).round() as usize * 10,
                (rng.random_range(ranges[2].clone()) * resource_factor).round() as usize * 10,
            );

            (*kind, resources)
        } else {
            let moons = PlanetKind::moons();
            (moons[rng.random_range(0..moons.len())], Resources::default())
        };

        let images = kind.indices();

        Self {
            id,
            name,
            kind,
            image: images[rng.random_range(0..images.len())],
            diameter: kind.diameter_with_rng(rng),
            temperature: kind.temperature_with_rng(rng),
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

    /// Returns whether this value moon.
    pub fn is_moon(&self) -> bool {
        PlanetKind::moons().contains(&self.kind)
    }

    /// Returns the runtime image key for this value.
    pub fn image(&self) -> String {
        if self.is_destroyed {
            "destroy".to_string()
        } else if self.is_moon() {
            format!("moon{}", self.image)
        } else {
            format!("planet{}", self.image)
        }
    }

    /// Returns the rendered diameter derived from planet kind and physical diameter.
    pub fn size(&self) -> f32 {
        if self.is_moon() {
            Self::SIZE * 0.7
        } else {
            Self::SIZE
        }
    }

    /// Initializes ownership, infrastructure, and forces for a player's home world.
    pub fn make_home_planet(&mut self, player_id: PlayerId) {
        self.colonize(player_id);
        self.army = Army::from([
            (Unit::Building(Building::MetalMine), 1),
            (Unit::Building(Building::CrystalMine), 1),
            (Unit::Building(Building::DeuteriumSynthesizer), 1),
            (Unit::Building(Building::Shipyard), 1),
            (Unit::Building(Building::Factory), 1),
        ]);
    }

    /// Removes invalid or zero-count unit entries from this planet.
    pub fn clean(&mut self) {
        self.owned = None;
        self.controlled = None;
        self.army.retain(|u, _| u.is_building());
        self.buy = Vec::new();
    }

    /// Claims this planet for a player and initializes owned-world state.
    pub fn colonize(&mut self, player_id: PlayerId) {
        self.owned = Some(player_id);
        self.controlled = Some(player_id);
    }

    /// Transfers control while applying conquest and demolition rules.
    pub fn control(&mut self, player_id: PlayerId) {
        self.control_with_rng(player_id, &mut rng());
    }

    /// Transfers control while selecting demolition targets from a supplied stream.
    pub fn control_with_rng<R: Rng + ?Sized>(&mut self, player_id: PlayerId, rng: &mut R) {
        // Destroy buildings if Nexus built and new controller
        if self.controlled != Some(player_id) {
            for _ in 0..self.army.amount(&Unit::Building(Building::DemolitionNexus)) {
                let pool = self.army.iter_mut().filter(|(u, c)| u.consumes_field() && **c > 0);
                if let Some((_, c)) = pool.choose(rng) {
                    *c -= 1;
                }
            }
        }

        self.controlled = Some(player_id);
        if self.owned != Some(player_id) {
            self.owned = None;
        }
    }

    /// Removes ownership and owner-only infrastructure from this planet.
    pub fn abandon(&mut self) {
        let former_owner = self.owned.take();
        self.army.retain(|u, _| !u.is_defense());
        self.controlled = if self.has_fleet() {
            // Ownership implies control, so use it as the authority when converting an owned
            // planet into a merely controlled one. Presentation projections may temporarily
            // omit the redundant `controlled` value for an owned planet.
            former_owner.or(self.controlled)
        } else {
            None
        };
    }

    /// Returns the bounded War Sun destruction chance for the current turn.
    pub fn destroy_probability(&self) -> f32 {
        match self.diameter {
            1000..2000 => 0.18,
            2000..3000 => 0.17,
            3000..4000 => 0.16,
            4000..6000 => 0.15,
            6000..9000 => 0.14,
            9000..13000 => 0.13,
            13000..20000 => 0.12,
            20000..100000 => 0.11,
            _ => 0.10,
        }
    }

    /// Moves all queued units into the stationed army with saturating counts.
    pub fn produce(&mut self) {
        for unit in self.buy.drain(..) {
            let count = self.army.entry(unit).or_default();
            *count = count.saturating_add(1);
        }
    }

    /// Computes resource production for the current owned worlds.
    pub fn resource_production(&self) -> Resources {
        Resources::new(
            self.resources
                .metal
                .saturating_mul(self.army.amount(&Unit::Building(Building::MetalMine))),
            self.resources
                .crystal
                .saturating_mul(self.army.amount(&Unit::Building(Building::CrystalMine))),
            self.resources
                .deuterium
                .saturating_mul(self.army.amount(&Unit::Building(Building::DeuteriumSynthesizer))),
        )
    }

    /// Counts lunar/building field slots consumed by constructed units.
    pub fn fields_consumed(&self) -> usize {
        self.army
            .iter()
            .filter_map(|(unit, count)| unit.consumes_field().then_some(*count))
            .fold(0, usize::saturating_add)
            .saturating_add(self.buy.iter().filter(|unit| unit.consumes_field()).count())
    }

    /// Returns the maximum fields allowed by current upgrades.
    pub fn max_fields(&self) -> usize {
        self.army.amount(&Unit::Building(Building::LunarBase))
    }

    /// Returns current shipyard production available on this world.
    pub fn fleet_production(&self) -> usize {
        self.buy
            .iter()
            .filter_map(|unit| unit.is_ship().then_some(unit.production()))
            .fold(0, usize::saturating_add)
    }

    /// Returns the maximum fleet production allowed by current upgrades.
    pub fn max_fleet_production(&self) -> usize {
        SHIPYARD_PRODUCTION_FACTOR
            .saturating_mul(self.army.amount(&Unit::Building(Building::Shipyard)))
    }

    /// Returns current factory production available for defenses.
    pub fn battery_production(&self) -> usize {
        self.buy
            .iter()
            .filter_map(|unit| unit.is_defense().then_some(unit.production()))
            .fold(0, usize::saturating_add)
    }

    /// Returns the maximum battery production allowed by current upgrades.
    pub fn max_battery_production(&self) -> usize {
        FACTORY_PRODUCTION_FACTOR
            .saturating_mul(self.army.amount(&Unit::Building(Building::Factory)))
    }

    /// Returns current silo capacity for offensive and defensive missiles.
    pub fn missile_capacity(&self) -> usize {
        self.army
            .iter()
            .filter_map(|(unit, count)| unit.is_missile().then_some(*count))
            .fold(0, usize::saturating_add)
    }

    /// Returns the maximum missile capacity allowed by current upgrades.
    pub fn max_missile_capacity(&self) -> usize {
        SILO_CAPACITY_FACTOR
            .saturating_mul(self.army.amount(&Unit::Building(Building::MissileSilo)))
    }

    /// Returns silo space after reserving slots for both missile types queued this turn.
    pub fn remaining_missile_capacity(&self) -> usize {
        self.max_missile_capacity().saturating_sub(
            self.missile_capacity()
                .saturating_add(self.buy.iter().filter(|unit| unit.is_missile()).count()),
        )
    }

    /// Returns the maximum jump capacity allowed by current upgrades.
    pub fn max_jump_capacity(&self) -> usize {
        FACTORY_PRODUCTION_FACTOR
            .saturating_mul(self.army.amount(&Unit::Building(Building::JumpGate)))
    }

    /// Returns whether at least one copy of the requested unit is stationed here.
    pub fn has(&self, unit: &Unit) -> bool {
        self.army.amount(unit) > 0
    }

    /// Returns whether this value has buildings.
    pub fn has_buildings(&self) -> bool {
        self.army.iter().any(|(u, c)| u.is_building() && *c > 0)
    }

    /// Returns whether this value has fleet.
    pub fn has_fleet(&self) -> bool {
        self.army.iter().any(|(u, c)| u.is_ship() && *c > 0)
    }

    /// Returns whether this value has defense.
    pub fn has_defense(&self) -> bool {
        self.army.iter().any(|(u, c)| u.is_defense() && *c > 0)
    }

    /// Releases control after a fleet departs when no infrastructure or ships remain.
    pub fn release_control_if_vacant(&mut self) {
        if !self.has_buildings() && !self.has_fleet() {
            self.controlled = None;
        }
    }

    /// Merge a fleet into the planet's fleet
    pub fn dock(&mut self, army: Army) {
        for (unit, count) in army {
            let stationed = self.army.entry(unit).or_default();
            *stationed = stationed.saturating_add(count);
        }
    }

    /// Permanently destroys this planet and clears all ownership and stationed units.
    pub fn destroy(&mut self) {
        self.owned = None;
        self.controlled = None;
        self.army = Army::new();
        self.buy = Vec::new();
        self.is_destroyed = true;
    }
}
