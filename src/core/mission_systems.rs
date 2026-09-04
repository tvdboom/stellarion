//! Bevy mission rendering, route animation, and command submission.

use std::f32::consts::{PI, TAU};
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
use crate::core::missions::{Mission, MissionRouteStyle, Missions, SendMissionMsg};
use crate::core::player::Player;
use crate::core::simulation::TurnCommand;
use crate::core::ui::systems::{MissionTab, UiState};
use crate::core::units::{Amount, Army};
use crate::multiplayer::client::{MultiplayerSession, PendingTurnCommands};

const MISSION_ROUTE_SPACING: f32 = 52.0;
// Three forward-facing ASCII arcs form a wave packet without relying on Unicode glyph coverage.
// Rotating the packet into the route direction makes its motion read as a travelling wave.
const JUMP_GATE_ROUTE_GLYPH: &str = ")))";
const MISSION_SIZE: f32 = 50.0;
const MISSION_HOVER_SIZE: f32 = 60.0;
const WAR_SUN_MISSION_SIZE: f32 = 50.0;
const WAR_SUN_MISSION_HOVER_SIZE: f32 = 60.0;
const COLONY_SHIP_MISSION_SIZE: f32 = 44.0;
const COLONY_SHIP_MISSION_HOVER_SIZE: f32 = 53.0;
const SPY_MISSION_SIZE: f32 = 36.0;
const SPY_MISSION_HOVER_SIZE: f32 = 43.0;
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

fn mission_map_rotation(mission: &Mission) -> f32 {
    if mission.return_objective.unwrap_or(mission.objective) == Icon::Spy {
        SPY_MISSION_MAP_ROTATION
    } else {
        0.0
    }
}

fn mission_flame_transform(size: f32, map_rotation: f32) -> Transform {
    let distance = size * 0.5;
    Transform {
        // Counter-rotate the child offset so the flame remains behind the route while the probe
        // artwork turns to place its lower exhaust over that anchor.
        translation: Vec3::new(-distance * map_rotation.cos(), distance * map_rotation.sin(), -0.1),
        scale: Vec3::splat(0.35),
        rotation: Quat::from_rotation_z(PI - map_rotation),
    }
}

#[derive(Component)]
/// One animated chevron in the hovered mission's origin or destination trail.
pub struct MissionRouteArrowCmp {
    index: usize,
    style: MissionRouteStyle,
}

/// Advances the visible ECS mission projection after canonical turn installation.
pub fn update_missions(
    mut commands: Commands,
    mut mission_q: Query<(Entity, &mut Sprite, &mut Transform, &MissionCmp)>,
    state: Res<UiState>,
    map: Res<Map>,
    player: Res<Player>,
    missions: Res<Missions>,
    assets: Res<WorldAssets>,
    session: Res<MultiplayerSession>,
) {
    let player_id = player.id;

    for mission in missions.iter() {
        if !mission_q.iter().any(|(_, _, _, m)| m.id == mission.id) {
            let id = mission.id;
            let owner = mission.owner;

            let destination = map.get(mission.destination);

            let direction = (-mission.position + destination.position).normalize();
            let angle = direction.y.atan2(direction.x);
            let size = mission_size(mission, false);
            let map_rotation = mission_map_rotation(mission);
            let image = mission.image(&player);

            let texture = assets.texture("flame");
            commands
                .spawn((
                    Sprite {
                        image: assets.image(image),
                        color: session.player_color(owner).color(),
                        custom_size: Some(Vec2::splat(size)),
                        flip_y: mission_map_flip_y(image, direction),
                        ..default()
                    },
                    Transform {
                        translation: mission.position.extend(MISSION_Z),
                        rotation: Quat::from_rotation_z(angle + map_rotation),
                        ..default()
                    },
                    Pickable::default(),
                    MissionCmp::new(id),
                    MapCmp,
                    children![(
                        Sprite::from_atlas_image(texture.image, texture.atlas),
                        mission_flame_transform(size, map_rotation),
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

    for (mission_e, mut mission_s, mut mission_t, mission_c) in &mut mission_q {
        if let Some(mission) = missions.iter().find(|m| m.id == mission_c.id) {
            // Update the direction the image is pointing at
            // Could change if the destination planet was destroyed
            let destination = map.get(mission.destination);

            let direction = (-mission.position + destination.position).normalize();
            let angle = direction.y.atan2(direction.x);
            let image = mission.image(&player);

            mission_t.rotation = Quat::from_rotation_z(angle + mission_map_rotation(mission));
            mission_s.image = assets.image(image);
            mission_s.color = session.player_color(mission.owner).color();
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
        MissionRouteStyle::MissileStrike => 58.0,
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
            let phase = (index as f32 * 0.91 + offset / spacing) * TAU;
            let (rotation, scale) = match style {
                MissionRouteStyle::Standard => (route_rotation, Vec3::ONE),
                MissionRouteStyle::JumpGate => {
                    let pulse = 0.9 + 0.2 * (phase.sin() * 0.5 + 0.5);
                    (route_rotation, Vec3::new(0.9, pulse, 1.0))
                },
                MissionRouteStyle::MissileStrike => {
                    let pulse = 0.85 + 0.2 * (phase.sin() * 0.5 + 0.5);
                    (route_rotation, Vec3::new(1.55 * pulse, 0.62 * pulse, 1.0))
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
        MissionRouteStyle::MissileStrike => 58.0,
    };
    let animation_speed = mission.route_animation_speed();
    // Motion is measured in world units, so speed and spacing do not depend on route length.
    let offset =
        |speed: f64| (time.elapsed_secs_f64() * speed).rem_euclid(f64::from(spacing)) as f32;
    let arrows = mission_route_markers(
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
    .collect::<Vec<_>>();
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
            MissionRouteStyle::MissileStrike => ("•", 25.0),
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
            crate::core::orders::validate_mission(&player, origin, destination, mission).is_ok()
        });
        if !valid
            || !pending.is_editable()
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

#[cfg(test)]
#[path = "../../tests/core/missions_systems_player_color.rs"]
mod player_color_tests;

#[cfg(test)]
#[path = "../../tests/core/missions_systems_audio.rs"]
mod audio_tests;
