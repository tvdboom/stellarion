//! Persisted planet/moon state, production, ownership, and capacity rules.

use std::collections::{btree_map, BTreeMap, BTreeSet};
use std::ops::{Deref, DerefMut};

use bevy::math::Vec2;
use rand::{rng, Rng, RngExt};
use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;

use crate::core::constants::{
    FACTORY_PRODUCTION_FACTOR, JUMP_GATE_CAPACITY_PER_LEVEL, PROBES_PER_PRODUCTION_LEVEL,
    SHIPYARD_PRODUCTION_FACTOR, SILO_CAPACITY_FACTOR, SPACE_DOCK_FLEET_PRODUCTION,
    TERRAFORMER_FOCUS_BONUS_PERCENT_PER_LEVEL, TERRAFORMER_OTHER_PENALTY_PERCENT_PER_LEVEL,
};
use crate::core::identity::PlayerId;
use crate::core::resources::{ResourceName, Resources};
use crate::core::units::buildings::{Building, FleetWithdrawal};
use crate::core::units::defense::Defense;
use crate::core::units::{Amount, Army, Unit};

/// Stable index identifying a planet inside one persisted map.
pub type PlanetId = usize;

/// Every force stationed on one world, partitioned by the player who may command it.
///
/// Keeping this ownership detail behind one type prevents callers from maintaining a flat army
/// and an ownership side channel that can drift out of sync. Ordinary army operations address
/// the controller's units; [`Self::combined`] and [`Self::combined_amount`] explicitly aggregate
/// all commanders.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Garrison {
    controller: Army,
    protectors: BTreeMap<PlayerId, Army>,
}

impl Garrison {
    /// Creates a garrison from a controller force and zero or more foreign protection fleets.
    pub fn from_parts(controller: Army, protectors: BTreeMap<PlayerId, Army>) -> Self {
        Self {
            controller,
            protectors: protectors.into_iter().filter(|(_, army)| army.has_army()).collect(),
        }
    }

    /// Returns only the units commanded by the planet controller.
    pub fn controller(&self) -> &Army {
        &self.controller
    }

    /// Returns mutable access to the units commanded by the planet controller.
    pub fn controller_mut(&mut self) -> &mut Army {
        &mut self.controller
    }

    /// Returns the protection fleet commanded by this player, if stationed here.
    pub fn protector(&self, player_id: PlayerId) -> Option<&Army> {
        self.protectors.get(&player_id)
    }

    /// Returns mutable access to an existing protection fleet.
    pub fn protector_mut(&mut self, player_id: PlayerId) -> Option<&mut Army> {
        self.protectors.get_mut(&player_id)
    }

    /// Iterates over every foreign protection fleet in player-id order.
    pub fn protectors(&self) -> impl Iterator<Item = (PlayerId, &Army)> {
        self.protectors.iter().map(|(player_id, army)| (*player_id, army))
    }

    /// Iterates over the IDs of all players with a protection fleet stationed here.
    pub fn protector_ids(&self) -> impl Iterator<Item = PlayerId> + '_ {
        self.protectors().map(|(player_id, _)| player_id)
    }

    /// Adds units to one player's protection fleet.
    pub fn dock_protector(&mut self, player_id: PlayerId, army: Army) {
        let fleet = self.protectors.entry(player_id).or_default();
        for (unit, count) in army {
            let stationed = fleet.entry(unit).or_default();
            *stationed = stationed.saturating_add(count);
        }
        fleet.retain(|_, count| *count > 0);
        if !fleet.has_army() {
            self.protectors.remove(&player_id);
        }
    }

    /// Removes and returns one player's protection fleet.
    pub fn remove_protector(&mut self, player_id: PlayerId) -> Option<Army> {
        self.protectors.remove(&player_id)
    }

    /// Removes every protection fleet while retaining the controller's units.
    pub fn clear_protectors(&mut self) {
        self.protectors.clear();
    }

    /// Retains only protection fleets accepted by the predicate.
    pub fn retain_protectors(&mut self, mut retain: impl FnMut(PlayerId, &mut Army) -> bool) {
        self.protectors.retain(|player_id, army| retain(*player_id, army));
    }

    /// Returns the complete stationed force without discarding ownership in this garrison.
    pub fn combined(&self) -> Army {
        let mut combined = self.controller.clone();
        for army in self.protectors.values() {
            for (unit, count) in army {
                let total = combined.entry(*unit).or_default();
                *total = total.saturating_add(*count);
            }
        }
        combined
    }

    /// Counts this unit across the controller and every protection fleet.
    pub fn combined_amount(&self, unit: &Unit) -> usize {
        self.protectors.values().fold(self.controller.amount(unit), |total, army| {
            total.saturating_add(army.amount(unit))
        })
    }

    /// Returns whether any commander has a ship stationed here.
    pub fn has_fleet(&self) -> bool {
        std::iter::once(&self.controller)
            .chain(self.protectors.values())
            .any(|army| army.iter().any(|(unit, count)| unit.is_ship() && *count > 0))
    }

    /// Removes every controller and protector unit.
    pub fn clear(&mut self) {
        self.controller.clear();
        self.protectors.clear();
    }

    /// Returns whether no owner has a nonzero unit count.
    pub fn is_empty(&self) -> bool {
        !self.controller.has_army() && !self.protectors.values().any(Army::has_army)
    }
}

impl From<Army> for Garrison {
    fn from(army: Army) -> Self {
        Self {
            controller: army,
            protectors: BTreeMap::new(),
        }
    }
}

impl FromIterator<(Unit, usize)> for Garrison {
    fn from_iter<T: IntoIterator<Item = (Unit, usize)>>(iter: T) -> Self {
        Self::from(iter.into_iter().collect::<Army>())
    }
}

impl Deref for Garrison {
    type Target = Army;

    fn deref(&self) -> &Self::Target {
        self.controller()
    }
}

impl DerefMut for Garrison {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.controller_mut()
    }
}

impl<'a> IntoIterator for &'a Garrison {
    type Item = (&'a Unit, &'a usize);
    type IntoIter = btree_map::Iter<'a, Unit, usize>;

    fn into_iter(self) -> Self::IntoIter {
        self.controller().iter()
    }
}

impl Amount for Garrison {
    fn amount(&self, unit: &Unit) -> usize {
        self.controller().amount(unit)
    }

    fn has_army(&self) -> bool {
        self.controller().has_army()
    }

    fn total_production(&self) -> usize {
        self.controller().total_production()
    }
}

/// One-turn operating state of a Planetary Shield overload.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShieldOverloadState {
    /// The shield can be overloaded for the next turn.
    #[default]
    Ready,
    /// The shield is drawing extra Energy and receives its strength bonus this turn.
    Overloaded,
    /// The shield cannot be overloaded during this turn after the previous turn's use.
    Cooldown,
}

impl ShieldOverloadState {
    /// Returns whether the overload is currently affecting Energy demand and combat strength.
    pub fn is_overloaded(self) -> bool {
        self == Self::Overloaded
    }

    /// Advances the one-use, one-cooldown-turn lifecycle after a turn resolves.
    pub fn finish_turn(self) -> Self {
        match self {
            Self::Ready => Self::Ready,
            Self::Overloaded => Self::Cooldown,
            Self::Cooldown => Self::Ready,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
/// A world's relative irradiation zone around the map's primary star.
pub enum SolarBand {
    /// Closest quarter of planets: hot rocky and desert worlds.
    Inner,
    /// Middle half of planets: the broadest mix, favoring water worlds.
    Temperate,
    /// Farthest quarter of planets: cold gas and ice worlds.
    Outer,
}

impl SolarBand {
    /// Returns energy generated by one Solar Satellite level in this band.
    pub fn satellite_energy(self) -> usize {
        match self {
            Self::Inner => 3,
            Self::Temperate => 2,
            Self::Outer => 1,
        }
    }

    /// Returns the user-facing band name.
    pub fn name(self) -> &'static str {
        match self {
            Self::Inner => "Inner",
            Self::Temperate => "Temperate",
            Self::Outer => "Outer",
        }
    }

    /// Generates the wide day/night temperature range of an airless moon in this band.
    pub fn lunar_temperature_with_rng<R: Rng + ?Sized>(self, rng: &mut R) -> (i16, i16) {
        match self {
            Self::Inner => (rng.random_range(-100..0), rng.random_range(100..=240)),
            Self::Temperate => (rng.random_range(-180..-100), rng.random_range(40..=130)),
            Self::Outer => (rng.random_range(-260..-180), rng.random_range(-120..=-20)),
        }
    }
}

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
#[serde(deny_unknown_fields)]
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
    /// Resource currently favored by this planet's Terraformer, or `None` while switched off.
    pub terraformer_focus: Option<ResourceName>,
    /// Whether the stationed Command Relay is broadcasting deceptive telemetry.
    pub command_relay_active: bool,
    /// Current overload or cooldown state of this world's Planetary Shield.
    pub shield_overload: ShieldOverloadState,
    /// Standing fleet withdrawal order for this colony; disabled until explicitly selected.
    pub fleet_withdrawal: FleetWithdrawal,
    /// Whether this planet has been permanently destroyed.
    pub is_destroyed: bool,

    // Ownership and units
    /// Owning player when colonized, or an ownership flag in presentation state.
    #[serde(deserialize_with = "crate::serialization::required_option")]
    pub owned: Option<PlayerId>,
    /// Player with current military control, if any.
    #[serde(deserialize_with = "crate::serialization::required_option")]
    pub controlled: Option<PlayerId>,
    /// All controller and protection forces stationed here, indexed internally by commander.
    pub army: Garrison,
    /// Players currently allowed by this world's controller to send a Protect mission here.
    pub protection_permissions: BTreeSet<PlayerId>,
    /// Units queued for production at the next turn transition.
    pub buy: Vec<Unit>,
    /// Permanent first-completion slots for surface artwork, retained across saves and control changes.
    #[serde(alias = "lunar_build_order")]
    pub surface_build_order: [Option<Building>; 4],
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
        Self::new_in_solar_band_with_rng(id, name, position, is_moon, resource_factor, None, rng)
    }

    /// Creates a world whose planet kind or lunar temperature follows the supplied solar band.
    pub fn new_in_solar_band_with_rng<R: Rng + ?Sized>(
        id: PlanetId,
        name: String,
        position: Vec2,
        is_moon: bool,
        resource_factor: f32,
        solar_band: Option<SolarBand>,
        rng: &mut R,
    ) -> Self {
        let (kind, resources) = if !is_moon {
            let low = 10.0..20.0;
            let medium = 20.0..30.0;
            let high = 30.0..40.0;

            let roll = rng.random_range(0..100);
            let kind = match solar_band {
                Some(SolarBand::Inner) if roll < 55 => PlanetKind::Dry,
                Some(SolarBand::Inner) => PlanetKind::Metallic,
                Some(SolarBand::Temperate) if roll < 40 => PlanetKind::Water,
                Some(SolarBand::Temperate) if roll < 60 => PlanetKind::Metallic,
                Some(SolarBand::Temperate) if roll < 75 => PlanetKind::Dry,
                Some(SolarBand::Temperate) if roll < 90 => PlanetKind::Gas,
                Some(SolarBand::Temperate) => PlanetKind::Ice,
                Some(SolarBand::Outer) if roll < 55 => PlanetKind::Gas,
                Some(SolarBand::Outer) if roll < 90 => PlanetKind::Ice,
                Some(SolarBand::Outer) => PlanetKind::Water,
                None => [
                    PlanetKind::Dry,
                    PlanetKind::Gas,
                    PlanetKind::Ice,
                    PlanetKind::Metallic,
                    PlanetKind::Water,
                ][rng.random_range(0..5)],
            };
            let ranges = match kind {
                PlanetKind::Dry | PlanetKind::Metallic => [&high, &low, &low],
                PlanetKind::Gas => [&low, &low, &high],
                PlanetKind::Ice => [&low, &high, &low],
                PlanetKind::Water => [&medium, &medium, &low],
                _ => unreachable!("lunar kind selected for a planet"),
            };

            let resources = Resources::new(
                (rng.random_range(ranges[0].clone()) * resource_factor).round() as usize * 10,
                (rng.random_range(ranges[1].clone()) * resource_factor).round() as usize * 10,
                (rng.random_range(ranges[2].clone()) * resource_factor).round() as usize * 10,
            );

            (kind, resources)
        } else {
            let moons = PlanetKind::moons();
            (moons[rng.random_range(0..moons.len())], Resources::default())
        };

        let images = kind.indices();
        let image = images[rng.random_range(0..images.len())];
        let diameter = kind.diameter_with_rng(rng);
        let temperature = if is_moon {
            solar_band
                .map(|band| band.lunar_temperature_with_rng(rng))
                .unwrap_or_else(|| kind.temperature_with_rng(rng))
        } else {
            kind.temperature_with_rng(rng)
        };

        Self {
            id,
            name,
            kind,
            image,
            diameter,
            temperature,
            position,
            resources,
            jump_gate: 0,
            terraformer_focus: None,
            command_relay_active: true,
            shield_overload: ShieldOverloadState::Ready,
            fleet_withdrawal: FleetWithdrawal::Off,
            is_destroyed: false,
            owned: None,
            controlled: None,
            army: Garrison::default(),
            protection_permissions: BTreeSet::new(),
            buy: vec![],
            surface_build_order: [None; 4],
        }
    }

    /// Returns whether this value moon.
    pub fn is_moon(&self) -> bool {
        PlanetKind::moons().contains(&self.kind)
    }

    /// Returns an icon summarizing this world's generated surface temperature.
    pub fn temperature_emoji(&self) -> &str {
        if !self.is_moon() {
            return self.kind.temperature_emoji();
        }
        if self.temperature.0 >= -100 && self.temperature.1 >= 100 {
            "🔥"
        } else if self.temperature.1 > 0 {
            "☀"
        } else {
            "❄"
        }
    }

    /// Returns the runtime image key for this value.
    pub fn image(&self) -> String {
        // Index zero is the destroyed artwork; "destroy" is the mission action icon.
        let image = if self.is_destroyed {
            0
        } else {
            self.image
        };
        if self.is_moon() {
            format!("moon{image}")
        } else {
            format!("planet{image}")
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
            (Unit::Building(Building::Reactor), 1),
            (Unit::Defense(Defense::RocketLauncher), 5),
        ])
        .into();
        self.surface_build_order =
            [Some(Building::MetalMine), Some(Building::Shipyard), None, None];
    }

    /// Removes invalid or zero-count unit entries from this planet.
    pub fn clean(&mut self) {
        self.shield_overload = ShieldOverloadState::Ready;
        self.fleet_withdrawal = FleetWithdrawal::Off;
        self.owned = None;
        self.controlled = None;
        self.protection_permissions.clear();
        self.army.retain(|u, _| u.is_building());
        self.buy = Vec::new();
    }

    /// Claims this planet for a player and initializes empty-world infrastructure.
    pub fn colonize(&mut self, player_id: PlayerId) {
        self.owned = Some(player_id);
        self.controlled = Some(player_id);
        if !self.is_moon() && !self.has_buildings() {
            self.record_surface_building(Building::MetalMine);
            for building in [
                Building::MetalMine,
                Building::CrystalMine,
                Building::DeuteriumSynthesizer,
                Building::Reactor,
            ] {
                self.army.insert(Unit::Building(building), 1);
            }
        }
    }

    /// Transfers control to another player.
    pub fn control(&mut self, player_id: PlayerId) {
        if self.controlled != Some(player_id) {
            self.fleet_withdrawal = FleetWithdrawal::Off;
            self.protection_permissions.clear();
        }
        self.controlled = Some(player_id);
        if self.owned != Some(player_id) {
            self.owned = None;
        }
    }

    /// Removes ownership and owner-only infrastructure from this planet.
    pub fn abandon(&mut self) {
        self.shield_overload = ShieldOverloadState::Ready;
        self.fleet_withdrawal = FleetWithdrawal::Off;
        let former_owner = self.owned.take();
        self.army.retain(|u, _| !u.is_defense());
        self.controlled = if self.army.has_fleet() {
            // Ownership implies control, so use it as the authority when converting an owned
            // planet into a merely controlled one. Presentation projections may temporarily
            // omit the redundant `controlled` value for an owned planet.
            former_owner.or(self.controlled)
        } else {
            None
        };
        if self.controlled.is_none() {
            self.protection_permissions.clear();
        }
    }

    /// Returns the diameter-dependent War Sun destruction chance in hundredths of one percent.
    pub fn destroy_probability_basis_points(&self) -> u16 {
        match self.diameter {
            1000..2000 => 1_800,
            2000..3000 => 1_700,
            3000..4000 => 1_600,
            4000..6000 => 1_500,
            6000..9000 => 1_400,
            9000..13000 => 1_300,
            13000..20000 => 1_200,
            20000..100000 => 1_100,
            _ => 1_000,
        }
    }

    /// Returns the bounded War Sun destruction chance for the current turn.
    pub fn destroy_probability(&self) -> f32 {
        f32::from(self.destroy_probability_basis_points()) / 10_000.0
    }

    /// Moves all queued units into the stationed army with saturating counts.
    pub fn produce(&mut self) {
        let is_moon = self.is_moon();
        for unit in self.buy.drain(..) {
            if let Unit::Building(building) = unit {
                Self::record_surface_building_in(&mut self.surface_build_order, building, is_moon);
            }
            let count = self.army.entry(unit).or_default();
            *count = count.saturating_add(1);
        }
    }

    /// Reserves the next permanent artwork slot for a newly completed surface-building category.
    pub(crate) fn record_surface_building(&mut self, building: Building) {
        let is_moon = self.is_moon();
        Self::record_surface_building_in(&mut self.surface_build_order, building, is_moon);
    }

    fn record_surface_building_in(
        order: &mut [Option<Building>; 4],
        building: Building,
        is_moon: bool,
    ) {
        let Some(category) = Self::surface_art_category(building, is_moon) else {
            return;
        };
        if order
            .iter()
            .flatten()
            .any(|building| Self::surface_art_category(*building, is_moon) == Some(category))
        {
            return;
        }
        if let Some(slot) = order.iter_mut().find(|slot| slot.is_none()) {
            *slot = Some(category);
        }
    }

    fn surface_art_category(building: Building, is_moon: bool) -> Option<Building> {
        if is_moon {
            return matches!(
                building,
                Building::LunarBase
                    | Building::TidalGenerator
                    | Building::OrbitalRadar
                    | Building::Laboratory
                    | Building::Shipyard
            )
            .then_some(building);
        }
        match building {
            Building::MetalMine
            | Building::CrystalMine
            | Building::DeuteriumSynthesizer
            | Building::Reactor => Some(Building::MetalMine),
            Building::Shipyard | Building::Factory => Some(Building::Shipyard),
            Building::MissileSilo => Some(Building::MissileSilo),
            Building::Senate => Some(Building::Senate),
            Building::Terraformer => Some(Building::Terraformer),
            Building::ColonialAdministration => Some(Building::ColonialAdministration),
            _ => None,
        }
    }

    /// Computes resource production for the current owned worlds.
    pub fn resource_production(&self) -> Resources {
        let mut production = Resources::new(
            self.resources
                .metal
                .saturating_mul(self.army.amount(&Unit::Building(Building::MetalMine))),
            self.resources
                .crystal
                .saturating_mul(self.army.amount(&Unit::Building(Building::CrystalMine))),
            self.resources
                .deuterium
                .saturating_mul(self.army.amount(&Unit::Building(Building::DeuteriumSynthesizer))),
        );
        let terraformer =
            self.army.amount(&Unit::Building(Building::Terraformer)).min(Building::MAX_LEVEL);
        if terraformer == 0 {
            return production;
        }
        let Some(focus) = self.terraformer_focus else {
            return production;
        };
        for resource in [ResourceName::Metal, ResourceName::Crystal, ResourceName::Deuterium] {
            let percent = if resource == focus {
                100usize.saturating_add(
                    TERRAFORMER_FOCUS_BONUS_PERCENT_PER_LEVEL.saturating_mul(terraformer),
                )
            } else {
                100usize.saturating_sub(
                    TERRAFORMER_OTHER_PENALTY_PERCENT_PER_LEVEL.saturating_mul(terraformer),
                )
            };
            let amount = production.get(&resource);
            *production.get_mut(&resource) =
                ((amount as u128 * percent as u128) / 100).min(usize::MAX as u128) as usize;
        }
        production
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
        let shipyard = self.army.amount(&Unit::Building(Building::Shipyard));
        SHIPYARD_PRODUCTION_FACTOR.saturating_mul(shipyard).saturating_add(
            SPACE_DOCK_FLEET_PRODUCTION.saturating_mul(self.army.amount(&Unit::space_dock())),
        )
    }

    /// Returns current factory production available for defenses.
    pub fn battery_production(&self) -> usize {
        self.buy
            .iter()
            .filter_map(|unit| {
                (unit.is_defense() && *unit != Unit::space_dock()).then_some(unit.production())
            })
            .fold(0, usize::saturating_add)
    }

    /// Returns the maximum battery production allowed by current upgrades.
    pub fn max_battery_production(&self) -> usize {
        let factory = self.army.amount(&Unit::Building(Building::Factory));
        FACTORY_PRODUCTION_FACTOR.saturating_mul(factory)
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
        let gate = self.army.amount(&Unit::Building(Building::JumpGate));
        JUMP_GATE_CAPACITY_PER_LEVEL.saturating_mul(gate)
    }

    /// Returns whether this Relay diverts an arriving Spy group before combat.
    pub fn command_relay_diverts(&self, arriving_probes: usize) -> bool {
        let relay =
            self.army.amount(&Unit::Building(Building::CommandRelay)).min(Building::MAX_LEVEL);
        self.command_relay_active
            && relay > 0
            && arriving_probes > 0
            && arriving_probes <= relay.saturating_mul(PROBES_PER_PRODUCTION_LEVEL)
    }

    /// Returns whether at least one copy of the requested unit is stationed here.
    pub fn has(&self, unit: &Unit) -> bool {
        self.army.controller().amount(unit) > 0
    }

    /// Returns whether this value has buildings.
    pub fn has_buildings(&self) -> bool {
        self.army.iter().any(|(u, c)| u.is_building() && *c > 0)
    }

    /// Returns whether this value has fleet.
    pub fn has_fleet(&self) -> bool {
        self.army.iter().any(|(u, c)| u.is_ship() && *c > 0)
    }

    /// Returns the units this player may dispatch from this world.
    ///
    /// A controller uses the ordinary planet army. A foreign protector may dispatch only their
    /// own separately stationed fleet, while receiving no access to local structures or defenses.
    pub fn mission_origin_army(&self, player_id: PlayerId) -> Option<&Army> {
        if self.controlled == Some(player_id) || self.owned == Some(player_id) {
            Some(self.army.controller())
        } else {
            self.army.protector(player_id)
        }
    }

    /// Returns this player's mutable dispatch pool on the world.
    pub fn mission_origin_army_mut(&mut self, player_id: PlayerId) -> Option<&mut Army> {
        if self.controlled == Some(player_id) || self.owned == Some(player_id) {
            Some(self.army.controller_mut())
        } else {
            self.army.protector_mut(player_id)
        }
    }

    /// Returns whether the current controller has invited this player to protect the world.
    pub fn allows_protection(&self, player_id: PlayerId) -> bool {
        self.controlled.is_some()
            && self.controlled != Some(player_id)
            && self.protection_permissions.contains(&player_id)
    }

    /// Stations one foreign player's surviving Protect fleet without transferring control.
    pub fn dock_protecting_fleet(&mut self, player_id: PlayerId, army: Army) {
        self.army.dock_protector(player_id, army);
    }

    /// Returns whether this world has a non-orbital defense unit.
    pub fn has_defense(&self) -> bool {
        self.army
            .iter()
            .any(|(unit, count)| unit.is_defense() && *unit != Unit::space_dock() && *count > 0)
    }

    /// Releases control after a fleet departs when no infrastructure or ships remain.
    pub fn release_control_if_vacant(&mut self) {
        if !self.has_buildings() && !self.army.has_fleet() {
            self.controlled = None;
            self.protection_permissions.clear();
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
        self.army.clear();
        self.protection_permissions.clear();
        self.buy = Vec::new();
        self.is_destroyed = true;
        self.terraformer_focus = None;
        self.command_relay_active = true;
        self.shield_overload = ShieldOverloadState::Ready;
        self.fleet_withdrawal = FleetWithdrawal::Off;
        self.surface_build_order = [None; 4];
    }
}
