//! Local atmosphere and weather overlays. These never read private infrastructure or alter turns.

use std::f32::consts::TAU;

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use super::model::{Map, MapCmp};
use super::planet::{PlanetId, PlanetKind};
use super::systems::draw_map;
use crate::core::camera::MainCamera;
use crate::core::constants::PLANET_Z;
use crate::core::loading::{refresh_gameplay_projection, refresh_turn_draft};
use crate::core::states::{AppState, GameState};

const CELL: u32 = 128;
const FRAMES: usize = 16;
const COLUMNS: u32 = 4;

fn smooth(value: f32) -> f32 {
    let value = value.clamp(0.0, 1.0);
    value * value * (3.0 - 2.0 * value)
}

fn hash(mut seed: u32) -> f32 {
    seed = (seed ^ (seed >> 16)).wrapping_mul(0x7feb_352d);
    seed = (seed ^ (seed >> 15)).wrapping_mul(0x846c_a68b);
    (seed ^ (seed >> 16)) as f32 / u32::MAX as f32
}

fn noise(point: Vec3) -> f32 {
    let cell = point.floor().as_ivec3();
    let blend = (point - point.floor()).map(smooth);
    let sample = |x: i32, y: i32, z: i32| {
        hash(
            (cell.x.wrapping_add(x) as u32).wrapping_mul(73_856_093)
                ^ (cell.y.wrapping_add(y) as u32).wrapping_mul(19_349_663)
                ^ (cell.z.wrapping_add(z) as u32).wrapping_mul(83_492_791),
        )
    };
    let mix = |a: f32, b: f32, t: f32| a + (b - a) * t;
    let row = |y, z| mix(sample(0, y, z), sample(1, y, z), blend.x);
    mix(mix(row(0, 0), row(1, 0), blend.y), mix(row(0, 1), row(1, 1), blend.y), blend.z)
}

#[derive(Clone, Copy)]
enum TextureKind {
    Rim,
    Clouds,
    Bands,
    Aurora,
    Flash,
}

fn pixel(kind: TextureKind, point: Vec2, phase: f32) -> [u8; 4] {
    let radius = point.length();
    let alpha = match kind {
        TextureKind::Rim => {
            let edge = (-((radius - 0.89) / 0.035).powi(2)).exp();
            edge * (0.22 + 0.78 * (point.x / radius.max(0.01)).max(0.0))
        },
        TextureKind::Flash => (1.0 - radius).max(0.0).powi(3),
        TextureKind::Clouds | TextureKind::Bands => {
            if radius >= 0.99 {
                return [0; 4];
            }
            // Rotate the sampled sphere, rather than a flat disc, so clouds follow the limb.
            let depth = (1.0 - radius * radius).sqrt();
            let sphere = Vec3::new(
                point.x * phase.cos() + depth * phase.sin(),
                point.y,
                depth * phase.cos() - point.x * phase.sin(),
            );
            let field = noise(sphere * 3.8 + Vec3::splat(17.0)) * 0.57
                + noise(sphere * 8.1 + Vec3::splat(31.0)) * 0.29
                + noise(sphere * 17.3 + Vec3::splat(43.0)) * 0.14;
            let density = if matches!(kind, TextureKind::Bands) {
                smooth(((point.y * 25.0 + field * 5.0).sin() * 0.5 + 0.5 - 0.43) * 2.2)
                    * (0.35 + field * 0.65)
            } else {
                smooth((field - 0.46) * 4.5)
            };
            density * smooth((0.99 - radius) / 0.13) * (0.42 + depth * 0.58)
        },
        TextureKind::Aurora => {
            let wave = (point.x * 12.0 + phase).sin() * 0.035
                + (point.x * 23.0 - phase * 2.0).sin() * 0.016;
            let curtain = (-((point.y.abs() - 0.71 - wave) / 0.045).powi(2)).exp();
            curtain * smooth((0.94 - radius) / 0.12) * smooth((0.72 - point.x.abs()) / 0.3)
        },
    };
    let rgb = if matches!(kind, TextureKind::Aurora) {
        [92, 255, 202]
    } else {
        [255; 3]
    };
    [rgb[0], rgb[1], rgb[2], (alpha.clamp(0.0, 1.0) * 255.0).round() as u8]
}

fn texture(kind: TextureKind, animated: bool) -> Image {
    let columns = if animated {
        COLUMNS
    } else {
        1
    };
    let rows = if animated {
        FRAMES as u32 / COLUMNS
    } else {
        1
    };
    let width = CELL * columns;
    let height = CELL * rows;
    let mut pixels = vec![0; (width * height * 4) as usize];
    for frame in 0..columns * rows {
        let phase = frame as f32 / FRAMES as f32 * TAU;
        for y in 0..CELL {
            for x in 0..CELL {
                let point = Vec2::new(
                    (x as f32 + 0.5) / CELL as f32 * 2.0 - 1.0,
                    1.0 - (y as f32 + 0.5) / CELL as f32 * 2.0,
                );
                let at = (((frame / columns * CELL + y) * width + frame % columns * CELL + x) * 4)
                    as usize;
                pixels[at..at + 4].copy_from_slice(&pixel(kind, point, phase));
            }
        }
    }
    let mut image = Image::new(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        pixels,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    image.sampler = bevy::image::ImageSampler::linear();
    image
}

/// Shared soft light footprint for weather flashes and close-zoom shipyard work.
pub(super) fn soft_glow_image() -> Image {
    texture(TextureKind::Flash, false)
}

#[derive(Resource)]
struct WeatherTextures {
    rim: Handle<Image>,
    clouds: Handle<Image>,
    bands: Handle<Image>,
    aurora: Handle<Image>,
    flash: Handle<Image>,
}

impl FromWorld for WeatherTextures {
    fn from_world(world: &mut World) -> Self {
        let mut images = world.resource_mut::<Assets<Image>>();
        Self {
            rim: images.add(texture(TextureKind::Rim, false)),
            clouds: images.add(texture(TextureKind::Clouds, true)),
            bands: images.add(texture(TextureKind::Bands, true)),
            aurora: images.add(texture(TextureKind::Aurora, true)),
            flash: images.add(texture(TextureKind::Flash, false)),
        }
    }
}

#[derive(Component, Clone, Copy)]
enum WeatherLayer {
    Rim,
    Clouds(usize),
    Aurora(usize),
    Lightning,
}

#[derive(Component)]
struct PlanetWeather {
    planet: PlanetId,
    opacity: f32,
    phase: f32,
}

#[derive(Resource, Default)]
struct WeatherTime(f64);

fn atmosphere_color(kind: PlanetKind) -> Option<Color> {
    match kind {
        PlanetKind::Water => Some(Color::srgb(0.32, 0.68, 1.0)),
        PlanetKind::Gas => Some(Color::srgb(0.95, 0.73, 0.46)),
        PlanetKind::Ice => Some(Color::srgb(0.48, 0.83, 0.95)),
        PlanetKind::Dry => Some(Color::srgb(0.93, 0.59, 0.32)),
        PlanetKind::Metallic
        | PlanetKind::Blue
        | PlanetKind::Brown
        | PlanetKind::Gray
        | PlanetKind::Red
        | PlanetKind::Yellow => None,
    }
}

fn spawn_weather(commands: &mut Commands, map: &Map, textures: &WeatherTextures) {
    for planet in &map.planets {
        let Some(color) = atmosphere_color(planet.kind).filter(|_| !planet.is_destroyed) else {
            continue;
        };
        let mut layers = vec![(WeatherLayer::Rim, textures.rim.clone(), color, 0.42, 1.12)];
        let (clouds, tint, opacity) = match planet.kind {
            PlanetKind::Gas => (&textures.bands, Color::srgb(1.0, 0.91, 0.78), 0.18),
            PlanetKind::Dry => (&textures.clouds, Color::srgb(1.0, 0.82, 0.64), 0.10),
            PlanetKind::Ice => (&textures.clouds, Color::srgb(0.80, 0.93, 1.0), 0.19),
            _ => (&textures.clouds, Color::srgb(0.85, 0.94, 1.0), 0.34),
        };
        for layer in 0..2 {
            layers.push((WeatherLayer::Clouds(layer), clouds.clone(), tint, opacity, 1.0));
            if planet.kind == PlanetKind::Ice {
                layers.push((
                    WeatherLayer::Aurora(layer),
                    textures.aurora.clone(),
                    Color::WHITE,
                    0.32,
                    1.0,
                ));
            }
        }
        if planet.kind == PlanetKind::Gas {
            layers.push((
                WeatherLayer::Lightning,
                textures.flash.clone(),
                Color::srgb(0.72, 0.85, 1.0),
                0.48,
                0.22,
            ));
        }
        for (index, (layer, image, color, opacity, size)) in layers.into_iter().enumerate() {
            commands.spawn((
                Sprite {
                    image,
                    color: color.with_alpha(0.0),
                    custom_size: Some(Vec2::splat(planet.size() * size)),
                    ..default()
                },
                Transform::from_translation(
                    planet.position.extend(PLANET_Z + 0.015 + index as f32 * 0.001),
                ),
                Visibility::Hidden,
                Pickable::IGNORE,
                PlanetWeather {
                    planet: planet.id,
                    opacity,
                    phase: hash(planet.id as u32 + 97),
                },
                layer,
                MapCmp,
            ));
        }
    }
}

fn atlas_frame(frame: usize) -> Rect {
    let frame = frame % FRAMES;
    let min =
        Vec2::new((frame as u32 % COLUMNS * CELL) as f32, (frame as u32 / COLUMNS * CELL) as f32);
    Rect::from_corners(min, min + Vec2::splat(CELL as f32))
}

fn lightning_alpha(age: f32) -> f32 {
    let flash = |start: f32, duration: f32| {
        let progress = (age - start) / duration;
        if (0.0..1.0).contains(&progress) {
            (1.0 - progress).powi(2)
        } else {
            0.0
        }
    };
    flash(0.0, 0.16) + flash(0.25, 0.24) * 0.55
}

fn animate_weather(
    time: Res<Time>,
    game: Res<State<GameState>>,
    map: Res<Map>,
    camera: Single<&Projection, With<MainCamera>>,
    mut elapsed: ResMut<WeatherTime>,
    mut weather: Query<(
        &PlanetWeather,
        &WeatherLayer,
        &mut Sprite,
        &mut Transform,
        &mut Visibility,
    )>,
) {
    if *game.get() == GameState::Playing {
        elapsed.0 += time.delta_secs_f64();
    }
    let scale = match *camera {
        Projection::Orthographic(ref projection) => projection.scale,
        _ => f32::INFINITY,
    };
    let map_visible = !matches!(*game.get(), GameState::Combat | GameState::CombatMenu);
    for (weather, layer, mut sprite, mut transform, mut visibility) in &mut weather {
        let Some(planet) = map.try_get(weather.planet).filter(|planet| !planet.is_destroyed) else {
            *visibility = Visibility::Hidden;
            continue;
        };
        let seconds = elapsed.0 as f32;
        let zoom = match layer {
            WeatherLayer::Rim => smooth((5.0 - scale) / 2.0),
            WeatherLayer::Clouds(_) => smooth((2.6 - scale) / 1.6),
            WeatherLayer::Aurora(_) | WeatherLayer::Lightning => smooth((1.8 - scale) / 0.9),
        };
        let aurora = matches!(*layer, WeatherLayer::Aurora(_));
        let brightness = match *layer {
            WeatherLayer::Rim => {
                transform.rotation =
                    Quat::from_rotation_z((map.solar_star_position() - planet.position).to_angle());
                1.0
            },
            WeatherLayer::Clouds(frame_layer) | WeatherLayer::Aurora(frame_layer) => {
                let cycle = (elapsed.0
                    / if aurora {
                        24.0
                    } else {
                        180.0 + weather.phase as f64 * 60.0
                    }
                    + weather.phase as f64)
                    .fract() as f32
                    * FRAMES as f32;
                sprite.rect = Some(atlas_frame(cycle.floor() as usize + frame_layer));
                let blend = smooth(cycle.fract());
                let weight = if frame_layer == 0 {
                    1.0 - blend
                } else {
                    blend
                };
                weight
                    * if aurora {
                        0.65 + 0.35 * (seconds * 0.4 + weather.phase * TAU).sin().powi(2)
                    } else {
                        1.0
                    }
            },
            WeatherLayer::Lightning => {
                let cycle = elapsed.0 / (13.0 + weather.phase as f64 * 9.0) + weather.phase as f64;
                let seed = (planet.id as u32).wrapping_mul(71).wrapping_add(cycle.floor() as u32);
                let offset = Vec2::from_angle(hash(seed) * TAU)
                    * (0.12 + hash(seed.wrapping_add(1)) * 0.15)
                    * planet.size();
                transform.translation = (planet.position + offset).extend(transform.translation.z);
                lightning_alpha(cycle.fract() as f32 * (13.0 + weather.phase * 9.0))
            },
        };
        let alpha = weather.opacity * zoom * brightness;
        sprite.color.set_alpha(alpha);
        *visibility = if map_visible && alpha > 0.001 {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
}

pub(crate) struct PlanetAmbiencePlugin;

impl Plugin for PlanetAmbiencePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WeatherTextures>()
            .init_resource::<WeatherTime>()
            .add_systems(
                OnEnter(AppState::Game),
                (
                    |mut elapsed: ResMut<WeatherTime>| {
                        elapsed.0 = 0.0;
                    },
                    |mut commands: Commands, map: Res<Map>, textures: Res<WeatherTextures>| {
                        spawn_weather(&mut commands, &map, &textures);
                    },
                )
                    .after(draw_map),
            )
            .add_systems(
                Update,
                animate_weather
                    .after(refresh_gameplay_projection)
                    .after(refresh_turn_draft)
                    .run_if(in_state(AppState::Game)),
            );
    }
}

#[cfg(test)]
#[path = "../../../tests/core/map_ambience.rs"]
mod tests;
