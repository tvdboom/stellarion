//! Client-only battle aftermath on the strategic map, derived from visible turn reports.

use std::collections::{BTreeMap, BTreeSet};

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

use super::icon::Icon;
use super::model::{Map, MapCmp};
use super::planet::{Planet, PlanetId};
use super::systems::draw_map;
use crate::core::assets::WorldAssets;
use crate::core::audio::PlayAudioMsg;
use crate::core::combat::report::{MissionReport, ReportId};
use crate::core::constants::EXPLOSION_Z;
use crate::core::loading::{refresh_gameplay_projection, refresh_turn_draft};
use crate::core::mission_systems::SPY_MISSION_SIZE;
use crate::core::missions::{MissionId, Missions, SuppressedReturningSpies};
use crate::core::player::Player;
use crate::core::settings::Settings;
use crate::core::states::{AppState, GameState};

const AFTERMATH_SECONDS: f32 = 4.2;
const EXPLOSION_SECONDS: f32 = 1.55;
const RIPPLE_COUNT: usize = 4;
const RIPPLE_INTERVAL_SECONDS: f32 = 0.38;
const RIPPLE_SECONDS: f32 = 1.35;
const SPY_ARRIVAL_SECONDS: f32 = 0.9;
const SPY_SCAN_START_SECONDS: f32 = 0.78;
const SPY_SCAN_ARC_COUNT: usize = 3;
const SPY_SCAN_ARC_INTERVAL_SECONDS: f32 = 0.28;
const SPY_SCAN_ARC_SECONDS: f32 = 0.62;
const SPY_PLANET_WAVE_DELAY_SECONDS: f32 = SPY_SCAN_ARC_SECONDS;
const SPY_PLANET_WAVE_SECONDS: f32 = 0.92;
const SPY_INTERCEPT_START_SECONDS: f32 = 1.24;
const SPY_INTERCEPT_SECONDS: f32 = 0.72;
const SPY_DESTROY_SECONDS: f32 = SPY_INTERCEPT_START_SECONDS + SPY_INTERCEPT_SECONDS;
const SPY_EXPLOSION_SECONDS: f32 = 0.78;
const SPY_DEPART_START_SECONDS: f32 = 2.32;
const SPY_DEPART_SECONDS: f32 = 0.72;
const SPY_ICON_ROTATION: f32 = -std::f32::consts::FRAC_PI_4;
const CONQUEST_APPROACH_SECONDS: f32 = 0.86;
const CONQUEST_LANDING_SECONDS: f32 = 0.44;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Outcome {
    Victory,
    Defeat,
    Draw,
    Mixed,
}

impl Outcome {
    fn from_report(report: &MissionReport, player: &Player) -> Option<Self> {
        // Spy and missile reports have their own success rules, not fleet victory/defeat.
        // Territory headlines take precedence whenever this player's ownership or
        // control changes. Colonization has its own presentation system; captures
        // and losses are handled by `TerritoryOutcome` below.
        let ownership_changed = (report.planet.owned == Some(player.id))
            != (report.destination_owned == Some(player.id));
        let control_changed = (report.planet.controlled == Some(player.id))
            != (report.destination_controlled == Some(player.id));
        if report.hidden
            || report.planet_destroyed
            || report.combat_report.is_none()
            || !matches!(report.mission.objective, Icon::Attack | Icon::Colonize | Icon::Destroy)
            || ownership_changed
            || control_changed
            || !(report.mission.owner == player.id
                || report.planet.owned == Some(player.id)
                || report.planet.controlled == Some(player.id))
        {
            return None;
        }
        Some(match report.winner() {
            Some(id) if id == player.id => Self::Victory,
            Some(_) => Self::Defeat,
            None => Self::Draw,
        })
    }

    fn label(self) -> &'static str {
        match self {
            Self::Victory => "BATTLE WON",
            Self::Defeat => "BATTLE LOST",
            Self::Draw => "BATTLE DRAW",
            Self::Mixed => "MIXED BATTLE RESULTS",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TerritoryOutcome {
    Conquered,
    Lost,
}

impl TerritoryOutcome {
    fn from_report(report: &MissionReport, player: &Player) -> Option<Self> {
        if report.hidden || report.planet_destroyed {
            return None;
        }
        let controlled_before =
            report.planet.owned == Some(player.id) || report.planet.controlled == Some(player.id);
        let controlled_after = report.destination_owned == Some(player.id)
            || report.destination_controlled == Some(player.id);

        if controlled_before && !controlled_after {
            Some(Self::Lost)
        } else if !controlled_before
            && controlled_after
            && report.mission.owner == player.id
            && report.mission.objective != Icon::Colonize
        {
            // Colonize outcomes use the richer colony expansion effect instead.
            Some(Self::Conquered)
        } else {
            None
        }
    }

    fn label(self, is_moon: bool) -> &'static str {
        match (self, is_moon) {
            (Self::Conquered, false) => "PLANET CONQUERED",
            (Self::Conquered, true) => "MOON CONQUERED",
            (Self::Lost, false) => "PLANET LOST",
            (Self::Lost, true) => "MOON LOST",
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum MissionArrivalImage {
    Fleet,
    Colony,
    Destroy,
}

impl MissionArrivalImage {
    fn from_report(report: &MissionReport, player: &Player) -> Self {
        match report.mission.image(player) {
            "mission colonize" => Self::Colony,
            "mission destroy" | "mission destroy jump" => Self::Destroy,
            _ => Self::Fleet,
        }
    }

    fn key(self) -> &'static str {
        match self {
            Self::Fleet => "mission",
            Self::Colony => "mission colonize",
            Self::Destroy => "mission destroy",
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct TerritoryArrival {
    direction: Vec2,
    image: MissionArrivalImage,
}

impl TerritoryArrival {
    fn from_report(report: &MissionReport, player: &Player, map: &Map) -> Option<Self> {
        if TerritoryOutcome::from_report(report, player)? != TerritoryOutcome::Conquered {
            return None;
        }
        let origin = map.try_get(report.mission.origin)?;
        let destination = map.try_get(report.mission.destination)?;
        let direction = (destination.position - origin.position).normalize_or_zero();
        Some(Self {
            direction: if direction == Vec2::ZERO {
                Vec2::X
            } else {
                direction
            },
            image: MissionArrivalImage::from_report(report, player),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MissileOutcome {
    Strike,
    Impact,
}

impl MissileOutcome {
    fn from_report(report: &MissionReport, player: &Player) -> Option<Self> {
        if report.hidden
            || report.mission.objective != Icon::MissileStrike
            || !(report.mission.owner == player.id
                || report.planet.owned == Some(player.id)
                || report.planet.controlled == Some(player.id))
        {
            return None;
        }
        Some(if report.mission.owner == player.id {
            Self::Strike
        } else {
            Self::Impact
        })
    }

    fn label(self) -> &'static str {
        match self {
            Self::Strike => "MISSILE STRIKE",
            Self::Impact => "MISSILE IMPACT",
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct MissilePresentation {
    outcome: MissileOutcome,
    direction: Vec2,
}

impl MissilePresentation {
    fn from_report(report: &MissionReport, player: &Player, map: &Map) -> Option<Self> {
        let outcome = MissileOutcome::from_report(report, player)?;
        let origin = map.try_get(report.mission.origin)?;
        let destination = map.try_get(report.mission.destination)?;
        let direction = (destination.position - origin.position).normalize_or_zero();
        Some(Self {
            outcome,
            // Degenerate report geometry uses a stable left-to-right direction.
            direction: if direction == Vec2::ZERO {
                Vec2::X
            } else {
                direction
            },
        })
    }

    fn label(self) -> &'static str {
        self.outcome.label()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SpyOutcome {
    Success,
    Failed,
    Detected,
    Mixed,
}

impl SpyOutcome {
    fn from_report(report: &MissionReport, player: &Player) -> Option<Self> {
        if report.hidden
            || report.mission.objective != Icon::Spy
            || !(report.mission.owner == player.id
                || report.planet.owned == Some(player.id)
                || report.planet.controlled == Some(player.id))
        {
            return None;
        }
        Some(if report.mission.owner != player.id {
            Self::Detected
        } else if report.scout_probes > 0 {
            Self::Success
        } else {
            Self::Failed
        })
    }

    fn label(self) -> &'static str {
        match self {
            Self::Success => "SPY MISSION SUCCESSFUL",
            Self::Failed => "SPY MISSION FAILED",
            Self::Detected => "ENEMY PROBES DETECTED",
            Self::Mixed => "SPY MISSIONS RESOLVED",
        }
    }

    fn merge(self, other: Self) -> Self {
        if self == other {
            self
        } else {
            Self::Mixed
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct SpyPresentation {
    outcome: SpyOutcome,
    direction: Vec2,
    intercepted: bool,
}

impl SpyPresentation {
    fn from_report(report: &MissionReport, player: &Player, map: &Map) -> Option<Self> {
        let outcome = SpyOutcome::from_report(report, player)?;
        let direction = if report.mission.owner == player.id {
            let origin = map.try_get(report.mission.origin)?;
            let destination = map.try_get(report.mission.destination)?;
            (destination.position - origin.position).normalize_or_zero()
        } else {
            // Detection must not reveal the hidden origin of an enemy spy mission.
            Vec2::X
        };
        Some(Self {
            outcome,
            direction: if direction == Vec2::ZERO {
                Vec2::X
            } else {
                direction
            },
            // Both players derive the physical result from the same survivor count: a
            // defender may detect a probe that still completes its scan and escapes.
            intercepted: report.scout_probes == 0,
        })
    }

    fn label(self) -> &'static str {
        self.outcome.label()
    }

    fn merge(self, other: Self) -> Self {
        Self {
            outcome: self.outcome.merge(other.outcome),
            // One site gets one combined effect; retain its first deterministic approach vector.
            direction: self.direction,
            // A combined marker still depicts an interception if any represented probe was lost.
            intercepted: self.intercepted || other.intercepted,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct SiteOutcome {
    planet_destroyed: bool,
    battle: Option<Outcome>,
    territory: Option<TerritoryOutcome>,
    territory_arrival: Option<TerritoryArrival>,
    missile: Option<MissilePresentation>,
    spy: Option<SpyPresentation>,
}

impl SiteOutcome {
    fn labels(self, planet: &Planet) -> Vec<&'static str> {
        let mut labels = Vec::with_capacity(4);
        if self.planet_destroyed {
            labels.push(if planet.is_moon() {
                "MOON DESTROYED"
            } else {
                "PLANET DESTROYED"
            });
        } else if let Some(territory) = self.territory {
            labels.push(territory.label(planet.is_moon()));
        } else if let Some(battle) = self.battle {
            labels.push(battle.label());
        }
        if let Some(missile) = self.missile {
            labels.push(missile.label());
        }
        if let Some(spy) = self.spy {
            labels.push(spy.label());
        }
        labels
    }

    fn has_impact(self) -> bool {
        self.planet_destroyed
            || self.battle.is_some()
            || self.territory.is_some()
            || self.missile.is_some()
    }

    fn has_audible_impact(self, planet: &Planet) -> bool {
        (self.has_impact() && !planet.is_destroyed) || self.spy.is_some_and(|spy| spy.intercepted)
    }
}

fn planet_destruction_visible(report: &MissionReport, player: &Player) -> bool {
    !report.hidden
        && report.planet_destroyed
        && report.mission.objective == Icon::Destroy
        && (report.mission.owner == player.id
            || report.planet.owned == Some(player.id)
            || report.planet.controlled == Some(player.id))
}

#[derive(Resource, Default)]
struct BattleSites {
    turn: usize,
    observed: BTreeSet<ReportId>,
    outcomes: BTreeMap<PlanetId, SiteOutcome>,
    pending: BTreeSet<PlanetId>,
}

impl BattleSites {
    fn observe(&mut self, player: &Player, map: &Map, turn: usize) -> bool {
        if self.turn != turn {
            *self = Self {
                turn,
                ..default()
            };
        }
        let mut added = false;
        for report in player.reports.iter().filter(|report| report.turn == turn) {
            let planet_destroyed = planet_destruction_visible(report, player);
            let battle = Outcome::from_report(report, player);
            let territory = TerritoryOutcome::from_report(report, player);
            let territory_arrival = TerritoryArrival::from_report(report, player, map);
            let missile = MissilePresentation::from_report(report, player, map);
            let spy = SpyPresentation::from_report(report, player, map);
            if !planet_destroyed
                && battle.is_none()
                && territory.is_none()
                && missile.is_none()
                && spy.is_none()
            {
                continue;
            }
            if !self.observed.insert(report.id) {
                continue;
            }
            let site = self.outcomes.entry(report.mission.destination).or_default();
            site.planet_destroyed |= planet_destroyed;
            if let Some(outcome) = battle {
                site.battle = Some(match site.battle {
                    Some(previous) if previous != outcome => Outcome::Mixed,
                    Some(previous) => previous,
                    None => outcome,
                });
            }
            if let Some(territory) = territory {
                site.territory = Some(territory);
            }
            if site.territory_arrival.is_none() {
                site.territory_arrival = territory_arrival;
            }
            if let Some(missile) = missile {
                site.missile = Some(match site.missile {
                    // A local impact is the more urgent label if both sides launch at one site.
                    Some(previous)
                        if previous.outcome == MissileOutcome::Impact
                            || previous.outcome == missile.outcome =>
                    {
                        previous
                    },
                    _ => missile,
                });
            }
            if let Some(spy) = spy {
                site.spy = Some(site.spy.map_or(spy, |previous| previous.merge(spy)));
            }
            self.pending.insert(report.mission.destination);
            added = true;
        }
        added
    }
}

/// Loading a saved turn establishes a baseline instead of replaying historical explosions.
fn initialize_battles(
    mut sites: ResMut<BattleSites>,
    mut suppressed: ResMut<SuppressedReturningSpies>,
    player: Res<Player>,
    settings: Res<Settings>,
) {
    suppressed.clear();
    *sites = BattleSites {
        turn: settings.turn,
        observed: player.reports.iter().map(|report| report.id).collect(),
        ..default()
    };
}

#[derive(Component)]
pub(crate) struct BattleEffect {
    planet: PlanetId,
    turn: usize,
    returning_spies: Vec<MissionId>,
    timer: Timer,
}

#[derive(Clone, Copy)]
struct ReturningSpy {
    id: MissionId,
    position: Vec2,
}

#[derive(Component)]
enum EffectPart {
    TerritoryArrival {
        start: Vec2,
        orbit: Vec2,
        landing: Vec2,
    },
    Explosion {
        delay: f32,
        last_index: usize,
    },
    Ripple {
        delay: f32,
        radius: f32,
    },
    Missile {
        delay: f32,
        start: Vec2,
        end: Vec2,
    },
    SpyProbe {
        start: Vec2,
        station: Vec2,
        departure: Vec2,
        intercepted: bool,
    },
    SpyScanArc {
        delay: f32,
        radius: f32,
    },
    SpyPlanetWave {
        delay: f32,
        radius: f32,
    },
    SpyInterceptor {
        start: Vec2,
        end: Vec2,
    },
    SpyExplosion {
        delay: f32,
        last_index: usize,
    },
    Label {
        y: f32,
    },
}

fn show_battles(
    mut commands: Commands,
    mut sites: ResMut<BattleSites>,
    player: Res<Player>,
    settings: Res<Settings>,
    game_state: Res<State<GameState>>,
    map: Res<Map>,
    missions: Res<Missions>,
    mut suppressed: ResMut<SuppressedReturningSpies>,
    effects: Query<(Entity, &BattleEffect)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    assets: Res<WorldAssets>,
    mut audio: MessageWriter<PlayAudioMsg>,
) {
    if (player.is_changed() || sites.turn != settings.turn)
        && sites.observe(&player, &map, settings.turn)
    {
        for planet in &sites.pending {
            if sites.outcomes.get(planet).is_some_and(|outcome| outcome.spy.is_some()) {
                for mission in returning_spies_for(&missions, player.id, settings.turn, *planet) {
                    suppressed.suppress(mission.id);
                }
            }
        }
        // Let the projection's StartTurnMsg open combat before displaying the aftermath.
        return;
    }
    if *game_state.get() != GameState::Playing {
        return;
    }
    let mut exploded = false;
    for id in std::mem::take(&mut sites.pending) {
        let returning_spies = returning_spies_for(&missions, player.id, settings.turn, id);
        let Some(planet) = map.try_get(id) else {
            for mission in returning_spies {
                suppressed.release(mission.id);
            }
            continue;
        };
        let Some(&outcome) = sites.outcomes.get(&id) else {
            for mission in returning_spies {
                suppressed.release(mission.id);
            }
            continue;
        };
        let returning_spies = if outcome.spy.is_some() {
            returning_spies
        } else {
            Vec::new()
        };
        for (entity, effect) in &effects {
            if effect.planet == id {
                for mission in &effect.returning_spies {
                    suppressed.release(*mission);
                }
                commands.entity(entity).despawn();
            }
        }
        if outcome.spy.is_some() {
            for mission in &returning_spies {
                suppressed.suppress(mission.id);
            }
        }
        let spy_departure =
            returning_spies.first().map(|mission| mission.position - planet.position);
        let returning_spies = returning_spies.into_iter().map(|mission| mission.id).collect();
        spawn_aftermath(
            &mut commands,
            planet,
            settings.turn,
            outcome,
            player.color().color(),
            returning_spies,
            spy_departure,
            &assets,
            &mut meshes,
            &mut materials,
        );
        exploded |= outcome.has_audible_impact(planet);
    }
    if exploded {
        audio.write(PlayAudioMsg::new("short explosion"));
    }
}

fn returning_spies_for(
    missions: &Missions,
    player: crate::core::identity::PlayerId,
    turn: usize,
    origin: PlanetId,
) -> Vec<ReturningSpy> {
    missions
        .iter()
        .filter(|mission| {
            mission.owner == player
                && mission.send == turn
                && mission.origin == origin
                && mission.return_objective == Some(Icon::Spy)
        })
        .map(|mission| ReturningSpy {
            id: mission.id,
            position: mission.position,
        })
        .collect()
}

fn spawn_aftermath(
    commands: &mut Commands,
    planet: &Planet,
    turn: usize,
    outcome: SiteOutcome,
    viewer_color: Color,
    returning_spies: Vec<MissionId>,
    spy_departure: Option<Vec2>,
    assets: &WorldAssets,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
) {
    let size = planet.size();
    let texture = assets.texture("explosion");
    commands
        .spawn((
            Transform::from_translation(planet.position.extend(EXPLOSION_Z)),
            Visibility::Inherited,
            Pickable::IGNORE,
            MapCmp,
            BattleEffect {
                planet: planet.id,
                turn,
                returning_spies,
                timer: Timer::from_seconds(AFTERMATH_SECONDS, TimerMode::Once),
            },
        ))
        .with_children(|parent| {
            if let Some(arrival) = outcome.territory_arrival {
                let start = -arrival.direction * size * 1.7;
                let orbit = -arrival.direction * size * 0.73;
                let landing = -arrival.direction * size * 0.1;
                let image = arrival.image.key();
                parent.spawn((
                    Sprite {
                        image: assets.image(image),
                        custom_size: Some(Vec2::splat(size * 0.44)),
                        color: viewer_color.with_alpha(0.0),
                        flip_y: image == "mission colonize" && arrival.direction.x < 0.0,
                        ..default()
                    },
                    Transform {
                        translation: start.extend(0.27),
                        rotation: Quat::from_rotation_z(
                            arrival.direction.y.atan2(arrival.direction.x),
                        ),
                        ..default()
                    },
                    Pickable::IGNORE,
                    EffectPart::TerritoryArrival {
                        start,
                        orbit,
                        landing,
                    },
                ));
            }
            if let Some(missile) = outcome.missile {
                for (index, y) in [-0.55, 0.0, 0.48].into_iter().enumerate() {
                    let (start, end) = missile_path(size, index, y, missile.direction);
                    let direction = end - start;
                    parent.spawn((
                        Sprite {
                            image: assets.image("mission missile"),
                            custom_size: Some(Vec2::splat(size * 0.46)),
                            color: viewer_color.with_alpha(0.0),
                            ..default()
                        },
                        Transform {
                            translation: start.extend(0.24),
                            rotation: Quat::from_rotation_z(direction.y.atan2(direction.x)),
                            ..default()
                        },
                        Pickable::IGNORE,
                        EffectPart::Missile {
                            delay: index as f32 * 0.16,
                            start,
                            end,
                        },
                    ));
                }
            }
            if let Some(spy) = outcome.spy {
                let (start, station) = spy_path(size, spy.direction);
                let departure = spy_departure.unwrap_or(start);
                let direction = station - start;
                parent.spawn((
                    Sprite {
                        image: assets.image("mission spy"),
                        custom_size: Some(Vec2::splat(SPY_MISSION_SIZE)),
                        color: viewer_color.with_alpha(0.0),
                        ..default()
                    },
                    Transform {
                        translation: start.extend(0.24),
                        rotation: Quat::from_rotation_z(
                            direction.y.atan2(direction.x) + SPY_ICON_ROTATION,
                        ),
                        ..default()
                    },
                    Pickable::IGNORE,
                    EffectPart::SpyProbe {
                        start,
                        station,
                        departure,
                        intercepted: spy.intercepted,
                    },
                ));

                let scan_arc = meshes.add(spy_scan_arc_mesh());
                let planet_wave = meshes.add(Annulus::new(0.965, 1.0));
                // Stop the directional wavefront at the near edge of the world. The full
                // planetary ring takes over there, avoiding arcs drawn across the planet.
                let scan_radius = spy_scan_radius(size, station);
                let scan_rotation = Quat::from_rotation_z(spy.direction.y.atan2(spy.direction.x));
                for index in 0..SPY_SCAN_ARC_COUNT {
                    let delay =
                        SPY_SCAN_START_SECONDS + index as f32 * SPY_SCAN_ARC_INTERVAL_SECONDS;
                    parent.spawn((
                        Mesh2d(scan_arc.clone()),
                        MeshMaterial2d(materials.add(viewer_color.with_alpha(0.0))),
                        Transform {
                            translation: station.extend(0.22),
                            rotation: scan_rotation,
                            ..default()
                        },
                        Pickable::IGNORE,
                        EffectPart::SpyScanArc {
                            delay,
                            radius: scan_radius,
                        },
                    ));
                    parent.spawn((
                        Mesh2d(planet_wave.clone()),
                        MeshMaterial2d(materials.add(viewer_color.with_alpha(0.0))),
                        Transform::from_xyz(0.0, 0.0, 0.23),
                        Pickable::IGNORE,
                        EffectPart::SpyPlanetWave {
                            delay: delay + SPY_PLANET_WAVE_DELAY_SECONDS,
                            radius: size * 0.62,
                        },
                    ));
                }

                if spy.intercepted {
                    let lateral = Vec2::new(-spy.direction.y, spy.direction.x);
                    for offset in [-0.18, 0.18] {
                        let fighter_start = lateral * size * offset;
                        let fighter_end = station + lateral * size * offset * 0.28;
                        let fighter_direction = fighter_end - fighter_start;
                        parent.spawn((
                            Sprite {
                                image: assets.image("light fighter"),
                                custom_size: Some(Vec2::splat(size * 0.28)),
                                color: viewer_color.with_alpha(0.0),
                                ..default()
                            },
                            Transform {
                                translation: fighter_start.extend(0.25),
                                rotation: Quat::from_rotation_z(
                                    fighter_direction.y.atan2(fighter_direction.x),
                                ),
                                ..default()
                            },
                            Pickable::IGNORE,
                            EffectPart::SpyInterceptor {
                                start: fighter_start,
                                end: fighter_end,
                            },
                        ));
                    }
                    parent.spawn((
                        Sprite {
                            image: texture.image.clone(),
                            texture_atlas: Some(texture.atlas.clone()),
                            custom_size: Some(Vec2::splat(size * 0.64)),
                            color: viewer_color.with_alpha(0.0),
                            ..default()
                        },
                        Transform::from_translation(station.extend(0.26)),
                        Pickable::IGNORE,
                        EffectPart::SpyExplosion {
                            delay: SPY_DESTROY_SECONDS,
                            last_index: texture.last_index,
                        },
                    ));
                }
            }
            // Destroyed worlds already receive the larger planet-destruction animation.
            if outcome.has_impact() && !planet.is_destroyed {
                for (index, offset) in
                    [Vec2::new(-0.28, 0.1), Vec2::new(0.18, -0.18), Vec2::new(0.25, 0.21)]
                        .into_iter()
                        .enumerate()
                {
                    parent.spawn((
                        Sprite {
                            image: texture.image.clone(),
                            texture_atlas: Some(texture.atlas.clone()),
                            custom_size: Some(Vec2::splat(size * 0.7)),
                            color: viewer_color.with_alpha(0.0),
                            ..default()
                        },
                        Transform::from_translation((offset * size).extend(0.1)),
                        Pickable::IGNORE,
                        EffectPart::Explosion {
                            delay: index as f32 * 0.3,
                            last_index: texture.last_index,
                        },
                    ));
                }
            }
            if outcome.has_impact() {
                let ripple = meshes.add(Annulus::new(0.98, 1.0));
                for index in 0..RIPPLE_COUNT {
                    let radius = size * 0.52;
                    parent.spawn((
                        Mesh2d(ripple.clone()),
                        MeshMaterial2d(materials.add(viewer_color.with_alpha(0.0))),
                        Transform::from_scale(Vec3::splat(radius)),
                        Pickable::IGNORE,
                        EffectPart::Ripple {
                            delay: index as f32 * RIPPLE_INTERVAL_SECONDS,
                            radius,
                        },
                    ));
                }
            }
            for (index, label) in outcome.labels(planet).into_iter().enumerate() {
                let y = super::aftermath_label_y(size, index);
                parent.spawn((
                    Text2d::new(label),
                    TextFont {
                        font: assets.font("bold").into(),
                        font_size: 17.0.into(),
                        ..default()
                    },
                    TextColor(viewer_color.with_alpha(0.0)),
                    // Planet names sit at 0.7 * size; stack results above them and colony labels.
                    Transform::from_xyz(0.0, y, 0.2 + index as f32 * 0.01),
                    Pickable::IGNORE,
                    EffectPart::Label {
                        y,
                    },
                ));
            }
        });
}

fn missile_path(size: f32, index: usize, lateral_offset: f32, direction: Vec2) -> (Vec2, Vec2) {
    let lateral = Vec2::new(-direction.y, direction.x);
    let start = -direction * size * (2.0 + index as f32 * 0.18) + lateral * size * lateral_offset;
    let end =
        direction * size * (-0.18 + index as f32 * 0.17) + lateral * size * lateral_offset * 0.25;
    (start, end)
}

fn spy_path(size: f32, direction: Vec2) -> (Vec2, Vec2) {
    (
        -direction * size * 1.65,
        // Park beyond the world's edge so the probe visibly scans from orbit.
        -direction * size * 0.82,
    )
}

fn spy_scan_radius(size: f32, station: Vec2) -> f32 {
    (station.length() - size * 0.48).max(size * 0.2)
}

/// Builds a narrow radar wavefront aimed along +X; the spawned transform rotates it planetward.
fn spy_scan_arc_mesh() -> Mesh {
    const SEGMENTS: usize = 24;
    const HALF_ANGLE: f32 = 0.48;
    let mut positions = Vec::with_capacity((SEGMENTS + 1) * 2);
    let mut indices = Vec::with_capacity(SEGMENTS * 6);
    for index in 0..=SEGMENTS {
        let angle = -HALF_ANGLE + 2.0 * HALF_ANGLE * index as f32 / SEGMENTS as f32;
        let direction = Vec2::new(angle.cos(), angle.sin());
        positions.push([direction.x * 0.965, direction.y * 0.965, 0.0]);
        positions.push([direction.x, direction.y, 0.0]);
        if index < SEGMENTS {
            let vertex = (index * 2) as u32;
            indices.extend([vertex, vertex + 2, vertex + 3, vertex, vertex + 3, vertex + 1]);
        }
    }
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_indices(Indices::U32(indices))
}

/// Fade out and remove the aftermath; overlays hide and pause the animation.
fn animate_battles(
    mut commands: Commands,
    time: Res<Time>,
    game_state: Res<State<GameState>>,
    settings: Res<Settings>,
    map: Res<Map>,
    mut suppressed: ResMut<SuppressedReturningSpies>,
    mut effects: Query<(Entity, &mut BattleEffect, &Children, &mut Visibility)>,
    mut parts: Query<(
        Entity,
        &EffectPart,
        &mut Transform,
        Option<&mut Sprite>,
        Option<&MeshMaterial2d<ColorMaterial>>,
        Option<&mut TextColor>,
    )>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    for (entity, mut effect, children, mut visibility) in &mut effects {
        if effect.turn != settings.turn || map.try_get(effect.planet).is_none() {
            for mission in &effect.returning_spies {
                suppressed.release(*mission);
            }
            commands.entity(entity).despawn();
            continue;
        }
        let playing = *game_state.get() == GameState::Playing;
        *visibility = if playing {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if !playing {
            continue;
        }
        effect.timer.tick(time.delta());
        if effect.timer.is_finished() {
            for mission in &effect.returning_spies {
                suppressed.release(*mission);
            }
            commands.entity(entity).despawn();
            continue;
        }
        let elapsed = effect.timer.elapsed_secs();
        if elapsed >= SPY_DEPART_START_SECONDS + SPY_DEPART_SECONDS
            && !effect.returning_spies.is_empty()
        {
            for mission in std::mem::take(&mut effect.returning_spies) {
                suppressed.release(mission);
            }
        }
        let settle = ((elapsed - 2.8) / (AFTERMATH_SECONDS - 2.8)).clamp(0.0, 1.0);
        for child in children.iter() {
            let Ok((entity, part, mut transform, sprite, material, text)) = parts.get_mut(child)
            else {
                continue;
            };
            match part {
                EffectPart::TerritoryArrival {
                    start,
                    orbit,
                    landing,
                } => {
                    let approach = (elapsed / CONQUEST_APPROACH_SECONDS).clamp(0.0, 1.0);
                    let approach_eased = approach * approach * (3.0 - 2.0 * approach);
                    transform.translation = start.lerp(*orbit, approach_eased).extend(0.27);
                    let landing_progress = ((elapsed - CONQUEST_APPROACH_SECONDS)
                        / CONQUEST_LANDING_SECONDS)
                        .clamp(0.0, 1.0);
                    if landing_progress > 0.0 {
                        let landing_eased =
                            landing_progress * landing_progress * (3.0 - 2.0 * landing_progress);
                        transform.translation = orbit.lerp(*landing, landing_eased).extend(0.27);
                        transform.scale = Vec3::splat(1.0 - 0.58 * landing_eased);
                    }
                    if let Some(mut sprite) = sprite {
                        let alpha = (approach / 0.14).clamp(0.0, 1.0)
                            * ((1.0 - landing_progress) / 0.28).clamp(0.0, 1.0);
                        sprite.color.set_alpha(alpha);
                    }
                },
                EffectPart::Explosion {
                    delay,
                    last_index,
                } => {
                    let progress = (elapsed - delay) / EXPLOSION_SECONDS;
                    if progress >= 1.0 {
                        commands.entity(entity).despawn();
                    } else if let Some(mut sprite) = sprite {
                        sprite.color.set_alpha(if progress >= 0.0 {
                            0.9
                        } else {
                            0.0
                        });
                        if let Some(atlas) = &mut sprite.texture_atlas {
                            atlas.index = ((progress.max(0.0) * (*last_index + 1) as f32) as usize)
                                .min(*last_index);
                        }
                    }
                },
                EffectPart::Ripple {
                    delay,
                    radius,
                } => {
                    let progress = (elapsed - delay) / RIPPLE_SECONDS;
                    if progress >= 1.0 {
                        commands.entity(entity).despawn();
                    } else if progress > 0.0 {
                        let outward = 1.0 - (1.0 - progress).powi(2);
                        transform.scale = Vec3::splat(radius * (1.0 + 1.8 * outward));
                        if let Some(mut material) =
                            material.and_then(|handle| materials.get_mut(&handle.0))
                        {
                            let fade_in = (progress / 0.08).min(1.0);
                            material.color.set_alpha(0.68 * fade_in * (1.0 - progress).powf(1.35));
                        }
                    }
                },
                EffectPart::Missile {
                    delay,
                    start,
                    end,
                } => {
                    let progress = ((elapsed - delay) / 0.82).clamp(0.0, 1.0);
                    if progress >= 1.0 {
                        commands.entity(entity).despawn();
                    } else if progress > 0.0 {
                        let eased = progress * progress * (3.0 - 2.0 * progress);
                        transform.translation = start.lerp(*end, eased).extend(0.24);
                        transform.scale = Vec3::splat(0.82 + 0.3 * (1.0 - progress));
                        if let Some(mut sprite) = sprite {
                            sprite.color.set_alpha((progress * 8.0).min(1.0));
                        }
                    }
                },
                EffectPart::SpyProbe {
                    start,
                    station,
                    departure,
                    intercepted,
                } => {
                    let arrival = (elapsed / SPY_ARRIVAL_SECONDS).clamp(0.0, 1.0);
                    let eased = arrival * arrival * (3.0 - 2.0 * arrival);
                    transform.translation = start.lerp(*station, eased).extend(0.24);
                    transform.scale =
                        Vec3::splat(0.9 + 0.08 * (elapsed * std::f32::consts::TAU * 1.8).sin());
                    if let Some(mut sprite) = sprite {
                        let mut alpha = (arrival / 0.18).clamp(0.0, 1.0);
                        if *intercepted && elapsed >= SPY_DESTROY_SECONDS {
                            alpha = 0.0;
                        } else if !*intercepted && elapsed > SPY_DEPART_START_SECONDS {
                            let progress = ((elapsed - SPY_DEPART_START_SECONDS)
                                / SPY_DEPART_SECONDS)
                                .clamp(0.0, 1.0);
                            let eased = progress * progress * (3.0 - 2.0 * progress);
                            transform.translation = station.lerp(*departure, eased).extend(0.24);
                            // The mission artwork is natively upright. Mirror it for the return
                            // leg instead of rotating it along the path, which can make the probe
                            // appear upside down as it moves away from the planet.
                            transform.rotation = Quat::IDENTITY;
                            transform.scale = transform.scale.lerp(Vec3::ONE, eased);
                            sprite.flip_x = true;
                            if progress >= 1.0 {
                                alpha = 0.0;
                            }
                        }
                        sprite.color.set_alpha(alpha);
                    }
                },
                EffectPart::SpyScanArc {
                    delay,
                    radius,
                } => {
                    let progress = (elapsed - delay) / SPY_SCAN_ARC_SECONDS;
                    if progress >= 1.0 {
                        commands.entity(entity).despawn();
                    } else if progress > 0.0 {
                        transform.scale = Vec3::splat(radius * (0.06 + 0.94 * progress));
                        if let Some(mut material) =
                            material.and_then(|handle| materials.get_mut(&handle.0))
                        {
                            let fade_in = (progress / 0.08).min(1.0);
                            material.color.set_alpha(0.82 * fade_in * (1.0 - progress).powf(0.75));
                        }
                    }
                },
                EffectPart::SpyPlanetWave {
                    delay,
                    radius,
                } => {
                    let progress = (elapsed - delay) / SPY_PLANET_WAVE_SECONDS;
                    if progress >= 1.0 {
                        commands.entity(entity).despawn();
                    } else if progress > 0.0 {
                        let outward = 1.0 - (1.0 - progress).powi(2);
                        transform.scale = Vec3::splat(radius * (0.52 + 0.48 * outward));
                        if let Some(mut material) =
                            material.and_then(|handle| materials.get_mut(&handle.0))
                        {
                            let fade_in = (progress / 0.08).min(1.0);
                            material.color.set_alpha(0.86 * fade_in * (1.0 - progress).powf(1.35));
                        }
                    }
                },
                EffectPart::SpyInterceptor {
                    start,
                    end,
                } => {
                    let progress = ((elapsed - SPY_INTERCEPT_START_SECONDS)
                        / SPY_INTERCEPT_SECONDS)
                        .clamp(0.0, 1.0);
                    if progress >= 1.0 {
                        commands.entity(entity).despawn();
                    } else if progress > 0.0 {
                        let eased = progress * progress * (3.0 - 2.0 * progress);
                        transform.translation = start.lerp(*end, eased).extend(0.25);
                        transform.scale = Vec3::splat(0.82 + 0.18 * progress);
                        if let Some(mut sprite) = sprite {
                            let fade_out = ((1.0 - progress) / 0.12).clamp(0.0, 1.0);
                            sprite.color.set_alpha((progress * 8.0).min(1.0) * fade_out);
                        }
                    }
                },
                EffectPart::SpyExplosion {
                    delay,
                    last_index,
                } => {
                    let progress = (elapsed - delay) / SPY_EXPLOSION_SECONDS;
                    if progress >= 1.0 {
                        commands.entity(entity).despawn();
                    } else if let Some(mut sprite) = sprite {
                        sprite.color.set_alpha(if progress >= 0.0 {
                            0.95
                        } else {
                            0.0
                        });
                        if let Some(atlas) = &mut sprite.texture_atlas {
                            atlas.index = ((progress.max(0.0) * (*last_index + 1) as f32) as usize)
                                .min(*last_index);
                        }
                    }
                },
                EffectPart::Label {
                    y,
                } => {
                    transform.scale = Vec3::splat(1.0 - 0.12 * settle);
                    transform.translation.y = *y + 5.0 * (1.0 - settle);
                    if let Some(mut text) = text {
                        text.0.set_alpha(((elapsed - 0.3) / 0.4).clamp(0.0, 1.0) * (1.0 - settle));
                    }
                },
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, SystemSet)]
pub(crate) struct BattleAftermathSet;

pub(crate) struct BattleAftermathPlugin;

impl Plugin for BattleAftermathPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BattleSites>()
            .init_resource::<SuppressedReturningSpies>()
            .add_systems(OnEnter(AppState::Game), initialize_battles.after(draw_map))
            .add_systems(
                Update,
                (show_battles, animate_battles)
                    .chain()
                    .in_set(BattleAftermathSet)
                    .after(refresh_gameplay_projection)
                    .after(refresh_turn_draft)
                    .run_if(in_state(AppState::Game)),
            );
    }
}

#[cfg(test)]
#[path = "../../../tests/core/map_battle.rs"]
mod tests;
