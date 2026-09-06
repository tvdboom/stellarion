//! Bevy mission rendering, route animation, and command submission.

use std::f32::consts::PI;
use std::time::Duration;

use bevy::prelude::*;
use bevy::window::SystemCursorIcon;
use bevy_tweening::{RepeatCount, Tween, TweenAnim};

use crate::core::assets::WorldAssets;
use crate::core::audio::{PlayAudioMsg, SoundEffect};
use crate::core::constants::MISSION_Z;
use crate::core::map::icon::Icon;
use crate::core::map::model::{Map, MapCmp};
use crate::core::map::systems::MissionCmp;
use crate::core::map::utils::{cursor, SpriteFrameLens};
use crate::core::messages::MessageMsg;
use crate::core::missions::{
    Mission, MissionRouteStyle, Missions, RecallMissionMsg, SendMissionMsg,
    SuppressedReturningSpies,
};
use crate::core::player::Player;
use crate::core::settings::Settings;
use crate::core::simulation::TurnCommand;
use crate::core::ui::systems::{MissionTab, UiState};
use crate::core::units::{Amount, Army};
use crate::multiplayer::client::{MultiplayerSession, PendingTurnCommands};

const MISSION_ROUTE_SPACING: f32 = 52.0;
// A closed ring is foreshortened along the route: it faces the travelling ship, not the camera.
const JUMP_GATE_ROUTE_GLYPH: &str = "O";
const MISSION_SIZE: f32 = 50.0;
const MISSION_HOVER_SIZE: f32 = 60.0;
const WAR_SUN_MISSION_SIZE: f32 = 50.0;
const WAR_SUN_MISSION_HOVER_SIZE: f32 = 60.0;
const COLONY_SHIP_MISSION_SIZE: f32 = 44.0;
const COLONY_SHIP_MISSION_HOVER_SIZE: f32 = 53.0;
pub(crate) const SPY_MISSION_SIZE: f32 = 36.0;
const SPY_MISSION_HOVER_SIZE: f32 = 43.0;
const RECALL_ANIMATION_SECONDS: f32 = 1.25;
const RECALL_TURN_SECONDS: f32 = 0.65;
const RECALL_PULSE_COUNT: usize = 3;
const RECALL_PULSE_INTERVAL_SECONDS: f32 = 0.16;
const RECALL_PULSE_SECONDS: f32 = 0.85;
// The probe artwork's exhaust is diagonal. Rotate only its map presentation so that exhaust
// aligns with the route's trailing flame without changing the shared source image.
const SPY_MISSION_MAP_ROTATION: f32 = -PI / 4.0;

fn mission_size(mission: &Mission, hovered: bool) -> f32 {
    let image_objective = mission.return_objective.unwrap_or(mission.objective);
    if image_objective == Icon::Colonize || mission.uses_colony_ship_image() {
        return if hovered {
            COLONY_SHIP_MISSION_HOVER_SIZE
        } else {
            COLONY_SHIP_MISSION_SIZE
        };
    }

    match (image_objective, mission.uses_war_sun_image(), hovered) {
        (_, true, true) => WAR_SUN_MISSION_HOVER_SIZE,
        (_, true, false) => WAR_SUN_MISSION_SIZE,
        (Icon::Spy, false, true) => SPY_MISSION_HOVER_SIZE,
        (Icon::Spy, false, false) => SPY_MISSION_SIZE,
        (_, false, true) => MISSION_HOVER_SIZE,
        (_, false, false) => MISSION_SIZE,
    }
}

/// Mirrors left-bound colony artwork before route rotation so its habitat stays above the hull.
fn mission_map_flip_y(image: &str, direction: Vec2) -> bool {
    image == "mission colonize" && direction.x < 0.0
}

/// Keeps a fleet facing along its route when it is already sitting on the destination point.
fn mission_map_direction(mission: &Mission, map: &Map) -> Vec2 {
    let destination = map.get(mission.destination);
    let remaining = destination.position - mission.position;
    if remaining.length_squared() > f32::EPSILON {
        remaining.normalize()
    } else {
        (destination.position - map.get(mission.origin).position).normalize_or_zero()
    }
}

fn mission_map_rotation(mission: &Mission) -> f32 {
    if mission.return_objective.unwrap_or(mission.objective) == Icon::Spy {
        SPY_MISSION_MAP_ROTATION
    } else {
        0.0
    }
}

fn mission_world_rotation(mission: &Mission, route_angle: f32) -> f32 {
    if mission.return_objective == Some(Icon::Spy) {
        0.0
    } else {
        route_angle + mission_map_rotation(mission)
    }
}

fn mission_map_flip_x(mission: &Mission) -> bool {
    mission.return_objective == Some(Icon::Spy)
}

fn mission_flame_transform(size: f32, route_angle: f32, image_rotation: f32) -> Transform {
    let distance = size * 0.5;
    let relative_angle = route_angle + PI - image_rotation;
    Transform {
        // Counter-rotate the child so its world-space position remains behind the route even
        // when upright returning-spy artwork no longer rotates with that route.
        translation: Vec3::new(
            distance * relative_angle.cos(),
            distance * relative_angle.sin(),
            -0.1,
        ),
        scale: Vec3::splat(0.35),
        rotation: Quat::from_rotation_z(relative_angle),
    }
}

#[derive(Component)]
/// One animated chevron or wave front in the hovered mission's route.
pub struct MissionRouteArrowCmp {
    index: usize,
    style: MissionRouteStyle,
}

#[derive(Message)]
#[doc(hidden)]
pub struct MissionRecallAnimationMsg {
    mission_id: u64,
    position: Vec2,
    color: Color,
    radius: f32,
    from_rotation: Quat,
    from_flip_x: bool,
    from_flip_y: bool,
    to_rotation: Quat,
    to_flip_x: bool,
    to_flip_y: bool,
}

#[derive(Component)]
pub(crate) struct MissionRecallAnimation {
    timer: Timer,
    from_rotation: Quat,
    from_flip_x: bool,
    from_flip_y: bool,
    to_rotation: Quat,
    to_flip_x: bool,
    to_flip_y: bool,
}

#[derive(Component)]
pub(crate) struct MissionRecallEffect {
    timer: Timer,
}

#[derive(Component)]
pub(crate) struct MissionRecallPulse {
    delay: f32,
    radius: f32,
}

/// Advances the visible ECS mission projection after canonical turn installation.
pub fn update_missions(
    mut commands: Commands,
    mut mission_q: Query<(Entity, &mut Sprite, &mut Transform, &mut Visibility, &MissionCmp)>,
    state: Res<UiState>,
    map: Res<Map>,
    player: Res<Player>,
    missions: Res<Missions>,
    assets: Res<WorldAssets>,
    session: Res<MultiplayerSession>,
    suppressed_spies: Option<Res<SuppressedReturningSpies>>,
) {
    let player_id = player.id;

    for mission in missions.iter() {
        if !mission_q.iter().any(|(_, _, _, _, m)| m.id == mission.id) {
            let id = mission.id;
            let owner = mission.owner;

            let direction = mission_map_direction(mission, &map);
            let angle = direction.y.atan2(direction.x);
            let size = mission_size(mission, false);
            let image_rotation = mission_world_rotation(mission, angle);
            let image = mission.image(&player);

            let texture = assets.texture("flame");
            commands
                .spawn((
                    Sprite {
                        image: assets.image(image),
                        color: session.player_color(owner).color(),
                        custom_size: Some(Vec2::splat(size)),
                        flip_x: mission_map_flip_x(mission),
                        flip_y: mission_map_flip_y(image, direction),
                        ..default()
                    },
                    Transform {
                        translation: mission.position.extend(MISSION_Z),
                        rotation: Quat::from_rotation_z(image_rotation),
                        ..default()
                    },
                    Pickable::default(),
                    if suppressed_spies.as_ref().is_some_and(|suppressed| suppressed.contains(id)) {
                        Visibility::Hidden
                    } else {
                        Visibility::Inherited
                    },
                    MissionCmp::new(id),
                    MapCmp,
                    children![(
                        Sprite::from_atlas_image(texture.image, texture.atlas),
                        mission_flame_transform(size, angle, image_rotation),
                        TweenAnim::new(
                            Tween::new(
                                EaseFunction::Linear,
                                Duration::from_millis(1000),
                                SpriteFrameLens(texture.last_index),
                            )
                            .with_repeat_count(RepeatCount::Infinite),
                        ),
                    )],
                ))
                .observe(cursor::<Over>(SystemCursorIcon::Pointer))
                .observe(cursor::<Out>(SystemCursorIcon::Default))
                .observe(move |_: On<Pointer<Over>>, mut state: ResMut<UiState>| {
                    state.mission_hover = Some(id);
                    state.mission_hover_from_ui = false;
                })
                .observe(|_: On<Pointer<Out>>, mut state: ResMut<UiState>| {
                    state.mission_hover = None;
                    state.mission_hover_from_ui = false;
                })
                .observe(move |event: On<Pointer<Click>>, mut state: ResMut<UiState>| {
                    if event.button == PointerButton::Primary {
                        state.mission = true;
                        state.planet_selected = None;
                        state.mission_tab = if owner == player_id {
                            MissionTab::ActiveMissions
                        } else {
                            MissionTab::EnemyMissions
                        }
                    }
                });
        }
    }

    for (mission_e, mut mission_s, mut mission_t, mut visibility, mission_c) in &mut mission_q {
        if let Some(mission) = missions.iter().find(|m| m.id == mission_c.id) {
            *visibility = if suppressed_spies
                .as_ref()
                .is_some_and(|suppressed| suppressed.contains(mission.id))
            {
                Visibility::Hidden
            } else {
                Visibility::Inherited
            };
            // Update the direction the image is pointing at
            // Could change if the destination planet was destroyed
            let direction = mission_map_direction(mission, &map);
            let angle = direction.y.atan2(direction.x);
            let image = mission.image(&player);

            mission_t.rotation = Quat::from_rotation_z(mission_world_rotation(mission, angle));
            mission_s.image = assets.image(image);
            mission_s.color = session.player_color(mission.owner).color();
            mission_s.flip_x = mission_map_flip_x(mission);
            mission_s.flip_y = mission_map_flip_y(image, direction);

            if state.mission_hover.is_some_and(|id| id == mission.id) {
                // Lift above other missions while staying below the planet's icons.
                mission_t.translation = mission.position.extend(MISSION_Z + 0.1);
                // Size, rather than a blue/red texture swap, indicates hover without losing identity.
                mission_s.custom_size = Some(Vec2::splat(mission_size(mission, true)));
            } else {
                mission_t.translation = mission.position.extend(MISSION_Z);
                mission_s.custom_size = Some(Vec2::splat(mission_size(mission, false)));
            }
        } else {
            commands.entity(mission_e).despawn();
        }
    }
}

/// Places evenly spaced route markers inside a route's endpoint clearances.
fn mission_route_markers(
    from: Vec2,
    to: Vec2,
    start_clearance: f32,
    end_clearance: f32,
    color: Color,
    offset: f32,
    style: MissionRouteStyle,
) -> Vec<(Transform, TextColor)> {
    let route = to - from;
    let direction = route.normalize_or_zero();
    let start = from + direction * start_clearance;
    let spacing = match style {
        MissionRouteStyle::Standard => MISSION_ROUTE_SPACING,
        MissionRouteStyle::JumpGate => 64.0,
    };
    // Subtract clearances before clamping: overlapping endpoints must never reverse the trail.
    let length = (route.length() - start_clearance - end_clearance).max(0.0);
    let count = (length / spacing).ceil() as usize;
    let route_rotation = Quat::from_rotation_z(direction.y.atan2(direction.x));

    (0..count)
        .filter_map(move |index| {
            let distance = index as f32 * spacing + offset;
            if distance >= length {
                return None;
            }
            // Fade whole glyphs at the edges instead of squeezing or clipping them to fit.
            let fade = (distance.min(length - distance) / 18.0).clamp(0.0, 1.0);
            let (rotation, scale) = match style {
                MissionRouteStyle::Standard => (route_rotation, Vec3::ONE),
                MissionRouteStyle::JumpGate => {
                    let expansion = 0.5 + 2.5 * distance / length;
                    (route_rotation, Vec3::new(0.4 * expansion, expansion, 1.0))
                },
            };
            Some((
                Transform {
                    // Route trails sit above planets, behind missions and planet icons.
                    translation: (start + direction * distance).extend(MISSION_Z - 0.2),
                    rotation,
                    scale,
                },
                TextColor(color.with_alpha(color.alpha() * fade)),
            ))
        })
        .collect()
}

/// Animates the travelled and remaining route of the hovered mission.
pub fn update_mission_route_arrow(
    mut commands: Commands,
    mut arrow_q: Query<(Entity, &mut Transform, &mut TextColor, &MissionRouteArrowCmp)>,
    state: Res<UiState>,
    map: Res<Map>,
    player: Res<Player>,
    missions: Res<Missions>,
    session: Res<MultiplayerSession>,
    assets: Res<WorldAssets>,
    time: Res<Time>,
) {
    let Some(mission) = state.mission_hover.and_then(|id| missions.get(id)) else {
        for (entity, _, _, _) in &mut arrow_q {
            commands.entity(entity).despawn();
        }
        return;
    };

    let origin = map.get(mission.origin);
    let destination = map.get(mission.destination);
    let style = mission.route_style(&player);
    let spacing = match style {
        MissionRouteStyle::Standard => MISSION_ROUTE_SPACING,
        MissionRouteStyle::JumpGate => 64.0,
    };
    let animation_speed = mission.route_animation_speed();
    // Motion is measured in world units, so speed and spacing do not depend on route length.
    let offset =
        |speed: f64| (time.elapsed_secs_f64() * speed).rem_euclid(f64::from(spacing)) as f32;
    let arrows = if style == MissionRouteStyle::JumpGate {
        // Keep one continuous wave from origin to destination, including across the fleet.
        mission_route_markers(
            origin.position,
            destination.position,
            origin.size() * 0.7,
            destination.size() * 0.7,
            session.player_color(mission.owner).color(),
            offset(animation_speed * 0.5),
            style,
        )
    } else {
        mission_route_markers(
            origin.position,
            mission.position,
            origin.size() * 0.7,
            48.0,
            Color::srgba(0.72, 0.77, 0.84, 0.55),
            offset(animation_speed * 0.625),
            style,
        )
        .into_iter()
        .chain(mission_route_markers(
            mission.position,
            destination.position,
            38.0,
            destination.size() * 0.7,
            session.player_color(mission.owner).color(),
            offset(animation_speed),
            style,
        ))
        .collect::<Vec<_>>()
    };
    let mut present = vec![false; arrows.len()];

    for (entity, mut transform, mut text_color, arrow) in &mut arrow_q {
        if arrow.style != style {
            commands.entity(entity).despawn();
            continue;
        }
        let Some((next_transform, next_color)) = arrows.get(arrow.index) else {
            commands.entity(entity).despawn();
            continue;
        };
        present[arrow.index] = true;
        *transform = *next_transform;
        *text_color = *next_color;
    }

    for (index, (transform, text_color)) in arrows.into_iter().enumerate() {
        if present[index] {
            continue;
        }
        let (glyph, font_size) = match style {
            MissionRouteStyle::Standard => (">", 28.0),
            MissionRouteStyle::JumpGate => (JUMP_GATE_ROUTE_GLYPH, 26.0),
        };
        commands.spawn((
            Text2d::new(glyph),
            TextFont {
                font: assets.font("bold").into(),
                font_size: font_size.into(),
                ..default()
            },
            text_color,
            transform,
            Pickable::IGNORE,
            MissionRouteArrowCmp {
                index,
                style,
            },
            MapCmp,
        ));
    }
}

/// Validates the selected mission UI, removes committed units/resources, and emits its command.
pub fn send_mission(
    mut send_mission: MessageReader<SendMissionMsg>,
    mut message: MessageWriter<MessageMsg>,
    mut play_audio: MessageWriter<PlayAudioMsg>,
    mut map: ResMut<Map>,
    mut player: ResMut<Player>,
    mut missions: ResMut<Missions>,
    mut pending: ResMut<PendingTurnCommands>,
) {
    for SendMissionMsg {
        mission,
    } in send_mission.read()
    {
        let worlds = map
            .planets
            .iter()
            .find(|p| p.id == mission.origin)
            .zip(map.planets.iter().find(|p| p.id == mission.destination));
        let valid = worlds.is_some_and(|(origin, destination)| {
            crate::core::orders::validate_mission(&player, &map, origin, destination, mission)
                .is_ok()
        });
        if !valid
            || !pending.can_accept_commands()
            || mission.fuel_consumption(&map) > player.resources.deuterium
        {
            message.write(MessageMsg::error(
                "This mission is unavailable. Continue your turn before changing orders.",
            ));
            continue;
        }
        let army = mission
            .army
            .iter()
            .filter(|(_, count)| **count > 0)
            .map(|(unit, count)| (*unit, *count))
            .collect::<Army>();
        if !pending.push(TurnCommand::SendMission {
            mission_id: mission.id,
            origin: mission.origin,
            destination: mission.destination,
            objective: mission.objective,
            army,
            bombing: mission.bombing.clone(),
            combat_probes: mission.combat_probes,
            jump_gate: mission.jump_gate,
        }) {
            message.write(MessageMsg::error(
                "This turn already contains the maximum number of commands.",
            ));
            continue;
        }
        player.resources.deuterium =
            player.resources.deuterium.saturating_sub(mission.fuel_consumption(&map));

        let origin = map.get_mut(mission.origin);

        if mission.jump_gate {
            origin.jump_gate = origin.jump_gate.saturating_add(mission.jump_cost());
        }

        // Subtract armies from the origin planet
        origin.army.iter_mut().for_each(|(u, c)| {
            *c = c.saturating_sub(mission.army.amount(u));
        });

        // Keep the immediate map projection aligned with the deterministic turn preview.
        origin.release_control_if_vacant();

        missions.0.push(mission.clone());

        play_audio.write(SoundEffect::MissionLaunched.request());
        message.write(MessageMsg::info("Mission sent.").silent());
    }
}

/// Validates an owned active mission, queues its deterministic recall, and updates the preview.
pub fn recall_mission(
    mut recalls: MessageReader<RecallMissionMsg>,
    mut message: MessageWriter<MessageMsg>,
    mut animations: MessageWriter<MissionRecallAnimationMsg>,
    map: Res<Map>,
    player: Res<Player>,
    settings: Res<Settings>,
    mut missions: ResMut<Missions>,
    mut pending: ResMut<PendingTurnCommands>,
) {
    for RecallMissionMsg {
        mission_id,
    } in recalls.read()
    {
        let Some(mission) = missions.0.iter_mut().find(|mission| mission.id == *mission_id) else {
            message.write(MessageMsg::error("This mission is no longer active."));
            continue;
        };
        if mission.owner != player.id || mission.is_returning() || !pending.can_accept_commands() {
            message.write(MessageMsg::error(
                "This mission cannot be recalled. Continue your turn before changing orders.",
            ));
            continue;
        }
        if !pending.push(TurnCommand::RecallMission {
            mission_id: *mission_id,
        }) {
            message.write(MessageMsg::error(
                "This turn already contains the maximum number of commands.",
            ));
            continue;
        }

        let from_direction = mission_map_direction(mission, &map);
        let image = mission.image(&player).to_owned();
        let from_angle = from_direction.y.atan2(from_direction.x);
        let from_rotation = Quat::from_rotation_z(mission_world_rotation(mission, from_angle));
        let from_flip_x = mission_map_flip_x(mission);
        let from_flip_y = mission_map_flip_y(&image, from_direction);
        let position = mission.position;
        let radius = mission_size(mission, false) * 1.25;

        mission.recall(&map, settings.turn);
        let to_direction = mission_map_direction(mission, &map);
        let to_angle = to_direction.y.atan2(to_direction.x);
        animations.write(MissionRecallAnimationMsg {
            mission_id: *mission_id,
            position,
            color: player.color().color(),
            radius,
            from_rotation,
            from_flip_x,
            from_flip_y,
            to_rotation: Quat::from_rotation_z(mission_world_rotation(mission, to_angle)),
            to_flip_x: mission_map_flip_x(mission),
            to_flip_y: mission_map_flip_y(&image, to_direction),
        });
        message.write(MessageMsg::info("Mission recalled.").silent());
    }
}

/// Plays a map-only recall cue while the authoritative mission is already returning.
pub(crate) fn animate_mission_recalls(
    mut commands: Commands,
    time: Res<Time>,
    mut starts: MessageReader<MissionRecallAnimationMsg>,
    mut missions: Query<
        (Entity, &MissionCmp, &mut Sprite, &mut Transform, Option<&mut MissionRecallAnimation>),
        Without<MissionRecallPulse>,
    >,
    mut effects: Query<(Entity, &mut MissionRecallEffect, &Children)>,
    mut pulses: Query<
        (&MissionRecallPulse, &mut Transform, &MeshMaterial2d<ColorMaterial>),
        Without<MissionCmp>,
    >,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    for start in starts.read() {
        let Some((entity, _, mut sprite, mut transform, animation)) =
            missions.iter_mut().find(|(_, mission, _, _, _)| mission.id == start.mission_id)
        else {
            continue;
        };
        if animation.is_some() {
            continue;
        }

        transform.rotation = start.from_rotation;
        transform.scale = Vec3::ONE;
        sprite.flip_x = start.from_flip_x;
        sprite.flip_y = start.from_flip_y;
        commands.entity(entity).insert(MissionRecallAnimation {
            timer: Timer::from_seconds(RECALL_ANIMATION_SECONDS, TimerMode::Once),
            from_rotation: start.from_rotation,
            from_flip_x: start.from_flip_x,
            from_flip_y: start.from_flip_y,
            to_rotation: start.to_rotation,
            to_flip_x: start.to_flip_x,
            to_flip_y: start.to_flip_y,
        });

        let ring = meshes.add(Annulus::new(0.965, 1.0));
        commands
            .spawn((
                Transform::from_translation(start.position.extend(0.0)),
                Visibility::Inherited,
                Pickable::IGNORE,
                MapCmp,
                MissionRecallEffect {
                    timer: Timer::from_seconds(RECALL_ANIMATION_SECONDS, TimerMode::Once),
                },
            ))
            .with_children(|parent| {
                for index in 0..RECALL_PULSE_COUNT {
                    parent.spawn((
                        Mesh2d(ring.clone()),
                        MeshMaterial2d(materials.add(start.color.with_alpha(0.0))),
                        Transform::from_xyz(0.0, 0.0, MISSION_Z + 0.2),
                        Pickable::IGNORE,
                        MissionRecallPulse {
                            delay: index as f32 * RECALL_PULSE_INTERVAL_SECONDS,
                            radius: start.radius,
                        },
                    ));
                }
            });
    }

    for (entity, _, mut sprite, mut transform, animation) in &mut missions {
        let Some(mut animation) = animation else {
            continue;
        };
        animation.timer.tick(time.delta());
        let elapsed = animation.timer.elapsed_secs();

        let progress = (elapsed / RECALL_TURN_SECONDS).clamp(0.0, 1.0);
        let eased = progress * progress * (3.0 - 2.0 * progress);
        transform.rotation = animation.from_rotation.slerp(animation.to_rotation, eased);
        transform.scale = Vec3::ONE;
        if progress < 1.0 {
            sprite.flip_x = animation.from_flip_x;
            sprite.flip_y = animation.from_flip_y;
        } else {
            sprite.flip_x = animation.to_flip_x;
            sprite.flip_y = animation.to_flip_y;
        }

        if animation.timer.is_finished() {
            transform.scale = Vec3::ONE;
            commands.entity(entity).remove::<MissionRecallAnimation>();
        }
    }

    for (effect_entity, mut effect, children) in &mut effects {
        effect.timer.tick(time.delta());
        if effect.timer.is_finished() {
            commands.entity(effect_entity).despawn();
            continue;
        }
        let elapsed = effect.timer.elapsed_secs();
        for child in children.iter() {
            let Ok((pulse, mut transform, material)) = pulses.get_mut(child) else {
                continue;
            };
            let progress = ((elapsed - pulse.delay) / RECALL_PULSE_SECONDS).clamp(0.0, 1.0);
            transform.scale = Vec3::splat(pulse.radius * (0.08 + progress));
            let alpha = (progress * 10.0).min(1.0) * (1.0 - progress).powi(2) * 0.8;
            if let Some(mut material) = materials.get_mut(&material.0) {
                material.color.set_alpha(alpha);
            }
        }
    }
}

#[cfg(test)]
#[path = "../../tests/core/missions_systems_player_color.rs"]
mod player_color_tests;

#[cfg(test)]
#[path = "../../tests/core/missions_systems_audio.rs"]
mod audio_tests;

#[cfg(test)]
#[path = "../../tests/core/missions_recall_animation.rs"]
mod recall_animation_tests;
