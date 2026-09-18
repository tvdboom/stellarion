//! Headless playback checks run the real Bevy combat systems against resolver reports.

#[path = "combat_owner_borders.rs"]
mod owner_borders;

use bevy::ecs::system::RunSystemOnce;
use bevy_tweening::CycleCompletedEvent;
use std::collections::BTreeSet;

use super::*;
use crate::core::combat::report::MissionReport;
use crate::core::combat::resolution::resolve_combat_with_rng;
use crate::core::map::planet::Planet;
use crate::core::missions::{JointAttackMission, Mission};
use crate::core::random::DeterministicRngState;
use crate::core::units::defense::Defense;
use crate::core::units::Army;

fn report(bombers: usize, shield: usize, guarded: bool, seed: u64) -> MissionReport {
    let mut rng = DeterministicRngState::from_u64(seed).next_rng();
    let mut origin = Planet::new_with_rng(0, "Origin".into(), Vec2::ZERO, false, 1., &mut rng);
    origin.colonize(1);
    let mut target = Planet::new_with_rng(1, "Target".into(), Vec2::X, false, 1., &mut rng);
    target.colonize(2);
    target.army = Unit::resource_buildings().into_iter().map(|u| (u, 5)).collect();
    if shield > 0 {
        target.army.insert(Unit::planetary_shield(), shield);
    }
    if guarded {
        target.army.insert(Unit::Defense(Defense::GaussCannon), 12);
    }
    let mission = Mission::new_with_id(
        10,
        1,
        1,
        &origin,
        &target,
        Icon::Attack,
        Army::from([(Unit::Ship(Ship::Bomber), bombers)]),
        BombingRaid::Economic,
        false,
        false,
        None,
    );
    resolve_combat_with_rng(1, &mission, &target, &mut rng)
}

fn playback_app(report: MissionReport, round: usize, phase: CombatState) -> App {
    let report_id = report.id;
    let mut player = Player::new(1, 0);
    player.reports.push(report);
    let mut app = App::new();
    app.add_plugins((TaskPoolPlugin::default(), AssetPlugin::default()))
        .init_asset::<Image>()
        .init_asset::<Font>()
        .init_asset::<TextureAtlasLayout>()
        .init_asset::<bevy_kira_audio::AudioSource>()
        .init_resource::<WorldAssets>()
        .init_resource::<Settings>()
        .init_resource::<Time>()
        .insert_resource(player)
        .insert_resource(UiState {
            in_combat: Some(report_id),
            combat_round: round,
            ..default()
        })
        .insert_resource(State::new(phase))
        .init_resource::<NextState<CombatState>>()
        .add_message::<SpawnShotMsg>()
        .add_message::<PlayAudioMsg>()
        .add_message::<AnimCompletedEvent>()
        .add_message::<CycleCompletedEvent>();
    app.world_mut()
        .run_system_once(
            |mut assets: ResMut<WorldAssets>,
             server: Res<AssetServer>,
             mut layouts: ResMut<Assets<TextureAtlasLayout>>| {
                assets.begin_gameplay_loading(&server, &mut layouts);
            },
        )
        .unwrap();
    app.world_mut().spawn((
        MainCamera,
        Transform::default(),
        Projection::Orthographic(OrthographicProjection::default_2d()),
    ));
    app.world_mut().spawn((BackgroundImageCmp, Sprite::default()));
    app.world_mut().spawn(Window::default());
    app
}

fn spawn_unit(app: &mut App, unit: Unit, count: usize, side: Side, fire: FireState) -> Entity {
    let hull = if unit.is_building() || unit == Unit::colony_ship() {
        count
    } else {
        count * unit.hull()
    };
    app.world_mut()
        .spawn((
            Sprite {
                custom_size: Some(Vec2::splat(100.)),
                ..default()
            },
            Transform::default(),
            CombatUnitCmp {
                unit,
                side,
                fire,
                hull,
                max_hull: hull,
                shield: count * unit.shield(),
                max_shield: count * unit.shield(),
                outcome_visible: false,
            },
        ))
        .id()
}

fn owner_entity(app: &App, descendants: &[Entity], owner: PlayerId) -> Entity {
    descendants
        .iter()
        .copied()
        .find(|entity| {
            app.world().get::<CountCmp>(*entity).is_some_and(|counter| counter.owner == Some(owner))
        })
        .unwrap()
}

#[test]
fn combat_participants_list_the_side_commanders_before_other_players() {
    let mut report = report(5, 0, true, 31);
    let fighter = Unit::Ship(Ship::LightFighter);
    report.planet.army.dock_protector(1, Army::from([(fighter, 3)]));
    assert_eq!(report.defender_players(), vec![2, 1]);

    report.mission.owner = 3;
    report.mission.joint_attack = Some(JointAttackMission {
        attackers: [
            (1, Army::from([(fighter, 1)])),
            (2, Army::from([(fighter, 2)])),
            (3, Army::from([(fighter, 3)])),
        ]
        .into(),
        ..default()
    });
    assert_eq!(report.attacker_players(), vec![3, 1, 2]);
}

#[test]
fn combat_identity_line_uses_the_same_player_strength_segments_as_details() {
    let mut report = report(2, 0, true, 37);
    let fighter = Unit::Ship(Ship::LightFighter);
    report.mission.joint_attack = Some(JointAttackMission {
        attackers: [(1, Army::from([(fighter, 5)])), (3, Army::from([(fighter, 5)]))].into(),
        ..default()
    });
    let mut rng = DeterministicRngState::from_u64(37).next_rng();
    let origin = Planet::new_with_rng(0, "Origin".into(), Vec2::ZERO, false, 1., &mut rng);
    let map = Map {
        rect: Rect::new(-100., -100., 100., 100.),
        solar_corner: crate::core::map::model::SolarCorner::BottomLeft,
        planets: vec![origin, report.planet.clone()],
    };
    let mut app = playback_app(report, 0, CombatState::Fire);
    app.insert_resource(map).init_resource::<MultiplayerSession>();
    let attacker_colors = {
        let session = app.world().resource::<MultiplayerSession>();
        [session.player_color(1).color(), session.player_color(3).color()]
    };
    app.world_mut().run_system_once(setup_combat).unwrap();

    let identity_nodes = app
        .world_mut()
        .query_filtered::<&Node, With<CombatIdentityCmp>>()
        .iter(app.world())
        .collect::<Vec<_>>();
    assert_eq!(identity_nodes.len(), 2);
    assert!(identity_nodes
        .iter()
        .all(|node| node.width == Val::Auto && node.min_width == Val::Px(220.0)));
    assert!(app
        .world_mut()
        .query::<(&Text, &TextLayout)>()
        .iter(app.world())
        .filter(|(text, _)| text.as_str().contains("Player"))
        .all(|(_, layout)| layout.linebreak == LineBreak::NoWrap));

    let segments = app
        .world_mut()
        .query_filtered::<(&Node, &BackgroundColor), With<CombatIdentityAccentSegmentCmp>>()
        .iter(app.world())
        .map(|(node, color)| (node.height, color.0))
        .collect::<Vec<_>>();
    for color in attacker_colors {
        assert!(segments.contains(&(Val::Percent(50.0), color)));
    }
}

#[test]
fn joint_attack_badge_keeps_every_count_in_order_and_fits_crowded_fleets() {
    let mut report = report(5, 0, true, 31);
    let bomber = Unit::Ship(Ship::Bomber);
    report.mission.owner = 3;
    report.mission.joint_attack = Some(JointAttackMission {
        attackers: [
            (1, Army::from([(bomber, 99)])),
            (2, Army::from([(bomber, 99)])),
            (3, Army::from([(bomber, 99)])),
            (4, Army::from([(bomber, 99)])),
        ]
        .into(),
        ..default()
    });
    let (owner, count, others) = combat_unit_counts(&report, &Side::Attacker, &bomber);
    assert_eq!((owner, count), (Some(3), 99));
    assert_eq!(others, vec![(1, 99), (2, 99), (4, 99)]);
    let badge_width = combat_count_badge_width(UNIT_SIZE, count, &others);
    assert!(combat_count_font_size(badge_width, 1.0, count, &others) < COMBAT_COUNT_FONT_SIZE);
}

#[test]
fn individual_formation_fits_sixty_ships_and_keeps_capital_ships_behind_fodder() {
    let fighter = Unit::Ship(Ship::LightFighter);
    let destroyer = Unit::Ship(Ship::Destroyer);
    let war_sun = Unit::Ship(Ship::WarSun);
    let units = std::iter::repeat_n(fighter, 40)
        .chain(std::iter::repeat_n(destroyer, 10))
        .chain(std::iter::repeat_n(war_sun, 10))
        .collect::<Vec<_>>();

    let layout = individual_formation_layout(&units, 0.0, 1_280.0, 80.0, 300.0, true, UNIT_SIZE);

    assert_eq!(layout.len(), 60);
    for (position, size) in &layout {
        assert!(position.x - size * 0.5 >= -640.1);
        assert!(position.x + size * 0.5 <= 640.1);
        assert!(position.y - size * 0.75 >= 79.9);
        assert!(position.y + size * 0.5 <= 300.1);
    }
    let average_y = |unit| {
        let positions = units
            .iter()
            .zip(&layout)
            .filter_map(|(candidate, (position, _))| (*candidate == unit).then_some(position.y));
        let positions = positions.collect::<Vec<_>>();
        positions.iter().sum::<f32>() / positions.len() as f32
    };
    let average_size = |unit| {
        let sizes = units
            .iter()
            .zip(&layout)
            .filter_map(|(candidate, (_, size))| (*candidate == unit).then_some(*size));
        let sizes = sizes.collect::<Vec<_>>();
        sizes.iter().sum::<f32>() / sizes.len() as f32
    };
    assert!(average_y(war_sun) > average_y(destroyer));
    assert!(average_y(destroyer) > average_y(fighter));
    assert!(average_size(war_sun) > average_size(destroyer));
    assert!(average_size(destroyer) > average_size(fighter));
    for unit in [fighter, destroyer, war_sun] {
        let rows = units
            .iter()
            .zip(&layout)
            .filter_map(|(candidate, (position, _))| (*candidate == unit).then_some(position.y))
            .collect::<Vec<_>>();
        assert!(rows.iter().all(|y| (*y - rows[0]).abs() < f32::EPSILON));
    }

    let defender_layout =
        individual_formation_layout(&units, 0.0, 1_280.0, -300.0, -80.0, false, UNIT_SIZE);
    for (position, size) in &defender_layout {
        assert!(position.y - size * 0.75 >= -300.1);
        assert!(position.y + size * 0.5 <= -79.9);
    }
    let defender_average_y = |unit| {
        let positions = units
            .iter()
            .zip(&defender_layout)
            .filter_map(|(candidate, (position, _))| (*candidate == unit).then_some(position.y))
            .collect::<Vec<_>>();
        positions.iter().sum::<f32>() / positions.len() as f32
    };
    assert!(defender_average_y(war_sun) < defender_average_y(destroyer));
    assert!(defender_average_y(destroyer) < defender_average_y(fighter));
}

#[test]
fn individual_formation_keeps_twelve_of_each_ship_together_with_clear_type_gaps() {
    let ship_types = [
        Ship::LightFighter,
        Ship::HeavyFighter,
        Ship::Destroyer,
        Ship::Cruiser,
        Ship::Bomber,
        Ship::Battleship,
        Ship::Dreadnought,
        Ship::WarSun,
    ];
    let units = ship_types
        .into_iter()
        .flat_map(|ship| std::iter::repeat_n(Unit::Ship(ship), 12))
        .collect::<Vec<_>>();
    let layout = individual_formation_layout(&units, 0.0, 1_280.0, 80.0, 300.0, true, UNIT_SIZE);

    assert_eq!(layout.len(), ship_types.len() * 12);
    for ship in ship_types {
        let unit = Unit::Ship(ship);
        let cards = units
            .iter()
            .zip(&layout)
            .filter_map(|(candidate, layout)| (*candidate == unit).then_some(layout))
            .collect::<Vec<_>>();
        assert_eq!(cards.len(), 12);
        assert!(cards.iter().all(|(position, _)| position.y == cards[0].0.y));
    }

    let fighter = Unit::Ship(Ship::LightFighter);
    let cruiser = Unit::Ship(Ship::Cruiser);
    let war_sun = Unit::Ship(Ship::WarSun);
    let sample = [fighter, fighter, fighter, cruiser, cruiser, cruiser, war_sun, war_sun, war_sun];
    let sample_layout =
        individual_formation_layout(&sample, 0.0, 1_280.0, 0.0, 500.0, true, UNIT_SIZE);
    let edge_gap = |left: usize, right: usize| {
        sample_layout[right].0.x
            - sample_layout[left].0.x
            - (sample_layout[left].1 + sample_layout[right].1) * 0.5
    };
    assert!(edge_gap(2, 3) > edge_gap(0, 1) * 3.0);
    assert!(sample_layout[3].1 > sample_layout[0].1 * 1.3);
    assert!(sample_layout[6].1 > sample_layout[3].1 * 1.3);
}

#[test]
fn combat_formation_toggle_splits_and_combines_cards_smoothly() {
    let mut app = App::new();
    app.init_resource::<Settings>().init_resource::<Time>();
    let unit = Unit::Ship(Ship::Destroyer);
    let group = app
        .world_mut()
        .spawn((
            Sprite::default(),
            Transform::default(),
            Visibility::Inherited,
            GroupedCombatUnitCmp,
            CombatUnitCmp {
                unit,
                side: Side::Attacker,
                fire: FireState::Idle,
                shield: unit.shield(),
                max_shield: unit.shield(),
                hull: unit.hull(),
                max_hull: unit.hull(),
                outcome_visible: false,
            },
        ))
        .id();
    let individual = app
        .world_mut()
        .spawn((
            Transform::default(),
            Visibility::Hidden,
            IndividualCombatUnitCmp {
                id: Some(7),
                owner: Some(1),
                unit,
                side: Side::Attacker,
                group,
                home: Vec3::new(100.0, 40.0, COMBAT_SHIP_Z),
                display_size: 70.0,
                transition_start: Vec3::ZERO,
                shield: unit.shield(),
                max_shield: unit.shield(),
                hull: unit.hull(),
                max_hull: unit.hull(),
            },
        ))
        .id();
    app.insert_resource(CombatFormationState::new(false));
    app.world_mut().resource_mut::<Settings>().combat_individual_units = true;

    app.world_mut()
        .resource_mut::<Time>()
        .advance_by(Duration::from_secs_f32(COMBAT_FORMATION_TRANSITION_SECS * 0.5));
    app.world_mut().run_system_once(update_combat_formation).unwrap();
    let midpoint = app.world().get::<Transform>(individual).unwrap().translation;
    assert!(midpoint.x > 0.0 && midpoint.x < 100.0);
    assert_eq!(*app.world().get::<Visibility>(group).unwrap(), Visibility::Hidden);
    assert_eq!(app.world().get::<Sprite>(group).unwrap().color.alpha(), 0.0);
    assert_eq!(*app.world().get::<Visibility>(individual).unwrap(), Visibility::Inherited);

    app.world_mut()
        .resource_mut::<Time>()
        .advance_by(Duration::from_secs_f32(COMBAT_FORMATION_TRANSITION_SECS * 0.5));
    app.world_mut().run_system_once(update_combat_formation).unwrap();
    assert_eq!(app.world().get::<Transform>(individual).unwrap().translation.x, 100.0);
    assert_eq!(*app.world().get::<Visibility>(group).unwrap(), Visibility::Hidden);

    app.world_mut().resource_mut::<Settings>().combat_individual_units = false;
    app.world_mut()
        .resource_mut::<Time>()
        .advance_by(Duration::from_secs_f32(COMBAT_FORMATION_TRANSITION_SECS));
    app.world_mut().run_system_once(update_combat_formation).unwrap();
    assert_eq!(*app.world().get::<Visibility>(group).unwrap(), Visibility::Inherited);
    assert_eq!(*app.world().get::<Visibility>(individual).unwrap(), Visibility::Hidden);
    assert_eq!(app.world().get::<Transform>(individual).unwrap().translation, Vec3::ZERO);
}

#[test]
fn individual_mode_spawns_one_visible_card_for_every_recorded_combatant() {
    let report = report(5, 0, true, 61);
    let round = report.combat_report.as_ref().unwrap().rounds[0].clone();
    let expected = round.attacker.len() + round.defender.len();
    let mut rng = DeterministicRngState::from_u64(61).next_rng();
    let origin = Planet::new_with_rng(0, "Origin".into(), Vec2::ZERO, false, 1.0, &mut rng);
    let map = Map {
        rect: Rect::new(-100.0, -100.0, 100.0, 100.0),
        solar_corner: crate::core::map::model::SolarCorner::BottomLeft,
        planets: vec![origin, report.planet.clone()],
    };
    let mut app = playback_app(report, 0, CombatState::Fire);
    app.insert_resource(map).init_resource::<MultiplayerSession>();
    app.world_mut().resource_mut::<Settings>().combat_individual_units = true;

    app.world_mut().run_system_once(setup_combat).unwrap();

    let cards = app
        .world_mut()
        .query::<(&Sprite, &Transform, &Visibility, &IndividualCombatUnitCmp)>()
        .iter(app.world())
        .map(|(sprite, transform, visibility, individual)| {
            (
                sprite.custom_size,
                transform.translation,
                *visibility,
                individual.home,
                individual.unit,
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(cards.len(), expected);
    assert!(cards.iter().all(|(_, _, visibility, _, _)| *visibility == Visibility::Inherited));
    assert!(cards
        .iter()
        .all(|(size, _, _, _, _)| { size.is_some_and(|size| size.x < UNIT_SIZE && size.x > 0.0) }));
    assert!(cards
        .iter()
        .all(|(_, position, _, home, _)| { position.x == home.x && position.y != home.y }));
    let mut bomber_entry_x = cards
        .iter()
        .filter_map(|(_, position, _, _, unit)| {
            (*unit == Unit::Ship(Ship::Bomber)).then_some(position.x)
        })
        .collect::<Vec<_>>();
    bomber_entry_x.sort_by(f32::total_cmp);
    bomber_entry_x.dedup_by(|left, right| (*left - *right).abs() < f32::EPSILON);
    assert_eq!(bomber_entry_x.len(), 5);
    assert!(app
        .world_mut()
        .query_filtered::<&Visibility, With<GroupedCombatUnitCmp>>()
        .iter(app.world())
        .all(|visibility| *visibility == Visibility::Hidden));
}

#[test]
fn individual_fleets_keep_the_center_clear_and_straddle_the_planetary_shield() {
    let mut report = report(12, 3, true, 73);
    report.planet.army.insert(Unit::Ship(Ship::LightFighter), 12);
    let mut rng = DeterministicRngState::from_u64(73).next_rng();
    let report = resolve_combat_with_rng(1, &report.mission, &report.planet, &mut rng);
    let origin = Planet::new_with_rng(0, "Origin".into(), Vec2::ZERO, false, 1.0, &mut rng);
    let map = Map {
        rect: Rect::new(-100.0, -100.0, 100.0, 100.0),
        solar_corner: crate::core::map::model::SolarCorner::BottomLeft,
        planets: vec![origin, report.planet.clone()],
    };
    let mut app = playback_app(report, 0, CombatState::Fire);
    app.insert_resource(map).init_resource::<MultiplayerSession>();
    app.world_mut().resource_mut::<Settings>().combat_individual_units = true;
    let camera =
        app.world_mut().query_filtered::<Entity, With<MainCamera>>().single(app.world()).unwrap();
    let mut projection = OrthographicProjection::default_2d();
    projection.area = Rect::new(-960.0, -540.0, 960.0, 540.0);
    app.world_mut().entity_mut(camera).insert(Projection::Orthographic(projection));
    app.world_mut().run_system_once(setup_combat).unwrap();

    let individuals = app
        .world_mut()
        .query::<(Entity, &IndividualCombatUnitCmp)>()
        .iter(app.world())
        .map(|(entity, card)| (entity, card.unit, card.side.clone(), card.home, card.display_size))
        .collect::<Vec<_>>();
    let attacker_bottom = individuals
        .iter()
        .filter(|(_, _, side, _, _)| *side == Side::Attacker)
        .map(|(_, _, _, home, size)| home.y - size * INDIVIDUAL_CARD_LOWER_EXTENT)
        .fold(f32::INFINITY, f32::min);
    let defender_ship_top = individuals
        .iter()
        .filter(|(_, unit, side, _, _)| *side == Side::Defender && unit.is_ship())
        .map(|(_, _, _, home, size)| home.y + size * INDIVIDUAL_CARD_UPPER_EXTENT)
        .fold(f32::NEG_INFINITY, f32::max);
    assert!(
        attacker_bottom - defender_ship_top >= 1_080.0 * INDIVIDUAL_FLEET_SEPARATION_FACTOR - 0.1,
        "the opposing front ranks retain the central combat corridor"
    );

    let (shield_home, shield_size) = app
        .world_mut()
        .query::<(&Sprite, &CombatUnitCmp, &CombatCardHome)>()
        .iter(app.world())
        .find_map(|(sprite, card, home)| {
            (card.unit == Unit::planetary_shield()).then_some((home.0, sprite.custom_size.unwrap()))
        })
        .unwrap();
    let shield_top = shield_home.y + shield_size.y * 0.5;
    let shield_bottom = shield_home.y - shield_size.y * 0.5;
    assert!(individuals
        .iter()
        .filter(|(_, unit, side, _, _)| *side == Side::Defender && unit.is_ship())
        .all(|(_, _, _, home, size)| {
            home.y - size * INDIVIDUAL_CARD_LOWER_EXTENT
                >= shield_top + COMBAT_SHIELD_DEFENSE_GAP - 0.1
        }));
    assert!(individuals
        .iter()
        .filter(|(_, unit, side, _, _)| {
            *side == Side::Defender && !unit.is_ship() && *unit != Unit::space_dock()
        })
        .all(|(_, _, _, home, size)| {
            home.y + size * INDIVIDUAL_CARD_UPPER_EXTENT
                <= shield_bottom - COMBAT_SHIELD_DEFENSE_GAP + 0.1
        }));

    let bomber = individuals
        .iter()
        .find_map(|(entity, unit, side, _, size)| {
            (*side == Side::Attacker && *unit == Unit::Ship(Ship::Bomber))
                .then_some((*entity, *size))
        })
        .unwrap();
    let descendants = app
        .world_mut()
        .run_system_once(move |children: Query<&Children>| {
            children.iter_descendants(bomber.0).collect::<Vec<_>>()
        })
        .unwrap();
    let frame = |marker: fn(&World, Entity) -> bool| {
        let fill = descendants.iter().copied().find(|entity| marker(app.world(), *entity)).unwrap();
        let frame = app.world().get::<ChildOf>(fill).unwrap().parent();
        (
            app.world().get::<Transform>(frame).unwrap().translation.y,
            app.world().get::<Sprite>(frame).unwrap().custom_size.unwrap().y,
        )
    };
    let (shield_y, shield_height) = frame(|world, entity| world.get::<ShieldCmp>(entity).is_some());
    let (hull_y, hull_height) = frame(|world, entity| world.get::<HullCmp>(entity).is_some());
    assert!((shield_y + shield_height * 0.5 + bomber.1 * 0.5).abs() < 0.01);
    assert!((hull_y + hull_height * 0.5 - (shield_y - shield_height * 0.5)).abs() < 0.01);
}

#[test]
fn undefended_fleets_and_space_fauna_use_the_low_defender_row() {
    let cases = [
        (Army::from([(Unit::Ship(Ship::LightFighter), 12)]), BombingRaid::Economic),
        (Army::from([(Unit::Fauna(SpaceFauna::AetherRay), 12)]), BombingRaid::Economic),
        (
            Army::from([(Unit::Ship(Ship::LightFighter), 12), (Unit::planetary_shield(), 1)]),
            BombingRaid::None,
        ),
    ];

    for (case, (defenders, bombing)) in cases.into_iter().enumerate() {
        let seed = 79 + case as u64;
        let mut open_field = report(12, 0, false, seed);
        open_field.planet.army = defenders.into();
        open_field.mission.bombing = bombing;
        let mut rng = DeterministicRngState::from_u64(seed).next_rng();
        let report = resolve_combat_with_rng(1, &open_field.mission, &open_field.planet, &mut rng);
        let origin = Planet::new_with_rng(0, "Origin".into(), Vec2::ZERO, false, 1.0, &mut rng);
        let map = Map {
            rect: Rect::new(-100.0, -100.0, 100.0, 100.0),
            solar_corner: crate::core::map::model::SolarCorner::BottomLeft,
            planets: vec![origin, report.planet.clone()],
        };
        let mut app = playback_app(report, 0, CombatState::Fire);
        app.insert_resource(map).init_resource::<MultiplayerSession>();
        app.world_mut().resource_mut::<Settings>().combat_individual_units = true;
        let camera = app
            .world_mut()
            .query_filtered::<Entity, With<MainCamera>>()
            .single(app.world())
            .unwrap();
        let mut projection = OrthographicProjection::default_2d();
        projection.area = Rect::new(-960.0, -540.0, 960.0, 540.0);
        app.world_mut().entity_mut(camera).insert(Projection::Orthographic(projection));
        app.world_mut().run_system_once(setup_combat).unwrap();

        let individuals = app
            .world_mut()
            .query::<&IndividualCombatUnitCmp>()
            .iter(app.world())
            .map(|card| (card.unit, card.side.clone(), card.home, card.display_size))
            .collect::<Vec<_>>();
        let defender_top = individuals
            .iter()
            .filter(|(_, side, _, _)| *side == Side::Defender)
            .map(|(_, _, home, size)| home.y + size * INDIVIDUAL_CARD_UPPER_EXTENT)
            .fold(f32::NEG_INFINITY, f32::max);
        let attacker_bottom = individuals
            .iter()
            .filter(|(_, side, _, _)| *side == Side::Attacker)
            .map(|(_, _, home, size)| home.y - size * INDIVIDUAL_CARD_LOWER_EXTENT)
            .fold(f32::INFINITY, f32::min);
        let low_row_top = -540.0
            + 180.0
            + UNIT_SIZE * (COMBAT_DEFENDER_Y_OFFSET_FACTOR + INDIVIDUAL_CARD_UPPER_EXTENT);

        assert!(defender_top <= low_row_top + 0.1, "case {case} stays in the low defender band");
        assert!(
            attacker_bottom - defender_top > 1_080.0 * 0.25,
            "case {case} leaves the unused center of the battlefield open"
        );
        if case == 1 {
            assert!(individuals.iter().any(|(unit, side, _, _)| {
                *side == Side::Defender && *unit == Unit::Fauna(SpaceFauna::AetherRay)
            }));
        }
    }
}

#[test]
fn individual_fire_uses_each_shooter_and_updates_the_recorded_target() {
    let report = report(5, 0, true, 67);
    let round = report.combat_report.as_ref().unwrap().rounds[0].clone();
    let mut app = playback_app(report.clone(), 0, CombatState::Fire);
    app.world_mut().resource_mut::<Settings>().combat_individual_units = true;
    app.insert_resource(CombatFormationState::new(true));

    let bomber = Unit::Ship(Ship::Bomber);
    let gauss = Unit::Defense(Defense::GaussCannon);
    let bomber_group = spawn_unit(
        &mut app,
        bomber,
        round.attacker.iter().filter(|unit| unit.unit == bomber).count(),
        Side::Attacker,
        FireState::Firing,
    );
    let gauss_count = round.defender.iter().filter(|unit| unit.unit == gauss).count();
    let gauss_group = spawn_unit(&mut app, gauss, gauss_count, Side::Defender, FireState::Idle);

    for (index, combatant) in round.attacker.iter().filter(|unit| unit.unit == bomber).enumerate() {
        app.world_mut().spawn((
            Sprite {
                custom_size: Some(Vec2::splat(60.0)),
                ..default()
            },
            Transform::from_xyz(-250.0 + index as f32 * 80.0, 180.0, COMBAT_SHIP_Z),
            IndividualCombatUnitCmp {
                id: Some(combatant.id),
                owner: combatant.owner,
                unit: bomber,
                side: Side::Attacker,
                group: bomber_group,
                home: Vec3::ZERO,
                display_size: 60.0,
                transition_start: Vec3::ZERO,
                shield: report.unit_shield(bomber, &Side::Attacker),
                max_shield: report.unit_shield(bomber, &Side::Attacker),
                hull: report.unit_hull(bomber, &Side::Attacker),
                max_hull: report.unit_hull(bomber, &Side::Attacker),
            },
        ));
    }
    for (index, combatant) in round.defender.iter().filter(|unit| unit.unit == gauss).enumerate() {
        app.world_mut().spawn((
            Sprite {
                custom_size: Some(Vec2::splat(54.0)),
                ..default()
            },
            Transform::from_xyz(-300.0 + index as f32 * 55.0, -120.0, COMBAT_SHIP_Z),
            IndividualCombatUnitCmp {
                id: Some(combatant.id),
                owner: combatant.owner,
                unit: gauss,
                side: Side::Defender,
                group: gauss_group,
                home: Vec3::ZERO,
                display_size: 54.0,
                transition_start: Vec3::ZERO,
                shield: report.unit_shield(gauss, &Side::Defender),
                max_shield: report.unit_shield(gauss, &Side::Defender),
                hull: report.unit_hull(gauss, &Side::Defender),
                max_hull: report.unit_hull(gauss, &Side::Defender),
            },
        ));
    }

    app.world_mut().run_system_once(animate_combat).unwrap();
    let messages =
        app.world_mut().resource_mut::<Messages<SpawnShotMsg>>().drain().collect::<Vec<_>>();
    assert!(!messages.is_empty());
    let source_ids = messages
        .iter()
        .filter_map(|message| {
            message
                .source
                .and_then(|(entity, _, _)| app.world().get::<IndividualCombatUnitCmp>(entity))
                .and_then(|unit| unit.id)
        })
        .collect::<BTreeSet<_>>();
    let expected_sources = round
        .attacker
        .iter()
        .filter(|unit| unit.unit == bomber && unit.shots.iter().any(|shot| !shot.is_bombing()))
        .map(|unit| unit.id)
        .collect::<BTreeSet<_>>();
    assert_eq!(source_ids, expected_sources);
    assert!(messages.iter().all(|message| message.shot.target_id.is_some()));

    for message in messages {
        app.world_mut().resource_mut::<Messages<SpawnShotMsg>>().write(message);
    }
    app.add_systems(Update, run_combat_animations);
    app.update();
    app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs(5));
    app.update();

    for (_, individual) in app
        .world_mut()
        .query::<(Entity, &IndividualCombatUnitCmp)>()
        .iter(app.world())
        .filter(|(_, unit)| unit.side == Side::Defender && unit.unit == gauss)
    {
        let recorded =
            round.defender.iter().find(|record| Some(record.id) == individual.id).unwrap();
        assert_eq!(individual.hull, recorded.hull);
        assert_eq!(individual.shield, recorded.shield);
    }
}

#[test]
fn colonial_withdrawal_hides_colony_ships_and_flies_combat_ships_to_an_upper_corner() {
    use crate::core::combat::resolution::resolve_combat_with_retreat_with_rng;
    use crate::core::energy::EnergyGrid;
    use crate::core::units::buildings::{Building, FleetWithdrawal};
    for level in [4, 5] {
        let mut source = report(1, 0, false, 17);
        source.mission.bombing = BombingRaid::None;
        source.mission.army = Army::from([(Unit::Ship(Ship::LightFighter), 1)]);
        source.planet.army = Army::from([
            (Unit::Building(Building::ColonialAdministration), level),
            (Unit::Ship(Ship::Dreadnought), 2),
            (Unit::colony_ship(), 1),
            (Unit::Defense(Defense::GaussCannon), 3),
        ])
        .into();
        source.planet.fleet_withdrawal = FleetWithdrawal::Immediate;
        let mut rng = DeterministicRngState::from_u64(17).next_rng();
        let report = resolve_combat_with_retreat_with_rng(
            1,
            &source.mission,
            &source.planet,
            EnergyGrid {
                supply: 1,
                demand: 1,
            },
            Some(2),
            &mut rng,
        );
        assert!(report.combat_report.as_ref().unwrap().defender_retreat.is_some());
        let mut app = playback_app(report, 0, CombatState::Fire);
        app.add_plugins(bevy_tweening::TweeningPlugin);
        let ship = spawn_unit(
            &mut app,
            Unit::Ship(Ship::Dreadnought),
            2,
            Side::Defender,
            FireState::Fired,
        );
        let ground = spawn_unit(
            &mut app,
            Unit::Defense(Defense::GaussCannon),
            3,
            Side::Defender,
            FireState::Fired,
        );
        app.world_mut().resource_mut::<Settings>().combat_paused = true;
        app.world_mut().run_system_once(animate_combat).unwrap();
        assert!(app.world().get::<FleetRetreatCmp>(ship).is_none());
        app.world_mut().resource_mut::<Settings>().combat_paused = false;
        app.world_mut().run_system_once(animate_combat).unwrap();
        assert!(app.world().get::<FleetRetreatCmp>(ship).is_some());
        assert!(app.world().get::<FleetRetreatCmp>(ground).is_none());
        assert!(app
            .world_mut()
            .query::<&CombatUnitCmp>()
            .iter(app.world())
            .all(|unit| unit.unit != Unit::colony_ship()));
        TweenAnim::step_all(app.world_mut(), Duration::from_millis(450));
        let retreat_position = app.world().get::<Transform>(ship).unwrap().translation;
        assert!(retreat_position.x > 0.);
        assert!(retreat_position.y > 0.);
        assert_eq!(app.world().get::<Transform>(ground).unwrap().translation, Vec3::ZERO);
        app.world_mut().run_system_once(animate_combat).unwrap();
        assert!(app.world().get_entity(ship).is_ok());
        TweenAnim::step_all(app.world_mut(), Duration::from_millis(500));
        app.world_mut().run_system_once(animate_combat).unwrap();
        assert!(app.world().get_entity(ship).is_err());
        assert!(app.world().get_entity(ground).is_ok());
        app.world_mut().run_system_once(animate_combat).unwrap();
        let playback =
            app.world_mut().query::<&FleetRetreatPlayback>().single(app.world()).unwrap();
        assert!(playback.complete);
        assert!(app.world().resource::<Messages<SpawnShotMsg>>().is_empty());
    }
}

#[test]
fn combat_setup_never_spawns_colony_ship_cards() {
    let mut report = report(1, 0, true, 29);
    report.mission.army.insert(Unit::colony_ship(), 2);
    report.planet.army.insert(Unit::colony_ship(), 3);
    let mut rng = DeterministicRngState::from_u64(29).next_rng();
    let origin = Planet::new_with_rng(0, "Origin".into(), Vec2::ZERO, false, 1., &mut rng);
    let map = Map {
        rect: Rect::new(-100., -100., 100., 100.),
        solar_corner: crate::core::map::model::SolarCorner::BottomLeft,
        planets: vec![origin, report.planet.clone()],
    };
    let mut app = playback_app(report, 0, CombatState::Fire);
    app.insert_resource(map).init_resource::<MultiplayerSession>();
    app.world_mut().run_system_once(setup_combat).unwrap();

    assert!(app
        .world_mut()
        .query::<&CombatUnitCmp>()
        .iter(app.world())
        .all(|unit| unit.unit != Unit::colony_ship()));
}

#[test]
fn defender_cards_show_spaced_equally_sized_counts_in_each_players_color() {
    let mut unresolved = report(5, 0, true, 31);
    let fighter = Unit::Ship(Ship::LightFighter);
    unresolved.planet.army.insert(fighter, 6);
    unresolved.planet.army.dock_protector(3, Army::from([(fighter, 3)]));
    let mut rng = DeterministicRngState::from_u64(31).next_rng();
    let report = resolve_combat_with_rng(1, &unresolved.mission, &unresolved.planet, &mut rng);
    assert_eq!(combat_unit_counts(&report, &Side::Defender, &fighter), (Some(2), 6, vec![(3, 3)]));

    let origin = Planet::new_with_rng(0, "Origin".into(), Vec2::ZERO, false, 1., &mut rng);
    let map = Map {
        rect: Rect::new(-100., -100., 100., 100.),
        solar_corner: crate::core::map::model::SolarCorner::BottomLeft,
        planets: vec![origin, report.planet.clone()],
    };
    let mut app = playback_app(report, 0, CombatState::Fire);
    app.insert_resource(map).init_resource::<MultiplayerSession>();
    app.world_mut().run_system_once(setup_combat).unwrap();

    let card = app
        .world_mut()
        .query::<(Entity, &CombatUnitCmp)>()
        .iter(app.world())
        .find_map(|(entity, card)| {
            (card.side == Side::Defender && card.unit == fighter).then_some(entity)
        })
        .unwrap();
    let descendants = app
        .world_mut()
        .run_system_once(move |children: Query<&Children>| {
            children.iter_descendants(card).collect::<Vec<_>>()
        })
        .unwrap();
    let owner = descendants
        .iter()
        .find_map(|entity| {
            let counter = app.world().get::<CountCmp>(*entity)?;
            let text = app.world().get::<Text2d>(*entity)?;
            let font = app.world().get::<TextFont>(*entity)?;
            let color = app.world().get::<TextColor>(*entity)?;
            let transform = app.world().get::<Transform>(*entity)?;
            (counter.owner == Some(2)).then_some((
                text.0.clone(),
                font.font_size,
                color.0,
                transform.scale,
            ))
        })
        .unwrap();
    let protector = descendants
        .iter()
        .find_map(|entity| {
            let counter = app.world().get::<CountCmp>(*entity)?;
            let text = app.world().get::<TextSpan>(*entity)?;
            let font = app.world().get::<TextFont>(*entity)?;
            let color = app.world().get::<TextColor>(*entity)?;
            (counter.owner == Some(3)).then_some((text.0.clone(), font.font_size, color.0))
        })
        .unwrap();

    assert_eq!(owner.0, "6");
    assert_eq!(owner.2, app.world().resource::<MultiplayerSession>().player_color(2).color());
    assert!(matches!(
        owner.1,
        FontSize::Px(size) if size > 0.0 && size <= COMBAT_COUNT_FONT_SIZE
    ));
    assert_eq!(owner.3, Vec3::ONE);
    assert_eq!(protector.0, "  3");
    assert_eq!(protector.1, owner.1);
    assert_eq!(protector.2, app.world().resource::<MultiplayerSession>().player_color(3).color());

    {
        let mut player = app.world_mut().resource_mut::<Player>();
        let round =
            &mut player.reports.first_mut().unwrap().combat_report.as_mut().unwrap().rounds[0];
        let mut owner_survivors = 0;
        let mut protector_survivors = 0;
        for combatant in round.defender.iter_mut().filter(|combatant| combatant.unit == fighter) {
            let keep = match combatant.owner {
                Some(2) if owner_survivors < 2 => {
                    owner_survivors += 1;
                    true
                },
                Some(3) if protector_survivors < 1 => {
                    protector_survivors += 1;
                    true
                },
                _ => false,
            };
            if keep {
                combatant.hull = combatant.unit.hull();
            } else {
                combatant.hull = 0;
            }
        }
    }

    // Recorded casualties stay hidden until the volley has fully resolved.
    app.world_mut().run_system_once(update_combat_stats).unwrap();
    assert_eq!(app.world().get::<Text2d>(owner_entity(&app, &descendants, 2)).unwrap().0, "6");
    assert_eq!(app.world().get::<TextSpan>(owner_entity(&app, &descendants, 3)).unwrap().0, "  3");

    app.world_mut().get_mut::<CombatUnitCmp>(card).unwrap().outcome_visible = true;
    app.world_mut().run_system_once(update_combat_stats).unwrap();
    assert_eq!(
        app.world()
            .get::<Text2d>(
                descendants
                    .iter()
                    .copied()
                    .find(|entity| app
                        .world()
                        .get::<CountCmp>(*entity)
                        .is_some_and(|c| c.owner == Some(2)))
                    .unwrap()
            )
            .unwrap()
            .0,
        "2"
    );
    assert_eq!(
        app.world()
            .get::<TextSpan>(
                descendants
                    .iter()
                    .copied()
                    .find(|entity| app
                        .world()
                        .get::<CountCmp>(*entity)
                        .is_some_and(|c| c.owner == Some(3)))
                    .unwrap()
            )
            .unwrap()
            .0,
        "  1"
    );
}

#[test]
fn volley_fire_selects_every_attacker_before_any_defender() {
    let mut report = report(20, 0, true, 41);
    let round = &mut report.combat_report.as_mut().unwrap().rounds[0];
    let mut fighter = round
        .attacker
        .iter()
        .find(|unit| unit.shots.iter().any(|shot| !shot.is_bombing()))
        .unwrap()
        .clone();
    fighter.unit = Unit::Ship(Ship::LightFighter);
    round.attacker.push(fighter);

    let mut app = playback_app(report, 0, CombatState::Fire);
    let bomber =
        spawn_unit(&mut app, Unit::Ship(Ship::Bomber), 20, Side::Attacker, FireState::Idle);
    let fighter =
        spawn_unit(&mut app, Unit::Ship(Ship::LightFighter), 1, Side::Attacker, FireState::Idle);
    let defender = spawn_unit(
        &mut app,
        Unit::Defense(Defense::GaussCannon),
        12,
        Side::Defender,
        FireState::Idle,
    );
    let repair_truck =
        spawn_unit(&mut app, Unit::repair_truck(), 1, Side::Defender, FireState::Idle);
    app.world_mut().run_system_once(animate_combat).unwrap();
    assert_eq!(
        app.world_mut()
            .query::<&CombatUnitCmp>()
            .iter(app.world())
            .filter(|card| card.side == Side::Attacker && card.fire == FireState::Select)
            .count(),
        1,
        "the default presentation still selects one unit kind at a time"
    );
    {
        let world = app.world_mut();
        let mut cards = world.query::<&mut CombatUnitCmp>();
        for mut card in cards.iter_mut(world) {
            card.fire = FireState::Idle;
        }
    }

    app.world_mut().resource_mut::<Settings>().combat_volley_fire = true;
    app.world_mut().run_system_once(animate_combat).unwrap();

    assert!(app.world().get::<CombatUnitCmp>(bomber).unwrap().fire == FireState::Select);
    assert!(app.world().get::<CombatUnitCmp>(fighter).unwrap().fire == FireState::Select);
    assert!(app.world().get::<CombatUnitCmp>(defender).unwrap().fire == FireState::Idle);
    assert!(app.world().get::<CombatUnitCmp>(repair_truck).unwrap().fire == FireState::Idle);

    app.world_mut().get_mut::<CombatUnitCmp>(bomber).unwrap().fire = FireState::Fired;
    app.world_mut().get_mut::<CombatUnitCmp>(fighter).unwrap().fire = FireState::Fired;
    app.world_mut().run_system_once(animate_combat).unwrap();
    assert!(app.world().get::<CombatUnitCmp>(defender).unwrap().fire == FireState::Select);
    assert!(
        app.world().get::<CombatUnitCmp>(repair_truck).unwrap().fire == FireState::Idle,
        "healing support must not animate as part of the defender's weapon volley"
    );
}

#[test]
fn volley_fire_shoots_immediately_then_waits_after_impacts_before_return_fire() {
    let report = report(20, 0, true, 41);
    let expected_shots = report.combat_report.as_ref().unwrap().rounds[0]
        .attacker
        .iter()
        .filter(|unit| unit.unit == Unit::Ship(Ship::Bomber))
        .flat_map(|unit| &unit.shots)
        .filter(|shot| !shot.is_bombing())
        .count();
    let mut app = playback_app(report, 0, CombatState::Fire);
    app.add_plugins(bevy_tweening::TweeningPlugin);
    app.world_mut().resource_mut::<Settings>().combat_volley_fire = true;
    let bomber =
        spawn_unit(&mut app, Unit::Ship(Ship::Bomber), 20, Side::Attacker, FireState::Select);
    let defender = spawn_unit(
        &mut app,
        Unit::Defense(Defense::GaussCannon),
        12,
        Side::Defender,
        FireState::Idle,
    );
    let original = Transform::from_xyz(25.0, 40.0, 3.0).with_scale(Vec3::splat(0.8));
    app.world_mut().entity_mut(bomber).insert(original);
    let mut shots = app.world().resource::<Messages<SpawnShotMsg>>().get_cursor();

    app.world_mut().run_system_once(animate_combat).unwrap();
    assert!(app.world().get::<CombatUnitCmp>(bomber).unwrap().fire == FireState::Firing);
    assert!(app.world().get::<TweenAnim>(bomber).is_none());
    assert_eq!(*app.world().get::<Transform>(bomber).unwrap(), original);
    assert_eq!(shots.read(app.world().resource::<Messages<SpawnShotMsg>>()).count(), 0);

    app.world_mut().run_system_once(animate_combat).unwrap();
    assert!(app.world().get::<CombatUnitCmp>(bomber).unwrap().fire == FireState::Deselect);
    assert_eq!(
        shots.read(app.world().resource::<Messages<SpawnShotMsg>>()).count(),
        expected_shots
    );

    app.world_mut().run_system_once(animate_combat).unwrap();
    assert!(app.world().get::<CombatUnitCmp>(bomber).unwrap().fire == FireState::VolleyResolving);
    assert!(app.world().get::<TweenAnim>(bomber).is_none());
    assert_eq!(*app.world().get::<Transform>(bomber).unwrap(), original);
    assert_eq!(shots.read(app.world().resource::<Messages<SpawnShotMsg>>()).count(), 0);

    app.world_mut().run_system_once(animate_combat).unwrap();
    let pause =
        app.world_mut().query_filtered::<Entity, With<VolleyResolutionPause>>().single(app.world());
    assert!(pause.is_ok());
    let pause = pause.unwrap();
    assert!(app.world().get::<CombatUnitCmp>(defender).unwrap().fire == FireState::Idle);
    assert_eq!(
        app.world().get::<TweenAnim>(pause).unwrap().tweenable().cycle_duration(),
        Duration::from_millis(VOLLEY_RESOLUTION_PAUSE_MS)
    );

    TweenAnim::step_all(app.world_mut(), Duration::from_millis(VOLLEY_RESOLUTION_PAUSE_MS));
    app.world_mut().run_system_once(animate_combat).unwrap();
    assert!(app.world().get::<CombatUnitCmp>(bomber).unwrap().fire == FireState::Fired);
    assert!(app.world().get_entity(pause).is_err());
    assert!(app.world().get::<CombatUnitCmp>(defender).unwrap().fire == FireState::Idle);

    app.world_mut().run_system_once(animate_combat).unwrap();
    assert!(app.world().get::<CombatUnitCmp>(defender).unwrap().fire == FireState::Select);
}

#[test]
fn repair_keeps_its_deliberate_highlight_when_volley_fire_is_enabled() {
    let mut app = playback_app(report(1, 0, true, 41), 0, CombatState::Repair);
    app.world_mut().resource_mut::<Settings>().combat_volley_fire = true;
    let repair_truck =
        spawn_unit(&mut app, Unit::repair_truck(), 1, Side::Defender, FireState::Select);

    app.world_mut().run_system_once(animate_combat).unwrap();

    assert!(app.world().get::<CombatUnitCmp>(repair_truck).unwrap().fire == FireState::PreFire);
    assert_eq!(
        app.world().get::<TweenAnim>(repair_truck).unwrap().tweenable().cycle_duration(),
        Duration::from_millis(500)
    );
}

#[test]
fn combat_cards_show_empty_shield_slots_for_probes_and_support_units() {
    let mut report = report(1, 0, true, 7);
    report.mission.objective = Icon::MissileStrike;
    report.mission.bombing = BombingRaid::None;
    report.mission.army = Army::from([(Unit::interplanetary_missile(), 4)]);
    report.planet.army.insert(Unit::antiballistic_missile(), 3);
    report.planet.army.insert(Unit::crawler(), 1);
    report.planet.army.insert(Unit::repair_truck(), 1);
    let mut rng = DeterministicRngState::from_u64(7).next_rng();
    let mut report = resolve_combat_with_rng(1, &report.mission, &report.planet, &mut rng);
    report.mission.army.insert(Unit::probe(), 1);
    let origin = Planet::new_with_rng(0, "Origin".into(), Vec2::ZERO, false, 1., &mut rng);
    let map = Map {
        rect: Rect::new(-100., -100., 100., 100.),
        solar_corner: crate::core::map::model::SolarCorner::BottomLeft,
        planets: vec![origin, report.planet.clone()],
    };
    let mut app = playback_app(report, 0, CombatState::AntiBallistic);
    app.insert_resource(map).init_resource::<MultiplayerSession>();
    app.world_mut().run_system_once(setup_combat).unwrap();

    let cards = app
        .world_mut()
        .query::<(Entity, &CombatUnitCmp)>()
        .iter(app.world())
        .map(|(entity, card)| (entity, card.unit))
        .collect::<Vec<_>>();
    for expected in [
        Unit::antiballistic_missile(),
        Unit::interplanetary_missile(),
        Unit::crawler(),
        Unit::repair_truck(),
        Unit::probe(),
    ] {
        assert!(cards.iter().any(|(_, unit)| *unit == expected));
    }
    for (entity, unit) in cards {
        let descendants = app
            .world_mut()
            .run_system_once(move |children: Query<&Children>| {
                children.iter_descendants(entity).collect::<Vec<_>>()
            })
            .unwrap();
        let hull_bars = descendants
            .iter()
            .filter(|&&child| app.world().get::<HullCmp>(child).is_some())
            .count();
        let shield_bars = descendants
            .iter()
            .filter(|&&child| app.world().get::<ShieldCmp>(child).is_some())
            .count();
        let empty_shield_slots = descendants
            .iter()
            .filter(|&&child| app.world().get::<EmptyShieldCmp>(child).is_some())
            .count();
        assert_eq!(hull_bars, usize::from(unit.hull() > 0), "{unit:?} hull bars");
        assert_eq!(shield_bars, usize::from(unit.shield() > 0), "{unit:?} shield bars");
        assert_eq!(
            empty_shield_slots,
            usize::from(
                unit == Unit::probe() || unit == Unit::crawler() || unit == Unit::repair_truck()
            ),
            "{unit:?} empty shield slots"
        );

        for empty in descendants
            .iter()
            .copied()
            .filter(|&child| app.world().get::<EmptyShieldCmp>(child).is_some())
        {
            let fill = app.world().get::<Sprite>(empty).unwrap();
            assert_eq!(fill.color, BG2_COLOR);
            let frame = app.world().get::<ChildOf>(empty).unwrap().parent();
            assert_eq!(app.world().get::<Sprite>(frame).unwrap().color, BG2_COLOR);
        }
    }
}

#[test]
fn shieldless_fauna_put_their_hull_bar_directly_below_the_image() {
    let fauna = Unit::Fauna(SpaceFauna::VoidManta);
    let mut report = report(1, 0, true, 43);
    report.planet.name = "Void Manta Nursery".into();
    report.planet.army.clear();
    report.planet.army.insert(fauna, 2);
    let mut rng = DeterministicRngState::from_u64(43).next_rng();
    let origin = Planet::new_with_rng(0, "Origin".into(), Vec2::ZERO, false, 1., &mut rng);
    let map = Map {
        rect: Rect::new(-100., -100., 100., 100.),
        solar_corner: crate::core::map::model::SolarCorner::BottomLeft,
        planets: vec![origin, report.planet.clone()],
    };
    let mut app = playback_app(report, 0, CombatState::Fire);
    app.insert_resource(map).init_resource::<MultiplayerSession>();
    app.world_mut().run_system_once(setup_combat).unwrap();

    let background = app
        .world_mut()
        .query_filtered::<&Sprite, (With<BackgroundImageCmp>, With<CombatCmp>)>()
        .single(app.world())
        .unwrap();
    assert_eq!(background.color, SPACE_FAUNA_BACKGROUND_TINT);

    assert!(app
        .world_mut()
        .query_filtered::<&BackgroundColor, With<CombatIdentityAccentSegmentCmp>>()
        .iter(app.world())
        .any(|color| color.0 == Color::srgb_u8(190, 198, 210)));

    let card = app
        .world_mut()
        .query::<(Entity, &CombatUnitCmp)>()
        .iter(app.world())
        .find_map(|(entity, card)| (card.unit == fauna).then_some(entity))
        .expect("fauna combat card");
    let card_size = app.world().get::<Sprite>(card).unwrap().custom_size.unwrap().x;
    let descendants = app
        .world_mut()
        .run_system_once(move |children: Query<&Children>| {
            children.iter_descendants(card).collect::<Vec<_>>()
        })
        .unwrap();
    assert!(descendants.iter().all(|&entity| app.world().get::<ShieldCmp>(entity).is_none()));
    assert!(descendants.iter().all(|&entity| app.world().get::<EmptyShieldCmp>(entity).is_none()));

    let hull_fill = descendants
        .iter()
        .copied()
        .find(|&entity| app.world().get::<HullCmp>(entity).is_some())
        .expect("fauna hull fill");
    let hull_frame = app.world().get::<ChildOf>(hull_fill).unwrap().parent();
    let hull_y = app.world().get::<Transform>(hull_frame).unwrap().translation.y;
    assert!((hull_y + card_size * 0.57).abs() < 0.01);
}

#[test]
fn planetary_shield_uses_the_full_width_bar_with_its_icon_and_level_count() {
    let mut report = report(5, 3, true, 11);
    for unit in Unit::ships().into_iter().filter(|unit| *unit != Unit::colony_ship()) {
        report.planet.army.insert(unit, 1);
    }
    report.planet.army.insert(Unit::space_dock(), 1);
    let mut rng = DeterministicRngState::from_u64(11).next_rng();
    let origin = Planet::new_with_rng(0, "Origin".into(), Vec2::ZERO, false, 1., &mut rng);
    let map = Map {
        rect: Rect::new(-100., -100., 100., 100.),
        solar_corner: crate::core::map::model::SolarCorner::BottomLeft,
        planets: vec![origin, report.planet.clone()],
    };
    let mut app = playback_app(report, 0, CombatState::Fire);
    app.insert_resource(map).init_resource::<MultiplayerSession>();
    let camera =
        app.world_mut().query_filtered::<Entity, With<MainCamera>>().single(app.world()).unwrap();
    let mut projection = OrthographicProjection::default_2d();
    projection.area = Rect::new(-800.0, -450.0, 800.0, 450.0);
    app.world_mut().entity_mut(camera).insert(Projection::Orthographic(projection));
    app.world_mut().run_system_once(setup_combat).unwrap();

    let (shield_entity, dimensions, shield_home) = app
        .world_mut()
        .query::<(Entity, &Sprite, &CombatUnitCmp, &CombatCardHome)>()
        .iter(app.world())
        .find_map(|(entity, sprite, card, home)| {
            (card.unit == Unit::planetary_shield()).then_some((
                entity,
                sprite.custom_size.unwrap(),
                home.0,
            ))
        })
        .unwrap();
    assert!(dimensions.x > dimensions.y * 30.0, "the old shield is a full-width bar");

    let descendants = app
        .world_mut()
        .run_system_once(move |children: Query<&Children>| {
            children.iter_descendants(shield_entity).collect::<Vec<_>>()
        })
        .unwrap();
    let fill = descendants
        .iter()
        .find_map(|&entity| {
            app.world().get::<ShieldCmp>(entity).and_then(|_| app.world().get::<Sprite>(entity))
        })
        .unwrap();
    assert!(fill.custom_size.unwrap().x > dimensions.x * 0.99);
    assert!(descendants
        .iter()
        .any(|&entity| app.world().get::<PSCombatImageCmp>(entity).is_some()));
    assert!(descendants
        .iter()
        .any(|&entity| { app.world().get::<Text2d>(entity).is_some_and(|text| text.0 == "3") }));
    let shield_icon_offset = descendants
        .iter()
        .find_map(|&entity| {
            app.world()
                .get::<PSCombatImageCmp>(entity)
                .and_then(|_| app.world().get::<Transform>(entity))
                .map(|transform| transform.translation)
        })
        .unwrap();
    let dock_home = app
        .world_mut()
        .query::<(&CombatUnitCmp, &CombatCardHome)>()
        .iter(app.world())
        .find_map(|(card, home)| (card.unit == Unit::space_dock()).then_some(home.0))
        .unwrap();
    assert!(
        (shield_home.x
            - dimensions.x * 0.5
            - (shield_home.x + shield_icon_offset.x + UNIT_SIZE * 0.5))
            .abs()
            < 0.01,
        "the planetary shield bar begins exactly at the image's right edge"
    );
    assert!(
        (shield_home.y + dimensions.y * 0.5
            - (shield_home.y + shield_icon_offset.y + UNIT_SIZE * 0.5))
            .abs()
            < 0.01,
        "the planetary shield bar top aligns exactly with the image's top edge"
    );
    assert!(
        (shield_home.x + dimensions.x * 0.5 - (dock_home.x + UNIT_SIZE * 0.5)).abs() < 0.01,
        "the planetary shield bar ends flush with the space dock"
    );
}

#[test]
fn defense_cards_clear_combat_controls_and_bombing_targets() {
    let mut report = report(5, 3, true, 11);
    report.planet.army.insert(Unit::Ship(Ship::LightFighter), 1);
    for unit in Unit::defenses() {
        if !unit.is_missile() && unit != Unit::space_dock() {
            report.planet.army.insert(unit, 1);
        }
    }
    let mut rng = DeterministicRngState::from_u64(11).next_rng();
    let origin = Planet::new_with_rng(0, "Origin".into(), Vec2::ZERO, false, 1., &mut rng);
    let map = Map {
        rect: Rect::new(-100., -100., 100., 100.),
        solar_corner: crate::core::map::model::SolarCorner::BottomLeft,
        planets: vec![origin, report.planet.clone()],
    };
    let mut app = playback_app(report, 0, CombatState::Fire);
    app.insert_resource(map).init_resource::<MultiplayerSession>();
    let camera =
        app.world_mut().query_filtered::<Entity, With<MainCamera>>().single(app.world()).unwrap();
    let mut projection = OrthographicProjection::default_2d();
    projection.area = Rect::new(-960.0, -540.0, 960.0, 540.0);
    app.world_mut().entity_mut(camera).insert(Projection::Orthographic(projection));
    app.world_mut().run_system_once(setup_combat).unwrap();

    let homes = app
        .world_mut()
        .query::<(&CombatUnitCmp, &CombatCardHome)>()
        .iter(app.world())
        .map(|(card, home)| (card.unit, home.0))
        .collect::<Vec<_>>();
    let defenses = homes
        .iter()
        .filter(|(unit, _)| matches!(unit, Unit::Defense(_)) && !unit.is_missile())
        .map(|(_, home)| *home)
        .collect::<Vec<_>>();
    let buildings = homes
        .iter()
        .filter(|(unit, _)| unit.is_building() && *unit != Unit::planetary_shield())
        .map(|(_, home)| *home)
        .collect::<Vec<_>>();
    assert!(!defenses.is_empty() && !buildings.is_empty());

    let size = UNIT_SIZE;
    let defense_bottom = defenses
        .iter()
        .map(|home| home.y - size * COMBAT_CARD_LOWER_EXTENT_FACTOR)
        .fold(f32::INFINITY, f32::min);
    let controls_top = -540.0 + MAIN_BUTTON_BOTTOM + MAIN_BUTTON_HEIGHT;
    assert!(
        defense_bottom >= controls_top - size * 0.05
            && defense_bottom < controls_top + COMBAT_SHIELD_DEFENSE_GAP,
        "the defense row moves slightly down without materially entering the controls"
    );

    let defense_right =
        defenses.iter().map(|home| home.x + size * 0.5).fold(f32::NEG_INFINITY, f32::max);
    let building_left = buildings
        .iter()
        .map(|home| home.x - size * COMBAT_BUILDING_SIZE_FACTOR * 0.5)
        .fold(f32::INFINITY, f32::min);
    assert!(
        defense_right + COMBAT_SHIELD_DEFENSE_GAP <= building_left,
        "defense cards clear the bombing targets"
    );

    let (shield_entity, shield_home, shield_size) = app
        .world_mut()
        .query::<(Entity, &Sprite, &CombatUnitCmp, &CombatCardHome)>()
        .iter(app.world())
        .find_map(|(entity, sprite, card, home)| {
            (card.unit == Unit::planetary_shield()).then_some((
                entity,
                home.0,
                sprite.custom_size.unwrap(),
            ))
        })
        .unwrap();
    let shield_icon_offset = app
        .world_mut()
        .run_system_once(
            move |children: Query<&Children>, icons: Query<&Transform, With<PSCombatImageCmp>>| {
                children
                    .iter_descendants(shield_entity)
                    .find_map(|entity| {
                        icons.get(entity).ok().map(|transform| transform.translation)
                    })
                    .unwrap()
            },
        )
        .unwrap();
    let shield_bar_bottom = shield_home.y - shield_size.y * 0.5;
    let shield_bar_top = shield_home.y + shield_size.y * 0.5;
    let shield_icon_top = shield_home.y + shield_icon_offset.y + size * 0.5;
    let shield_bar_left = shield_home.x - shield_size.x * 0.5;
    let shield_icon_left = shield_home.x + shield_icon_offset.x - size * 0.5;
    let shield_icon_right = shield_home.x + shield_icon_offset.x + size * 0.5;
    let crawler_x =
        homes.iter().find_map(|(unit, home)| (*unit == Unit::crawler()).then_some(home.x)).unwrap();
    let repair_truck_x = homes
        .iter()
        .find_map(|(unit, home)| (*unit == Unit::repair_truck()).then_some(home.x))
        .unwrap();
    let shield_to_crawler_gap = crawler_x - size * 0.5 - shield_icon_right;
    let crawler_to_repair_gap = repair_truck_x - size * 0.5 - (crawler_x + size * 0.5);
    assert!(
        (shield_to_crawler_gap - crawler_to_repair_gap).abs() < 0.01,
        "the planetary shield-to-Crawler gap matches the Crawler-to-Repair Truck gap"
    );
    assert!(shield_icon_left > -960.0, "the planetary shield remains inside the viewport");
    assert!(
        (shield_bar_left - shield_icon_right).abs() < 0.01,
        "the planetary shield bar begins exactly at the shield image's right edge"
    );
    let defense_top =
        defenses.iter().map(|home| home.y + size * 0.5).fold(f32::NEG_INFINITY, f32::max);
    assert!(
        (shield_bar_bottom - defense_top - COMBAT_SHIELD_DEFENSE_GAP * 0.5).abs() < 0.01,
        "the planetary shield health bar sits just above the defense row"
    );
    assert!(
        (shield_icon_top - shield_bar_top).abs() < 0.01,
        "the planetary shield image's top-right corner anchors the health bar"
    );
    let shield_fill = app
        .world_mut()
        .query_filtered::<&Sprite, (With<PlanetaryShieldFillCmp>, With<ShieldCmp>)>()
        .single(app.world())
        .unwrap();
    assert_eq!(shield_fill.color, SHIELD_COLOR);
    let full_fill_width = shield_fill.custom_size.unwrap().x;

    app.world_mut().get_mut::<CombatUnitCmp>(shield_entity).unwrap().shield /= 2;
    app.world_mut().resource_mut::<Time>().advance_by(Duration::from_millis(100));
    app.world_mut().run_system_once(update_combat_stats).unwrap();
    let animated_fill_width = app
        .world_mut()
        .query_filtered::<&Sprite, (With<PlanetaryShieldFillCmp>, With<ShieldCmp>)>()
        .single(app.world())
        .unwrap()
        .custom_size
        .unwrap()
        .x;
    assert!(
        animated_fill_width > full_fill_width * 0.5 && animated_fill_width < full_fill_width,
        "the single shield fill rapidly interpolates instead of jumping to its new value"
    );
    app.world_mut().resource_mut::<Time>().advance_by(Duration::from_millis(400));
    app.world_mut().run_system_once(update_combat_stats).unwrap();
    let settled_fill_width = app
        .world_mut()
        .query_filtered::<&Sprite, (With<PlanetaryShieldFillCmp>, With<ShieldCmp>)>()
        .single(app.world())
        .unwrap()
        .custom_size
        .unwrap()
        .x;
    assert!((settled_fill_width - full_fill_width * 0.5).abs() < 0.01);

    let defender_ship = homes
        .iter()
        .find_map(|(unit, home)| (*unit == Unit::Ship(Ship::LightFighter)).then_some(*home))
        .unwrap();
    assert!(
        defender_ship.y < -108.0,
        "defending ships move slightly down from their former middle-row position"
    );
    assert!(
        defender_ship.y - size * COMBAT_CARD_LOWER_EXTENT_FACTOR
            >= shield_bar_top + COMBAT_SHIELD_DEFENSE_GAP,
        "defending ships and their stat bars clear the planetary shield health bar"
    );
}

#[test]
fn single_round_battles_go_directly_to_fire_without_a_round_banner() {
    let report = report(150, 0, true, 7);
    assert_eq!(report.combat_report.as_ref().unwrap().rounds.len(), 1);
    let mut app = playback_app(report, 0, CombatState::DisplayRound);
    app.world_mut().run_system_once(animate_combat).unwrap();
    assert!(matches!(
        *app.world().resource::<NextState<CombatState>>(),
        NextState::Pending(CombatState::Fire)
    ));
    assert!(app
        .world_mut()
        .query_filtered::<Entity, With<DisplayTextCmp>>()
        .iter(app.world())
        .next()
        .is_none());
}

#[test]
fn round_banner_after_navigation_uses_the_normal_playback_duration() {
    let report = report(12, 5, true, 2);
    assert!(report.combat_report.as_ref().unwrap().rounds.len() > 1);
    let mut app = playback_app(report, 0, CombatState::DisplayRound);
    app.insert_resource(CombatRoundJump);

    app.world_mut().run_system_once(animate_combat).unwrap();

    let tween = app
        .world_mut()
        .query_filtered::<&TweenAnim, With<DisplayTextCmp>>()
        .single(app.world())
        .unwrap();
    assert_eq!(tween.tweenable().cycle_duration(), Duration::from_millis(1200));
}

#[test]
fn surviving_probes_play_one_flyaway_cue_when_their_retreat_begins() {
    let mut report = report(12, 5, true, 2);
    assert!(report.combat_report.as_ref().unwrap().rounds.len() > 1);
    report.mission.bombing = BombingRaid::None;
    let mut app = playback_app(report, 0, CombatState::Fire);
    let probe = spawn_unit(&mut app, Unit::probe(), 3, Side::Attacker, FireState::Fired);

    app.world_mut().run_system_once(animate_combat).unwrap();

    assert!(app.world().get::<ProbeRetreatCmp>(probe).is_some());
    assert!(app.world().get::<TweenAnim>(probe).is_some());
    let sounds =
        app.world_mut().resource_mut::<Messages<PlayAudioMsg>>().drain().collect::<Vec<_>>();
    assert_eq!(sounds.len(), 1);
    assert_eq!(sounds[0].name, "probe retreat");
    assert_eq!(sounds[0].playback_rate, 1.2);

    // The marker prevents later first-round phases from retriggering the same retreat cue.
    app.world_mut().resource_mut::<UiState>().combat_round = 0;
    app.world_mut().insert_resource(NextState::<CombatState>::Unchanged);
    app.world_mut().run_system_once(animate_combat).unwrap();
    assert!(app.world().resource::<Messages<PlayAudioMsg>>().is_empty());
}

#[test]
fn probes_do_not_fly_away_during_space_fauna_playback() {
    let mut report = report(12, 5, true, 2);
    assert!(report.combat_report.as_ref().unwrap().rounds.len() > 1);
    report.planet.army.insert(Unit::Fauna(crate::core::units::fauna::SpaceFauna::VoidManta), 1);
    let mut app = playback_app(report, 0, CombatState::Fire);
    let probe = spawn_unit(&mut app, Unit::probe(), 3, Side::Attacker, FireState::Fired);

    app.world_mut().run_system_once(animate_combat).unwrap();

    assert!(app.world().get::<ProbeRetreatCmp>(probe).is_none());
    assert!(app.world().get::<TweenAnim>(probe).is_none());
    assert!(app.world().resource::<Messages<PlayAudioMsg>>().is_empty());
}

#[test]
fn extinction_ray_uses_a_long_single_charge_before_impact() {
    let mut app = playback_app(report(1, 0, false, 51), 0, CombatState::Fire);
    let behemoth = Unit::Fauna(SpaceFauna::NullstarBehemoth);
    let source = spawn_unit(&mut app, behemoth, 1, Side::Defender, FireState::Fired);
    spawn_unit(&mut app, Unit::war_sun(), 1, Side::Attacker, FireState::Fired);
    app.world_mut().resource_mut::<Messages<SpawnShotMsg>>().write(SpawnShotMsg {
        shot: ShotReport {
            unit: Some(Unit::war_sun()),
            target_id: Some(1),
            shield_damage: Unit::war_sun().shield(),
            hull_damage: SpaceFauna::NullstarBehemoth.damage() - Unit::war_sun().shield(),
            ..default()
        },
        repair: false,
        side: Side::Attacker,
        source: Some((source, behemoth, Vec3::new(-300., 0., 0.))),
    });
    app.add_systems(Update, run_combat_animations);

    app.update();
    assert_eq!(app.world_mut().query::<&PendingImpact>().iter(app.world()).count(), 1);

    app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs(1));
    app.update();
    assert!(app.world().resource::<Messages<PlayAudioMsg>>().is_empty());

    app.world_mut().resource_mut::<Time>().advance_by(Duration::from_millis(400));
    app.update();
    let sounds =
        app.world_mut().resource_mut::<Messages<PlayAudioMsg>>().drain().collect::<Vec<_>>();
    assert_eq!(sounds.len(), 1);
    assert_eq!(sounds[0].name, "fauna roar");
    assert_eq!(sounds[0].playback_rate, 0.52);
    assert_eq!(app.world_mut().query::<&PendingImpact>().iter(app.world()).count(), 1);

    app.world_mut().resource_mut::<Time>().advance_by(Duration::from_millis(1_100));
    app.update();
    assert_eq!(app.world_mut().query::<&PendingImpact>().iter(app.world()).count(), 0);
}

#[test]
fn death_ray_playback_completes_for_successful_and_failed_destroy_missions() {
    for destroys in [false, true] {
        let report = (0..128)
            .map(|seed| {
                let mut rng = DeterministicRngState::from_u64(seed).next_rng();
                let mut origin =
                    Planet::new_with_rng(0, "Origin".into(), Vec2::ZERO, false, 1., &mut rng);
                origin.colonize(1);
                let mut target =
                    Planet::new_with_rng(1, "Target".into(), Vec2::X, false, 1., &mut rng);
                target.colonize(2);
                let mission = Mission::new_with_id(
                    10,
                    1,
                    1,
                    &origin,
                    &target,
                    Icon::Destroy,
                    Army::from([(Unit::war_sun(), 1)]),
                    BombingRaid::None,
                    false,
                    false,
                    None,
                );
                resolve_combat_with_rng(1, &mission, &target, &mut rng)
            })
            .find(|report| report.planet_destroyed == destroys)
            .expect("seeds must cover both destruction outcomes");
        let combat = report.combat_report.as_ref().unwrap();
        let round = combat.rounds.len() - 1;
        assert!(combat.rounds[round].destroy_probability > 0.);
        let mut app = playback_app(report, round, CombatState::Fire);
        app.add_plugins(bevy_tweening::TweeningPlugin);
        let sun = spawn_unit(&mut app, Unit::war_sun(), 1, Side::Attacker, FireState::Fired);

        app.world_mut().run_system_once(animate_combat).unwrap();
        assert!(matches!(
            *app.world().resource::<NextState<CombatState>>(),
            NextState::Pending(CombatState::DeathRay)
        ));
        app.insert_resource(State::new(CombatState::DeathRay));
        app.insert_resource(NextState::<CombatState>::Unchanged);
        app.world_mut().get_mut::<CombatUnitCmp>(sun).unwrap().fire = FireState::Firing;
        app.world_mut().run_system_once(animate_combat).unwrap();
        let ray = app
            .world_mut()
            .query_filtered::<Entity, With<DeathRayCmp>>()
            .single(app.world())
            .unwrap();
        assert!(app.world().get::<Cinematic>(ray).is_some());

        // Exercise the actual tween target and completion message, not a synthetic event.
        TweenAnim::step_all(app.world_mut(), Duration::from_secs_f32(DEATH_RAY_DURATION - 0.2));
        app.world_mut().run_system_once(animate_combat).unwrap();
        assert!(app.world().get::<CombatUnitCmp>(sun).unwrap().fire == FireState::Firing);
        assert!(app.world().get_entity(ray).is_ok());
        TweenAnim::step_all(app.world_mut(), Duration::from_millis(250));
        app.world_mut().run_system_once(animate_combat).unwrap();
        assert!(app.world().get_entity(ray).is_err());
        assert!(app.world().get::<CombatUnitCmp>(sun).unwrap().fire == FireState::Deselect);
        let destroyed_background = app.world().resource::<WorldAssets>().image("destroyed bg");
        let background = app
            .world_mut()
            .query_filtered::<&Sprite, With<BackgroundImageCmp>>()
            .single(app.world())
            .unwrap();
        assert_eq!(background.image == destroyed_background, destroys);

        app.world_mut().get_mut::<CombatUnitCmp>(sun).unwrap().fire = FireState::Fired;
        app.world_mut().run_system_once(animate_combat).unwrap();
        assert!(matches!(
            *app.world().resource::<NextState<CombatState>>(),
            NextState::Pending(CombatState::EndCombat)
        ));
    }
}

#[test]
fn bombing_animation_runs_only_for_the_recorded_raid_round() {
    let report = report(12, 5, true, 2);
    let combat = report.combat_report.as_ref().unwrap();
    assert!(combat.rounds.len() > 2);
    let mut raid_rounds = 0;
    for (index, round) in combat.rounds.iter().enumerate() {
        let has_raid = round.attacker.iter().flat_map(|cu| &cu.shots).any(ShotReport::is_bombing);
        raid_rounds += usize::from(has_raid);
        for phase in [CombatState::Fire, CombatState::Repair] {
            let mut app = playback_app(report.clone(), index, phase);
            spawn_unit(&mut app, Unit::Ship(Ship::Bomber), 12, Side::Attacker, FireState::Fired);
            app.world_mut().run_system_once(animate_combat).unwrap();
            assert_eq!(
                matches!(
                    *app.world().resource::<NextState<CombatState>>(),
                    NextState::Pending(CombatState::Bomb)
                ),
                has_raid,
                "wrong raid transition in round {index}",
            );
        }
    }
    assert_eq!(raid_rounds, 1);
}

#[test]
fn bombing_animation_includes_misses_and_emits_each_recorded_attempt_once() {
    for count in [1, 30, 150] {
        let report = report(count, 0, false, 4);
        let expected = report.combat_report.as_ref().unwrap().rounds[0]
            .attacker
            .iter()
            .flat_map(|cu| &cu.shots)
            .filter(|shot| shot.is_bombing())
            .cloned()
            .collect::<Vec<_>>();
        assert!(!expected.is_empty());
        assert!(expected.iter().any(|shot| shot.missed));
        let mut app = playback_app(report, 0, CombatState::Fire);
        let bomber =
            spawn_unit(&mut app, Unit::Ship(Ship::Bomber), count, Side::Attacker, FireState::Fired);
        app.world_mut().run_system_once(animate_combat).unwrap();
        assert!(matches!(
            *app.world().resource::<NextState<CombatState>>(),
            NextState::Pending(CombatState::Bomb)
        ));

        app.world_mut().insert_resource(State::new(CombatState::Bomb));
        app.world_mut().get_mut::<CombatUnitCmp>(bomber).unwrap().fire = FireState::Firing;
        let mut cursor = app.world().resource::<Messages<SpawnShotMsg>>().get_cursor();
        app.world_mut().run_system_once(animate_combat).unwrap();
        let messages = app.world().resource::<Messages<SpawnShotMsg>>();
        let actual = cursor
            .read(messages)
            .map(|message| {
                assert_eq!(message.side, Side::Defender);
                assert!(!message.repair);
                message.shot.clone()
            })
            .collect::<Vec<_>>();
        assert_eq!(serde_json::to_value(&actual).unwrap(), serde_json::to_value(expected).unwrap());
        app.world_mut().run_system_once(animate_combat).unwrap();
        assert_eq!(cursor.read(app.world().resource::<Messages<SpawnShotMsg>>()).count(), 0);
    }
}

#[test]
fn weapon_animation_keeps_shield_fire_separate_from_building_raids() {
    // An unguarded shield still absorbs weapon fire even though no defender ship exists.
    for count in [1, 30] {
        let report = report(count, 5, false, 7);
        let expected = report.combat_report.as_ref().unwrap().rounds[0]
            .attacker
            .iter()
            .flat_map(|cu| &cu.shots)
            .filter(|shot| !shot.is_bombing())
            .cloned()
            .collect::<Vec<_>>();
        assert!(expected.iter().any(|shot| shot.planetary_shield_damage > 0));
        let mut app = playback_app(report, 0, CombatState::Fire);
        let bomber =
            spawn_unit(&mut app, Unit::Ship(Ship::Bomber), count, Side::Attacker, FireState::Idle);
        app.world_mut().run_system_once(animate_combat).unwrap();
        assert!(app.world().get::<CombatUnitCmp>(bomber).unwrap().fire == FireState::Select);
        app.world_mut().get_mut::<CombatUnitCmp>(bomber).unwrap().fire = FireState::Firing;
        let mut cursor = app.world().resource::<Messages<SpawnShotMsg>>().get_cursor();
        app.world_mut().run_system_once(animate_combat).unwrap();
        let actual = cursor
            .read(app.world().resource::<Messages<SpawnShotMsg>>())
            .map(|message| message.shot.clone())
            .collect::<Vec<_>>();
        assert_eq!(serde_json::to_value(actual).unwrap(), serde_json::to_value(expected).unwrap());
    }
}

#[test]
fn weapon_animation_replays_simultaneous_return_fire_from_destroyed_defenders() {
    let report = report(150, 0, true, 7);
    let defenders = &report.combat_report.as_ref().unwrap().rounds[0].defender;
    assert!(defenders.iter().all(|cu| cu.hull == 0));
    let expected = defenders.iter().flat_map(|cu| &cu.shots).cloned().collect::<Vec<_>>();
    assert!(expected.iter().any(|shot| shot.hull_damage > 0));
    let mut app = playback_app(report, 0, CombatState::Fire);
    spawn_unit(&mut app, Unit::Ship(Ship::Bomber), 150, Side::Attacker, FireState::Fired);
    let gauss = spawn_unit(
        &mut app,
        Unit::Defense(Defense::GaussCannon),
        0,
        Side::Defender,
        FireState::Idle,
    );
    app.world_mut().run_system_once(animate_combat).unwrap();
    assert!(app.world().get::<CombatUnitCmp>(gauss).unwrap().fire == FireState::Select);
    app.world_mut().get_mut::<CombatUnitCmp>(gauss).unwrap().fire = FireState::Firing;
    let mut cursor = app.world().resource::<Messages<SpawnShotMsg>>().get_cursor();
    app.world_mut().run_system_once(animate_combat).unwrap();
    let actual = cursor
        .read(app.world().resource::<Messages<SpawnShotMsg>>())
        .map(|message| message.shot.clone())
        .collect::<Vec<_>>();
    assert_eq!(serde_json::to_value(actual).unwrap(), serde_json::to_value(expected).unwrap());
}

#[test]
fn bombing_explosions_apply_exactly_the_reported_building_losses() {
    let report = report(150, 0, false, 8);
    let shots = report.combat_report.as_ref().unwrap().rounds[0]
        .attacker
        .iter()
        .flat_map(|cu| &cu.shots)
        .filter(|shot| shot.is_bombing())
        .cloned()
        .collect::<Vec<_>>();
    let mut app = playback_app(report.clone(), 0, CombatState::Bomb);
    let mut entities = std::collections::BTreeMap::new();
    for unit in Unit::resource_buildings() {
        entities.insert(unit, spawn_unit(&mut app, unit, 5, Side::Defender, FireState::Fired));
    }
    for shot in &shots {
        app.world_mut().resource_mut::<Messages<SpawnShotMsg>>().write(SpawnShotMsg {
            shot: shot.clone(),
            repair: false,
            side: Side::Defender,
            source: None,
        });
    }
    // Keep the system's message cursor across frames, so shots are spawned only once.
    app.add_systems(Update, run_combat_animations);
    app.update();
    for _ in 0..100 {
        app.world_mut().resource_mut::<Time>().advance_by(Duration::from_millis(50));
        app.update();
    }
    for (unit, entity) in entities {
        let shown = app.world().get::<CombatUnitCmp>(entity).unwrap().hull;
        assert_eq!(shown, report.surviving_defender.amount(&unit));
        assert!(shown >= 2, "animation exceeded the three-level loss cap");
    }
    assert_eq!(app.world_mut().query::<&PendingImpact>().iter(app.world()).count(), 0);
}

#[test]
fn paused_playback_does_not_start_a_new_volley() {
    let mut app = playback_app(report(30, 0, false, 4), 0, CombatState::Bomb);
    spawn_unit(&mut app, Unit::Ship(Ship::Bomber), 30, Side::Attacker, FireState::Firing);
    app.world_mut().resource_mut::<Settings>().combat_paused = true;
    app.world_mut().run_system_once(animate_combat).unwrap();
    assert!(app.world().resource::<Messages<SpawnShotMsg>>().is_empty());
    app.world_mut().resource_mut::<Settings>().combat_paused = false;
    app.world_mut().run_system_once(animate_combat).unwrap();
    assert!(!app.world().resource::<Messages<SpawnShotMsg>>().is_empty());
}

#[test]
fn completion_received_while_paused_is_applied_after_resume() {
    let mut app = playback_app(report(1, 0, true, 41), 0, CombatState::Fire);
    app.add_plugins(bevy_tweening::TweeningPlugin).add_systems(Update, animate_combat);
    let unit =
        spawn_unit(&mut app, Unit::Ship(Ship::Bomber), 1, Side::Attacker, FireState::PreFire);
    app.world_mut().entity_mut(unit).insert((
        CombatCmp,
        TweenAnim::new(Tween::new(
            EaseFunction::Linear,
            Duration::from_millis(1),
            TransformScaleLens {
                start: Vec3::ONE,
                end: Vec3::splat(1.3),
            },
        )),
    ));
    app.world_mut().resource_mut::<Settings>().combat_paused = true;

    // Reproduce the race: the tween finishes on the frame pause becomes visible. Its Bevy
    // completion message expires while the state machine remains paused.
    TweenAnim::step_all(app.world_mut(), Duration::from_millis(1));
    assert!(app.world().get::<TweenAnim>(unit).is_none());
    for _ in 0..4 {
        app.update();
    }
    assert!(matches!(app.world().get::<CombatUnitCmp>(unit).unwrap().fire, FireState::PreFire));

    app.world_mut().resource_mut::<Settings>().combat_paused = false;
    app.update();
    assert!(matches!(app.world().get::<CombatUnitCmp>(unit).unwrap().fire, FireState::Firing));
}

#[test]
fn round_waits_for_travelling_damage_before_finishing() {
    let mut app = playback_app(report(30, 0, false, 4), 0, CombatState::Bomb);
    let bomber =
        spawn_unit(&mut app, Unit::Ship(Ship::Bomber), 30, Side::Attacker, FireState::Fired);
    let building = Unit::resource_buildings()[0];
    spawn_unit(&mut app, building, 5, Side::Defender, FireState::Fired);
    app.world_mut().resource_mut::<Messages<SpawnShotMsg>>().write(SpawnShotMsg {
        shot: ShotReport {
            unit: Some(building),
            killed: true,
            ..default()
        },
        repair: false,
        side: Side::Defender,
        source: Some((bomber, Unit::Ship(Ship::Bomber), Vec3::Y * 200.)),
    });
    app.world_mut().run_system_once(run_combat_animations).unwrap();
    app.world_mut().run_system_once(animate_combat).unwrap();
    assert!(matches!(*app.world().resource::<NextState<CombatState>>(), NextState::Unchanged));
}

#[test]
fn repair_truck_replays_real_resolver_repairs_and_restores_exact_recorded_hull() {
    // Find a deterministic battle with surviving, damaged turrets. Missing shields
    // or a reduced unit count alone are not eligible for Repair Truck repairs.
    let mut selected = None;
    for seed in 0..40 {
        let mut rng = DeterministicRngState::from_u64(seed).next_rng();
        let mut origin = Planet::new_with_rng(0, "Origin".into(), Vec2::ZERO, false, 1., &mut rng);
        origin.colonize(1);
        let mut target = Planet::new_with_rng(1, "Target".into(), Vec2::X, false, 1., &mut rng);
        target.colonize(2);
        target.army = Army::from([
            (Unit::repair_truck(), 8),
            (Unit::Defense(Defense::GaussCannon), 8),
            (Unit::Defense(Defense::PlasmaTurret), 3),
        ])
        .into();
        let mission = Mission::new_with_id(
            10,
            1,
            1,
            &origin,
            &target,
            Icon::Attack,
            Army::from([(Unit::Ship(Ship::Cruiser), 14)]),
            BombingRaid::None,
            false,
            false,
            None,
        );
        let report = resolve_combat_with_rng(1, &mission, &target, &mut rng);
        if let Some(index) =
            report.combat_report.as_ref().unwrap().rounds.iter().enumerate().find_map(
                |(index, round)| {
                    (index + 1 < report.combat_report.as_ref().unwrap().rounds.len()
                        && round.defender.iter().any(|unit| !unit.repairs.is_empty()))
                    .then_some(index)
                },
            )
        {
            selected = Some((report, index));
            break;
        }
    }
    let (report, index) = selected.expect("fixture must exercise real repair events");
    let round = report.combat_report.as_ref().unwrap().rounds[index].clone();
    let mut app = playback_app(report, index, CombatState::Fire);
    let mut targets = Vec::new();
    for kind in Unit::defenses() {
        let combatants = round.defender.iter().filter(|u| u.unit == kind).collect::<Vec<_>>();
        let survivors =
            round.defender.iter().filter(|u| u.unit == kind && u.hull > 0).collect::<Vec<_>>();
        if survivors.is_empty() {
            continue;
        }
        let final_hull: usize = survivors.iter().map(|u| u.hull).sum();
        let repaired: usize = survivors.iter().flat_map(|u| &u.repairs).sum();
        let entity = spawn_unit(&mut app, kind, combatants.len(), Side::Defender, FireState::Fired);
        app.world_mut().get_mut::<CombatUnitCmp>(entity).unwrap().hull = final_hull - repaired;
        targets.push((kind, entity, survivors.len(), final_hull, repaired));
    }
    app.world_mut().run_system_once(animate_combat).unwrap();
    assert!(matches!(
        *app.world().resource::<NextState<CombatState>>(),
        NextState::Pending(CombatState::Repair)
    ));
    for (kind, entity, survivors, _, _) in &targets {
        let card = app.world().get::<CombatUnitCmp>(*entity).unwrap();
        assert!(card.outcome_visible);
        assert_eq!(card.max_hull, survivors * kind.hull());
        assert_eq!(card.max_shield, survivors * kind.shield());
        assert_eq!(card.shield, card.max_shield);
    }
    let repair_truck =
        targets.iter().find(|(kind, _, _, _, _)| *kind == Unit::repair_truck()).unwrap().1;
    app.world_mut().insert_resource(State::new(CombatState::Repair));
    app.world_mut().get_mut::<CombatUnitCmp>(repair_truck).unwrap().fire = FireState::Firing;
    app.world_mut().run_system_once(animate_combat).unwrap();
    let messages = app.world().resource::<Messages<SpawnShotMsg>>();
    let mut cursor = messages.get_cursor();
    let messages = cursor.read(messages).collect::<Vec<_>>();
    assert!(!messages.is_empty());
    assert!(messages.iter().all(|m| m.repair && m.source.unwrap().0 == repair_truck));
    app.add_systems(Update, run_combat_animations);
    app.update();
    assert!(app.world_mut().query::<&PendingImpact>().iter(app.world()).count() > 0);
    app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs(3));
    app.update();
    for (_, entity, _, final_hull, _) in targets {
        assert_eq!(app.world().get::<CombatUnitCmp>(entity).unwrap().hull, final_hull);
    }
    assert!(
        app.world_mut()
            .query::<&crate::core::combat::effects::CombatReadout>()
            .iter(app.world())
            .count()
            > 0
    );
}

#[test]
fn crawler_pulses_in_place_and_only_non_zero_salvage_pickups_float_up() {
    let mut report = report(1, 0, true, 19);
    report.planet.army =
        Army::from([(Unit::crawler(), 4), (Unit::Defense(Defense::RocketLauncher), 5)]).into();
    report.surviving_attacker.clear();
    report.surviving_defender =
        Army::from([(Unit::crawler(), 2), (Unit::Defense(Defense::RocketLauncher), 3)]).into();
    assert_eq!(report.defender_salvage(), crate::core::resources::Resources::new(2, 0, 0));

    let mut app = playback_app(report, 0, CombatState::Salvage);
    app.add_plugins(bevy_tweening::TweeningPlugin);
    let crawler = spawn_unit(&mut app, Unit::crawler(), 2, Side::Defender, FireState::Fired);
    let crawler_home = app.world().get::<Transform>(crawler).unwrap().translation;
    app.world_mut().run_system_once(animate_combat).unwrap();
    assert!(app.world().get::<SalvageCrawlerCmp>(crawler).is_some());
    let result = app
        .world_mut()
        .query_filtered::<Entity, With<DisplayTextCmp>>()
        .single(app.world())
        .unwrap();
    let backdrop = app.world().get::<ChildOf>(result).unwrap().parent();
    assert_eq!(
        app.world().get::<BackgroundColor>(backdrop).unwrap().0.alpha(),
        0.,
        "the dark center band starts transparent with its lettering"
    );
    let band_node = app.world().get::<Node>(backdrop).unwrap();
    assert_eq!(band_node.height, Val::Vh(result_banner::BAR_HEIGHT_FRACTION * 100.));
    assert_eq!(band_node.overflow, Overflow::clip());
    assert_eq!(app.world().get::<UiTransform>(result).unwrap().scale, Vec2::ONE);
    assert_eq!(app.world().get::<ImageNode>(result).unwrap().color.alpha(), 0.);
    let root = app.world().get::<ChildOf>(backdrop).unwrap().parent();
    assert_eq!(app.world().get::<Node>(root).unwrap().height, Val::Percent(100.));
    assert_eq!(app.world().get::<Transform>(crawler).unwrap().translation, crawler_home);
    assert_eq!(app.world_mut().query::<&SalvagePickupCmp>().iter(app.world()).count(), 0);
    assert_eq!(app.world_mut().query::<&SalvageTimerCmp>().iter(app.world()).count(), 0);
    assert!(matches!(*app.world().resource::<NextState<CombatState>>(), NextState::Unchanged));

    TweenAnim::step_all(app.world_mut(), Duration::from_millis(SALVAGE_HIGHLIGHT_TIME_MS));
    let opacity = result_banner::entrance_opacity(SALVAGE_HIGHLIGHT_TIME_MS as f32 / 1000.);
    assert!((app.world().get::<ImageNode>(result).unwrap().color.alpha() - opacity).abs() < 0.001);
    assert!(
        (app.world().get::<BackgroundColor>(backdrop).unwrap().0.alpha()
            - opacity * result_banner::BAR_ALPHA as f32 / 255.)
            .abs()
            < 0.001
    );
    assert_eq!(app.world().get::<UiTransform>(result).unwrap().scale, Vec2::ONE);
    app.world_mut().run_system_once(animate_combat).unwrap();
    let pickups = app
        .world_mut()
        .query::<(Entity, &SalvagePickupCmp)>()
        .iter(app.world())
        .map(|(entity, pickup)| (entity, pickup.resource, pickup.amount))
        .collect::<Vec<_>>();
    assert_eq!(pickups.len(), 1);
    assert_eq!((pickups[0].1, pickups[0].2), (ResourceName::Metal, 2));
    let metal_image = app.world().resource::<WorldAssets>().image("metal");
    assert!(app
        .world()
        .get::<Children>(pickups[0].0)
        .unwrap()
        .iter()
        .filter_map(|child| app.world().get::<Sprite>(child))
        .any(|sprite| sprite.image == metal_image));
    assert_eq!(app.world_mut().query::<&SalvageTimerCmp>().iter(app.world()).count(), 1);
    assert_eq!(app.world().get::<Transform>(crawler).unwrap().translation, crawler_home);
    assert!(app.world_mut().query::<&Text2d>().iter(app.world()).any(|text| text.0 == "+2"));
    let amount_text = app
        .world()
        .get::<Children>(pickups[0].0)
        .unwrap()
        .iter()
        .find(|child| app.world().get::<Text2d>(*child).is_some())
        .unwrap();
    assert_eq!(
        app.world().get::<TweenAnim>(pickups[0].0).unwrap().tweenable().cycle_duration(),
        Duration::from_millis(SALVAGE_PICKUP_TIME_MS)
    );
    assert_eq!(
        app.world().get::<TweenAnim>(amount_text).unwrap().tweenable().cycle_duration(),
        Duration::from_millis(SALVAGE_PICKUP_TIME_MS)
    );
    assert!(matches!(*app.world().resource::<NextState<CombatState>>(), NextState::Unchanged));

    TweenAnim::step_all(app.world_mut(), Duration::from_millis(SALVAGE_PICKUP_REVEAL_TIME_MS));
    assert_eq!(app.world().get::<Transform>(amount_text).unwrap().scale, Vec3::ONE);
    TweenAnim::step_all(app.world_mut(), Duration::from_millis(SALVAGE_PICKUP_DRIFT_TIME_MS));
    assert_eq!(app.world().get::<Transform>(amount_text).unwrap().scale, Vec3::ONE);
    TweenAnim::step_all(app.world_mut(), Duration::from_millis(SALVAGE_PICKUP_REVEAL_TIME_MS));
    app.world_mut().run_system_once(animate_combat).unwrap();
    assert_eq!(app.world().get::<Transform>(crawler).unwrap().translation, crawler_home);
    assert!(matches!(
        *app.world().resource::<NextState<CombatState>>(),
        NextState::Pending(CombatState::EndCombat)
    ));
}
