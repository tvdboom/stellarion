use super::*;

#[test]
fn schematic_owner_borders_follow_individual_owners_and_mixed_group_counts() {
    let mut unresolved = report(5, 0, true, 91);
    let bomber = Unit::Ship(Ship::Bomber);
    let fighter = Unit::Ship(Ship::LightFighter);
    unresolved.mission.army.insert(Unit::war_sun(), 1);
    unresolved.mission.joint_attack = Some(JointAttackMission {
        attackers: [
            (1, Army::from([(bomber, 2)])),
            (4, Army::from([(bomber, 3), (Unit::war_sun(), 1)])),
        ]
        .into(),
        ..default()
    });
    unresolved.planet.army.insert(fighter, 6);
    unresolved.planet.army.dock_protector(3, Army::from([(fighter, 3)]));
    let mut rng = DeterministicRngState::from_u64(91).next_rng();
    let report = resolve_combat_with_rng(1, &unresolved.mission, &unresolved.planet, &mut rng);
    let origin = Planet::new_with_rng(0, "Origin".into(), Vec2::ZERO, false, 1.0, &mut rng);
    let map = Map {
        rect: Rect::new(-100.0, -100.0, 100.0, 100.0),
        solar_corner: crate::core::map::model::SolarCorner::BottomLeft,
        planets: vec![origin, report.planet.clone()],
    };
    let mut app = playback_app(report, 0, CombatState::Fire);
    app.insert_resource(map).init_resource::<MultiplayerSession>();
    app.world_mut().run_system_once(setup_combat).unwrap();
    let individuals = app
        .world_mut()
        .query::<(Entity, &IndividualCombatUnitCmp)>()
        .iter(app.world())
        .map(|(entity, card)| (entity, card.owner, card.display_size))
        .collect::<Vec<_>>();
    let groups = app
        .world_mut()
        .query_filtered::<(Entity, &CombatUnitCmp, &Sprite), With<GroupedCombatUnitCmp>>()
        .iter(app.world())
        .map(|(entity, card, sprite)| {
            (entity, card.side.clone(), card.unit, sprite.custom_size.unwrap().x)
        })
        .collect::<Vec<_>>();
    let color = |owner| app.world().resource::<MultiplayerSession>().player_color(owner).color();
    let edges = |entity| {
        app.world()
            .get::<Children>(entity)
            .unwrap()
            .iter()
            .filter(|child| app.world().get::<CombatOwnerBorder>(*child).is_some())
            .map(|child| {
                let sprite = app.world().get::<Sprite>(child).unwrap();
                let transform = app.world().get::<Transform>(child).unwrap();
                (sprite.color, sprite.custom_size.unwrap(), transform.translation)
            })
            .collect::<Vec<_>>()
    };
    assert!(!individuals.is_empty());
    for (entity, owner, card_size) in individuals {
        let border = edges(entity);
        assert_eq!(border.len(), 4);
        let horizontal_edges =
            border.iter().filter(|(_, size, _)| size.x > size.y).collect::<Vec<_>>();
        let top = horizontal_edges
            .iter()
            .map(|(_, _, position)| position.y)
            .fold(f32::NEG_INFINITY, f32::max);
        let bottom = horizontal_edges
            .iter()
            .map(|(_, _, position)| position.y)
            .fold(f32::INFINITY, f32::min);
        assert!(
            border.iter().all(|(tint, size, position)| {
                *tint == color(owner.unwrap()) && size.min_element() <= 1.25 && position.z > 0.0
            }),
            "Each thin edge must use the card's actual owner's color"
        );
        assert!((top - card_size * 0.5).abs() < 0.001);
        let stat_bottom = app
            .world()
            .get::<Children>(entity)
            .unwrap()
            .iter()
            .filter_map(|child| {
                let sprite = app.world().get::<Sprite>(child)?;
                let transform = app.world().get::<Transform>(child)?;
                (transform.translation.z < 0.2 && transform.translation.y < -card_size * 0.5)
                    .then(|| transform.translation.y - sprite.custom_size.unwrap().y * 0.5)
            })
            .reduce(f32::min)
            .unwrap_or(-card_size * 0.5);
        assert!(
            (bottom - stat_bottom).abs() < 0.001,
            "The owner border must enclose every individual stat bar"
        );
    }
    for (side, unit, counts) in [
        (Side::Attacker, bomber, vec![(1, 2), (4, 3)]),
        (Side::Defender, fighter, vec![(2, 6), (3, 3)]),
        (Side::Attacker, Unit::war_sun(), vec![(4, 1)]),
    ] {
        let (entity, _, _, size) = groups
            .iter()
            .find(|(_, card_side, card_unit, _)| *card_side == side && *card_unit == unit)
            .unwrap();
        let border = edges(*entity);
        let total = counts.iter().map(|(_, count)| *count).sum::<usize>() as f32;
        let perimeter: f32 =
            border.iter().map(|(_, size, _)| size.max_element() - size.min_element()).sum();
        assert!(perimeter > size * 4.0, "Stat bars must extend the colored border below the image");
        for (owner, count) in counts {
            let owner_perimeter: f32 = border
                .iter()
                .filter(|(tint, _, _)| *tint == color(owner))
                .map(|(_, size, _)| size.max_element() - size.min_element())
                .sum();
            assert!((owner_perimeter - perimeter * count as f32 / total).abs() < 0.01,
                "Grouped borders must show every ally's share, including types absent from the commander's fleet");
        }
    }
}
