use super::*;
use crate::core::camera::MainCamera;
use crate::core::map::systems::OrbitalRailgunCmp;
use crate::core::simulation::{GameModel, GameRules};
use crate::core::turns::{finish_end_game_presentation, EndGamePresentation};
use bevy::asset::AssetPlugin;
use bevy::ecs::system::RunSystemOnce;
use bevy_kira_audio::AudioSource;

#[test]
fn animation_systems_wait_until_the_map_is_loaded() {
    let mut app = App::new();
    app.add_plugins(OrbitalRailgunAnimationPlugin);

    app.update();
}

#[test]
fn a_single_railgun_fires_directly_without_moving_the_camera() {
    let mut model = GameModel::new([91; 32], GameRules::default()).unwrap();
    model.start().unwrap();
    let origin = model.players[0].home_planet;
    let target = model.players[1].home_planet;
    let marker_anchor = Vec2::new(0.0, 100.0);
    let target_position = model.map.get(target).position;
    let direction = target_position - model.map.get(origin).position;
    let firing_origin =
        model.map.get(origin).position + direction.normalize_or_zero() * marker_anchor.length();

    let mut app = App::new();
    app.add_plugins((TaskPoolPlugin::default(), AssetPlugin::default()))
        .init_asset::<Image>()
        .init_asset::<Font>()
        .init_asset::<TextureAtlasLayout>()
        .init_asset::<AudioSource>()
        .init_resource::<WorldAssets>()
        .insert_resource(model.map)
        .insert_resource(Settings {
            turn: model.turn as usize,
            ..default()
        })
        .insert_resource(OrbitalStrikes(vec![OrbitalStrike {
            turn: model.turn,
            origins: vec![origin],
            target,
            chance_basis_points: 350,
            destroyed: false,
        }]))
        .add_message::<StartTurnMsg>()
        .add_systems(Update, start_orbital_strikes);
    let camera_position = Vec3::new(-432.0, 876.0, 999.0);
    let camera =
        app.world_mut().spawn((MainCamera, Transform::from_translation(camera_position))).id();
    let marker = app
        .world_mut()
        .spawn((
            OrbitalRailgunCmp {
                planet: origin,
                anchor: marker_anchor,
                base_rotation: 0.0,
                phase: 0.0,
            },
            Transform::from_translation(marker_anchor.extend(2.22)),
        ))
        .id();
    app.world_mut().write_message(StartTurnMsg::new(false, false));

    app.update();

    let mut charge_rings = 0;
    let mut charge_cores = 0;
    let mut charge_motes = 0;
    let mut feeders = Vec::new();
    let mut convergence_rings = 0;
    let mut convergence_cores = 0;
    let mut main_beams = 0;
    let mut beam_motes = 0;
    let mut impact_rings = 0;
    let mut afterglows = 0;
    let mut explosions = 0;
    let world = app.world_mut();
    let mut parts = world.query::<(&StrikePart, &Sprite)>();
    for (part, _sprite) in parts.iter(world) {
        match part {
            StrikePart::ChargeRing {
                ..
            } => charge_rings += 1,
            StrikePart::ChargeCore {
                ..
            } => charge_cores += 1,
            StrikePart::ChargeMote {
                ..
            } => charge_motes += 1,
            StrikePart::FeederBeam {
                start,
                end,
                direct,
                ..
            } => feeders.push((*start, *end, *direct)),
            StrikePart::ConvergenceRing {
                ..
            } => convergence_rings += 1,
            StrikePart::ConvergenceCore => convergence_cores += 1,
            StrikePart::MainBeam {
                ..
            } => main_beams += 1,
            StrikePart::BeamMote {
                ..
            } => beam_motes += 1,
            StrikePart::ImpactRing {
                ..
            } => impact_rings += 1,
            StrikePart::ExplosionAfterglow {
                ..
            } => afterglows += 1,
            StrikePart::Explosion {
                ..
            } => explosions += 1,
        }
    }
    assert_eq!(charge_rings, 3);
    assert_eq!(charge_cores, 1);
    assert_eq!(charge_motes, 12);
    assert_eq!(feeders.len(), 3);
    assert!(feeders.iter().all(|(origin, end, direct)| {
        origin.distance(firing_origin) < 0.001 && end.distance(target_position) < 0.001 && *direct
    }));
    let firing = app.world().get::<OrbitalRailgunFiring>(marker).unwrap();
    assert!(
        firing.target_anchor.distance(direction.normalize_or_zero() * marker_anchor.length())
            < 0.001
    );
    let muzzle_direction = (Quat::from_rotation_z(firing.target_rotation) * Vec3::NEG_Y).truncate();
    assert!(
        muzzle_direction.dot(direction.normalize_or_zero()) > 0.999,
        "a lone Railgun must face the target before its direct beam fires"
    );
    assert_eq!(convergence_rings, 0);
    assert_eq!(convergence_cores, 0);
    assert_eq!(main_beams, 0);
    assert_eq!(beam_motes, 0);
    assert_eq!(impact_rings, 4);
    assert_eq!(afterglows, 0, "a failed shot must not leave a destruction afterglow");
    assert_eq!(explosions, 0, "a failed shot must leave the planet visually intact");
    assert_eq!(app.world().get::<Transform>(camera).unwrap().translation, camera_position);
}

#[test]
fn shots_converge_only_when_the_target_is_away_from_the_firing_group() {
    let target = Vec2::ZERO;

    assert!(!railgun_shots_should_converge(&[Vec2::new(-100.0, 0.0)], target));
    assert!(!railgun_shots_should_converge(
        &[Vec2::new(-100.0, 0.0), Vec2::new(100.0, 0.0)],
        target,
    ));
    assert!(railgun_shots_should_converge(
        &[Vec2::new(-100.0, -20.0), Vec2::new(-100.0, 20.0)],
        target,
    ));
}

#[test]
fn firing_pose_sweeps_radially_to_the_aim_point_then_returns_to_idle() {
    let transform = Transform::from_translation(Vec3::X * 100.0)
        .with_rotation(Quat::from_rotation_z(-PI * 0.5));
    let mut firing = OrbitalRailgunFiring::new(&transform, Vec2::new(100.0, 0.0), Vec2::Y);

    firing.elapsed = AIM_SECONDS * 0.5;
    let (halfway, halfway_rotation, finished) = firing.pose(Vec2::X * 100.0, 0.0);
    assert!(!finished);
    assert!((halfway.length() - 100.0).abs() < 0.001);
    assert!(halfway.x > 0.0 && halfway.y > 0.0);
    assert!((-PI..-PI * 0.5).contains(&halfway_rotation));

    firing.elapsed = AIM_SECONDS;
    let (aimed, aimed_rotation, finished) = firing.pose(Vec2::X * 100.0, 0.0);
    assert!(!finished);
    assert!(aimed.distance(Vec2::Y * 100.0) < 0.001);
    let muzzle_direction = (Quat::from_rotation_z(aimed_rotation) * Vec3::NEG_Y).truncate();
    assert!(muzzle_direction.distance(Vec2::Y) < 0.001);

    firing.elapsed = firing.return_start() + RETURN_SECONDS + 0.01;
    let (returned, returned_rotation, finished) = firing.pose(Vec2::X * 100.0, -0.1);
    assert!(finished);
    assert!(returned.distance(Vec2::X * 100.0) < 0.001);
    let returned_muzzle = (Quat::from_rotation_z(returned_rotation) * Vec3::NEG_Y).truncate();
    let idle_muzzle = (Quat::from_rotation_z(-0.1) * Vec3::NEG_Y).truncate();
    assert!(returned_muzzle.distance(idle_muzzle) < 0.001);
}

#[test]
fn a_match_ending_multi_railgun_strike_plays_before_the_end_game_overlay() {
    let mut model = GameModel::new([92; 32], GameRules::default()).unwrap();
    model.start().unwrap();
    let first = model.players[0].home_planet;
    let target = model.players[1].home_planet;
    let second = model
        .map
        .planets
        .iter()
        .find(|planet| planet.id != first && planet.id != target)
        .unwrap()
        .id;
    model.map.get_mut(first).position = Vec2::new(-800.0, -200.0);
    model.map.get_mut(second).position = Vec2::new(-800.0, 200.0);
    model.map.get_mut(target).position = Vec2::new(800.0, 0.0);

    let mut app = App::new();
    app.add_plugins((TaskPoolPlugin::default(), AssetPlugin::default()))
        .init_asset::<Image>()
        .init_asset::<Font>()
        .init_asset::<TextureAtlasLayout>()
        .init_asset::<AudioSource>()
        .init_resource::<WorldAssets>()
        .init_resource::<Time>()
        .insert_resource(model.map)
        .insert_resource(Settings {
            turn: model.turn as usize,
            ..default()
        })
        .insert_resource(OrbitalStrikes(vec![OrbitalStrike {
            turn: model.turn,
            origins: vec![first, second],
            target,
            chance_basis_points: 1_000,
            destroyed: true,
        }]))
        .insert_resource(State::new(GameState::Playing))
        .init_resource::<NextState<GameState>>()
        .init_resource::<EndGamePresentation>()
        .add_message::<StartTurnMsg>()
        .add_message::<PlayAudioMsg>();
    for (index, planet) in [first, second].into_iter().enumerate() {
        app.world_mut().spawn((
            OrbitalRailgunCmp {
                planet,
                anchor: Vec2::new(100.0, 0.0),
                base_rotation: 0.0,
                phase: index as f32,
            },
            Transform::from_xyz(100.0, 0.0, 2.22),
        ));
    }
    app.world_mut().write_message(StartTurnMsg::new(false, false));

    app.world_mut().run_system_once(start_orbital_strikes).unwrap();
    let expected_directions = {
        let map = app.world().resource::<Map>();
        let target_position = map.get(target).position;
        let provisional_origins = [first, second].map(|origin| {
            let origin_position = map.get(origin).position;
            origin_position + (target_position - origin_position).normalize_or_zero() * 100.0
        });
        assert!(railgun_shots_should_converge(&provisional_origins, target_position));
        let convergence = convergence_point(&provisional_origins, target_position);
        [first, second].map(|origin| {
            let origin_position = map.get(origin).position;
            (
                origin,
                (convergence - origin_position).normalize_or_zero(),
                (target_position - origin_position).normalize_or_zero(),
            )
        })
    };
    let world = app.world_mut();
    let mut firing_railguns = world.query::<(&OrbitalRailgunCmp, &OrbitalRailgunFiring)>();
    let mut aimed_at_convergence = 0;
    let mut convergence_differs_from_target = false;
    for (railgun, firing) in firing_railguns.iter(world) {
        let Some((_, convergence_direction, target_direction)) =
            expected_directions.iter().find(|(origin, _, _)| *origin == railgun.planet)
        else {
            continue;
        };
        let muzzle_direction =
            (Quat::from_rotation_z(firing.target_rotation) * Vec3::NEG_Y).truncate();
        assert!(muzzle_direction.dot(*convergence_direction) > 0.999);
        convergence_differs_from_target |= convergence_direction.dot(*target_direction) < 0.999;
        aimed_at_convergence += 1;
    }
    assert_eq!(aimed_at_convergence, 2);
    assert!(
        convergence_differs_from_target,
        "the fixture must distinguish aiming at convergence from aiming at the target world"
    );
    app.world_mut().resource_mut::<EndGamePresentation>().request();
    app.world_mut().run_system_once(finish_end_game_presentation).unwrap();

    assert!(matches!(*app.world().resource::<NextState<GameState>>(), NextState::Unchanged));
    let world = app.world_mut();
    let mut effects = world.query::<&mut OrbitalStrikeEffect>();
    let mut effect = effects.single_mut(world).unwrap();
    let effect_duration = effect.timer.duration();
    effect.timer.set_elapsed(effect_duration);

    app.world_mut().run_system_once(animate_orbital_strikes).unwrap();
    app.world_mut().run_system_once(finish_end_game_presentation).unwrap();

    assert!(matches!(
        *app.world().resource::<NextState<GameState>>(),
        NextState::Pending(GameState::EndGame)
    ));
}

#[test]
fn destruction_layers_fade_instead_of_disappearing_on_the_last_frame() {
    let last_explosion_index = 47;
    let explosion_duration = EXPLOSION_FRAME_SECONDS * (last_explosion_index + 1) as f32;
    let fixed_cutoff_age = EFFECT_SECONDS
        - AIM_SECONDS
        - (CHARGE_SECONDS + CONVERGE_SECONDS + FOCUS_SECONDS + 0.12 + 0.18);

    assert_eq!(explosion_alpha(-0.01, explosion_duration), 0.0);
    assert_eq!(explosion_alpha(explosion_duration, explosion_duration), 0.0);
    assert!(explosion_alpha(explosion_duration * 0.8, explosion_duration) > 0.0);
    assert!(
        explosion_alpha(fixed_cutoff_age, explosion_duration) > 0.0,
        "a fixed effect lifetime would remove an explosion before its final frame"
    );
    assert!(
        destruction_effect_seconds(last_explosion_index)
            >= AIM_SECONDS
                + CHARGE_SECONDS
                + CONVERGE_SECONDS
                + FOCUS_SECONDS
                + 0.12
                + 0.18
                + explosion_duration
    );
    assert!(afterglow_alpha(AFTERGLOW_SECONDS * 0.8, 0.0) > 0.0);
    assert_eq!(afterglow_alpha(AFTERGLOW_SECONDS, 0.0), 0.0);
}
