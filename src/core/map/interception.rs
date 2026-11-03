//! Report-driven space dogfights. Flight, exhaust and weapons are presentation only.

use std::collections::{BTreeMap, BTreeSet};
use std::f32::consts::{FRAC_PI_4, PI};

use bevy::prelude::*;

use super::battle::BattleAftermathSet;
use super::model::MapCmp;
use super::systems::draw_map;
use crate::core::assets::WorldAssets;
use crate::core::audio::PlayAudioMsg;
use crate::core::combat::report::ReportId;
use crate::core::constants::EXPLOSION_Z;
use crate::core::loading::{refresh_gameplay_projection, refresh_turn_draft};
use crate::core::map::icon::Icon;
use crate::core::mission_systems::{
    mission_map_flip_x, mission_map_flip_y, mission_size, mission_world_rotation,
};
use crate::core::missions::{Mission, MissionId, Missions, SuppressedMapMissions};
use crate::core::player::Player;
use crate::core::settings::Settings;
use crate::core::states::{AppState, GameState};
use crate::core::units::{Amount, Army, Unit};
use crate::multiplayer::client::MultiplayerSession;

const DURATION: f32 = 7.0;
const APPROACH: f32 = 1.0;
const IMPACT: f32 = 3.2;
const DEPARTURE: f32 = 4.2;

#[derive(Clone, Copy)]
struct ShipArt {
    image: &'static str,
    size: f32,
    rotation_offset: f32,
    upright: bool,
    flip_x: bool,
}

impl ShipArt {
    fn from_mission(mission: &Mission, player: &Player) -> Self {
        let mut art = Self::from_image(mission.image(player));
        art.size = mission_size(mission, false) * 0.8;
        art.rotation_offset = mission_world_rotation(mission, 0.0);
        art.upright = mission.return_objective == Some(Icon::Spy);
        art.flip_x = mission_map_flip_x(mission);
        art
    }

    fn from_image(image: &str) -> Self {
        let (image, size) = match image {
            "mission colonize" => ("mission colonize", 44.0),
            "mission destroy" => ("mission destroy", 50.0),
            "mission spy" => ("mission spy", 36.0),
            _ => ("mission", 50.0),
        };
        Self {
            image,
            size: size * 0.8,
            rotation_offset: if image == "mission spy" {
                -FRAC_PI_4
            } else {
                0.0
            },
            upright: false,
            flip_x: false,
        }
    }

    /// Reports without a previously visible target retain only its pre-combat units.
    fn from_army(army: &Army) -> Self {
        Self::from_image(if army.amount(&Unit::colony_ship()) > 0 {
            "mission colonize"
        } else if army.amount(&Unit::war_sun()) > 0 {
            "mission destroy"
        } else if army.has_army()
            && army.iter().all(|(unit, count)| *count == 0 || *unit == Unit::probe())
        {
            "mission spy"
        } else {
            "mission"
        })
    }
}

#[derive(Resource, Default)]
struct InterceptSites {
    turn: usize,
    observed: BTreeSet<ReportId>,
    pending: BTreeSet<ReportId>,
    artwork: BTreeMap<MissionId, ShipArt>,
    previous_artwork: BTreeMap<MissionId, ShipArt>,
}

impl InterceptSites {
    fn observe(&mut self, player: &Player, turn: usize) -> bool {
        if self.turn != turn {
            self.turn = turn;
            self.observed.clear();
            self.pending.clear();
            // Keep the silhouette from before combat even if that fleet was destroyed or lost
            // the ship that supplied its artwork. Bound this cache to two visible turns.
            self.previous_artwork = std::mem::take(&mut self.artwork);
        }
        let mut changed = false;
        for report in player.reports.iter().filter(|report| {
            report.turn == turn
                && !report.hidden
                && report.combat_report.is_some()
                && matches!(report.mission.objective, Icon::Intercept | Icon::Patrol)
                && !report.is_space_fauna_encounter()
                && (report.is_attacker(player.id) || report.is_defender(player.id))
        }) {
            if self.observed.insert(report.id) {
                self.pending.insert(report.id);
                changed = true;
            }
        }
        changed
    }
}

#[derive(Component)]
pub(crate) struct InterceptEffect {
    turn: usize,
    timer: Timer,
    missions: Vec<MissionId>,
    sounded: bool,
}

#[derive(Component, Clone, Copy)]
enum Part {
    Ship {
        side: u8,
        survives: bool,
        art: ShipArt,
    },
    Exhaust {
        side: u8,
        survives: bool,
        size: f32,
        last_frame: usize,
    },
    Beam(u8),
    Label,
}

fn initialize(
    mut sites: ResMut<InterceptSites>,
    player: Res<Player>,
    settings: Res<Settings>,
    missions: Res<Missions>,
) {
    *sites = InterceptSites::default();
    sites.observe(&player, settings.turn);
    // Loading or resuming starts from the saved map, not a replay of old effects.
    sites.pending.clear();
    sites.artwork = missions
        .iter()
        .map(|mission| (mission.id, ShipArt::from_mission(mission, &player)))
        .collect();
}

/// Converging entry, banking passes, then departure: no route ever points at a real launch world.
fn ship_position(side: u8, t: f32) -> Vec2 {
    let sign = if side == 0 {
        -1.0
    } else {
        1.0
    };
    if t < APPROACH {
        let q = (t / APPROACH).clamp(0.0, 1.0);
        let p = 1.0 - q;
        return sign
            * (Vec2::new(135.0, -24.0) * p.powi(3)
                + Vec2::new(90.0, -24.0) * 3.0 * p.powi(2) * q
                + Vec2::new(52.0, -17.6) * 3.0 * p * q.powi(2)
                + Vec2::new(52.0, 0.0) * q.powi(3));
    }
    let angle = (t - APPROACH).min(DEPARTURE - APPROACH) * 1.65;
    let position = sign * Vec2::new(52.0 * angle.cos(), 32.0 * angle.sin());
    if t <= DEPARTURE {
        return position;
    }
    let velocity = sign * Vec2::new(-52.0 * angle.sin(), 32.0 * angle.cos()) * 1.65;
    let elapsed = t - DEPARTURE;
    position + velocity * elapsed + velocity.normalize_or_zero() * 14.0 * elapsed.powi(2)
}

fn ship_alpha(survives: bool, t: f32) -> f32 {
    let fade_in = (t / 0.25).clamp(0.0, 1.0);
    let fade_out = if survives {
        ((6.2 - t) / 0.8).clamp(0.0, 1.0)
    } else {
        ((IMPACT + 0.35 - t) / 0.3).clamp(0.0, 1.0)
    };
    fade_in * fade_out
}

fn show(
    mut commands: Commands,
    mut sites: ResMut<InterceptSites>,
    player: Res<Player>,
    settings: Res<Settings>,
    game_state: Res<State<GameState>>,
    assets: Res<WorldAssets>,
    visible_missions: Res<Missions>,
    session: Res<MultiplayerSession>,
    mut suppressed: ResMut<SuppressedMapMissions>,
) {
    let turn_changed = sites.turn != settings.turn;
    let discovered = (player.is_changed() || turn_changed) && sites.observe(&player, settings.turn);
    if visible_missions.is_changed() || player.is_changed() || turn_changed {
        sites.artwork = visible_missions
            .iter()
            .map(|mission| (mission.id, ShipArt::from_mission(mission, &player)))
            .collect();
    }
    if discovered {
        return;
    }
    if *game_state.get() != GameState::Playing {
        return;
    }
    for id in std::mem::take(&mut sites.pending) {
        let Some(report) = player.reports.iter().find(|report| report.id == id) else {
            continue;
        };
        let mut missions = vec![report.mission.id];
        if let Some(intercept) = &report.mission.intercept {
            missions.push(intercept.target);
        }
        for mission in &missions {
            suppressed.suppress(*mission);
        }
        let attacker_survives = report.surviving_attacker.has_army();
        let defender_survives = report.surviving_defender.has_army();
        let target_art = report.mission.intercept.as_ref().and_then(|intercept| {
            sites
                .previous_artwork
                .get(&intercept.target)
                .or_else(|| sites.artwork.get(&intercept.target))
                .copied()
        });
        let artwork = [
            ShipArt::from_mission(&report.mission, &player),
            target_art.unwrap_or_else(|| ShipArt::from_army(&report.planet.army.combined())),
        ];
        let color = |owner| {
            if owner == player.id {
                player.color().color()
            } else {
                session.player_color(owner).color()
            }
        };
        let colors =
            [color(report.mission.owner), report.planet.controlled.map_or(Color::WHITE, color)];
        let flame = assets.texture("flame");
        let label = if report.is_stalemate() {
            "INTERCEPTION · BOTH FLEETS SURVIVED"
        } else if report.won_by(player.id) {
            "INTERCEPTION WON"
        } else if report.mission.owner == player.id {
            "INTERCEPTION FAILED"
        } else {
            "MISSION INTERCEPTED"
        };
        commands
            .spawn((
                Transform::from_translation(report.planet.position.extend(EXPLOSION_Z)),
                Visibility::Inherited,
                MapCmp,
                Pickable::IGNORE,
                InterceptEffect {
                    turn: settings.turn,
                    timer: Timer::from_seconds(DURATION, TimerMode::Once),
                    missions,
                    sounded: false,
                },
            ))
            .with_children(|parent| {
                for side in 0..2 {
                    let art = artwork[side as usize];
                    let survives = if side == 0 {
                        attacker_survives
                    } else {
                        defender_survives
                    };
                    parent.spawn((
                        Sprite {
                            image: assets.image(art.image),
                            custom_size: Some(Vec2::splat(art.size)),
                            color: colors[side as usize].with_alpha(0.0),
                            flip_x: art.flip_x,
                            ..default()
                        },
                        Transform::default(),
                        Pickable::IGNORE,
                        Part::Ship {
                            side,
                            survives,
                            art,
                        },
                    ));
                    parent.spawn((
                        Sprite::from_atlas_image(flame.image.clone(), flame.atlas.clone()),
                        Transform::default(),
                        Pickable::IGNORE,
                        Part::Exhaust {
                            side,
                            survives,
                            size: art.size,
                            last_frame: flame.last_index,
                        },
                    ));
                }
                for index in 0..4 {
                    parent.spawn((
                        Sprite::from_color(colors[(index % 2) as usize].with_alpha(0.0), Vec2::ONE),
                        Transform::default(),
                        Pickable::IGNORE,
                        Part::Beam(index),
                    ));
                }
                parent.spawn((
                    Text2d::new(label),
                    TextFont {
                        font: assets.font("bold").into(),
                        font_size: 15.0.into(),
                        ..default()
                    },
                    TextColor(player.color().color().with_alpha(0.0)),
                    Transform::from_xyz(0.0, 70.0, 1.0),
                    Pickable::IGNORE,
                    Part::Label,
                ));
            });
    }
}

fn animate(
    mut commands: Commands,
    time: Res<Time>,
    settings: Res<Settings>,
    game_state: Res<State<GameState>>,
    mut roots: Query<(Entity, &mut InterceptEffect, &Children, &mut Visibility)>,
    mut parts: Query<(&Part, &mut Transform, Option<&mut Sprite>, Option<&mut TextColor>)>,
    mut suppressed: ResMut<SuppressedMapMissions>,
    mut audio: MessageWriter<PlayAudioMsg>,
) {
    for (entity, mut effect, children, mut visibility) in &mut roots {
        if effect.turn != settings.turn || effect.timer.is_finished() {
            for mission in &effect.missions {
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
        let t = effect.timer.elapsed_secs();
        if t >= IMPACT && !effect.sounded {
            effect.sounded = true;
            audio.write(PlayAudioMsg::new("short explosion"));
        }
        for child in children.iter() {
            let Ok((part, mut transform, sprite, text)) = parts.get_mut(child) else {
                continue;
            };
            match *part {
                Part::Ship {
                    side,
                    survives,
                    art,
                } => {
                    let position = ship_position(side, t);
                    let direction =
                        ship_position(side, t + 0.01) - ship_position(side, (t - 0.01).max(0.0));
                    transform.translation = position.extend(0.2);
                    transform.rotation = Quat::from_rotation_z(if art.upright {
                        0.0
                    } else {
                        direction.to_angle() + art.rotation_offset
                    });
                    if let Some(mut sprite) = sprite {
                        sprite.color.set_alpha(ship_alpha(survives, t));
                        sprite.flip_y = mission_map_flip_y(art.image, direction);
                    }
                },
                Part::Exhaust {
                    side,
                    survives,
                    size,
                    last_frame,
                } => {
                    let position = ship_position(side, t);
                    let direction = (ship_position(side, t + 0.01)
                        - ship_position(side, (t - 0.01).max(0.0)))
                    .normalize_or_zero();
                    transform.translation = (position - direction * size * 0.5).extend(0.1);
                    transform.rotation = Quat::from_rotation_z(direction.to_angle() + PI);
                    transform.scale = Vec3::splat(0.28);
                    if let Some(mut sprite) = sprite {
                        sprite.color.set_alpha(ship_alpha(survives, t));
                        if let Some(atlas) = &mut sprite.texture_atlas {
                            atlas.index = (t * 12.0) as usize % last_frame.saturating_add(1);
                        }
                    }
                },
                Part::Beam(index) => {
                    let side = index % 2;
                    let phase = (t - 1.05 - index as f32 * 0.2) / 0.9;
                    let q = phase.rem_euclid(1.0);
                    let active = t > 1.05 + index as f32 * 0.2 && t < IMPACT && q < 0.22;
                    let start = ship_position(side, t);
                    let end = ship_position(1 - side, t);
                    let head = start.lerp(end, (q / 0.22).clamp(0.0, 1.0));
                    transform.translation = head.extend(0.3);
                    transform.rotation = Quat::from_rotation_z((end - start).to_angle());
                    if let Some(mut sprite) = sprite {
                        sprite.custom_size = Some(Vec2::new(10.0, 1.25));
                        sprite.color.set_alpha(if active {
                            0.65
                        } else {
                            0.0
                        });
                    }
                },
                Part::Label => {
                    let (y, alpha) =
                        super::aftermath_label_motion(70.0, t, 3.5, DURATION, 0.5, 1.0);
                    transform.translation.y = y;
                    if let Some(mut text) = text {
                        text.0.set_alpha(alpha * 0.8);
                    }
                },
            }
        }
    }
}

pub(crate) struct InterceptionAnimationPlugin;

impl Plugin for InterceptionAnimationPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<InterceptSites>()
            .add_systems(OnEnter(AppState::Game), initialize.after(draw_map))
            .add_systems(
                Update,
                (show, animate)
                    .chain()
                    .in_set(BattleAftermathSet)
                    .after(refresh_gameplay_projection)
                    .after(refresh_turn_draft)
                    .run_if(in_state(AppState::Game)),
            );
    }
}

#[cfg(test)]
#[path = "../../../tests/core/map_interception.rs"]
mod tests;
