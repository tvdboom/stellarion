//! Client-only presentation of colony ownership changes; never changes the simulation.

use std::collections::{BTreeMap, BTreeSet};
use std::f32::consts::TAU;

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::prelude::*;

use super::model::{Map, MapCmp};
use super::planet::{Planet, PlanetId};
use super::systems::{draw_map, VoronoiCmp};
use crate::core::assets::WorldAssets;
use crate::core::constants::{PLANET_Z, VORONOI_Z};
use crate::core::loading::{refresh_gameplay_projection, refresh_turn_draft};
use crate::core::messages::{MessageAction, MessageMsg};
use crate::core::player::Player;
use crate::core::settings::Settings;
use crate::core::simulation::TurnCommand;
use crate::core::states::{AppState, GameState};
use crate::multiplayer::client::PendingTurnCommands;

const CELEBRATION_SECONDS: f32 = 4.2;
const WAVE_BANDS: usize = 4;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum ColonyEvent {
    Colonized,
    Conquered,
    Abandoned,
}

#[derive(Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
enum ColonyAnnouncement {
    Acquired,
    Abandoned,
}

impl ColonyEvent {
    fn for_world(player: &Player, planet: PlanetId, turn: usize) -> Self {
        if player.reports.iter().rev().any(|report| {
            report.turn == turn
                && report.mission.destination == planet
                && report.mission.owner == player.id
                && report.planet_colonized
                && report.planet.has_buildings()
        }) {
            Self::Conquered
        } else {
            Self::Colonized
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Colonized => "PLANET COLONIZED",
            Self::Conquered => "PLANET CONQUERED",
            Self::Abandoned => "PLANET ABANDONED",
        }
    }

    fn message(self, planet: &Planet) -> String {
        match self {
            Self::Colonized => format!("Planet {} has been colonized.", planet.name),
            Self::Conquered => format!("Planet {} has been conquered.", planet.name),
            Self::Abandoned => format!("Planet {} has been abandoned.", planet.name),
        }
    }

    fn still_applies(self, planet: &Planet, player: &Player) -> bool {
        if planet.is_destroyed {
            return false;
        }
        match self {
            Self::Colonized | Self::Conquered => player.owns(planet),
            Self::Abandoned => planet.owned.is_none(),
        }
    }

    fn announcement(self) -> ColonyAnnouncement {
        match self {
            Self::Colonized | Self::Conquered => ColonyAnnouncement::Acquired,
            Self::Abandoned => ColonyAnnouncement::Abandoned,
        }
    }
}

/// Remembers ownership across projections, including immediate local command previews.
#[derive(Resource, Default)]
struct Colonies {
    owned: BTreeSet<PlanetId>,
    pending: BTreeMap<PlanetId, ColonyEvent>,
    announced: BTreeSet<(PlanetId, ColonyAnnouncement)>,
    turn: usize,
}

fn owned_colonies(map: &Map, player: &Player) -> BTreeSet<PlanetId> {
    map.planets
        .iter()
        .filter(|planet| player.owns(planet) && !planet.is_destroyed && !planet.is_moon())
        .map(|planet| planet.id)
        .collect()
}

impl Colonies {
    fn observe(
        &mut self,
        map: &Map,
        player: &Player,
        pending_commands: &PendingTurnCommands,
        turn: usize,
    ) -> bool {
        if self.turn != turn {
            self.turn = turn;
            self.announced.clear();
        }
        let owned = owned_colonies(map, player);
        let mut changed = false;
        for &planet in owned.difference(&self.owned) {
            let event = ColonyEvent::for_world(player, planet, turn);
            if !self.announced.contains(&(planet, event.announcement())) {
                changed |= self.pending.insert(planet, event) != Some(event);
            }
        }
        for &planet in self.owned.difference(&owned) {
            let was_abandoned = pending_commands.commands.iter().any(|command| {
                matches!(command, TurnCommand::AbandonPlanet { planet_id } if *planet_id == planet)
            });
            if was_abandoned && !self.announced.contains(&(planet, ColonyAnnouncement::Abandoned)) {
                changed |= self.pending.insert(planet, ColonyEvent::Abandoned)
                    != Some(ColonyEvent::Abandoned);
            }
        }
        self.pending.retain(|planet, event| {
            map.try_get(*planet).is_some_and(|world| event.still_applies(world, player))
        });
        self.owned = owned;
        changed
    }
}

/// Treats existing colonies as the baseline when starting or resuming a game.
fn initialize_colonies(
    mut colonies: ResMut<Colonies>,
    map: Res<Map>,
    player: Res<Player>,
    settings: Res<Settings>,
) {
    *colonies = Colonies {
        owned: owned_colonies(&map, &player),
        turn: settings.turn,
        ..default()
    };
}

#[derive(Component)]
struct ColonyEffect {
    planet: PlanetId,
    event: ColonyEvent,
    timer: Timer,
}

#[derive(Component)]
enum EffectPart {
    Territory {
        boundary: Vec<Vec2>,
        reach: f32,
    },
    Flare {
        size: Vec2,
    },
    Beacon {
        delay: f32,
        radius: f32,
    },
    Label {
        y: f32,
    },
}

/// Queues changes during combat, then announces them once normal map play resumes.
fn celebrate_colonies(
    mut commands: Commands,
    mut colonies: ResMut<Colonies>,
    map: Res<Map>,
    player: Res<Player>,
    pending_commands: Res<PendingTurnCommands>,
    settings: Res<Settings>,
    game_state: Res<State<GameState>>,
    cells: Query<(&VoronoiCmp, &Mesh2d)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    assets: Res<WorldAssets>,
    mut messages: MessageWriter<MessageMsg>,
) {
    if (map.is_changed() || player.is_changed() || settings.turn != colonies.turn)
        && colonies.observe(&map, &player, &pending_commands, settings.turn)
    {
        // The new projection's StartTurnMsg runs next frame and may open combat first.
        return;
    }
    if *game_state.get() != GameState::Playing {
        return;
    }

    for (id, event) in std::mem::take(&mut colonies.pending) {
        let Some(planet) = map.try_get(id).filter(|planet| event.still_applies(planet, &player))
        else {
            continue;
        };
        colonies.announced.insert((id, event.announcement()));
        messages.write(
            MessageMsg::info(event.message(planet)).with_action(MessageAction::FocusColony(id)),
        );
        let boundary = cells.iter().find(|(cell, _)| cell.0 == id).and_then(|(_, mesh)| {
            let positions =
                meshes.get(&mesh.0)?.attribute(Mesh::ATTRIBUTE_POSITION)?.as_float3()?;
            Some(positions.iter().map(|p| Vec2::new(p[0], p[1]) - planet.position).collect())
        });
        spawn_celebration(
            &mut commands,
            planet,
            event,
            player.color().color(),
            boundary,
            &mut meshes,
            &mut materials,
            &assets,
        );
    }
}

fn spawn_celebration(
    commands: &mut Commands,
    planet: &Planet,
    event: ColonyEvent,
    color: Color,
    boundary: Option<Vec<Vec2>>,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    assets: &WorldAssets,
) {
    let size = planet.size();
    let label_y = super::aftermath_label_y(size, 0);
    let glow = meshes.add(glow_mesh());
    let ring = meshes.add(Annulus::new(0.965, 1.0));
    commands
        .spawn((
            Transform::from_translation(planet.position.extend(0.0)),
            Visibility::Inherited,
            Pickable::IGNORE,
            MapCmp,
            ColonyEffect {
                planet: planet.id,
                event,
                timer: Timer::from_seconds(CELEBRATION_SECONDS, TimerMode::Once),
            },
        ))
        .with_children(|parent| {
            if let Some(polygon) = boundary.filter(|p| p.len() >= 3) {
                let boundary = sample_boundary(&polygon);
                // Outer Voronoi cells extend far beyond the playable map; bound the visual.
                let reach = boundary.iter().map(|p| p.length()).fold(0.0, f32::max).min(1000.0);
                let mesh = territory_mesh(&boundary);
                parent.spawn((
                    Mesh2d(meshes.add(mesh)),
                    MeshMaterial2d(materials.add(color.with_alpha(0.0))),
                    Transform::from_xyz(0.0, 0.0, VORONOI_Z + 0.05),
                    Visibility::Inherited,
                    Pickable::IGNORE,
                    EffectPart::Territory {
                        boundary,
                        reach,
                    },
                ));
            }
            // A soft surface flare with two narrow rays, rather than a screen-wide flash.
            for dimensions in [
                Vec2::splat(size * 0.38),
                Vec2::new(size * 0.7, size * 0.035),
                Vec2::new(size * 0.035, size * 0.5),
            ] {
                parent.spawn((
                    Mesh2d(glow.clone()),
                    MeshMaterial2d(materials.add(color.with_alpha(0.0))),
                    Transform::from_xyz(size * 0.18, size * 0.12, PLANET_Z + 1.2),
                    Pickable::IGNORE,
                    EffectPart::Flare {
                        size: dimensions,
                    },
                ));
            }
            for index in 0..3 {
                parent.spawn((
                    Mesh2d(ring.clone()),
                    MeshMaterial2d(materials.add(color.with_alpha(0.0))),
                    Transform::from_xyz(size * 0.18, size * 0.12, PLANET_Z + 1.1),
                    Pickable::IGNORE,
                    EffectPart::Beacon {
                        delay: 0.45 + index as f32 * 0.55,
                        radius: size * 0.95,
                    },
                ));
            }
            parent.spawn((
                Text2d::new(event.label()),
                TextFont {
                    font: assets.font("bold").into(),
                    font_size: 17.0.into(),
                    ..default()
                },
                TextColor(color.with_alpha(0.0)),
                Transform::from_xyz(0.0, label_y, PLANET_Z + 1.3),
                Pickable::IGNORE,
                EffectPart::Label {
                    y: label_y,
                },
            ));
        });
}

/// Smooth radial alpha falloff, reusable for the core and stretched flare rays.
fn glow_mesh() -> Mesh {
    let mut positions = vec![[0.0, 0.0, 0.0]];
    let mut colors = vec![[1.0; 4]];
    for i in 0..=48 {
        let angle = TAU * i as f32 / 48.0;
        positions.push([angle.cos(), angle.sin(), 0.0]);
        colors.push([1.0, 1.0, 1.0, 0.0]);
    }
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
        .with_inserted_indices(Indices::U32((1..=48).flat_map(|i| [0, i, i + 1]).collect()))
}

/// Includes every polygon corner so the expanding wave remains inside the actual cell.
fn sample_boundary(polygon: &[Vec2]) -> Vec<Vec2> {
    let mut boundary = Vec::new();
    for (index, &from) in polygon.iter().enumerate() {
        let to = polygon[(index + 1) % polygon.len()];
        let steps = (from.distance(to) / 24.0).ceil().clamp(1.0, 64.0) as usize;
        for step in 0..steps {
            boundary.push(from.lerp(to, step as f32 / steps as f32));
        }
    }
    boundary
}

fn territory_mesh(boundary: &[Vec2]) -> Mesh {
    let mut indices = Vec::new();
    let mut colors = Vec::new();
    for index in 0..boundary.len() {
        for alpha in [0.0, 0.10, 0.26, 0.0] {
            colors.push([1.0, 1.0, 1.0, alpha]);
        }
        let a = (index * WAVE_BANDS) as u32;
        let b = (((index + 1) % boundary.len()) * WAVE_BANDS) as u32;
        for band in 0..(WAVE_BANDS - 1) as u32 {
            indices.extend([
                a + band,
                b + band,
                b + band + 1,
                a + band,
                b + band + 1,
                a + band + 1,
            ]);
        }
    }
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(
            Mesh::ATTRIBUTE_POSITION,
            vec![[0.0; 3]; boundary.len() * WAVE_BANDS],
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
        .with_inserted_indices(Indices::U32(indices))
}

fn advance_wave(mesh: &mut Mesh, boundary: &[Vec2], radius: f32) {
    let Some(VertexAttributeValues::Float32x3(positions)) =
        mesh.attribute_mut(Mesh::ATTRIBUTE_POSITION)
    else {
        return;
    };
    for (edge, vertices) in boundary.iter().zip(positions.as_chunks_mut::<WAVE_BANDS>().0) {
        let distance = edge.length();
        let direction = edge.normalize_or_zero();
        for (vertex, offset) in vertices.iter_mut().zip([150.0, 75.0, 18.0, 0.0]) {
            let point = direction * (radius - offset).max(0.0).min(distance);
            *vertex = [point.x, point.y, 0.0];
        }
    }
}

/// Pauses behind overlays and removes an effect when it finishes or its ownership change is undone.
fn animate_colonies(
    mut commands: Commands,
    time: Res<Time>,
    game_state: Res<State<GameState>>,
    settings: Res<Settings>,
    map: Res<Map>,
    player: Res<Player>,
    mut effects: Query<(Entity, &mut ColonyEffect, &Children, &mut Visibility)>,
    mut parts: Query<(
        &EffectPart,
        &mut Transform,
        Option<&Mesh2d>,
        Option<&MeshMaterial2d<ColorMaterial>>,
        Option<&mut TextColor>,
    )>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    for (entity, mut effect, children, mut visibility) in &mut effects {
        if !map
            .try_get(effect.planet)
            .is_some_and(|planet| effect.event.still_applies(planet, &player))
        {
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
        for child in children.iter() {
            let Ok((part, mut transform, mesh, material, text)) = parts.get_mut(child) else {
                continue;
            };
            let alpha = match part {
                EffectPart::Territory {
                    boundary,
                    reach,
                } => {
                    let progress = ((elapsed - 0.35) / 2.8).clamp(0.0, 1.0);
                    if let Some(mut mesh) = mesh.and_then(|handle| meshes.get_mut(&handle.0)) {
                        advance_wave(&mut mesh, boundary, progress * (reach + 150.0));
                    }
                    if settings.show_cells {
                        (progress * 10.0).min(1.0) * (1.0 - progress)
                    } else {
                        0.0
                    }
                },
                EffectPart::Flare {
                    size,
                } => {
                    let progress = (elapsed / 1.1).clamp(0.0, 1.0);
                    transform.scale = (*size * (0.4 + progress * 0.7)).extend(1.0);
                    (progress * 7.0).min(1.0) * (1.0 - progress).powi(2)
                },
                EffectPart::Beacon {
                    delay,
                    radius,
                } => {
                    let progress = ((elapsed - delay) / 1.35).clamp(0.0, 1.0);
                    transform.scale = Vec3::splat(radius * (0.08 + progress));
                    (progress * 10.0).min(1.0) * (1.0 - progress).powi(2) * 0.8
                },
                EffectPart::Label {
                    y,
                } => {
                    let fade_in = ((elapsed - 0.55) / 0.4).clamp(0.0, 1.0);
                    transform.translation.y = y + 10.0 * (1.0 - fade_in);
                    fade_in * ((CELEBRATION_SECONDS - elapsed) / 0.8).clamp(0.0, 1.0)
                },
            };
            if let Some(mut material) = material.and_then(|handle| materials.get_mut(&handle.0)) {
                material.color.set_alpha(alpha);
            }
            if let Some(mut text) = text {
                text.0.set_alpha(alpha);
            }
        }
    }
}

pub(crate) struct ColonizationPlugin;

impl Plugin for ColonizationPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Colonies>()
            .add_systems(OnEnter(AppState::Game), initialize_colonies.after(draw_map))
            .add_systems(
                Update,
                (celebrate_colonies, animate_colonies)
                    .chain()
                    .after(refresh_gameplay_projection)
                    .after(refresh_turn_draft)
                    .run_if(in_state(AppState::Game)),
            );
    }
}

#[cfg(test)]
#[path = "../../../tests/core/map_colonization.rs"]
mod tests;
