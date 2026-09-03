use super::*;
use crate::core::camera::MainCamera;
use crate::core::identity::{GameCode, GameId};
use crate::core::map::icon::Icon;
use crate::core::map::systems::{draw_map, PlanetCmp};
use crate::core::missions::Mission;
use crate::core::player::PLAYER_COLOR_PALETTE;
use crate::core::simulation::{GameModel, GameRules, PersistedGame};
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
                objective: if index == 3 {
                    Icon::MissileStrike
                } else {
                    Icon::Attack
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
            assert_eq!(
                sprite.custom_size,
                Some(Vec2::splat(if hovered {
                    60.
                } else {
                    50.
                }))
            );
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
            Some(4) => Some(MissionRouteStyle::MissileStrike),
            Some(2) if model.players[viewer].id == 2 => Some(MissionRouteStyle::JumpGate),
            Some(2) => Some(MissionRouteStyle::Standard),
            _ => None,
        };
        assert_eq!(route_styles.first().copied(), expected_style);
        assert!(route_styles.iter().all(|&style| Some(style) == expected_style));
    }
}
