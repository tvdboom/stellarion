//! Bounded, presentation-only combat choreography. Reports supply all damage; visual
//! sampling, particle timing and curved trajectories never feed back into simulation.

use std::collections::{BTreeMap, BTreeSet};
use std::f32::consts::TAU;

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

pub(crate) use super::cinematic_timeline::{DEATH_RAY_COLLAPSE_AT, DEATH_RAY_DISCHARGE_AT};
use super::report::Side;
use super::resolution::ShotReport;
use super::systems::{
    BackgroundImageCmp, CombatCmp, CombatFormationState, CombatUnitCmp, IndividualCombatUnitCmp,
    PSCombatImageCmp, SpawnShotMsg,
};
use crate::core::assets::WorldAssets;
use crate::core::audio::PlayAudioMsg;
use crate::core::camera::MainCamera;
use crate::core::constants::{COMBAT_BACKGROUND_Z, COMBAT_EXPLOSION_Z};
use crate::core::settings::Settings;
use crate::core::units::defense::Defense;
use crate::core::units::fauna::FaunaAttack;
use crate::core::units::ships::Ship;
use crate::core::units::Unit;

const ICE: Color = Color::srgb(0.32, 0.85, 1.0);
const GOLD: Color = Color::srgb(1.0, 0.57, 0.16);
const MINT: Color = Color::srgb(0.35, 1.0, 0.68);
const VIOLET: Color = Color::srgb(0.75, 0.38, 1.0);
const MAX_PARTICLES: usize = 1800;
const COMBAT_READOUT_RASTER_SCALE: f32 = 0.05;
const MISSILE_FLIGHT_TIME: f32 = 0.95;
const MISSILE_CURVE_HEIGHT: f32 = 0.58;
/// Visible projectiles retained per missile or bomb source, target, and outcome.
const MISSILE_SALVO_LIMIT: usize = 12;
/// Keeps repeated missiles separately readable without turning them into sequential volleys.
const MISSILE_LAUNCH_STAGGER: f32 = 0.03;
// The original impact recording is quieter than the new firing cues. Preserve its
// character while keeping an actual hull strike audible beneath their tails.
const HULL_IMPACT_VOLUME: f32 = -10.0;
const WRECK_CARD_LIFETIME: f32 = 0.62;
// The final wreck stage launches debris with a short delay and a long drift. Keep an invisible
// wreck marker alive for that complete tail so conclusion playback cannot clear it early.
const WRECK_EFFECT_TAIL: f32 = 0.22 + 1.7;
const WRECK_STAGES: [f32; 4] = [0.0, 0.13, 0.27, 0.43];
const REPAIR_READOUT_PROGRESS: f32 = 0.45;
pub(crate) const DEATH_RAY_DURATION: f32 = 6.0;
pub(crate) const DEATH_RAY_FOCUS_AT: f32 = 1.15;

/// Heated envelope, energy column and white-hot core shared by both War Sun replays.
pub(crate) fn death_ray_beam_layers(size: f32) -> [(f32, Color); 3] {
    [
        (size * 2.35, GOLD.with_alpha(0.38)),
        (size * 1.05, GOLD),
        (size * 0.32, Color::srgb(1.0, 0.97, 0.78)),
    ]
}

/// Sustained discharges hold their intensity until the short final fade.
pub(crate) fn sustained_envelope(progress: f32) -> f32 {
    let progress = progress.clamp(0.0, 1.0);
    (progress * 28.0).min(1.0) * ((1.0 - progress) / 0.22).min(1.0)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Weapon {
    Laser,
    HeavyLaser,
    TwinLaser,
    Repeater,
    Railgun,
    Missile,
    Bomb,
    Broadside,
    Plasma,
    Ion,
    Lance,
    Solar,
    Siege,
    FaunaSonic,
    FaunaLightning,
    FaunaBioPlasma,
    FaunaGravity,
    FaunaVoid,
    FaunaFire,
    FaunaExtinction,
    Repair,
}

impl Weapon {
    /// Bombing is a recorded action, distinct from the same bomber's ordinary missiles.
    pub(crate) fn for_shot(unit: Unit, shot: &ShotReport) -> Self {
        if shot.is_bombing() {
            Self::Bomb
        } else {
            Self::for_unit(unit)
        }
    }

    pub(crate) fn for_unit(unit: Unit) -> Self {
        match unit {
            Unit::Ship(ship) => match ship {
                Ship::Probe | Ship::ColonyShip | Ship::LightFighter => Self::Laser,
                Ship::HeavyFighter => Self::TwinLaser,
                Ship::Destroyer => Self::Repeater,
                Ship::Cruiser => Self::Railgun,
                Ship::Bomber => Self::Missile,
                Ship::Battleship => Self::Broadside,
                Ship::Dreadnought => Self::Lance,
                Ship::WarSun => Self::Solar,
            },
            Unit::Defense(defense) => match defense {
                Defense::RepairTruck => Self::Repair,
                Defense::Crawler => Self::Laser,
                Defense::LightLaser => Self::Laser,
                Defense::HeavyLaser => Self::HeavyLaser,
                Defense::GaussCannon => Self::Railgun,
                Defense::IonCannon => Self::Ion,
                Defense::PlasmaTurret => Self::Plasma,
                Defense::SpaceDock => Self::Siege,
                Defense::RocketLauncher
                | Defense::AntiballisticMissile
                | Defense::InterplanetaryMissile => Self::Missile,
            },
            Unit::Fauna(fauna) => match fauna.attack() {
                FaunaAttack::SonicPulse => Self::FaunaSonic,
                FaunaAttack::Lightning => Self::FaunaLightning,
                FaunaAttack::BioPlasma => Self::FaunaBioPlasma,
                FaunaAttack::GravityPulse => Self::FaunaGravity,
                FaunaAttack::VoidLance => Self::FaunaVoid,
                FaunaAttack::StellarFire => Self::FaunaFire,
                FaunaAttack::ExtinctionRay => Self::FaunaExtinction,
            },
            Unit::Building(_) => Self::Laser,
        }
    }

    pub(crate) fn color(self) -> Color {
        match self {
            Self::Laser | Self::HeavyLaser | Self::TwinLaser => Color::srgb(1.0, 0.22, 0.3),
            Self::Repeater => ICE,
            Self::Railgun => Color::srgb(0.7, 0.83, 1.0),
            Self::Missile | Self::Bomb | Self::Broadside => GOLD,
            Self::Plasma => Color::srgb(0.18, 1.0, 0.28),
            Self::Ion => Color::srgb(0.14, 0.45, 1.0),
            Self::Lance => VIOLET,
            Self::Solar => Color::srgb(1.0, 0.82, 0.3),
            Self::Siege => Color::srgb(1.0, 0.2, 0.65),
            Self::FaunaSonic => Color::srgb(0.2, 0.95, 0.92),
            Self::FaunaLightning => Color::srgb(0.46, 0.78, 1.0),
            Self::FaunaBioPlasma => Color::srgb(0.42, 1.0, 0.18),
            Self::FaunaGravity => Color::srgb(0.55, 0.18, 0.9),
            Self::FaunaVoid => Color::srgb(0.86, 0.25, 1.0),
            Self::FaunaFire => Color::srgb(1.0, 0.35, 0.08),
            Self::FaunaExtinction => Color::srgb(0.82, 0.48, 1.0),
            Self::Repair => MINT,
        }
    }

    pub(crate) fn flight(self) -> f32 {
        match self {
            Self::Laser | Self::HeavyLaser | Self::Repeater => 0.28,
            Self::TwinLaser | Self::Railgun => 0.32,
            Self::Missile => MISSILE_FLIGHT_TIME,
            // A raid should read as a deliberate, heavy drop rather than gunfire.
            Self::Bomb => 1.75,
            Self::Broadside => 0.46,
            Self::Plasma | Self::Ion => 0.42,
            Self::Lance => 0.5,
            Self::Solar | Self::Siege => 0.62,
            Self::FaunaSonic => 0.7,
            Self::FaunaLightning => 0.3,
            Self::FaunaBioPlasma => 0.58,
            Self::FaunaGravity => 0.82,
            Self::FaunaVoid => 0.52,
            Self::FaunaFire => 0.74,
            Self::FaunaExtinction => 1.05,
            Self::Repair => 1.6,
        }
    }

    pub(crate) fn charge(self) -> f32 {
        match self {
            Self::Plasma | Self::Ion | Self::FaunaLightning | Self::FaunaBioPlasma => 0.16,
            Self::Lance => 0.3,
            Self::Solar => 0.58,
            Self::Siege => 0.45,
            Self::FaunaSonic => 0.32,
            Self::FaunaGravity => 0.48,
            Self::FaunaVoid => 0.35,
            Self::FaunaFire => 0.54,
            Self::FaunaExtinction => 1.25,
            _ => 0.,
        }
    }

    pub(crate) fn beam_width(self) -> Option<f32> {
        match self {
            Self::Plasma => Some(0.11),
            Self::Ion => Some(0.085),
            Self::Lance => Some(0.17),
            Self::Solar => Some(0.42),
            Self::Siege => Some(0.14),
            Self::FaunaLightning => Some(0.055),
            Self::FaunaGravity => Some(0.3),
            Self::FaunaVoid => Some(0.13),
            Self::FaunaFire => Some(0.34),
            Self::FaunaExtinction => Some(0.72),
            _ => None,
        }
    }

    pub(crate) fn barrels(self) -> usize {
        match self {
            Self::TwinLaser | Self::Broadside | Self::Siege | Self::FaunaLightning => 2,
            Self::Repeater => 3,
            _ => 1,
        }
    }

    fn salvo_limit(self) -> usize {
        match self {
            Self::Solar
            | Self::Siege
            | Self::Bomb
            | Self::FaunaSonic
            | Self::FaunaGravity
            | Self::FaunaVoid
            | Self::FaunaFire
            | Self::FaunaExtinction
            | Self::Repair => 1,
            Self::Plasma | Self::Ion | Self::Lance => 2,
            _ => 3,
        }
    }

    /// Returns whether playback uses the larger physical-projectile salvo.
    fn uses_missile_salvo(self) -> bool {
        matches!(self, Self::Missile | Self::Bomb)
    }

    fn visible_salvo_limit(self) -> usize {
        if self.uses_missile_salvo() {
            MISSILE_SALVO_LIMIT
        } else {
            self.salvo_limit()
        }
    }

    pub(crate) fn projectile_size(self, size: f32) -> Vec2 {
        size * match self {
            Self::Laser => Vec2::new(0.55, 0.06),
            Self::HeavyLaser => Vec2::new(0.55, 0.09),
            Self::TwinLaser => Vec2::new(0.5, 0.045),
            Self::Repeater => Vec2::new(0.4, 0.17),
            Self::Railgun => Vec2::new(1.25, 0.055),
            Self::Missile => Vec2::new(0.30, 0.11),
            Self::Bomb => Vec2::new(0.48, 0.20),
            Self::Broadside => Vec2::new(0.65, 0.10),
            Self::FaunaSonic => Vec2::new(0.46, 0.28),
            Self::FaunaBioPlasma => Vec2::new(0.38, 0.24),
            Self::Repair => Vec2::splat(0.15),
            Self::Plasma
            | Self::Ion
            | Self::Lance
            | Self::Solar
            | Self::Siege
            | Self::FaunaLightning
            | Self::FaunaGravity
            | Self::FaunaVoid
            | Self::FaunaFire
            | Self::FaunaExtinction => Vec2::ONE,
        }
    }

    pub(crate) fn launch_cue(self) -> Option<PlayAudioMsg> {
        match self {
            // Gain and pitch climb with weapon mass. Small fighters remain the reference;
            // heavier weapons stay at least as forceful even when their source recording is
            // softer or pitch-shifting stretches its transient.
            Self::Laser | Self::TwinLaser => Some(PlayAudioMsg::new("laser fire")),
            Self::HeavyLaser => Some(PlayAudioMsg::new("laser fire").rate(0.94).gain(-12.0)),
            Self::Repeater => Some(PlayAudioMsg::new("laser fire").rate(0.86).gain(-9.0)),
            Self::Railgun => Some(PlayAudioMsg::new("laser fire").rate(0.82).gain(-9.0)),
            Self::Missile => Some(PlayAudioMsg::new("missile fire").gain(-11.0)),
            Self::Bomb => Some(PlayAudioMsg::new("missile fire").rate(0.72).gain(-9.0)),
            Self::Broadside => Some(PlayAudioMsg::new("missile fire").rate(0.84).gain(-8.0)),
            Self::Plasma => Some(PlayAudioMsg::new("beam fire").rate(1.35).gain(-10.0)),
            Self::Ion => Some(PlayAudioMsg::new("beam fire").rate(1.6).gain(-10.0)),
            Self::Lance => Some(PlayAudioMsg::new("beam fire").rate(1.1).gain(-8.0)),
            Self::Solar => Some(PlayAudioMsg::new("beam fire").rate(0.76).gain(-5.0)),
            Self::Siege => Some(PlayAudioMsg::new("beam fire").rate(0.86).gain(-6.0)),
            Self::FaunaSonic => Some(PlayAudioMsg::new("fauna pulse").gain(-8.0)),
            Self::FaunaLightning => Some(PlayAudioMsg::new("fauna electric").gain(-7.0)),
            Self::FaunaBioPlasma => Some(PlayAudioMsg::new("fauna acid").gain(-7.0)),
            Self::FaunaGravity => Some(PlayAudioMsg::new("fauna roar").rate(0.72).gain(-5.0)),
            Self::FaunaVoid => Some(PlayAudioMsg::new("fauna roar").rate(1.2).gain(-7.0)),
            Self::FaunaFire => Some(PlayAudioMsg::new("fauna dragon").gain(-5.0)),
            Self::FaunaExtinction => Some(PlayAudioMsg::new("fauna roar").rate(0.52).gain(-3.0)),
            _ => None,
        }
    }

    /// The same charge envelope is sampled in both the schematic and cinematic views.
    pub(crate) fn massive(self) -> bool {
        matches!(
            self,
            Self::Solar
                | Self::Siege
                | Self::FaunaGravity
                | Self::FaunaFire
                | Self::FaunaExtinction
        )
    }

    pub(crate) fn charge_radius(self, size: f32) -> f32 {
        size * if self.massive() {
            1.35
        } else {
            0.65
        }
    }

    /// Charge ends at the saved launch, so a doomed shooter never fires after its death.
    pub(crate) fn cinematic_charge_start(self, launch: f32, impact: f32) -> f32 {
        launch - (impact - launch).max(0.0) * self.charge() / self.flight()
    }

    /// A penetrating hit uses the hull cue, without stacking the shield cue over it.
    pub(crate) fn impact_cue(self, shot: &ShotReport) -> Option<PlayAudioMsg> {
        if shot.missed {
            Some(miss_cue())
        } else if self == Self::Bomb && shot.killed {
            Some(PlayAudioMsg::new("large explosion"))
        } else if shot.hull_damage > 0 || shot.killed {
            Some(hull_impact_cue())
        } else if shot.shield_damage > 0 || shot.planetary_shield_damage > 0 {
            Some(PlayAudioMsg::new("shield impact"))
        } else {
            None
        }
    }
}

pub(crate) fn hull_impact_cue() -> PlayAudioMsg {
    PlayAudioMsg::new("short explosion").gain(HULL_IMPACT_VOLUME)
}

pub(crate) fn miss_cue() -> PlayAudioMsg {
    PlayAudioMsg::new("missile miss").rate(1.45)
}

/// Final wreck blast follows the same brief secondary-explosion sequence in both views.
pub(crate) fn wreck_cue(unit: Unit) -> (f32, PlayAudioMsg) {
    let scale = wreck_scale(unit);
    (
        WRECK_STAGES[3] * scale,
        PlayAudioMsg::new(if scale > 1.0 || unit == Unit::planetary_shield() {
            "large explosion"
        } else {
            "explosion"
        }),
    )
}

fn wreck_scale(unit: Unit) -> f32 {
    if matches!(unit, Unit::Ship(Ship::Battleship | Ship::Dreadnought | Ship::WarSun)) {
        1.5
    } else {
        1.0
    }
}

/// Creature weapon accents shared by combat playback and the short strategic-map encounter.
#[derive(Clone, Copy)]
pub(crate) enum WeaponTrail {
    Glow(Vec3, f32, Color, f32),
    Ring(Vec3, f32, Color, f32),
    Beam(Vec3, Vec3, f32, Color, f32),
    Sparks(Vec3, f32, Color, usize),
}

/// Samples the same electrical filaments, pressure rings, plasma and fire in both views.
/// The callback keeps per-frame sampling allocation-free.
pub(crate) fn fauna_weapon_trail(
    weapon: Weapon,
    origin: Vec3,
    position: Vec3,
    size: f32,
    elapsed: f32,
    progress: f32,
    mut emit: impl FnMut(WeaponTrail),
) {
    let color = weapon.color();
    match weapon {
        Weapon::Ion | Weapon::FaunaLightning => {
            let direction = position - origin;
            let normal = Vec3::new(-direction.y, direction.x, 0.).normalize_or_zero();
            let mut previous = origin;
            for i in 1..=7 {
                let t = i as f32 / 7.;
                let offset = if i == 7 {
                    0.
                } else {
                    (elapsed * 45. + i as f32 * 2.3).sin() * size * 0.035
                };
                let point = origin.lerp(position, t) + normal * offset;
                emit(WeaponTrail::Beam(previous, point, size * 0.012, ICE, 0.06));
                previous = point;
            }
        },
        Weapon::FaunaSonic => emit(WeaponTrail::Ring(
            position,
            size * (0.3 + progress * 0.45),
            color.with_alpha(0.55),
            0.2,
        )),
        Weapon::FaunaBioPlasma => {
            emit(WeaponTrail::Glow(position, size * 0.3, color.with_alpha(0.7), 0.2));
        },
        Weapon::FaunaGravity => {
            emit(WeaponTrail::Ring(position, size * 0.72, color.with_alpha(0.45), 0.28));
        },
        Weapon::FaunaVoid => emit(WeaponTrail::Sparks(position, size * 0.38, color, 4)),
        Weapon::FaunaFire => {
            emit(WeaponTrail::Glow(position, size * 0.55, color.with_alpha(0.7), 0.18));
            emit(WeaponTrail::Sparks(position, size * 0.45, GOLD, 3));
        },
        Weapon::FaunaExtinction => {
            emit(WeaponTrail::Ring(
                position,
                size * (0.48 + progress * 0.72),
                color.with_alpha(0.58),
                0.22,
            ));
            emit(WeaponTrail::Glow(position, size * 0.78, Color::WHITE, 0.16));
            emit(WeaponTrail::Sparks(position, size * 0.55, color, 5));
        },
        _ => {},
    }
}

/// Geometry shared by both combat renderers; progress never changes a recorded outcome.
#[derive(Clone, Copy)]
pub(crate) struct WeaponFlight {
    pub weapon: Weapon,
    pub origin: Vec3,
    pub destination: Vec3,
    pub size: f32,
    pub lane: f32,
}

/// A leading tip, body center and dimensions sampled without a frame-history dependency.
pub(crate) struct WeaponFlightSample {
    pub position: Vec3,
    pub center: Vec3,
    pub direction: Vec3,
    pub dimensions: Vec2,
}

impl WeaponFlight {
    pub(crate) fn position(self, progress: f32) -> Vec3 {
        let mut p = progress.clamp(0.0, 1.0);
        if self.weapon == Weapon::Bomb {
            p *= p;
        }
        let delta = self.destination - self.origin;
        let normal = Vec3::new(-delta.y, delta.x, 0.).normalize_or_zero();
        if self.weapon == Weapon::Repair {
            if p < 0.25 {
                return self.origin.lerp(self.destination, smooth(p * 4.));
            }
            if p > 0.8 {
                return self.destination.lerp(self.origin, smooth((p - 0.8) * 5.));
            }
            let orbit = (p - 0.25) / 0.55;
            let radius = (orbit * std::f32::consts::PI).sin() * self.size * 0.38;
            return self.destination
                + Vec3::new((orbit * TAU * 2.).cos(), (orbit * TAU * 2.).sin(), 0.) * radius;
        }
        let curve = match self.weapon {
            Weapon::Bomb => self.size * (self.lane - 0.5) * 1.4,
            Weapon::Missile => self.size * self.lane * MISSILE_CURVE_HEIGHT,
            _ => 0.0,
        };
        self.origin.lerp(self.destination, p) + normal * (std::f32::consts::PI * p).sin() * curve
    }

    pub(crate) fn sample(self, progress: f32) -> WeaponFlightSample {
        let position = self.position(progress);
        let direction = position - self.position((progress - 0.02).max(0.0));
        let (center, dimensions) = if let Some(width) = self.weapon.beam_width() {
            (
                (self.origin + position) * 0.5,
                Vec2::new(self.origin.distance(position).max(0.01), self.size * width),
            )
        } else if self.weapon != Weapon::Repair {
            let mut dimensions = self.weapon.projectile_size(self.size);
            dimensions.x = dimensions.x.min(self.origin.distance(position)).max(0.01);
            (position - direction.normalize_or_zero() * dimensions.x * 0.5, dimensions)
        } else {
            (position, self.weapon.projectile_size(self.size))
        };
        WeaponFlightSample {
            position,
            center,
            direction,
            dimensions,
        }
    }
}

/// Emits the same missile smoke, railgun trails, heavy-beam cores and creature accents.
/// Schematic playback spawns particles; cinematic playback samples their lifetime analytically.
pub(crate) fn weapon_trail(
    flight: WeaponFlight,
    elapsed: f32,
    progress: f32,
    mut emit: impl FnMut(WeaponTrail),
) {
    let position = flight.position(progress);
    let size = flight.size;
    let weapon = flight.weapon;
    match weapon {
        Weapon::Missile | Weapon::Bomb => {
            emit(WeaponTrail::Glow(position, size * 0.18, GOLD.with_alpha(0.65), 0.22));
            emit(WeaponTrail::Glow(
                position,
                size * 0.14,
                Color::srgb(0.42, 0.47, 0.56).with_alpha(0.45),
                0.48,
            ));
        },
        Weapon::Repair if (0.25..0.8).contains(&progress) => {
            emit(WeaponTrail::Beam(
                position,
                flight.destination,
                size * 0.025,
                MINT.with_alpha(0.7),
                0.085,
            ));
            emit(WeaponTrail::Glow(flight.destination, size * 0.2, MINT.with_alpha(0.25), 0.12));
        },
        Weapon::Railgun | Weapon::Broadside => emit(WeaponTrail::Beam(
            flight.position((progress - 0.2).max(0.0)),
            position,
            size * 0.025,
            weapon.color().with_alpha(0.45),
            0.12,
        )),
        Weapon::Solar | Weapon::Siege => emit(WeaponTrail::Glow(
            position,
            size * if weapon == Weapon::Siege {
                0.22
            } else {
                0.45
            },
            weapon.color().with_alpha(0.6),
            0.13,
        )),
        _ => fauna_weapon_trail(weapon, flight.origin, position, size, elapsed, progress, emit),
    }
}

/// Particle opacity envelope shared by live entities and stateless movie sampling.
pub(crate) fn particle_envelope(progress: f32) -> f32 {
    let progress = progress.clamp(0.0, 1.0);
    (progress * 18.0).min(1.0) * (1.0 - progress).powf(1.3)
}

/// One visible projectile or an aggregated representative salvo. Missiles and bombs use a larger
/// bound than other weapons, while every recorded outcome is accumulated into the visible effects.
#[derive(Component)]
pub struct PendingImpact {
    /// Entity whose artwork receives this visible effect.
    target: Entity,
    /// Aggregate type card updated for legacy/grouped playback and round progression.
    group_target: Entity,
    /// Exact cards whose recorded state changes are carried by this visible projectile.
    individual_outcomes: Vec<IndividualImpact>,
    source: Option<Entity>,
    origin: Vec3,
    destination: Vec3,
    size: f32,
    weapon: Weapon,
    missed: bool,
    hull: usize,
    shield: usize,
    planetary: usize,
    levels: usize,
    /// Full building-level loss shown once for this target's complete bombing volley.
    display_levels: usize,
    elapsed: f32,
    delay: f32,
    lane: f32,
    launched: bool,
    readout_shown: bool,
    trail_clock: f32,
}

#[derive(Clone, Copy)]
struct IndividualImpact {
    target: Entity,
    hull: usize,
    shield: usize,
}

impl PendingImpact {
    fn flight_path(&self) -> WeaponFlight {
        WeaponFlight {
            weapon: self.weapon,
            origin: self.origin,
            destination: self.destination,
            size: self.size,
            lane: self.lane,
        }
    }
}

/// A defeated card remains until its secondary blasts, flash and debris launch finish.
#[derive(Component)]
pub struct Wreck {
    origin: Vec3,
    size: f32,
    elapsed: f32,
    tail_elapsed: f32,
    stage: usize,
    unit: Unit,
    heavy: bool,
    audible: bool,
    card_hidden: bool,
}

impl Wreck {
    pub(crate) fn new(origin: Vec3, size: f32, unit: Unit) -> Self {
        Self {
            origin,
            size,
            elapsed: 0.,
            tail_elapsed: 0.,
            stage: 0,
            unit,
            heavy: matches!(unit, Unit::Ship(Ship::Battleship | Ship::Dreadnought | Ship::WarSun)),
            audible: true,
            card_hidden: false,
        }
    }
}

/// Death-ray charge, sustained beam and planetary shockwave, in playback seconds.
#[derive(Component)]
pub struct Cinematic {
    origins: Vec<Vec3>,
    target: Vec3,
    viewport: Vec2,
    size: f32,
    elapsed: f32,
    stage: usize,
    destroys_planet: bool,
    boom_stage: usize,
}

impl Cinematic {
    #[cfg(test)]
    pub(crate) fn new(
        origin: Vec3,
        target: Vec3,
        viewport: Vec2,
        size: f32,
        destroys_planet: bool,
    ) -> Self {
        Self::from_origins(vec![origin], target, viewport, size, destroys_planet)
    }

    pub(crate) fn from_origins(
        mut origins: Vec<Vec3>,
        target: Vec3,
        viewport: Vec2,
        size: f32,
        destroys_planet: bool,
    ) -> Self {
        if origins.is_empty() {
            origins.push(target);
        }
        for origin in &mut origins {
            origin.z = COMBAT_EXPLOSION_Z;
        }
        Self {
            origins,
            target: target.truncate().extend(COMBAT_EXPLOSION_Z),
            viewport,
            size,
            elapsed: 0.,
            stage: 0,
            destroys_planet,
            boom_stage: 0,
        }
    }

    fn focus(&self) -> Vec3 {
        let source_center = self.origins.iter().copied().sum::<Vec3>() / self.origins.len() as f32;
        source_center.lerp(self.target, 0.22)
    }

    fn combined_beam_size(&self) -> f32 {
        self.size * (1.0 + (self.origins.len() as f32).ln() * 0.12).min(1.5)
    }

    #[cfg(test)]
    pub(crate) fn origins(&self) -> &[Vec3] {
        &self.origins
    }
}

#[derive(Component, Default)]
/// Additive card motion, preserving its original color and tween-controlled scale.
pub struct UnitMotion {
    offset: Vec3,
    impulse: Vec3,
    flash: f32,
    miss_flash: f32,
    miss_cooldown: f32,
    sparks: f32,
    base_color: Color,
}

#[derive(Component)]
/// A transient sprite with analytical motion and a bounded lifetime.
pub struct Particle {
    origin: Vec3,
    velocity: Vec3,
    start_size: Vec2,
    end_size: Vec2,
    color: Color,
    elapsed: f32,
    delay: f32,
    lifetime: f32,
    spin: f32,
    sustained: bool,
}

impl Particle {
    fn glow(origin: Vec3, size: f32, color: Color, lifetime: f32, delay: f32) -> Self {
        Self {
            origin,
            velocity: Vec3::ZERO,
            start_size: Vec2::splat(size),
            end_size: Vec2::splat(size * 1.8),
            color,
            elapsed: 0.0,
            delay,
            lifetime,
            spin: 0.0,
            sustained: false,
        }
    }

    fn ring(origin: Vec3, size: f32, color: Color, lifetime: f32, delay: f32) -> Self {
        Self {
            start_size: Vec2::splat(size * 0.2),
            end_size: Vec2::splat(size),
            ..Self::glow(origin, size, color, lifetime, delay)
        }
    }

    fn blast(origin: Vec3, size: f32, lifetime: f32, delay: f32) -> Self {
        Self {
            end_size: Vec2::splat(size * 1.1),
            ..Self::glow(origin, size, Color::WHITE, lifetime, delay)
        }
    }

    fn spark(origin: Vec3, size: f32, color: Color, index: usize, debris: bool) -> Self {
        let angle = index as f32 * 2.399_963;
        let direction = Vec3::new(angle.cos(), angle.sin(), 0.0);
        let distance = size * (0.5 + (index % 5) as f32 * 0.17);
        let fragment = (size * 0.045).clamp(2.5, 11.0) * (0.6 + (index % 4) as f32 * 0.15);
        Self {
            origin: origin
                + if debris {
                    direction * size * 0.08
                } else {
                    Vec3::ZERO
                },
            velocity: direction * distance,
            start_size: if debris {
                Vec2::new(fragment, fragment * 0.6)
            } else {
                Vec2::new(size * 0.035, size * 0.018)
            },
            end_size: Vec2::splat(if debris {
                fragment * 0.35
            } else {
                size * 0.008
            }),
            color: if debris && color == GOLD {
                Color::srgb(0.28, 0.32, 0.38)
            } else {
                color
            },
            elapsed: 0.0,
            delay: if debris {
                0.22
            } else {
                0.0
            },
            lifetime: if debris {
                1.7
            } else {
                0.42
            },
            spin: if debris {
                angle - 3.0
            } else {
                0.0
            },
            sustained: false,
        }
    }

    fn sample(&self, age: f32) -> Option<ParticleSample> {
        let age = age - self.delay;
        if !(0.0..self.lifetime).contains(&age) {
            return None;
        }
        let progress = age / self.lifetime;
        let envelope = if self.sustained {
            sustained_envelope(progress)
        } else {
            particle_envelope(progress)
        };
        Some(ParticleSample {
            center: self.origin + self.velocity * age,
            size: self.start_size.lerp(self.end_size, smooth(progress)),
            rotation: self.spin * age,
            color: self.color.with_alpha(self.color.alpha() * envelope),
            progress,
        })
    }
}

struct ParticleSample {
    center: Vec3,
    size: Vec2,
    rotation: f32,
    color: Color,
    progress: f32,
}

#[derive(Clone, Copy)]
enum WreckMask {
    Blast,
    Glow,
    Ring,
    Shard,
}

/// Expands one shared destruction stage without allocating a per-frame effect collection.
fn wreck_stage(
    origin: Vec3,
    size: f32,
    unit: Unit,
    stage: usize,
    mut emit: impl FnMut(WreckMask, Particle),
) {
    let heavy = wreck_scale(unit);
    if stage < 3 {
        let angle = stage as f32 * 2.4;
        let origin = origin + Vec3::new(angle.cos(), angle.sin(), 0.0) * size * 0.2;
        emit(WreckMask::Blast, Particle::blast(origin, size * 0.65, 0.42, 0.0));
        emit(WreckMask::Glow, Particle::glow(origin, size * 0.75, GOLD, 0.24, 0.0));
        for index in 0..6 {
            emit(WreckMask::Glow, Particle::spark(origin, size * 0.7, GOLD, index, false));
        }
    } else {
        emit(WreckMask::Blast, Particle::blast(origin, size * 1.6 * heavy, 0.95, 0.0));
        emit(WreckMask::Glow, Particle::glow(origin, size * 1.8 * heavy, GOLD, 0.55, 0.0));
        emit(WreckMask::Glow, Particle::glow(origin, size * 0.95 * heavy, Color::WHITE, 0.16, 0.0));
        emit(
            WreckMask::Ring,
            Particle::ring(origin, size * 2.4 * heavy, GOLD.with_alpha(0.65), 0.85, 0.0),
        );
        for index in 0..18 {
            emit(WreckMask::Shard, Particle::spark(origin, size * heavy, GOLD, index, true));
        }
    }
}

/// A movie-frame sample of the same atlas blasts and masks spawned by schematic wrecks.
pub(crate) struct WreckSample {
    pub texture: &'static str,
    pub atlas_frame: Option<usize>,
    pub center: Vec3,
    pub size: Vec2,
    pub rotation: f32,
    pub color: Color,
}

/// Seekable destruction retains secondary blasts, delayed shards and the unit's heavy timing.
pub(crate) fn sample_wreck(size: f32, unit: Unit, age: f32, mut paint: impl FnMut(WreckSample)) {
    for (stage, starts_at) in WRECK_STAGES.into_iter().enumerate() {
        let age = age - starts_at * wreck_scale(unit);
        if age < 0.0 {
            continue;
        }
        wreck_stage(Vec3::ZERO, size, unit, stage, |mask, particle| {
            let Some(sample) = particle.sample(age) else {
                return;
            };
            paint(WreckSample {
                texture: match mask {
                    WreckMask::Blast => "explosion",
                    WreckMask::Glow => "combat fx glow",
                    WreckMask::Ring => "combat fx ring",
                    WreckMask::Shard => "combat fx shard",
                },
                atlas_frame: matches!(mask, WreckMask::Blast)
                    .then_some((sample.progress * 47.0) as usize),
                center: sample.center,
                size: sample.size,
                rotation: sample.rotation,
                color: sample.color,
            });
        });
    }
}

#[derive(Component)]
/// Atlas frames sampled from effect age, including frames crossed by fast-forward.
pub struct BlastFrames(usize);

#[derive(Component)]
/// Opaque cover while the planetary backdrop switches under its explosion cloud.
pub struct PlanetFlash;

#[derive(Component)]
/// The War Sun's main discharge holds at full intensity before its final fade.
struct CinematicBeam;

#[derive(Component)]
/// One attacking War Sun's feeder beam into the shared death-ray focus.
struct CinematicConvergenceBeam;

#[derive(Component)]
/// One short, irregular molten segment in the planet-destruction fracture pattern.
struct PlanetFissure;

#[derive(Component)]
/// A brief miss or repair label, timed with the same playback clock.
pub struct CombatReadout {
    age: f32,
    origin: Vec3,
    size: f32,
    color: Color,
}

/// Shared procedural masks; no downloads or per-frame images.
#[derive(Default)]
pub struct EffectTextures {
    pub(crate) glow: Handle<Image>,
    pub(crate) ring: Handle<Image>,
    pub(crate) shard: Handle<Image>,
    pub(crate) beam: Handle<Image>,
    pub(crate) missile: Handle<Image>,
    ready: bool,
}

impl EffectTextures {
    /// Egui variants of the exact schematic masks, with alpha encoded for its blending mode.
    pub(crate) fn cinematic_images(
        images: &mut Assets<Image>,
    ) -> [(&'static str, Handle<Image>); 5] {
        let mut textures = Self::default();
        textures.initialize(images);
        [
            ("combat fx glow", textures.glow),
            ("combat fx ring", textures.ring),
            ("combat fx shard", textures.shard),
            ("combat fx beam", textures.beam),
            ("combat fx missile", textures.missile),
        ]
        .map(|(name, handle)| {
            if let Some(mut image) = images.get_mut(&handle) {
                if let Some(pixels) = &mut image.data {
                    for pixel in pixels.as_chunks_mut::<4>().0 {
                        let alpha = pixel[3];
                        pixel[..3].fill(alpha);
                    }
                }
            }
            (name, handle)
        })
    }

    pub(crate) fn initialize(&mut self, images: &mut Assets<Image>) {
        if self.ready {
            return;
        }
        for kind in 0..5 {
            let resolution = if kind == 2 {
                32
            } else {
                256
            };
            let center = (resolution as f32 - 1.) * 0.5;
            let mut pixels = Vec::with_capacity(resolution * resolution * 4);
            for y in 0..resolution {
                for x in 0..resolution {
                    let uv = Vec2::new((x as f32 - center) / center, (y as f32 - center) / center);
                    let r = uv.length();
                    let alpha = match kind {
                        1 => (1. - ((r - 0.79) / 0.03).abs()).clamp(0., 1.),
                        2 => ((0.8 - uv.x.abs()).min(uv.y + 0.4).min(0.35 - uv.x * 0.3 - uv.y)
                            * 20.)
                            .clamp(0., 1.),
                        3 => {
                            (1. - uv.y.abs()).clamp(0., 1.).powf(2.)
                                * ((1. - uv.x.abs()) * 14.).clamp(0., 1.)
                        },
                        4 => {
                            // Pointed nose, substantial body and swept fins remain legible
                            // after the mask is stretched into both missiles and heavy bombs.
                            let body = (-0.72..=0.5).contains(&uv.x) && uv.y.abs() <= 0.25;
                            let nose =
                                (0.5..=0.96).contains(&uv.x) && uv.y.abs() <= (0.96 - uv.x) * 0.55;
                            let fins = (-0.72..=-0.3).contains(&uv.x)
                                && uv.y.abs() <= 0.68 - (uv.x + 0.72) * 0.9;
                            if body || nose || fins {
                                1.0
                            } else {
                                0.0
                            }
                        },
                        _ => (1. - r).clamp(0., 1.).powf(2.5),
                    };
                    // Sub-byte dithering avoids visible alpha bands in screen-sized glows.
                    let dither = ((x * 73 + y * 151 + x * y * 17) % 101) as f32 / 101.;
                    pixels.extend_from_slice(&[255, 255, 255, (255. * alpha + dither) as u8]);
                }
            }
            let mut image = Image::new(
                Extent3d {
                    width: resolution as u32,
                    height: resolution as u32,
                    depth_or_array_layers: 1,
                },
                TextureDimension::D2,
                pixels,
                TextureFormat::Rgba8UnormSrgb,
                RenderAssetUsages::default(),
            );
            image.sampler = bevy::image::ImageSampler::linear();
            match kind {
                1 => self.ring = images.add(image),
                2 => self.shard = images.add(image),
                3 => self.beam = images.add(image),
                4 => self.missile = images.add(image),
                _ => self.glow = images.add(image),
            }
        }
        self.ready = true;
    }
}

struct Painter<'a, 'w, 's> {
    commands: &'a mut Commands<'w, 's>,
    textures: &'a EffectTextures,
    budget: usize,
    art: Option<&'a WorldAssets>,
}

impl Painter<'_, '_, '_> {
    fn particle(&mut self, ring: bool, mut p: Particle) {
        if self.budget == 0 {
            return;
        }
        self.budget -= 1;
        p.origin.z = COMBAT_EXPLOSION_Z + 0.2;
        self.commands.spawn((
            Sprite {
                image: if ring {
                    self.textures.ring.clone()
                } else if p.spin != 0. {
                    self.textures.shard.clone()
                } else {
                    self.textures.glow.clone()
                },
                color: p.color.with_alpha(0.),
                custom_size: Some(p.start_size),
                ..default()
            },
            Transform::from_translation(p.origin),
            p,
            CombatCmp,
            Pickable::IGNORE,
        ));
    }

    fn glow(&mut self, origin: Vec3, size: f32, color: Color, lifetime: f32) {
        self.glow_after(origin, size, color, lifetime, 0.);
    }

    fn glow_after(&mut self, origin: Vec3, size: f32, color: Color, lifetime: f32, delay: f32) {
        self.particle(false, Particle::glow(origin, size, color, lifetime, delay));
    }

    fn ring(&mut self, origin: Vec3, size: f32, color: Color, lifetime: f32) {
        self.ring_after(origin, size, color, lifetime, 0.);
    }

    fn ring_after(&mut self, origin: Vec3, size: f32, color: Color, lifetime: f32, delay: f32) {
        self.particle(true, Particle::ring(origin, size, color, lifetime, delay));
    }

    fn sparks(&mut self, origin: Vec3, size: f32, color: Color, count: usize, debris: bool) {
        for i in 0..count {
            self.particle(false, Particle::spark(origin, size, color, i, debris));
        }
    }

    fn blast(&mut self, origin: Vec3, size: f32, lifetime: f32) {
        self.blast_after(origin, size, lifetime, 0.);
    }

    fn blast_after(&mut self, origin: Vec3, size: f32, lifetime: f32, delay: f32) {
        self.blast_particle(Particle::blast(origin, size, lifetime, delay));
    }

    fn blast_particle(&mut self, mut particle: Particle) {
        if self.budget == 0 {
            return;
        }
        let Some(art) = self.art else {
            return;
        };
        self.budget -= 1;
        let texture = art.texture("explosion");
        particle.origin.z = COMBAT_EXPLOSION_Z + 0.15;
        self.commands.spawn((
            Sprite {
                image: texture.image,
                texture_atlas: Some(texture.atlas),
                color: Color::WHITE.with_alpha(0.),
                custom_size: Some(particle.start_size),
                ..default()
            },
            Transform::from_translation(particle.origin),
            particle,
            BlastFrames(texture.last_index),
            CombatCmp,
            Pickable::IGNORE,
        ));
    }

    fn beam(&mut self, from: Vec3, to: Vec3, width: f32, color: Color, lifetime: f32) {
        self.beam_after(from, to, width, color, lifetime, 0., false);
    }

    fn sustained_beam(&mut self, from: Vec3, to: Vec3, width: f32, color: Color, lifetime: f32) {
        self.beam_after(from, to, width, color, lifetime, 0., true);
    }

    fn convergence_beam(&mut self, from: Vec3, to: Vec3, width: f32, color: Color, lifetime: f32) {
        if let Some(entity) = self.beam_after(from, to, width, color, lifetime, 0., false) {
            self.commands.entity(entity).insert(CinematicConvergenceBeam);
        }
    }

    fn fissure(
        &mut self,
        from: Vec3,
        to: Vec3,
        width: f32,
        color: Color,
        lifetime: f32,
        delay: f32,
    ) {
        if let Some(entity) = self.beam_after(from, to, width, color, lifetime, delay, false) {
            self.commands.entity(entity).insert(PlanetFissure);
        }
    }

    fn beam_after(
        &mut self,
        from: Vec3,
        to: Vec3,
        width: f32,
        color: Color,
        lifetime: f32,
        delay: f32,
        sustained: bool,
    ) -> Option<Entity> {
        if self.budget == 0 {
            return None;
        }
        let d = to - from;
        if d.length_squared() < 0.01 {
            return None;
        }
        let entity = self
            .commands
            .spawn((
                Sprite {
                    image: self.textures.beam.clone(),
                    color,
                    custom_size: Some(Vec2::new(d.length(), width)),
                    ..default()
                },
                Transform::from_translation(Vec3::new(
                    (from.x + to.x) * 0.5,
                    (from.y + to.y) * 0.5,
                    COMBAT_EXPLOSION_Z + 0.1,
                ))
                .with_rotation(Quat::from_rotation_z(d.y.atan2(d.x))),
                Particle {
                    origin: Vec3::new(
                        (from.x + to.x) * 0.5,
                        (from.y + to.y) * 0.5,
                        COMBAT_EXPLOSION_Z + 0.1,
                    ),
                    velocity: Vec3::ZERO,
                    start_size: Vec2::new(d.length(), width),
                    end_size: Vec2::new(d.length(), width * 0.4),
                    color,
                    elapsed: 0.,
                    delay,
                    lifetime,
                    spin: 0.,
                    sustained,
                },
                CombatCmp,
                Pickable::IGNORE,
            ))
            .id();
        if sustained {
            self.commands.entity(entity).insert(CinematicBeam);
        }
        self.budget -= 1;
        Some(entity)
    }
}

fn smooth(p: f32) -> f32 {
    p * p * (3. - 2. * p)
}

fn queue_combat_sound(
    audio: &mut MessageWriter<PlayAudioMsg>,
    cooldowns: &mut BTreeMap<&'static str, f32>,
    cue: PlayAudioMsg,
) {
    if let std::collections::btree_map::Entry::Vacant(entry) = cooldowns.entry(cue.name) {
        audio.write(cue);
        entry.insert(0.12);
    }
}

#[derive(Component, Default)]
/// Removable camera offset; never persisted or applied to the simulation camera position.
pub struct CombatCameraMotion(Vec3);

/// Brief, small camera jolts only for capital-ship destruction and planetary shockwaves.
pub fn shake_combat_camera(
    mut commands: Commands,
    mut cameras: Query<
        (Entity, &mut Transform, &Projection, Option<&mut CombatCameraMotion>),
        With<MainCamera>,
    >,
    wrecks: Query<&Wreck>,
    rays: Query<&Cinematic>,
) {
    let mut shake = Vec2::ZERO;
    for wreck in &wrecks {
        let age = wreck.elapsed - 0.43 * 1.5;
        if wreck.heavy && (0.0..0.35).contains(&age) {
            shake += Vec2::new((age * 75.).sin(), (age * 59.).cos()) * 1.7 * (1. - age / 0.35);
        }
    }
    for ray in &rays {
        let impact_age = ray.elapsed - 2.;
        if (0.0..0.28).contains(&impact_age) {
            shake += Vec2::new((impact_age * 92.).sin(), (impact_age * 71.).cos())
                * 2.2
                * (1. - impact_age / 0.28);
        }
        let blast_age = ray.elapsed - 3.7;
        if ray.destroys_planet && (0.0..0.65).contains(&blast_age) {
            shake += Vec2::new((blast_age * 65.).sin(), (blast_age * 81.).sin())
                * 5.
                * (1. - blast_age / 0.65);
        }
    }
    for (entity, mut transform, projection, motion) in &mut cameras {
        let Projection::Orthographic(projection) = projection else {
            continue;
        };
        let offset = (shake.clamp_length_max(5.) * projection.scale).extend(0.);
        if let Some(mut motion) = motion {
            transform.translation += offset - motion.0;
            motion.0 = offset;
        } else {
            transform.translation += offset;
            commands.entity(entity).insert(CombatCameraMotion(offset));
        }
    }
}

/// Restores the exact map camera position even when playback is exited mid-shockwave.
pub fn restore_combat_camera(
    mut commands: Commands,
    mut cameras: Query<(Entity, &mut Transform, &CombatCameraMotion), With<MainCamera>>,
) {
    for (entity, mut transform, motion) in &mut cameras {
        transform.translation -= motion.0;
        commands.entity(entity).remove::<CombatCameraMotion>();
    }
}

/// Advances projectile arrivals, bounded particles, ship reactions, destruction and
/// the death ray using one pause/speed-aware clock. Arrival applies recorded totals once.
pub fn run_combat_animations(
    mut commands: Commands,
    mut shots: MessageReader<SpawnShotMsg>,
    mut pending: Query<
        (Entity, &mut PendingImpact, &mut Sprite, &mut Transform, &mut Visibility),
        Without<CombatUnitCmp>,
    >,
    combatants: (
        Query<
            (Entity, &mut Sprite, &mut Transform, &mut CombatUnitCmp, Option<&mut UnitMotion>),
            (Without<IndividualCombatUnitCmp>, Without<PendingImpact>, Without<Particle>),
        >,
        Query<
            (
                Entity,
                &mut Sprite,
                &mut Transform,
                &mut IndividualCombatUnitCmp,
                Option<&mut UnitMotion>,
            ),
            (Without<CombatUnitCmp>, Without<PendingImpact>, Without<Particle>),
        >,
    ),
    shields: Query<
        (&Sprite, &GlobalTransform),
        (
            With<PSCombatImageCmp>,
            Without<CombatUnitCmp>,
            Without<IndividualCombatUnitCmp>,
            Without<PendingImpact>,
            Without<Particle>,
        ),
    >,
    mut particles: Query<
        (
            Entity,
            &mut Particle,
            &mut Sprite,
            &mut Transform,
            Option<&BlastFrames>,
            Option<&PlanetFlash>,
        ),
        (Without<CombatUnitCmp>, Without<PendingImpact>),
    >,
    mut wrecks: Query<(Entity, &mut Wreck)>,
    mut cinematics: Query<&mut Cinematic>,
    mut textures: Local<EffectTextures>,
    mut images: ResMut<Assets<Image>>,
    playback: (Res<Settings>, Option<Res<CombatFormationState>>, Res<Time>),
    mut audio: MessageWriter<PlayAudioMsg>,
    presentation: (
        Option<Res<WorldAssets>>,
        Query<
            &mut Sprite,
            (
                With<BackgroundImageCmp>,
                Without<PSCombatImageCmp>,
                Without<CombatUnitCmp>,
                Without<IndividualCombatUnitCmp>,
                Without<PendingImpact>,
                Without<Particle>,
            ),
        >,
    ),
    mut readouts: Query<
        (Entity, &mut CombatReadout, &mut TextColor, &mut Transform),
        (
            Without<CombatUnitCmp>,
            Without<IndividualCombatUnitCmp>,
            Without<PendingImpact>,
            Without<Particle>,
        ),
    >,
    mut sound_cooldowns: Local<BTreeMap<&'static str, f32>>,
) {
    let (mut units, mut individuals) = combatants;
    let (settings, formation, time) = playback;
    let (art, mut backdrops) = presentation;
    textures.initialize(&mut images);
    // Audio remains at normal pitch/speed. Limit cues in real seconds so fast-forward
    // cannot stack a whole fleet's sound onto one audible instant.
    sound_cooldowns.retain(|_, remaining| {
        *remaining -= time.delta_secs();
        *remaining > 0.
    });
    let dt = time.delta_secs() * settings.speed();
    let mut painter = Painter {
        commands: &mut commands,
        textures: &textures,
        budget: MAX_PARTICLES.saturating_sub(particles.iter().len()),
        art: art.as_deref(),
    };

    // Keep every weapon bounded, but give physical missiles and raid bombs a larger salvo.
    // BTreeMap gives stable launch ordering and accumulated outcomes preserve combat results.
    let mut grouped = BTreeMap::<(Entity, Option<Entity>, bool, bool, usize), PendingImpact>::new();
    let mut counts = BTreeMap::new();
    let individual_mode =
        formation.as_ref().map_or(settings.combat_individual_units, |state| state.individual());
    for message in shots.read() {
        let Some((group_target, group_unit, group_position, group_dimensions)) = units
            .iter()
            .find(|(_, _, _, cu, _)| Some(cu.unit) == message.shot.unit && cu.side == message.side)
            .map(|(entity, sprite, transform, cu, _)| {
                (
                    entity,
                    cu.unit,
                    transform.translation,
                    sprite.custom_size.unwrap_or(Vec2::splat(120.)),
                )
            })
        else {
            continue;
        };
        let exact_target = message.shot.target_id.and_then(|target_id| {
            individuals
                .iter()
                .find(|(_, _, _, individual, _)| {
                    individual.id == Some(target_id) && individual.side == message.side
                })
                .map(|(entity, sprite, transform, _, _)| {
                    (entity, transform.translation, sprite.custom_size.unwrap_or(Vec2::splat(120.)))
                })
        });
        let (target, target_position, target_dimensions) = if individual_mode {
            exact_target.unwrap_or((group_target, group_position, group_dimensions))
        } else {
            (group_target, group_position, group_dimensions)
        };
        let (mut destination, size) = if group_unit == Unit::planetary_shield() {
            shields
                .iter()
                .next()
                .map(|(s, t)| {
                    let dimensions = s.custom_size.unwrap_or(Vec2::splat(120.));
                    let effect_size = if dimensions.x > dimensions.y * 2.0 {
                        dimensions.y * 0.65
                    } else {
                        dimensions.x
                    };
                    (t.translation(), effect_size)
                })
                .unwrap_or((group_position, 120.))
        } else {
            (target_position, target_dimensions.x)
        };
        let source = message.source.map(|s| s.0);
        let key = (target, source, message.repair, message.shot.missed);
        let count = counts.entry(key).or_insert(0usize);

        destination.z = COMBAT_EXPLOSION_Z;
        let mut origin = message.source.map_or(destination + Vec3::Y * size * 2., |s| s.2);
        origin.z = COMBAT_EXPLOSION_Z;
        origin += (destination - origin).normalize_or_zero() * size * 0.28;
        let weapon = if message.repair {
            Weapon::Repair
        } else if message.shot.is_bombing() {
            Weapon::Bomb
        } else {
            message.source.map_or(Weapon::Laser, |s| Weapon::for_unit(s.1))
        };
        let projectile_index = *count % weapon.visible_salvo_limit();
        let lane = *count % weapon.salvo_limit();
        *count += 1;
        let interceptor = message.source.is_some_and(|(_, unit, _)| {
            unit == Unit::antiballistic_missile()
                && message.shot.unit == Some(Unit::interplanetary_missile())
        });
        if interceptor {
            // Every interceptor resolves against the incoming missile card. Successful
            // shots spread across its body; failed shots finish near its rim so the MISS
            // readout communicates the result without the projectile flying into space.
            let center_lane = (weapon.salvo_limit() - 1) as f32 * 0.5;
            let lane_offset = lane as f32 - center_lane;
            if message.shot.missed {
                destination.x += target_dimensions.x * 0.36;
                destination.y += lane_offset * target_dimensions.y * 0.09;
            } else {
                destination.x += lane_offset * target_dimensions.x * 0.17;
            }
        } else if message.shot.missed {
            // Outcome feedback communicates the miss. The projectile itself still
            // terminates within the target artwork instead of firing into empty space.
            destination.x += size * (0.22 + lane as f32 * 0.06);
            destination.y += size * (lane as f32 - 1.0) * 0.08;
        } else {
            let center_lane = (weapon.salvo_limit() - 1) as f32 * 0.5;
            destination.x += (lane as f32 - center_lane) * size * 0.17;
        }
        let impact = grouped
            .entry((target, source, message.repair, message.shot.missed, projectile_index))
            .or_insert(PendingImpact {
                target,
                group_target,
                individual_outcomes: Vec::new(),
                source,
                origin,
                destination,
                size,
                weapon,
                missed: message.shot.missed,
                hull: 0,
                shield: 0,
                planetary: 0,
                levels: 0,
                display_levels: 0,
                elapsed: 0.,
                // Every target of one firing card belongs to the same visible volley. Lanes
                // spread projectiles spatially without making rapid-fire chains or later target
                // classes look like a second firing action.
                delay: 0.08
                    + weapon.charge()
                    + if weapon.uses_missile_salvo() {
                        projectile_index as f32 * MISSILE_LAUNCH_STAGGER
                    } else {
                        0.
                    },
                lane: if weapon.salvo_limit() == 1 {
                    0.
                } else {
                    lane as f32 - 1.
                },
                launched: false,
                readout_shown: false,
                trail_clock: 0.,
            });
        impact.hull = impact.hull.saturating_add(message.shot.hull_damage);
        impact.shield = impact.shield.saturating_add(message.shot.shield_damage);
        impact.planetary = impact.planetary.saturating_add(message.shot.planetary_shield_damage);
        impact.levels = impact.levels.saturating_add(usize::from(message.shot.killed));
        if let Some((target, _, _)) = exact_target {
            if let Some(outcome) =
                impact.individual_outcomes.iter_mut().find(|outcome| outcome.target == target)
            {
                outcome.hull = outcome.hull.saturating_add(message.shot.hull_damage);
                outcome.shield = outcome.shield.saturating_add(message.shot.shield_damage);
            } else {
                impact.individual_outcomes.push(IndividualImpact {
                    target,
                    hull: message.shot.hull_damage,
                    shield: message.shot.shield_damage,
                });
            }
        }
    }
    let bombing_level_totals = grouped
        .values()
        .filter(|impact| impact.weapon == Weapon::Bomb && !impact.missed)
        .fold(BTreeMap::<Entity, usize>::new(), |mut totals, impact| {
            *totals.entry(impact.target).or_default() += impact.levels;
            totals
        });
    let mut bombing_readouts_assigned = BTreeSet::new();
    for impact in grouped.values_mut() {
        if impact.weapon == Weapon::Bomb
            && impact.levels > 0
            && bombing_readouts_assigned.insert(impact.target)
        {
            impact.display_levels = bombing_level_totals[&impact.target];
        }
    }
    for (_, impact) in grouped {
        let color = impact.weapon.color();
        let massive = impact.weapon.massive();
        if impact.weapon.charge() > 0. {
            let radius = impact.weapon.charge_radius(impact.size);
            painter.ring(impact.origin, radius, color.with_alpha(0.6), impact.delay);
            painter.glow(impact.origin, radius * 0.6, color, impact.delay);
            if massive {
                for i in 0..8 {
                    let angle = i as f32 * TAU / 8.;
                    let offset = Vec3::new(angle.cos(), angle.sin(), 0.) * radius * 0.5;
                    painter.particle(
                        false,
                        Particle {
                            origin: impact.origin + offset,
                            velocity: -offset / impact.delay,
                            start_size: Vec2::splat(impact.size * 0.07),
                            end_size: Vec2::splat(impact.size * 0.12),
                            color,
                            elapsed: 0.,
                            delay: 0.,
                            lifetime: impact.delay,
                            spin: 0.,
                            sustained: false,
                        },
                    );
                }
            }
            if impact.weapon == Weapon::FaunaExtinction {
                for radius in [0.7, 1.1, 1.55] {
                    painter.ring(
                        impact.origin,
                        impact.size * radius,
                        impact.weapon.color().with_alpha(0.7),
                        impact.delay,
                    );
                }
            }
        }
        let weapon = impact.weapon;
        let projectile_image = if matches!(weapon, Weapon::Missile | Weapon::Bomb) {
            textures.missile.clone()
        } else if weapon == Weapon::Repair {
            textures.glow.clone()
        } else {
            textures.beam.clone()
        };
        painter
            .commands
            .spawn((
                Sprite {
                    image: projectile_image.clone(),
                    color: color.with_alpha(0.55),
                    custom_size: Some(Vec2::ONE),
                    ..default()
                },
                Transform::from_translation(impact.origin),
                Visibility::Hidden,
                impact,
                CombatCmp,
                Pickable::IGNORE,
            ))
            .with_children(|parent| {
                // A crisp inner core remains readable over the artwork; the parent's
                // local scale stretches every layer together without more per-frame entities.
                for barrel in 0..weapon.barrels() {
                    let offset = (barrel as f32 - (weapon.barrels() - 1) as f32 * 0.5) * 0.55;
                    parent.spawn((
                        Sprite {
                            image: projectile_image.clone(),
                            color: if weapon == Weapon::Repair {
                                MINT
                            } else {
                                Color::WHITE
                            },
                            custom_size: Some(Vec2::new(
                                0.96,
                                if matches!(weapon, Weapon::Missile | Weapon::Bomb | Weapon::Repair)
                                {
                                    0.5
                                } else {
                                    0.18
                                },
                            )),
                            ..default()
                        },
                        Transform::from_xyz(0., offset, 0.01),
                        Pickable::IGNORE,
                    ));
                    if weapon.barrels() > 1 {
                        parent.spawn((
                            Sprite {
                                image: textures.beam.clone(),
                                color,
                                custom_size: Some(Vec2::new(1., 0.43)),
                                ..default()
                            },
                            Transform::from_xyz(0., offset, 0.005),
                            Pickable::IGNORE,
                        ));
                    }
                }
            });
    }

    let mut hull_hit_sound = false;
    let mut building_destroyed_sound = false;
    let mut shield_hit_sound = false;
    let mut repair_sound = false;
    for (entity, mut impact, mut sprite, mut transform, mut visibility) in &mut pending {
        impact.elapsed += dt;
        if impact.elapsed < impact.delay || dt == 0. {
            continue;
        }
        if !impact.launched {
            impact.launched = true;
            *visibility = Visibility::Inherited;
            if let Some(cue) = impact.weapon.launch_cue() {
                queue_combat_sound(&mut audio, &mut sound_cooldowns, cue);
            }
            if let Some(source) = impact.source {
                if let Ok((_, _, _, _, Some(mut motion))) = units.get_mut(source) {
                    if impact.weapon != Weapon::Repair {
                        motion.impulse += (impact.origin - impact.destination).normalize_or_zero()
                            * impact.size
                            * 0.06;
                    }
                } else if let Ok((_, _, _, _, Some(mut motion))) = individuals.get_mut(source) {
                    if impact.weapon != Weapon::Repair {
                        motion.impulse += (impact.origin - impact.destination).normalize_or_zero()
                            * impact.size
                            * 0.06;
                    }
                }
            }
            if impact.weapon != Weapon::Repair {
                painter.glow(impact.origin, impact.size * 0.35, impact.weapon.color(), 0.18);
                if impact.weapon == Weapon::Bomb {
                    painter.ring(impact.destination, impact.size * 0.9, GOLD.with_alpha(0.5), 0.9);
                }
            } else {
                repair_sound = true;
                painter.beam(
                    impact.origin,
                    impact.destination,
                    impact.size * 0.035,
                    MINT.with_alpha(0.7),
                    0.7,
                );
                painter.ring(impact.destination, impact.size * 1.2, MINT.with_alpha(0.7), 1.1);
            }
        }
        let p = ((impact.elapsed - impact.delay) / impact.weapon.flight()).clamp(0., 1.);
        let sample = impact.flight_path().sample(p);
        let center = sample.center;
        let dimensions = sample.dimensions;
        let direction = sample.direction;
        transform.translation = Vec3::new(center.x, center.y, COMBAT_EXPLOSION_Z + 0.3);
        transform.rotation = Quat::from_rotation_z(direction.y.atan2(direction.x));
        transform.scale = dimensions.extend(1.);
        sprite.color = impact.weapon.color().with_alpha(0.6);
        impact.trail_clock += dt;
        if impact.trail_clock >= 0.035 && p < 1. {
            impact.trail_clock %= 0.035;
            weapon_trail(impact.flight_path(), impact.elapsed, p, |trail| match trail {
                WeaponTrail::Glow(at, size, color, life) => painter.glow(at, size, color, life),
                WeaponTrail::Ring(at, size, color, life) => painter.ring(at, size, color, life),
                WeaponTrail::Beam(from, to, width, color, life) => {
                    painter.beam(from, to, width, color, life)
                },
                WeaponTrail::Sparks(at, size, color, count) => {
                    painter.sparks(at, size, color, count, false)
                },
            });
        }
        if impact.weapon == Weapon::Repair && !impact.readout_shown && p >= REPAIR_READOUT_PROGRESS
        {
            impact.readout_shown = true;
            let origin = impact.destination.truncate().extend(COMBAT_EXPLOSION_Z + 0.5)
                + Vec3::Y * impact.size * 0.7;
            painter.commands.spawn((
                Text2d::new(format!("+{} HULL", impact.hull)),
                TextFont {
                    font_size: (impact.size * 0.15).into(),
                    ..default()
                },
                TextColor(MINT),
                Transform::from_translation(origin),
                CombatReadout {
                    age: 0.,
                    origin,
                    size: impact.size,
                    color: MINT,
                },
                CombatCmp,
                Pickable::IGNORE,
            ));
        }
        if p < 1. {
            continue;
        }
        if let Some(width) = impact.weapon.beam_width() {
            painter.beam(
                impact.origin,
                impact.destination,
                impact.size * width,
                impact.weapon.color().with_alpha(0.7),
                0.22,
            );
            for barrel in 0..impact.weapon.barrels() {
                let d = (impact.destination - impact.origin).normalize_or_zero();
                let normal = Vec3::new(-d.y, d.x, 0.);
                let offset = normal
                    * (barrel as f32 - (impact.weapon.barrels() - 1) as f32 * 0.5)
                    * impact.size
                    * width
                    * 0.55;
                painter.beam(
                    impact.origin + offset,
                    impact.destination + offset,
                    impact.size * width * 0.18,
                    Color::WHITE.with_alpha(0.85),
                    0.18,
                );
            }
        }
        painter.commands.entity(entity).despawn();
        if impact.weapon == Weapon::Repair {
            if let Ok((_, _, _, mut group, _)) = units.get_mut(impact.group_target) {
                group.hull = group.hull.saturating_add(impact.hull).min(group.max_hull);
            }
            for outcome in &impact.individual_outcomes {
                if let Ok((_, _, _, mut individual, _)) = individuals.get_mut(outcome.target) {
                    individual.hull =
                        individual.hull.saturating_add(outcome.hull).min(individual.max_hull);
                }
            }
            painter.ring(impact.destination, impact.size * 0.85, MINT.with_alpha(0.6), 0.5);
            continue;
        }
        if impact.missed {
            // A bright, quick fly-by distinguishes a clean miss from both launch and impact.
            queue_combat_sound(&mut audio, &mut sound_cooldowns, miss_cue());
            if impact.weapon == Weapon::Bomb {
                painter.ring(impact.destination, impact.size * 0.55, GOLD.with_alpha(0.42), 0.45);
                painter.sparks(impact.destination, impact.size * 0.35, GOLD, 5, false);
            }
            let mut on_cooldown = false;
            if let Ok((_, _, _, _, Some(mut motion))) = units.get_mut(impact.target) {
                on_cooldown = motion.miss_cooldown > 0.;
                if !on_cooldown {
                    motion.miss_flash = 0.4;
                    motion.miss_cooldown = 1.15;
                }
            } else if let Ok((_, _, _, _, Some(mut motion))) = individuals.get_mut(impact.target) {
                on_cooldown = motion.miss_cooldown > 0.;
                if !on_cooldown {
                    motion.miss_flash = 0.4;
                    motion.miss_cooldown = 1.15;
                }
            }
            if on_cooldown {
                continue;
            }
            let center = impact.destination.truncate().extend(COMBAT_EXPLOSION_Z + 0.5);
            let color = Color::srgb(0.9, 0.95, 1.0);
            for offset in [-0.12, 0.12] {
                painter.beam(
                    center + Vec3::new(-0.3, offset - 0.08, 0.) * impact.size,
                    center + Vec3::new(0.3, offset + 0.08, 0.) * impact.size,
                    impact.size * 0.018,
                    color.with_alpha(0.7),
                    0.35,
                );
            }
            let origin = center - Vec3::Y * impact.size * 0.32;
            painter.commands.spawn((
                Text2d::new("MISS"),
                TextFont {
                    // Rasterize well above the final display size and scale down in world
                    // space. Small directly-rasterized glyphs look blocky over moving cards.
                    font_size: (impact.size * 0.15 / COMBAT_READOUT_RASTER_SCALE).into(),
                    ..default()
                },
                TextColor(color),
                Transform::from_translation(origin)
                    .with_scale(Vec3::splat(COMBAT_READOUT_RASTER_SCALE)),
                CombatReadout {
                    age: 0.,
                    origin,
                    size: impact.size,
                    color,
                },
                CombatCmp,
                Pickable::IGNORE,
            ));
            continue;
        }
        if matches!(
            impact.weapon,
            Weapon::Solar
                | Weapon::Siege
                | Weapon::FaunaGravity
                | Weapon::FaunaFire
                | Weapon::FaunaExtinction
        ) {
            painter.ring(
                impact.destination,
                impact.size * 2.0,
                impact.weapon.color().with_alpha(0.75),
                0.55,
            );
            painter.glow(impact.destination, impact.size * 1.25, impact.weapon.color(), 0.3);
        }
        if impact.weapon == Weapon::FaunaExtinction {
            painter.blast(impact.destination, impact.size * 1.9, 0.8);
            painter.ring(
                impact.destination,
                impact.size * 3.4,
                impact.weapon.color().with_alpha(0.9),
                0.95,
            );
            painter.ring(
                impact.destination,
                impact.size * 2.45,
                Color::WHITE.with_alpha(0.8),
                0.62,
            );
            painter.sparks(impact.destination, impact.size * 1.8, impact.weapon.color(), 24, true);
        }
        let mut group_unit = None;
        let mut visual_shield_before = 0;
        let mut visual_shield_after = 0;
        if let Ok((_, _, _, mut group, _)) = units.get_mut(impact.group_target) {
            group_unit = Some(group.unit);
            if impact.target == impact.group_target {
                visual_shield_before = group.shield;
            }
            if group.unit == Unit::planetary_shield() {
                group.shield = group.shield.saturating_sub(impact.planetary);
            } else if group.unit.is_building() {
                group.hull = group.hull.saturating_sub(impact.levels);
            } else {
                group.shield = group.shield.saturating_sub(impact.shield);
                group.hull = group.hull.saturating_sub(impact.hull);
            }
            if impact.target == impact.group_target {
                visual_shield_after = group.shield;
            }
        }
        for outcome in &impact.individual_outcomes {
            if let Ok((_, _, _, mut individual, _)) = individuals.get_mut(outcome.target) {
                if impact.target == outcome.target {
                    visual_shield_before = individual.shield;
                }
                individual.shield = individual.shield.saturating_sub(outcome.shield);
                individual.hull = individual.hull.saturating_sub(outcome.hull);
                if impact.target == outcome.target {
                    visual_shield_after = individual.shield;
                }
            }
        }
        let hull_was_hit = impact.hull > 0 || impact.levels > 0;
        if visual_shield_before > visual_shield_after {
            // A penetrating hit reads as a hull impact; do not layer the shield cue over it.
            shield_hit_sound |= !hull_was_hit;
            painter.ring(impact.destination, impact.size * 1.15, ICE.with_alpha(0.8), 0.4);
            if visual_shield_after == 0 {
                painter.ring(impact.destination, impact.size * 1.8, ICE, 0.65);
                painter.sparks(impact.destination, impact.size, ICE, 16, true);
                if group_unit == Some(Unit::planetary_shield()) {
                    // The shield entity is centered on its long health bar, while the impact
                    // destination follows the image child. Keep the entire destruction sequence
                    // on the visible shield installation rather than exploding empty bar space.
                    painter.commands.entity(impact.group_target).insert(Wreck::new(
                        impact.destination,
                        impact.size,
                        Unit::planetary_shield(),
                    ));
                }
            }
        }
        if hull_was_hit {
            if impact.weapon == Weapon::Bomb && impact.levels > 0 {
                building_destroyed_sound = true;
            } else {
                hull_hit_sound = true;
            }
            painter.blast(impact.destination, impact.size * 0.65, 0.42);
            painter.glow(impact.destination, impact.size * 0.62, GOLD, 0.24);
            painter.glow(impact.destination, impact.size * 0.28, Color::WHITE, 0.1);
            painter.sparks(impact.destination, impact.size * 0.65, GOLD, 7, false);
            if let Ok((_, _, _, _, Some(mut motion))) = units.get_mut(impact.target) {
                motion.flash = 0.18;
            } else if let Ok((_, _, _, _, Some(mut motion))) = individuals.get_mut(impact.target) {
                motion.flash = 0.18;
            }
            if impact.weapon == Weapon::Bomb && impact.levels > 0 {
                painter.blast(impact.destination, impact.size * 1.4, 0.7);
                painter.ring(impact.destination, impact.size * 1.7, GOLD.with_alpha(0.6), 0.7);
                painter.sparks(impact.destination, impact.size * 1.4, GOLD, 18, true);
                if impact.display_levels > 0 {
                    let origin = impact.destination.truncate().extend(COMBAT_EXPLOSION_Z + 0.5)
                        + Vec3::Y * impact.size * 0.65;
                    painter.commands.spawn((
                        Text2d::new(format!(
                            "-{} {}",
                            impact.display_levels,
                            if impact.display_levels == 1 {
                                "LEVEL"
                            } else {
                                "LEVELS"
                            }
                        )),
                        TextFont {
                            font_size: (impact.size * 0.17).into(),
                            ..default()
                        },
                        TextColor(GOLD),
                        Transform::from_translation(origin),
                        CombatReadout {
                            age: 0.,
                            origin,
                            size: impact.size,
                            color: GOLD,
                        },
                        CombatCmp,
                        Pickable::IGNORE,
                    ));
                }
            }
        }
    }
    if hull_hit_sound {
        queue_combat_sound(&mut audio, &mut sound_cooldowns, hull_impact_cue());
    }
    if building_destroyed_sound {
        queue_combat_sound(&mut audio, &mut sound_cooldowns, PlayAudioMsg::new("large explosion"));
    }
    if shield_hit_sound {
        queue_combat_sound(&mut audio, &mut sound_cooldowns, PlayAudioMsg::new("shield impact"));
    }
    if repair_sound {
        queue_combat_sound(&mut audio, &mut sound_cooldowns, PlayAudioMsg::new("repair"));
    }

    // Existing card tweens control scale. Add/remove only our translation offset,
    // so firing recoil cannot accumulate drift or overwrite their animation.
    for (entity, mut sprite, mut transform, cu, motion) in &mut units {
        let Some(mut motion) = motion else {
            painter.commands.entity(entity).insert(UnitMotion {
                base_color: sprite.color,
                ..default()
            });
            continue;
        };
        transform.translation -= motion.offset;
        motion.impulse = motion
            .impulse
            .clamp_length_max(sprite.custom_size.unwrap_or(Vec2::splat(120.)).x * 0.18);
        motion.impulse *= (-dt * 9.).exp();
        motion.offset = motion.impulse;
        transform.translation += motion.offset;
        motion.flash = (motion.flash - dt).max(0.);
        motion.miss_flash = (motion.miss_flash - dt).max(0.);
        motion.miss_cooldown = (motion.miss_cooldown - dt).max(0.);
        let damage = 1. - cu.hull as f32 / cu.max_hull.max(1) as f32;
        let tint = 1. - damage * 0.25;
        let base = motion.base_color.to_srgba();
        // The planetary shield's feedback belongs on its image and its blue depletion layers.
        // Never tint the long bar for misses, hits, or the destruction sequence.
        sprite.color = if cu.unit == Unit::planetary_shield() {
            motion.base_color
        } else if motion.flash > 0. {
            Color::srgb(1.5, 1.25, 1.1)
        } else {
            let shimmer = (motion.miss_flash / 0.4 * std::f32::consts::PI).sin() * 0.4;
            Color::srgba(
                base.red * tint + shimmer,
                base.green * tint + shimmer,
                base.blue * tint + shimmer,
                base.alpha,
            )
        };
        if individual_mode || dt == 0. || cu.hull == 0 || damage < 0.2 {
            continue;
        }
        motion.sparks += dt;
        if motion.sparks > 0.65 + (1. - damage) * 1.2 {
            motion.sparks = 0.;
            let size = sprite.custom_size.unwrap_or(Vec2::splat(120.)).x;
            let origin = transform.translation + Vec3::new(size * 0.22, -size * 0.12, 0.);
            painter.sparks(origin, size * 0.3, GOLD.with_alpha(0.65), 3, false);
        }
    }

    for (entity, mut sprite, mut transform, cu, motion) in &mut individuals {
        let Some(mut motion) = motion else {
            painter.commands.entity(entity).insert(UnitMotion {
                base_color: sprite.color,
                ..default()
            });
            continue;
        };
        transform.translation -= motion.offset;
        motion.impulse = motion
            .impulse
            .clamp_length_max(sprite.custom_size.unwrap_or(Vec2::splat(120.)).x * 0.18);
        motion.impulse *= (-dt * 9.).exp();
        motion.offset = motion.impulse;
        transform.translation += motion.offset;
        motion.flash = (motion.flash - dt).max(0.);
        motion.miss_flash = (motion.miss_flash - dt).max(0.);
        motion.miss_cooldown = (motion.miss_cooldown - dt).max(0.);
        let damage = 1. - cu.hull as f32 / cu.max_hull.max(1) as f32;
        let tint = 1. - damage * 0.25;
        let base = motion.base_color.to_srgba();
        sprite.color = if motion.flash > 0. {
            Color::srgb(1.5, 1.25, 1.1)
        } else {
            let shimmer = (motion.miss_flash / 0.4 * std::f32::consts::PI).sin() * 0.4;
            Color::srgba(
                base.red * tint + shimmer,
                base.green * tint + shimmer,
                base.blue * tint + shimmer,
                base.alpha,
            )
        };
        if !individual_mode || dt == 0. || cu.hull == 0 || damage < 0.2 {
            continue;
        }
        motion.sparks += dt;
        if motion.sparks > 0.65 + (1. - damage) * 1.2 {
            motion.sparks = 0.;
            let size = sprite.custom_size.unwrap_or(Vec2::splat(120.)).x;
            let origin = transform.translation + Vec3::new(size * 0.22, -size * 0.12, 0.);
            painter.sparks(origin, size * 0.3, GOLD.with_alpha(0.65), 3, false);
        }
    }

    for (entity, mut wreck) in &mut wrecks {
        wreck.elapsed += dt;
        if dt == 0. {
            continue;
        }
        let heavy = if wreck.heavy {
            1.5
        } else {
            1.
        };
        let effect_tail_started = wreck.stage == WRECK_STAGES.len();
        if let Ok((_, _, _, _, Some(mut motion))) = units.get_mut(entity) {
            if wreck.stage == 0 && wreck.unit != Unit::planetary_shield() {
                motion.flash = 0.2;
            }
        } else if let Ok((_, _, _, _, Some(mut motion))) = individuals.get_mut(entity) {
            if wreck.stage == 0 {
                motion.flash = 0.2;
            }
        }
        while wreck.stage < WRECK_STAGES.len() && wreck.elapsed >= WRECK_STAGES[wreck.stage] * heavy
        {
            let i = wreck.stage;
            let planetary_shield = wreck.unit == Unit::planetary_shield();
            wreck_stage(wreck.origin, wreck.size, wreck.unit, i, |mask, particle| {
                if matches!(mask, WreckMask::Blast) {
                    painter.blast_particle(particle);
                } else {
                    painter.particle(matches!(mask, WreckMask::Ring), particle);
                }
            });
            if i < 3 {
                if wreck.audible && planetary_shield && matches!(i, 0 | 2) {
                    audio.write(PlayAudioMsg::new("large explosion").rate(if i == 0 {
                        1.05
                    } else {
                        0.9
                    }));
                }
            } else if wreck.audible {
                audio.write(wreck_cue(wreck.unit).1);
            }
            wreck.stage += 1;
        }
        if !wreck.card_hidden && wreck.elapsed > WRECK_CARD_LIFETIME * heavy {
            wreck.card_hidden = true;
            painter.commands.entity(entity).insert(Visibility::Hidden);
        }
        // Start this clock on the update after the final stage is emitted. That guarantees even
        // a low-frame-rate or fast-forwarded blast remains a blocking effect for its full tail.
        if effect_tail_started {
            wreck.tail_elapsed += dt;
        }
        if wreck.tail_elapsed > WRECK_EFFECT_TAIL {
            painter.commands.entity(entity).despawn();
        }
    }

    for mut ray in &mut cinematics {
        ray.elapsed += dt;
        if dt == 0. {
            continue;
        }
        let focus = ray.focus();
        let combined_size = ray.combined_beam_size();
        if ray.stage == 0 {
            ray.stage = 1;
            painter.commands.spawn((
                Sprite {
                    color: Color::BLACK.with_alpha(0.),
                    custom_size: Some(ray.viewport),
                    ..default()
                },
                Transform::from_xyz(ray.target.x, ray.target.y, COMBAT_BACKGROUND_Z + 0.5),
                Particle {
                    origin: Vec3::new(ray.target.x, ray.target.y, COMBAT_BACKGROUND_Z + 0.5),
                    velocity: Vec3::ZERO,
                    start_size: ray.viewport,
                    end_size: ray.viewport,
                    color: Color::BLACK.with_alpha(0.8),
                    elapsed: 0.,
                    delay: 0.,
                    lifetime: DEATH_RAY_DURATION,
                    spin: 0.,
                    sustained: false,
                },
                CombatCmp,
                Pickable::IGNORE,
            ));
            for (source_index, origin) in ray.origins.iter().copied().enumerate() {
                for i in 0..8 {
                    let angle = i as f32 * TAU / 8. + source_index as f32 * 0.41;
                    let offset = Vec3::new(angle.cos(), angle.sin(), 0.) * ray.size * 0.72;
                    painter.particle(
                        false,
                        Particle {
                            origin: origin + offset,
                            velocity: -offset / 1.65,
                            start_size: Vec2::splat(ray.size * 0.035),
                            end_size: Vec2::splat(ray.size * 0.13),
                            color: GOLD,
                            elapsed: 0.,
                            delay: 0.,
                            lifetime: 1.65,
                            spin: 0.,
                            sustained: false,
                        },
                    );
                }
                for radius in [0.7, 1.1] {
                    painter.particle(
                        true,
                        Particle {
                            origin,
                            velocity: Vec3::ZERO,
                            start_size: Vec2::splat(ray.size * radius),
                            end_size: Vec2::splat(ray.size * 0.12),
                            color: GOLD.with_alpha(0.6),
                            elapsed: 0.,
                            delay: 0.,
                            lifetime: 1.9,
                            spin: radius,
                            sustained: false,
                        },
                    );
                }
                painter.glow(origin, ray.size * 0.85, GOLD, 2.1);
            }
        }
        if ray.stage == 1 && ray.elapsed >= DEATH_RAY_FOCUS_AT {
            ray.stage = 2;
            for origin in ray.origins.iter().copied() {
                painter.convergence_beam(origin, focus, ray.size * 0.11, GOLD, 1.7);
                painter.convergence_beam(origin, focus, ray.size * 0.035, Color::WHITE, 1.7);
            }
            painter.glow(focus, combined_size * 1.05, Color::WHITE, 1.4);
            painter.ring(focus, combined_size * 1.9, GOLD, 1.2);
        }
        if ray.stage == 2 && ray.elapsed >= DEATH_RAY_DISCHARGE_AT {
            ray.stage = 3;
            // A broad heated envelope, dense energy column and white-hot core make
            // the discharge read as one sustained weapon instead of a quick tracer.
            for (width, color) in death_ray_beam_layers(combined_size) {
                painter.sustained_beam(focus, ray.target, width, color, 1.72);
            }

            // The surface absorbs several visible pulses before it gives way.
            painter.glow(ray.target, ray.size * 3.4, GOLD.with_alpha(0.8), 1.5);
            painter.glow(ray.target, ray.size * 1.65, Color::WHITE, 0.72);
            painter.ring(ray.target, ray.size * 2.7, GOLD.with_alpha(0.75), 0.75);
            painter.ring_after(
                ray.target,
                ray.size * 3.4,
                Color::srgb(1., 0.75, 0.28).with_alpha(0.55),
                0.75,
                0.2,
            );
            painter.glow_after(ray.target, ray.size * 2.3, Color::WHITE, 0.55, 0.38);
            painter.blast_after(ray.target, ray.size * 2.2, 0.7, 0.12);
            painter.sparks(ray.target, ray.size * 3.2, Color::WHITE, 42, false);
        }
        if ray.stage == 3 && ray.elapsed >= 2.72 {
            ray.stage = 4;
            if ray.destroys_planet {
                // Deterministic uneven paths grow a segment at a time. Offshoots
                // break the old wheel-spoke silhouette while keeping replay stable.
                let angles = [0.18_f32, 1.08, 2.46, 3.72, 5.04];
                for (path, base_angle) in angles.into_iter().enumerate() {
                    let direction = Vec3::new(base_angle.cos(), base_angle.sin(), 0.);
                    let mut point =
                        ray.target + direction * ray.size * (0.13 + path as f32 * 0.012);
                    let mut angle = base_angle;
                    for segment in 0..4 {
                        let bend_sign = if (path + segment) % 2 == 0 {
                            1.
                        } else {
                            -1.
                        };
                        let bend =
                            bend_sign * (0.1 + ((path * 3 + segment * 2) % 4) as f32 * 0.045);
                        angle += bend;
                        let length =
                            ray.size * (0.24 + segment as f32 * 0.065 + (path % 3) as f32 * 0.025);
                        let next = point + Vec3::new(angle.cos(), angle.sin(), 0.) * length;
                        let delay = segment as f32 * 0.075 + (path % 2) as f32 * 0.025;
                        painter.fissure(
                            point,
                            next,
                            ray.size * (0.058 - segment as f32 * 0.008),
                            Color::srgb(1., 0.34, 0.06),
                            0.82,
                            delay,
                        );
                        painter.fissure(
                            point,
                            next,
                            ray.size * (0.018 - segment as f32 * 0.002),
                            Color::srgb(1., 0.88, 0.42),
                            0.82,
                            delay + 0.018,
                        );
                        if segment == 1 || (segment == 2 && path % 2 == 0) {
                            let branch_angle = angle
                                + if (path + segment) % 2 == 0 {
                                    0.78
                                } else {
                                    -0.7
                                };
                            let branch_end = next
                                + Vec3::new(branch_angle.cos(), branch_angle.sin(), 0.)
                                    * ray.size
                                    * (0.22 + segment as f32 * 0.07);
                            painter.fissure(
                                next,
                                branch_end,
                                ray.size * 0.032,
                                Color::srgb(1., 0.52, 0.12),
                                0.68,
                                delay + 0.08,
                            );
                        }
                        point = next;
                    }
                }
            }
        }
        if ray.stage == 4 && ray.elapsed >= DEATH_RAY_COLLAPSE_AT {
            ray.stage = 5;
            if !ray.destroys_planet {
                painter.ring(ray.target, ray.size * 4., GOLD, 1.3);
                painter.glow(ray.target, ray.size * 3., GOLD.with_alpha(0.6), 1.0);
                continue;
            }
            // The backdrop changes in the same frame as an opaque flash. It stays
            // covered while the overlapping atlas explosions spread across the screen.
            let flash_origin = Vec3::new(ray.target.x, ray.target.y, COMBAT_EXPLOSION_Z + 0.12);
            let flash_size = ray.viewport * 1.08;
            painter.commands.spawn((
                Sprite {
                    color: Color::WHITE,
                    custom_size: Some(flash_size),
                    ..default()
                },
                Transform::from_translation(flash_origin),
                Particle {
                    origin: flash_origin,
                    velocity: Vec3::ZERO,
                    start_size: flash_size,
                    end_size: flash_size,
                    color: Color::WHITE,
                    elapsed: 0.,
                    delay: 0.,
                    lifetime: 1.0,
                    spin: 0.,
                    sustained: false,
                },
                PlanetFlash,
                CombatCmp,
                Pickable::IGNORE,
            ));
            if let Some(art) = painter.art {
                for mut backdrop in &mut backdrops {
                    backdrop.image = art.image("destroyed bg");
                }
            }
            let extent = ray.viewport.max_element();
            painter.blast(ray.target, extent * 0.72, 1.85);
            for i in 0..14 {
                let angle = i as f32 * 2.399_963;
                let radius = ray.viewport.min_element() * (0.08 + (i % 5) as f32 * 0.075);
                let center = ray.target + Vec3::new(angle.cos(), angle.sin(), 0.) * radius;
                painter.blast_after(
                    center,
                    extent * (0.28 + (i % 4) as f32 * 0.045),
                    1.55,
                    0.12 + (i % 5) as f32 * 0.075,
                );
            }
            painter.glow(ray.target, ray.size * 9., Color::WHITE, 0.3);
            painter.glow(ray.target, ray.size * 12., GOLD.with_alpha(0.85), 1.55);
            // One subdued amber pressure front replaces the cool concentric rings.
            painter.ring(
                ray.target,
                extent * 1.45,
                Color::srgb(1., 0.42, 0.08).with_alpha(0.32),
                1.75,
            );
            painter.sparks(ray.target, ray.size * 7., GOLD, 96, true);
            for (entity, sprite, transform, cu, _) in &units {
                if cu.side == Side::Defender {
                    painter.commands.entity(entity).insert(Wreck::new(
                        transform.translation,
                        sprite.custom_size.unwrap_or(Vec2::splat(ray.size)).x,
                        cu.unit,
                    ));
                }
            }
        }
        let boom_times = [3.7, 3.92, 4.15, 4.4, 4.65];
        while ray.destroys_planet
            && ray.boom_stage < boom_times.len()
            && ray.elapsed >= boom_times[ray.boom_stage]
        {
            let cue = if ray.boom_stage % 2 == 0 {
                "large explosion"
            } else {
                "explosion"
            };
            queue_combat_sound(&mut audio, &mut sound_cooldowns, PlayAudioMsg::new(cue));
            ray.boom_stage += 1;
        }
    }

    for (entity, mut label, mut color, mut transform) in &mut readouts {
        label.age += dt;
        if label.age >= 1.15 {
            painter.commands.entity(entity).despawn();
            continue;
        }
        transform.translation = label.origin + Vec3::Y * label.size * 0.14 * label.age;
        color.0 = label.color.with_alpha((1.15 - label.age).min(0.4) / 0.4);
    }

    for (entity, mut particle, mut sprite, mut transform, frames, planet_flash) in &mut particles {
        particle.elapsed += dt;
        if particle.elapsed < particle.delay {
            continue;
        }
        let Some(sample) = particle.sample(particle.elapsed) else {
            painter.commands.entity(entity).despawn();
            continue;
        };
        transform.translation = sample.center;
        // Delayed debris only spins for the active fraction of the first frame.
        transform.rotate_z(particle.spin * dt.min(particle.elapsed - particle.delay));
        sprite.custom_size = Some(sample.size);
        sprite.color = if planet_flash.is_some() {
            particle
                .color
                .with_alpha(particle.color.alpha() * ((1. - sample.progress) / 0.6).min(1.))
        } else {
            sample.color
        };
        if let (Some(frames), Some(atlas)) = (frames, sprite.texture_atlas.as_mut()) {
            atlas.index = (sample.progress * frames.0 as f32) as usize;
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/core/combat_effects.rs"]
mod tests;
