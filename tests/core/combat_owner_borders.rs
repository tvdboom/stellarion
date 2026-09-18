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
        assert!(
            border.iter().all(|(tint, size, position)| {
                *tint == color(owner.unwrap())
                    && size.min_element() <= 1.25
                    && (position.x.abs().max(position.y.abs()) - card_size * 0.5).abs() < 0.001
                    && position.z > 0.0
            }),
            "Each thin edge must be attached to the image and use its actual owner's color"
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
        for (owner, count) in counts {
            let perimeter: f32 = border
                .iter()
                .filter(|(tint, _, _)| *tint == color(owner))
                .map(|(_, size, _)| size.max_element() - size.min_element())
                .sum();
            assert!((perimeter - size * 4.0 * count as f32 / total).abs() < 0.01,
                "Grouped borders must show every ally's share, including types absent from the commander's fleet");
        }
    }
}
