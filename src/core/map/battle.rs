//! Client-only battle aftermath on the strategic map, derived from visible turn reports.

use std::collections::{BTreeMap, BTreeSet};

use bevy::prelude::*;

use super::icon::Icon;
use super::model::{Map, MapCmp};
use super::planet::{Planet, PlanetId};
use super::systems::draw_map;
use crate::core::assets::WorldAssets;
use crate::core::audio::PlayAudioMsg;
use crate::core::combat::report::{MissionReport, ReportId};
use crate::core::constants::{EXPLOSION_Z, TITLE_TEXT_SIZE};
use crate::core::loading::{refresh_gameplay_projection, refresh_turn_draft};
use crate::core::player::Player;
use crate::core::settings::Settings;
use crate::core::states::{AppState, GameState};

const AFTERMATH_SECONDS: f32 = 4.2;
const EXPLOSION_SECONDS: f32 = 1.55;

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

    fn color(self) -> Color {
        match self {
            Self::Victory => Color::srgb(0.3, 0.93, 0.74),
            Self::Defeat => Color::srgb(1.0, 0.35, 0.3),
            Self::Draw | Self::Mixed => Color::srgb(1.0, 0.78, 0.32),
        }
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
        if report.hidden {
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

    fn color(self) -> Color {
        match self {
            Self::Conquered => Color::srgb(0.3, 0.93, 0.74),
            Self::Lost => Color::srgb(1.0, 0.35, 0.3),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Conquered => "PLANET CONQUERED",
            Self::Lost => "PLANET LOST",
        }
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

    fn color(self) -> Color {
        match self {
            Self::Strike => Color::srgb(1.0, 0.72, 0.28),
            Self::Impact => Color::srgb(1.0, 0.38, 0.22),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Strike => "MISSILE STRIKE",
            Self::Impact => "MISSILE IMPACT",
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct SiteOutcome {
    battle: Option<Outcome>,
    territory: Option<TerritoryOutcome>,
    missile: Option<MissileOutcome>,
}

impl SiteOutcome {
    fn color(self) -> Color {
        self.territory
            .map(TerritoryOutcome::color)
            .or_else(|| self.battle.map(Outcome::color))
            .or_else(|| self.missile.map(MissileOutcome::color))
            .unwrap_or(Color::WHITE)
    }

    fn labels(self) -> Vec<(&'static str, Color)> {
        let mut labels = Vec::with_capacity(3);
        if let Some(territory) = self.territory {
            labels.push((territory.label(), territory.color()));
        } else if let Some(battle) = self.battle {
            labels.push((battle.label(), battle.color()));
        }
        if let Some(missile) = self.missile {
            labels.push((missile.label(), missile.color()));
        }
        labels
    }
}

#[derive(Resource, Default)]
struct BattleSites {
    turn: usize,
    observed: BTreeSet<ReportId>,
    outcomes: BTreeMap<PlanetId, SiteOutcome>,
    pending: BTreeSet<PlanetId>,
}

impl BattleSites {
    fn observe(&mut self, player: &Player, turn: usize) -> bool {
        if self.turn != turn {
            *self = Self {
                turn,
                ..default()
            };
        }
        let mut added = false;
        for report in player.reports.iter().filter(|report| report.turn == turn) {
            let battle = Outcome::from_report(report, player);
            let territory = TerritoryOutcome::from_report(report, player);
            let missile = MissileOutcome::from_report(report, player);
            if battle.is_none() && territory.is_none() && missile.is_none() {
                continue;
            }
            if !self.observed.insert(report.id) {
                continue;
            }
            let site = self.outcomes.entry(report.mission.destination).or_default();
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
            if let Some(missile) = missile {
                site.missile = Some(match (site.missile, missile) {
                    // A local impact is the more urgent label if both sides launch at one site.
                    (Some(MissileOutcome::Impact), _) | (_, MissileOutcome::Impact) => {
                        MissileOutcome::Impact
                    },
                    _ => MissileOutcome::Strike,
                });
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
    player: Res<Player>,
    settings: Res<Settings>,
) {
    *sites = BattleSites {
        turn: settings.turn,
        observed: player.reports.iter().map(|report| report.id).collect(),
        ..default()
    };
}

#[derive(Component)]
struct BattleEffect {
    planet: PlanetId,
    turn: usize,
    timer: Timer,
}

#[derive(Component)]
enum EffectPart {
    Explosion {
        delay: f32,
        last_index: usize,
    },
    Ring {
        radius: f32,
    },
    Missile {
        delay: f32,
        start: Vec2,
        end: Vec2,
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
    effects: Query<(Entity, &BattleEffect)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    assets: Res<WorldAssets>,
    mut audio: MessageWriter<PlayAudioMsg>,
) {
    if (player.is_changed() || sites.turn != settings.turn) && sites.observe(&player, settings.turn)
    {
        // Let the projection's StartTurnMsg open combat before displaying the aftermath.
        return;
    }
    if *game_state.get() != GameState::Playing {
        return;
    }
    let mut exploded = false;
    for id in std::mem::take(&mut sites.pending) {
        let Some(planet) = map.try_get(id) else {
            continue;
        };
        let Some(&outcome) = sites.outcomes.get(&id) else {
            continue;
        };
        for (entity, effect) in &effects {
            if effect.planet == id {
                commands.entity(entity).despawn();
            }
        }
        spawn_aftermath(
            &mut commands,
            planet,
            settings.turn,
            outcome,
            &assets,
            &mut meshes,
            &mut materials,
        );
        exploded |= !planet.is_destroyed;
    }
    if exploded {
        audio.write(PlayAudioMsg::new("short explosion"));
    }
}

fn spawn_aftermath(
    commands: &mut Commands,
    planet: &Planet,
    turn: usize,
    outcome: SiteOutcome,
    assets: &WorldAssets,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
) {
    let size = planet.size();
    let color = outcome.color();
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
                timer: Timer::from_seconds(AFTERMATH_SECONDS, TimerMode::Once),
            },
        ))
        .with_children(|parent| {
            if let Some(missile) = outcome.missile {
                // Local diagonals convey impact without revealing a hidden mission origin.
                for (index, y) in [-0.55, 0.0, 0.48].into_iter().enumerate() {
                    let start = Vec2::new(-size * (2.0 + index as f32 * 0.18), size * (1.25 + y));
                    let end = Vec2::new(size * (-0.18 + index as f32 * 0.17), size * y * 0.25);
                    let direction = end - start;
                    parent.spawn((
                        Sprite {
                            image: assets.image("mission missile"),
                            custom_size: Some(Vec2::splat(size * 0.46)),
                            color: missile.color().with_alpha(0.0),
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
            // Destroyed worlds already receive the larger planet-destruction animation.
            if !planet.is_destroyed {
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
                            color: Color::WHITE.with_alpha(0.0),
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
            parent.spawn((
                Mesh2d(meshes.add(Annulus::new(0.975, 1.0))),
                MeshMaterial2d(materials.add(color.with_alpha(0.0))),
                Transform::default(),
                Pickable::IGNORE,
                EffectPart::Ring {
                    radius: size * 0.57,
                },
            ));
            for (index, (label, label_color)) in outcome.labels().into_iter().enumerate() {
                let y = size * 0.7 + TITLE_TEXT_SIZE * (1.15 + index as f32 * 1.05);
                parent.spawn((
                    Text2d::new(label),
                    TextFont {
                        font: assets.font("bold").into(),
                        font_size: 17.0.into(),
                        ..default()
                    },
                    TextColor(label_color.with_alpha(0.0)),
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

/// Fade out and remove the aftermath; overlays hide and pause the animation.
fn animate_battles(
    mut commands: Commands,
    time: Res<Time>,
    game_state: Res<State<GameState>>,
    settings: Res<Settings>,
    map: Res<Map>,
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
            commands.entity(entity).despawn();
            continue;
        }
        let elapsed = effect.timer.elapsed_secs();
        let settle = ((elapsed - 2.8) / (AFTERMATH_SECONDS - 2.8)).clamp(0.0, 1.0);
        for child in children.iter() {
            let Ok((entity, part, mut transform, sprite, material, text)) = parts.get_mut(child)
            else {
                continue;
            };
            match part {
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
                EffectPart::Ring {
                    radius,
                } => {
                    let pulse = (elapsed * 5.0).sin().abs() * (1.0 - settle);
                    transform.scale = Vec3::splat(radius * (1.0 + 0.1 * pulse));
                    if let Some(mut material) =
                        material.and_then(|handle| materials.get_mut(&handle.0))
                    {
                        material.color.set_alpha(
                            (elapsed / 0.35).min(1.0) * (0.28 + 0.4 * pulse) * (1.0 - settle),
                        );
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

pub(crate) struct BattleAftermathPlugin;

impl Plugin for BattleAftermathPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BattleSites>()
            .add_systems(OnEnter(AppState::Game), initialize_battles.after(draw_map))
            .add_systems(
                Update,
                (show_battles, animate_battles)
                    .chain()
                    .after(refresh_gameplay_projection)
                    .after(refresh_turn_draft)
                    .run_if(in_state(AppState::Game)),
            );
    }
}

#[cfg(test)]
#[path = "../../../tests/core/map_battle.rs"]
mod tests;
