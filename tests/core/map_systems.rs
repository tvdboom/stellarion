use super::*;
use crate::core::combat::report::MissionReport;
use crate::core::identity::{GameCode, GameId};
use crate::core::player::{PlayerColor, PLAYER_COLOR_PALETTE};
use crate::core::simulation::{GameModel, GameRules, PersistedGame};
use crate::multiplayer::model::GameRecord;
use bevy::color::ColorToComponents;

/// Finds the defenses spawned by the real map setup without a window or GPU.
fn test_defenses(world: &mut World, planet: PlanetId) -> (Entity, Entity, Handle<ColorMaterial>) {
    let planet = world
        .query::<(Entity, &PlanetCmp)>()
        .iter(world)
        .find(|(_, cmp)| cmp.id == planet)
        .unwrap()
        .0;
    let children = world.get::<Children>(planet).unwrap();
    let shield =
        children.iter().find(|&child| world.get::<PlanetaryShieldCmp>(child).is_some()).unwrap();
    let dock = children.iter().find(|&child| world.get::<SpaceDockCmp>(child).is_some()).unwrap();
    let material = world.get::<MeshMaterial2d<ColorMaterial>>(shield).unwrap().0.clone();
    (shield, dock, material)
}

#[test]
fn defense_colors_follow_known_controllers_and_keep_their_hue_while_pulsing() {
    let mut model = GameModel::new(
        [17; 32],
        GameRules {
            player_count: 3,
            ..default()
        },
    )
    .unwrap();
    model.start().unwrap();
    let own_home = model.players[0].home_planet;
    let enemy_home = model.players[1].home_planet;
    let unknown_home = model.players[2].home_planet;
    let army = Army::from([(Unit::planetary_shield(), 1), (Unit::space_dock(), 1)]);
    for home in [own_home, enemy_home, unknown_home] {
        model.map.get_mut(home).army = army.clone();
    }
    let mut player = model.players[0].clone();
    player.reports.push(MissionReport {
        id: 1,
        turn: 1,
        mission: Mission {
            id: 1,
            owner: player.id,
            origin: own_home,
            destination: enemy_home,
            objective: Icon::Spy,
            ..default()
        },
        planet: model.map.get(enemy_home).clone(),
        scout_probes: 1_000_000,
        surviving_attacker: Army::new(),
        surviving_defender: army,
        planet_colonized: false,
        planet_destroyed: false,
        destination_owned: Some(2),
        destination_controlled: Some(2),
        combat_report: None,
        hidden: false,
    });
    let mut session = MultiplayerSession::default();
    session.active_game = Some(GameRecord {
        submitted_players: Vec::new(),
        id: GameId::new("defense-color-test"),
        code: GameCode::new("ABCDEF"),
        revision: 0,
        max_players: 3,
        status: model.status,
        persisted: PersistedGame::new(model.clone()),
        members: vec![],
    });
    let mut app = App::new();
    app.add_plugins((TaskPoolPlugin::default(), AssetPlugin::default()))
        .init_asset::<Image>()
        .init_asset::<Font>()
        .init_asset::<TextureAtlasLayout>()
        .init_asset::<bevy_kira_audio::AudioSource>()
        .init_resource::<Time>()
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<ColorMaterial>>()
        .init_resource::<WorldAssets>()
        .init_resource::<Missions>()
        .insert_resource(model.map)
        .insert_resource(player)
        .insert_resource(session)
        .add_plugins(bevy_tweening::TweeningPlugin)
        .add_systems(Startup, draw_map)
        .add_systems(
            Update,
            update_planet_defenses.before(bevy_tweening::AnimationSystem::AnimationUpdate),
        );
    app.world_mut().spawn((Camera2d, MainCamera));
    app.update();
    let own = test_defenses(app.world_mut(), own_home);
    let enemy = test_defenses(app.world_mut(), enemy_home);
    let unknown = test_defenses(app.world_mut(), unknown_home);

    // An unseen capture must not leak the new controller through the defense color.
    app.world_mut().resource_mut::<Map>().get_mut(enemy_home).controlled = Some(3);
    for (index, color) in PLAYER_COLOR_PALETTE.into_iter().enumerate() {
        let enemy_color = PLAYER_COLOR_PALETTE[(index + 2) % PLAYER_COLOR_PALETTE.len()];
        {
            let mut session = app.world_mut().resource_mut::<MultiplayerSession>();
            let model = &mut session.active_game.as_mut().unwrap().persisted.state;
            model.player_mut(1).unwrap().color = Some(color);
            model.player_mut(2).unwrap().color = Some(enemy_color);
        }
        app.update();
        for ((shield, dock, material), expected) in [(&own, color), (&enemy, enemy_color)] {
            assert_eq!(*app.world().get::<Visibility>(*shield).unwrap(), Visibility::Inherited);
            assert_eq!(*app.world().get::<Visibility>(*dock).unwrap(), Visibility::Inherited);
            assert_eq!(app.world().get::<Sprite>(*dock).unwrap().color, expected.color());
            let mut alphas = Vec::new();
            // Step across loop boundaries at a frame interval that does not divide the period.
            // Check the rendered material too: an opaque material ignores the animated alpha.
            for _ in 0..354 {
                app.world_mut().resource_mut::<Time>().advance_by(Duration::from_millis(17));
                app.update();
                let actual = app.world().resource::<Assets<ColorMaterial>>().get(material).unwrap();
                assert_eq!(actual.alpha_mode, bevy::sprite_render::AlphaMode2d::Blend);
                assert!(actual
                    .color
                    .with_alpha(1.)
                    .to_srgba()
                    .to_vec4()
                    .abs_diff_eq(expected.color().to_srgba().to_vec4(), 1e-6));
                alphas.push(actual.color.alpha());
            }
            for cycle in alphas.chunks_exact(177) {
                assert!(cycle.iter().any(|&alpha| alpha < 0.01), "shield fades fully out");
                assert!(cycle.iter().any(|&alpha| alpha > 0.9), "shield becomes visible again");
            }
            for pair in alphas.windows(2) {
                let change = (pair[1] - pair[0]).abs();
                assert!(change < 0.02, "shield must not pop between frames: {pair:?}");
                if pair[0] < 0.001 || pair[0] > 0.949 {
                    assert!(change < 0.002, "shield must gently reverse its fade: {pair:?}");
                }
            }
        }
        for entity in [unknown.0, unknown.1] {
            assert_eq!(*app.world().get::<Visibility>(entity).unwrap(), Visibility::Hidden);
        }
    }

    // Once the local player captures it, both defenses adopt the new controller immediately.
    let alpha_before_capture =
        app.world().resource::<Assets<ColorMaterial>>().get(&enemy.2).unwrap().color.alpha();
    let elapsed_before_capture =
        app.world().get::<TweenAnim>(enemy.0).unwrap().tweenable().elapsed();
    app.world_mut().resource_mut::<Time>().advance_by(Duration::ZERO);
    app.world_mut().resource_mut::<Map>().get_mut(enemy_home).controlled = Some(1);
    app.update();
    let own_color = app.world().get::<Sprite>(own.1).unwrap().color;
    assert_eq!(app.world().get::<Sprite>(enemy.1).unwrap().color, own_color);
    assert_eq!(app.world().get::<PlanetaryShieldCmp>(enemy.0).unwrap().color, Some(own_color));
    assert_eq!(
        app.world().get::<TweenAnim>(enemy.0).unwrap().tweenable().elapsed(),
        elapsed_before_capture,
        "changing controller must not restart the fade"
    );
    let alpha_after_capture =
        app.world().resource::<Assets<ColorMaterial>>().get(&enemy.2).unwrap().color.alpha();
    assert!((alpha_after_capture - alpha_before_capture).abs() < 1e-6);

    app.world_mut().resource_mut::<Map>().get_mut(enemy_home).is_destroyed = true;
    app.update();
    for entity in [enemy.0, enemy.1] {
        assert_eq!(*app.world().get::<Visibility>(entity).unwrap(), Visibility::Hidden);
    }
}

#[test]
fn planet_selection_updates_navigation_and_preserves_the_origin_for_other_owners() {
    let map = Map::new(2, 0);
    let mut planet = map.planets[0].clone();
    let player = Player::new(1, planet.id);
    let mut state = UiState {
        mission: true,
        combat_report: Some(3),
        ..default()
    };
    let previous_origin = map.planets[1].id;
    state.mission_info.origin = previous_origin;

    for owner in [None, Some(2)] {
        planet.owned = owner;
        select_planet(&planet, &mut state, &player);
        assert_eq!(state.planet_selected, Some(planet.id));
        assert!(state.to_selected);
        assert!(!state.mission);
        assert_eq!(state.combat_report, None);
        assert_eq!(state.mission_info.origin, previous_origin);
    }
    planet.owned = Some(player.id);
    state.planet_selected = None;
    select_planet(&planet, &mut state, &player);
    assert_eq!(state.planet_selected, Some(planet.id));
    assert_eq!(state.mission_info.origin, planet.id);
}

#[test]
fn scanner_ranges_follow_owned_planet_and_controlled_moon_hover() {
    let mut model = GameModel::new([31; 32], GameRules::default()).unwrap();
    model.start().unwrap();
    let player = model.players[0].clone();
    let home = player.home_planet;
    let enemy = model.players[1].home_planet;
    let moons = model.map.moons().iter().map(|moon| moon.id).collect::<Vec<_>>();
    let moon = moons[0];
    let enemy_moon = moons[1];
    model.map.get_mut(moon).controlled = Some(player.id);
    model.map.get_mut(enemy_moon).controlled = Some(model.players[1].id);
    for id in [home, enemy] {
        model.map.get_mut(id).army.insert(Unit::Building(Building::SensorPhalanx), 2);
    }
    for id in [moon, enemy_moon] {
        model.map.get_mut(id).army.insert(Unit::Building(Building::OrbitalRadar), 3);
    }

    let mut app = App::new();
    app.add_plugins((TaskPoolPlugin::default(), AssetPlugin::default()))
        .init_asset::<Image>()
        .init_asset::<Font>()
        .init_asset::<TextureAtlasLayout>()
        .init_asset::<bevy_kira_audio::AudioSource>()
        .init_resource::<WorldAssets>()
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<ColorMaterial>>()
        .init_resource::<Time>()
        .init_resource::<UiState>()
        .init_resource::<Settings>()
        .init_resource::<Missions>()
        .insert_resource(model.map)
        .insert_resource(player)
        .add_systems(Startup, draw_map)
        .add_systems(Update, update_planet_info);
    app.world_mut().spawn((Camera2d, MainCamera));

    for (hover, selected, expected) in [
        (Some(home), None, Some(home)),
        (None, Some(home), None), // Moving to the selected planet's shop hides the range.
        (Some(moon), Some(home), Some(moon)),
        (None, Some(moon), None),
        (Some(enemy), None, None),
        (Some(enemy_moon), None, None),
    ] {
        app.insert_resource(UiState {
            planet_hover: hover,
            planet_selected: selected,
            ..default()
        });
        app.world_mut().resource_mut::<Time>().advance_by(Duration::from_millis(100));
        app.update();
        let world = app.world_mut();
        let mut scanners =
            world.query_filtered::<(&ChildOf, &Visibility, &Mesh2d), With<ScannerCmp>>();
        let mut visible = 0;
        let mut outer_radius = 0.0_f32;
        for (parent, visibility, mesh) in scanners.iter(world) {
            let id = world.get::<PlanetCmp>(parent.parent()).unwrap().id;
            assert_eq!(*visibility == Visibility::Inherited, expected == Some(id));
            if expected != Some(id) {
                continue;
            }
            visible += 1;
            let positions = world
                .resource::<Assets<Mesh>>()
                .get(&mesh.0)
                .unwrap()
                .attribute(Mesh::ATTRIBUTE_POSITION)
                .unwrap()
                .as_float3()
                .unwrap();
            let radius =
                positions.iter().map(|p| Vec2::new(p[0], p[1]).length()).reduce(f32::max).unwrap();
            let expected_radius = if id == home {
                210.0
            } else {
                335.0
            };
            assert!(radius <= expected_radius + 0.01, "decorative arcs stay inside the range");
            outer_radius = outer_radius.max(radius);
        }
        assert_eq!(
            visible,
            if expected.is_some() {
                3
            } else {
                0
            }
        );
        if let Some(id) = expected {
            let expected_radius = if id == home {
                210.0
            } else {
                335.0
            };
            assert!(
                (outer_radius - expected_radius).abs() < 0.01,
                "scanner uses its installed level"
            );
        }
    }

    // An absent scanner and a destroyed world have no range to display.
    app.world_mut()
        .resource_mut::<Map>()
        .get_mut(home)
        .army
        .remove(&Unit::Building(Building::SensorPhalanx));
    app.world_mut().resource_mut::<Map>().get_mut(moon).is_destroyed = true;
    for id in [home, moon] {
        app.world_mut().resource_mut::<UiState>().planet_hover = Some(id);
        app.update();
        let world = app.world_mut();
        assert!(world
            .query_filtered::<&Visibility, With<ScannerCmp>>()
            .iter(world)
            .all(|visibility| *visibility == Visibility::Hidden));
    }
}

#[test]
fn modal_game_menus_hide_planet_details_even_with_selection_and_show_info() {
    for menu in [GameState::GameMenu, GameState::Settings] {
        let mut app = App::new();
        app.add_plugins(bevy::state::app::StatesPlugin)
            .init_state::<GameState>()
            .insert_resource(UiState {
                planet_hover: Some(1),
                planet_selected: Some(1),
                ..default()
            })
            .insert_resource(Settings {
                show_info: true,
                ..default()
            })
            .add_systems(OnEnter(menu), hide_planet_details);

        let planet = app.world_mut().spawn((PlanetCmp::new(1), Visibility::Inherited)).id();
        let name = app.world_mut().spawn((PlanetNameCmp, Visibility::Inherited)).id();
        let resources = app.world_mut().spawn((PlanetResourcesCmp, Visibility::Inherited)).id();
        let icon = app.world_mut().spawn((Icon::Fleet, Visibility::Inherited)).id();
        let scanner = app.world_mut().spawn((ScannerCmp::default(), Visibility::Inherited)).id();
        let defense = app.world_mut().spawn((SpaceDockCmp, Visibility::Inherited)).id();
        let details = [name, resources, icon, scanner];
        app.world_mut().entity_mut(planet).add_children(&details).add_child(defense);

        app.update();
        for entity in details {
            assert_eq!(*app.world().get::<Visibility>(entity).unwrap(), Visibility::Inherited);
        }

        app.world_mut().resource_mut::<NextState<GameState>>().set(menu);
        app.update();

        assert_eq!(*app.world().resource::<State<GameState>>().get(), menu);
        for entity in details {
            assert_eq!(*app.world().get::<Visibility>(entity).unwrap(), Visibility::Hidden);
        }
        for entity in [planet, defense] {
            assert_eq!(*app.world().get::<Visibility>(entity).unwrap(), Visibility::Inherited);
        }
        assert_eq!(app.world().resource::<UiState>().planet_selected, Some(1));
        assert!(app.world().resource::<Settings>().show_info);
    }
}

#[test]
fn modal_game_menus_hide_turn_controls() {
    let mut app = App::new();
    app.insert_resource(State::new(GameState::Playing))
        .insert_resource(Player::new(1, 0))
        .insert_resource(UiState {
            end_turn: true,
            ..default()
        })
        .insert_resource(crate::multiplayer::client::PendingTurnCommands {
            submission: crate::multiplayer::client::SubmissionState::Accepted,
            ..default()
        })
        .add_systems(Update, update_end_turn);
    let button = app
        .world_mut()
        .spawn((Visibility::Hidden, Text::new("End turn"), EndTurnButtonCmp, MainButtonLabelCmp))
        .id();
    let waiting = app.world_mut().spawn((Visibility::Hidden, EndTurnLabelCmp)).id();
    let spectator = app.world_mut().spawn((Visibility::Hidden, SpectatorLabelCmp)).id();

    app.update();
    assert_eq!(*app.world().get::<Visibility>(button).unwrap(), Visibility::Inherited);
    assert_eq!(*app.world().get::<Visibility>(waiting).unwrap(), Visibility::Inherited);
    assert_eq!(app.world().get::<Text>(button).unwrap().0, "Continue turn");
    assert_eq!(*app.world().get::<Visibility>(spectator).unwrap(), Visibility::Hidden);

    for state in [GameState::GameMenu, GameState::Settings] {
        app.insert_resource(State::new(state));
        app.update();
        for entity in [button, waiting, spectator] {
            assert_eq!(
                *app.world().get::<Visibility>(entity).unwrap(),
                Visibility::Hidden,
                "{state:?} leaves a turn control visible"
            );
        }
    }
}

#[test]
fn ownership_cells_render_above_background_in_local_and_multiplayer_games() {
    for player_count in [1, 2] {
        let mut model = GameModel::new(
            [7; 32],
            GameRules {
                player_count,
                practice_mode: player_count == 1,
                ..default()
            },
        )
        .unwrap();
        if player_count == 2 {
            model.players[0].color = PlayerColor::new(4);
        }
        model.start().unwrap();
        let player = model.players[0].clone();
        let expected_color = player.color().color();
        if player_count == 1 {
            assert_eq!(expected_color, OWN_COLOR, "local games default to blue");
        }
        let home = player.home_planet;
        let planet_count = model.map.planets.len();
        let mut session = MultiplayerSession::default();
        session.local_practice = player_count == 1;
        session.active_game = Some(GameRecord {
            submitted_players: Vec::new(),
            id: GameId::new("voronoi-test"),
            code: GameCode::new("ABCDEF"),
            revision: 0,
            max_players: player_count,
            status: model.status,
            persisted: PersistedGame::new(model.clone()),
            members: vec![],
        });
        let mut app = App::new();
        app.add_plugins(TransformPlugin)
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<ColorMaterial>>()
            .init_resource::<Settings>()
            .insert_resource(model.map)
            .insert_resource(Missions(model.missions))
            .insert_resource(player)
            .insert_resource(session)
            .add_systems(
                Startup,
                |mut commands: Commands,
                 map: Res<Map>,
                 mut meshes: ResMut<Assets<Mesh>>,
                 mut materials: ResMut<Assets<ColorMaterial>>| {
                    spawn_voronoi_cells(&mut commands, &map, &mut meshes, &mut materials);
                },
            )
            .add_systems(Update, update_voronoi);
        app.update();

        let world = app.world_mut();
        let mut cells = world.query::<(
            Entity,
            &VoronoiCmp,
            &Visibility,
            &GlobalTransform,
            &Mesh2d,
            &MeshMaterial2d<ColorMaterial>,
        )>();
        assert_eq!(cells.iter(world).count(), planet_count);
        let mut home_entity = None;
        for (entity, cell, visibility, transform, mesh, material) in cells.iter(world) {
            assert!(transform.translation().z > BACKGROUND_Z);
            assert!(transform.translation().z < PLANET_Z);
            let positions = world
                .resource::<Assets<Mesh>>()
                .get(&mesh.0)
                .unwrap()
                .attribute(Mesh::ATTRIBUTE_POSITION)
                .unwrap()
                .as_float3()
                .unwrap();
            assert!(positions.iter().all(|position| position[2] == 0.0));
            if cell.0 == home {
                home_entity = Some(entity);
                assert_eq!(*visibility, Visibility::Inherited);
                assert_eq!(
                    world.resource::<Assets<ColorMaterial>>().get(&material.0).unwrap().color,
                    expected_color.with_alpha(0.01)
                );
            } else {
                assert_eq!(*visibility, Visibility::Hidden, "unknown territory stays hidden");
            }
        }
        let home_entity = home_entity.expect("home world has an ownership cell");
        let mut edges = world.query::<(
            &VoronoiEdgeCmp,
            &Visibility,
            &GlobalTransform,
            &MeshMaterial2d<ColorMaterial>,
        )>();
        let mut home_edges = 0;
        for (edge, visibility, transform, material) in edges.iter(world) {
            assert!(transform.translation().z > VORONOI_Z);
            assert!(transform.translation().z < PLANET_Z);
            if edge.planet == home {
                home_edges += 1;
                assert_eq!(*visibility, Visibility::Inherited);
                assert_eq!(
                    world.resource::<Assets<ColorMaterial>>().get(&material.0).unwrap().color,
                    expected_color.with_alpha(0.58)
                );
            }
        }
        assert!(home_edges >= 3);

        app.world_mut().resource_mut::<Settings>().show_cells = false;
        app.update();
        let world = app.world_mut();
        assert!(world
            .query_filtered::<&Visibility, With<MapCmp>>()
            .iter(world)
            .all(|visibility| *visibility == Visibility::Hidden));
        app.world_mut().resource_mut::<Settings>().show_cells = true;
        app.update();
        assert_eq!(*app.world().get::<Visibility>(home_entity).unwrap(), Visibility::Inherited);
    }
}
