use super::*;

fn mixed_fleet(side: Side) -> CinematicPlayback {
    let mut movie = super::super::tests::replay();
    for index in 0..24 {
        let unit = Unit::Ship(match index {
            0 => Ship::Dreadnought,
            1..=2 => Ship::Battleship,
            3..=5 => Ship::Cruiser,
            6..=9 => Ship::Destroyer,
            _ => Ship::LightFighter,
        });
        movie.timeline.actors[index].unit = unit;
        movie.timeline.actors[index].side = side.clone();
        movie.visuals[index].size = unit_size(unit) * (9.0_f32 / 24.0).sqrt();
    }
    movie
}

#[test]
fn fleet_staging_reserves_capital_footprints_without_shrinking_ships() {
    for side in [Side::Attacker, Side::Defender] {
        let mut movie = mixed_fleet(side);
        let sizes: Vec<_> = movie.visuals.iter().map(|visual| visual.size).collect();
        layout_fleets(&mut movie.visuals, &movie.timeline.actors);
        assert_eq!(sizes, movie.visuals.iter().map(|visual| visual.size).collect::<Vec<_>>());
        for left in 0..24 {
            let a = &movie.visuals[left];
            for right in left + 1..24 {
                let b = &movie.visuals[right];
                let distance = ((a.home - b.home) / (footprint(a) + footprint(b))).length();
                assert!(distance > 0.80, "Hull {left} overlaps {right}: {distance}");
            }
        }
    }
}

#[test]
fn fleet_staging_follows_identity_and_is_independent_of_roster_order() {
    let mut movie = mixed_fleet(Side::Attacker);
    layout_fleets(&mut movie.visuals, &movie.timeline.actors);
    let homes: std::collections::BTreeMap<_, _> = movie
        .timeline
        .actors
        .iter()
        .zip(&movie.visuals)
        .map(|(actor, visual)| (actor.id, visual.home))
        .collect();
    movie.timeline.actors.reverse();
    movie.visuals.reverse();
    layout_fleets(&mut movie.visuals, &movie.timeline.actors);
    for (actor, visual) in movie.timeline.actors.iter().zip(&movie.visuals) {
        assert_eq!(homes[&actor.id], visual.home, "Reordering the report changed a hull's station");
    }
}

#[test]
fn staging_leaves_ground_stations_and_hidden_actors_untouched() {
    let mut movie = mixed_fleet(Side::Attacker);
    movie.timeline.actors[0].unit = Unit::space_dock();
    movie.visuals[1].visible = false;
    let homes: Vec<_> = movie.visuals.iter().map(|visual| visual.home).collect();
    layout_fleets(&mut movie.visuals, &movie.timeline.actors);
    for index in [0, 1].into_iter().chain(24..movie.visuals.len()) {
        assert_eq!(homes[index], movie.visuals[index].home);
    }
    assert_eq!(movie.visuals.len(), homes.len(), "Every actor must retain its sprite");
}

#[test]
fn fleet_staging_stays_finite_and_on_canvas_for_small_and_large_hulls() {
    for side in [Side::Attacker, Side::Defender] {
        let mut movie = mixed_fleet(side);
        for index in 0..24 {
            movie.visuals[index].size = if index % 3 == 0 {
                242.0
            } else {
                42.0
            };
        }
        layout_fleets(&mut movie.visuals, &movie.timeline.actors);
        for visual in &movie.visuals[..24] {
            assert!(visual.home.is_finite());
            assert!((0.05..0.95).contains(&visual.home.x));
            assert!((0.10..0.75).contains(&visual.home.y));
        }
    }
}
