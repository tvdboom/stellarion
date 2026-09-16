use super::*;
use crate::core::random::DeterministicRngState;
use crate::core::simulation::{GameModel, GameRules};

#[test]
fn belt_density_scales_with_circumference_without_crowding_small_rings() {
    let mut previous_count = 0;
    for radius in [400.0, 800.0, 1600.0, 3200.0] {
        let count = asteroid_belt_asteroid_count(radius);
        let spacing = TAU * radius / count as f32;
        assert!((63.0..=65.0).contains(&spacing), "radius={radius}, spacing={spacing}");
        assert!(count > previous_count);
        previous_count = count;
    }
}

#[test]
fn generated_belts_keep_varied_spacing_clearance_and_deterministic_placements() {
    let mut minimum_visible = usize::MAX;
    for player_count in 1..=4 {
        for planets_per_player in [5, 10, 20] {
            for moons_percent in [0, 30, 100] {
                let rules = GameRules {
                    player_count,
                    planets_per_player,
                    moons_percent,
                    practice_mode: player_count == 1,
                    ..Default::default()
                };
                for sample in 0..32 {
                    let seed = DeterministicRngState::from_u64(sample).seed;
                    let map = GameModel::new(seed, rules.clone()).unwrap().map;
                    let layout = asteroid_belt_layout(&map).unwrap();
                    let placements = asteroid_belt_placements(&map, layout);
                    let count = asteroid_belt_asteroid_count(layout.radius);
                    assert!((count * 3 / 4..=count).contains(&placements.len()));
                    let repeated = asteroid_belt_placements(&map, layout);
                    let star = map.solar_star_position();
                    let planets = map.planets();
                    for (placement, repeat) in placements.iter().zip(&repeated) {
                        assert_eq!(placement.phase.to_bits(), repeat.phase.to_bits());
                        assert_eq!(placement.radius.to_bits(), repeat.radius.to_bits());
                        assert_eq!(placement.diameter.to_bits(), repeat.diameter.to_bits());
                        let position = star + Vec2::from_angle(placement.phase) * placement.radius;
                        for planet in &planets {
                            assert!(
                                position.distance(planet.position)
                                    > planet.size() * 0.5
                                        + placement.diameter
                                            * 0.5
                                            * ASTEROID_MAX_RENDER_SCALE
                                            * ASTEROID_TUMBLE_SCALE_MARGIN
                                        + ASTEROID_PLANET_CLEARANCE
                                        + ASTEROID_MAXIMUM_WOBBLE
                            );
                        }
                    }
                    let mut phases =
                        placements.iter().map(|p| p.phase.rem_euclid(TAU)).collect::<Vec<_>>();
                    phases.sort_by(f32::total_cmp);
                    let average_spacing = TAU * layout.radius / count as f32;
                    let mut shortest_gap = f32::MAX;
                    let mut longest_gap = 0.0_f32;
                    let mut short_gaps = 0;
                    let mut long_gaps = 0;
                    for (start, end) in phases
                        .windows(2)
                        .map(|pair| (pair[0], pair[1]))
                        .chain(std::iter::once((phases[phases.len() - 1], phases[0] + TAU)))
                    {
                        let distance = (end - start) * layout.radius;
                        shortest_gap = shortest_gap.min(distance);
                        longest_gap = longest_gap.max(distance);
                        short_gaps += usize::from(distance < average_spacing * 0.82);
                        long_gaps += usize::from(
                            distance > average_spacing * 1.18 && distance < average_spacing * 1.4,
                        );
                        assert!(distance >= 24.0,
                            "crowded ring: gap={distance}, players={player_count}, worlds={planets_per_player}, moons={moons_percent}, sample={sample}");
                        if distance > average_spacing * 1.6 + 0.1 {
                            // Larger interruptions must clear a planet, including at the wrap.
                            let midpoint =
                                star + Vec2::from_angle((start + end) * 0.5) * layout.radius;
                            assert!(
                                planets.iter().any(|planet| {
                                    midpoint.distance(planet.position)
                                        < planet.size() * 0.5
                                            + layout.maximum_asteroid_diameter
                                                * 0.5
                                                * ASTEROID_MAX_RENDER_SCALE
                                                * ASTEROID_TUMBLE_SCALE_MARGIN
                                            + ASTEROID_PLANET_CLEARANCE
                                            + ASTEROID_MAXIMUM_WOBBLE
                                            + layout.radial_half_width
                                            + 32.0
                                }),
                                "unexplained empty arc: gap={distance}, sample={sample}"
                            );
                        }
                    }
                    assert!(
                        shortest_gap < average_spacing * 0.75
                            && longest_gap > average_spacing * 1.25,
                        "belt looks evenly spaced: shortest={shortest_gap}, longest={longest_gap}, average={average_spacing}, sample={sample}"
                    );
                    assert!(
                        short_gaps >= placements.len() / 5 && long_gaps >= placements.len() / 5,
                        "local arcs lack variation: short={short_gaps}, long={long_gaps}, count={}, sample={sample}",
                        placements.len()
                    );
                    minimum_visible =
                        minimum_visible.min(visible_asteroid_count(&map, &placements));
                }
            }
        }
    }
    assert!(minimum_visible >= 16, "visible={minimum_visible}");
}

#[test]
fn two_player_practice_seeds_never_shrink_asteroids_below_the_legible_minimum() {
    let rules = GameRules {
        player_count: 2,
        practice_mode: true,
        ..Default::default()
    };

    let mut narrow_layouts = 0;
    for sample in 0..512 {
        let seed = DeterministicRngState::from_u64(sample).seed;
        let map = GameModel::new(seed, rules.clone()).unwrap().map;
        let layout = asteroid_belt_layout(&map).unwrap();
        let gap = solar_band_gaps(&map)
            .into_iter()
            .find(|gap| gap.between_bands == layout.between_bands)
            .unwrap();
        narrow_layouts += usize::from(
            gap.outer_edge > gap.inner_edge
                && (gap.outer_edge - gap.inner_edge) * 0.4 < ASTEROID_MINIMUM_DIAMETER,
        );
        let placements = asteroid_belt_placements(&map, layout);

        assert!(!placements.is_empty(), "sample {sample} produced no asteroid field");
        assert!(
            placements.iter().all(|placement| placement.diameter >= ASTEROID_MINIMUM_DIAMETER),
            "sample {sample} produced a sub-legible asteroid: layout={layout:?}"
        );
    }
    assert!(narrow_layouts > 0, "the regression sweep did not exercise any narrow belts");
}
