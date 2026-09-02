//! Cached radar coverage geometry; animation only rotates presentation transforms.

use std::f32::consts::TAU;

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

#[derive(Clone, Copy, Default)]
enum ScannerLayer {
    #[default]
    Field,
    Border,
    Sweep,
}

/// One visual layer of a hovered scanner, caching geometry until its range changes.
#[derive(Component, Default)]
pub struct ScannerCmp {
    layer: ScannerLayer,
    radius: f32,
}

impl ScannerCmp {
    pub(super) fn layers() -> [Self; 3] {
        [ScannerLayer::Field, ScannerLayer::Border, ScannerLayer::Sweep].map(|layer| Self {
            layer,
            radius: 0.,
        })
    }

    pub(super) fn update(
        &mut self,
        radius: f32,
        elapsed: f64,
        transform: &mut Transform,
        mesh: &mut Mesh2d,
        meshes: &mut Assets<Mesh>,
    ) {
        // Reuse the same mesh throughout a hover and when returning to an unchanged building.
        if self.radius != radius {
            mesh.0 = meshes.add(self.layer.mesh(radius));
            self.radius = radius;
        }

        let speed = match self.layer {
            ScannerLayer::Field => 0.,
            ScannerLayer::Border => -0.10,
            ScannerLayer::Sweep => 0.65,
        };
        transform.rotation = Quat::from_rotation_z((elapsed * speed).rem_euclid(TAU as f64) as f32);
    }
}

impl ScannerLayer {
    fn mesh(self, radius: f32) -> Mesh {
        let mut mesh = ScannerMesh::default();
        match self {
            Self::Field => {
                mesh.arc([0., radius], [0., TAU], [0.01, 0.01], false);
                // Keep every decorative band inside the actual detection boundary.
                mesh.arc([radius - 12., radius - 3.], [0., TAU], [0., 0.06], false);
                mesh.arc([radius - 3., radius], [0., TAU], [0.06, 0.], false);
                mesh.arc([radius - 0.7, radius], [0., TAU], [0.035, 0.035], false);
            },
            Self::Border => {
                // Similar dash lengths at every building level, with a clear gap between them.
                let count = (TAU * radius / 24.).round().clamp(32., 192.) as usize;
                let step = TAU / count as f32;
                for index in 0..count {
                    let start = index as f32 * step;
                    mesh.arc(
                        [radius - 2.4, radius],
                        [start, start + step * 0.58],
                        [0.65, 0.65],
                        false,
                    );
                }
            },
            Self::Sweep => {
                // Two opposing comet-like arcs travel along the inside of the segmented rim.
                for start in [0., TAU * 0.5] {
                    let angles = [start, start + TAU * 0.14];
                    mesh.arc([radius - 15., radius - 8.], angles, [0., 0.20], true);
                    mesh.arc([radius - 8., radius - 4.], angles, [0.20, 0.], true);
                    mesh.arc([radius - 8., radius - 6.], angles, [0.9, 0.9], true);
                }
            },
        }
        mesh.finish()
    }
}

#[derive(Default)]
struct ScannerMesh {
    positions: Vec<[f32; 3]>,
    colors: Vec<[f32; 4]>,
    indices: Vec<u32>,
}

impl ScannerMesh {
    /// Adds an annular strip with inner/outer alpha and an optional fading angular tail.
    fn arc(&mut self, radii: [f32; 2], angles: [f32; 2], alpha: [f32; 2], fade_tail: bool) {
        let span = angles[1] - angles[0];
        let steps = (span * radii[1] / 5.).ceil().clamp(2., 1024.) as u32;
        let base = self.positions.len() as u32;
        for step in 0..=steps {
            let fraction = step as f32 / steps as f32;
            let angle = angles[0] + span * fraction;
            let (sin, cos) = angle.sin_cos();
            let fade = if fade_tail {
                fraction * fraction
            } else {
                1.
            };
            for (radius, opacity) in radii.into_iter().zip(alpha) {
                self.positions.push([radius * cos, radius * sin, 0.]);
                self.colors.push([1., 1., 1., opacity * fade]);
            }
            if step < steps {
                let index = base + step * 2;
                self.indices.extend_from_slice(&[
                    index,
                    index + 1,
                    index + 3,
                    index,
                    index + 3,
                    index + 2,
                ]);
            }
        }
    }

    fn finish(self) -> Mesh {
        Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.positions)
            .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, self.colors)
            .with_inserted_indices(Indices::U32(self.indices))
    }
}
