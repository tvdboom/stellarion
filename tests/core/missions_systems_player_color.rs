use super::*;
use crate::core::camera::MainCamera;
use crate::core::identity::{GameCode, GameId};
use crate::core::map::icon::Icon;
use crate::core::map::systems::{draw_map, PlanetCmp};
use crate::core::missions::Mission;
use crate::core::player::PLAYER_COLOR_PALETTE;
use crate::core::simulation::{GameModel, GameRules, PersistedGame};
use crate::core::units::Unit;
use crate::multiplayer::model::GameRecord;
use bevy_kira_audio::AudioSource;

#[test]
fn mission_colors_follow_owners_on_spawn_hover_and_viewer_change() {
    let mut model = GameModel::new(
        [9; 32],
        GameRules {
            player_count: 4,
            ..default()
        },
    )
    .unwrap();
    for (index, player) in model.players.iter_mut().enumerate() {
        // Chosen colors deliberately differ from the player-slot defaults.
        player.color = Some(PLAYER_COLOR_PALETTE[(index + 2) % PLAYER_COLOR_PALETTE.len()]);
    }
    model.start().unwrap();
    let missions = Missions(
        model
            .players
            .iter()
            .enumerate()
            .map(|(index, player)| Mission {
                id: player.id,
                owner: player.id,
                origin: model.map.planets[0].id,
                destination: model.map.planets[1].id,
                position: model.map.planets[0].position,
                objective: match index {
                    1 => Icon::Attack,
                    2 => Icon::Spy,
                    3 => Icon::MissileStrike,
                    _ => Icon::Attack,
                },
                army: if index == 1 {
                    Army::from([(Unit::war_sun(), 1)])
                } else {
                    Army::new()
                },
                jump_gate: true,
                ..default()
            })
            .collect(),
    );
    let mut session = MultiplayerSession::default();
    session.active_game = Some(GameRecord {
        submitted_players: Vec::new(),
        id: GameId::new("mission-color-test"),
        code: GameCode::new("ABCDEF"),
        revision: 0,
        saved_at: 1_700_000_000,
        max_players: 4,
        status: model.status,
        persisted: PersistedGame::new(model.clone()),
        members: vec![],
    });
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default(), TransformPlugin))
        .init_asset::<Image>()
        .init_asset::<Font>()
        .init_asset::<TextureAtlasLayout>()
        .init_asset::<AudioSource>()
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<ColorMaterial>>()
        .init_resource::<WorldAssets>()
        .init_resource::<UiState>()
        .insert_resource(model.map.clone())
        .insert_resource(model.players[0].clone())
        .insert_resource(session)
        .insert_resource(missions)
        .add_systems(Startup, draw_map)
        .add_systems(Update, (update_missions, update_mission_route_arrow));
    app.world_mut().spawn((Camera2d, MainCamera));
    app.world_mut().resource_scope(|world, mut assets: Mut<WorldAssets>| {
        let server = world.resource::<AssetServer>().clone();
        assets.begin_gameplay_loading(
            &server,
            &mut world.resource_mut::<Assets<TextureAtlasLayout>>(),
        );
    });

    // Covers the first rendered frame, hover, and switching the inspected player's view.
    for (viewer, hover) in [(0, None), (0, Some(2)), (1, Some(2)), (0, Some(4)), (1, None)] {
        app.insert_resource(model.players[viewer].clone());
        app.world_mut().resource_mut::<UiState>().mission_hover = hover;
        app.update();
        let world = app.world_mut();
        let planet_z = world
            .query_filtered::<&GlobalTransform, With<PlanetCmp>>()
            .iter(world)
            .map(|transform| transform.translation().z)
            .reduce(f32::max)
            .unwrap();
        let icon_z = world
            .query_filtered::<&GlobalTransform, With<Icon>>()
            .iter(world)
            .map(|transform| transform.translation().z)
            .reduce(f32::min)
            .unwrap();
        // Compare the actual world transforms, including parent offsets on planet icons.
        for transform in world
            .query_filtered::<&GlobalTransform, Or<(With<MissionCmp>, With<MissionRouteArrowCmp>)>>(
            )
            .iter(world)
        {
            assert!(transform.translation().z > planet_z);
            assert!(transform.translation().z < icon_z);
        }
        let mut query = world.query::<(&MissionCmp, &Sprite, &Transform, &Children)>();
        assert_eq!(query.iter(world).count(), 4);
        for (mission, sprite, transform, children) in query.iter(world) {
            for child in children.iter() {
                let exhaust_z = world.get::<GlobalTransform>(child).unwrap().translation().z;
                assert!(exhaust_z > planet_z && exhaust_z < icon_z);
            }
            let owner = model.player(mission.id).unwrap();
            assert_eq!(sprite.color, owner.color().color());
            let hovered = hover == Some(mission.id);
            let expected_size = if mission.id == 2 {
                if hovered {
                    WAR_SUN_MISSION_HOVER_SIZE
                } else {
                    WAR_SUN_MISSION_SIZE
                }
            } else if mission.id == 3 {
                if hovered {
                    SPY_MISSION_HOVER_SIZE
                } else {
                    SPY_MISSION_SIZE
                }
            } else if hovered {
                MISSION_HOVER_SIZE
            } else {
                MISSION_SIZE
            };
            assert_eq!(sprite.custom_size, Some(Vec2::splat(expected_size)));
            assert_eq!(
                transform.translation.z,
                MISSION_Z
                    + if hovered {
                        0.1
                    } else {
                        0.
                    }
            );
            let key = if mission.id == 4 {
                "mission missile"
            } else if mission.id == 2 {
                "mission destroy"
            } else if mission.id == 3 {
                "mission spy"
            } else if owner.id == model.players[viewer].id {
                "mission jump"
            } else {
                "mission"
            };
            assert_eq!(sprite.image, world.resource::<WorldAssets>().image(key));
        }
        let route_styles = world
            .query::<&MissionRouteArrowCmp>()
            .iter(world)
            .map(|arrow| arrow.style)
            .collect::<Vec<_>>();
        let expected_style = match hover {
            Some(4) => Some(MissionRouteStyle::Standard),
            Some(2) if model.players[viewer].id == 2 => Some(MissionRouteStyle::JumpGate),
            Some(2) => Some(MissionRouteStyle::Standard),
            _ => None,
        };
        assert_eq!(route_styles.first().copied(), expected_style);
        assert!(route_styles.iter().all(|&style| Some(style) == expected_style));
        if expected_style == Some(MissionRouteStyle::JumpGate) {
            let glyphs = world
                .query_filtered::<&Text2d, With<MissionRouteArrowCmp>>()
                .iter(world)
                .map(|text| text.0.as_str())
                .collect::<Vec<_>>();
            assert!(!glyphs.is_empty());
            assert!(glyphs.iter().all(|glyph| *glyph == JUMP_GATE_ROUTE_GLYPH));
        }
    }
}

#[test]
fn jump_gate_wave_fronts_expand_toward_the_route_destination() {
    let markers = mission_route_markers(
        Vec2::ZERO,
        Vec2::new(0.0, 500.0),
        0.0,
        0.0,
        Color::WHITE,
        0.0,
        MissionRouteStyle::JumpGate,
    );

    assert!(!markers.is_empty());
    assert!(markers.windows(2).all(|pair| pair[1].0.scale.y > pair[0].0.scale.y));
    assert!(markers.iter().all(|(transform, _)| transform.scale.is_finite()));
    assert!(markers.iter().all(|(transform, _)| transform.scale.x < transform.scale.y));
    assert!(markers
        .iter()
        .all(|(transform, _)| { transform.rotation.mul_vec3(Vec3::X).dot(Vec3::Y) > 0.999 }));
}

#[test]
fn spy_map_rotation_keeps_the_flame_behind_the_route() {
    let spy = Mission {
        objective: Icon::Spy,
        ..default()
    };
    let map_rotation = mission_map_rotation(&spy);
    assert_eq!(map_rotation, SPY_MISSION_MAP_ROTATION);

    let parent_rotation = Quat::from_rotation_z(map_rotation);
    let flame = mission_flame_transform(SPY_MISSION_SIZE, map_rotation);
    let world_flame_offset = parent_rotation * flame.translation;
    assert!((world_flame_offset.x + SPY_MISSION_SIZE * 0.5).abs() < 0.0001);
    assert!(world_flame_offset.y.abs() < 0.0001);

    let world_flame_rotation = parent_rotation * flame.rotation;
    assert!(world_flame_rotation.dot(Quat::from_rotation_z(PI)).abs() > 1.0 - 0.0001);
}

#[test]
fn colony_ship_map_artwork_stays_upright_and_uses_the_smaller_size() {
    let colony = Mission {
        objective: Icon::Colonize,
        army: Army::from([(Unit::colony_ship(), 1), (Unit::war_sun(), 1)]),
        ..default()
    };

    assert_eq!(mission_size(&colony, false), COLONY_SHIP_MISSION_SIZE);
    assert_eq!(mission_size(&colony, true), COLONY_SHIP_MISSION_HOVER_SIZE);
    const {
        assert!(COLONY_SHIP_MISSION_SIZE < MISSION_SIZE);
        assert!(COLONY_SHIP_MISSION_HOVER_SIZE < MISSION_HOVER_SIZE);
    }

    assert!(mission_map_flip_y("mission colonize", Vec2::NEG_X));
    assert!(!mission_map_flip_y("mission colonize", Vec2::X));
    assert!(!mission_map_flip_y("mission", Vec2::NEG_X));

    // Sprite flipping happens before the route rotation. The local downward axis therefore
    // becomes screen-up after a 180-degree left turn instead of inverting the habitat dome.
    let world_up = Quat::from_rotation_z(PI) * Vec3::NEG_Y;
    assert!(world_up.dot(Vec3::Y) > 0.999);
}
