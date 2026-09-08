use super::*;
use crate::core::map::systems::OrbitalRailgunCmp;
use crate::core::simulation::{GameModel, GameRules};
use crate::multiplayer::client::MultiplayerSession;
use bevy::asset::AssetPlugin;
use bevy_kira_audio::AudioSource;

#[test]
fn animation_systems_wait_until_the_map_is_loaded() {
    let mut app = App::new();
    app.add_plugins(OrbitalRailgunAnimationPlugin);

    app.update();
}

#[test]
fn public_strike_stages_charge_convergence_and_beam_without_a_failure_explosion() {
    let mut model = GameModel::new([91; 32], GameRules::default()).unwrap();
    model.start().unwrap();
    let origin = model.players[0].home_planet;
    let target = model.players[1].home_planet;
    let marker_position = Vec3::new(321.0, -654.0, 2.22);

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
        .init_resource::<MultiplayerSession>()
        .insert_resource(OrbitalStrikes(vec![OrbitalStrike {
            turn: model.turn,
            origins: vec![origin],
            target,
            chance_basis_points: 350,
            destroyed: false,
        }]))
        .add_message::<StartTurnMsg>()
        .add_systems(Update, start_orbital_strikes);
    app.world_mut().spawn((
        OrbitalRailgunCmp {
            planet: origin,
            anchor: Vec2::ZERO,
            base_rotation: 0.0,
            phase: 0.0,
        },
        GlobalTransform::from_translation(marker_position),
    ));
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
                ..
            } => feeders.push(*start),
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
    assert_eq!(feeders, vec![marker_position.truncate(); 3]);
    assert_eq!(convergence_rings, 3);
    assert_eq!(convergence_cores, 1);
    assert_eq!(main_beams, 3);
    assert_eq!(beam_motes, 20);
    assert_eq!(impact_rings, 4);
    assert_eq!(afterglows, 0, "a failed shot must not leave a destruction afterglow");
    assert_eq!(explosions, 0, "a failed shot must leave the planet visually intact");
}

#[test]
fn destruction_layers_fade_instead_of_disappearing_on_the_last_frame() {
    let explosion_duration = EXPLOSION_FRAME_SECONDS * 16.0;

    assert_eq!(explosion_alpha(-0.01, explosion_duration), 0.0);
    assert_eq!(explosion_alpha(explosion_duration, explosion_duration), 0.0);
    assert!(explosion_alpha(explosion_duration * 0.8, explosion_duration) > 0.0);
    assert!(afterglow_alpha(explosion_duration, 0.0) > 0.0);
    assert_eq!(afterglow_alpha(AFTERGLOW_SECONDS, 0.0), 0.0);
}
