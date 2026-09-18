use super::*;
use crate::core::combat::resolution::resolve_combat_with_rng;
use crate::core::combat::systems::CombatCmp;
use crate::core::map::icon::Icon;
use crate::core::map::planet::Planet;
use crate::core::missions::{BombingRaid, Mission};
use crate::core::random::DeterministicRngState;
use crate::core::units::{ships::Ship, Army, Unit};
use bevy::ecs::system::RunSystemOnce;
use std::time::Duration;

fn app() -> App {
    let mut rng = DeterministicRngState::from_u64(423).next_rng();
    let mut origin = Planet::new_with_rng(0, "Origin".into(), Vec2::ZERO, false, 1.0, &mut rng);
    origin.colonize(1);
    let mut planet = Planet::new_with_rng(1, "Target".into(), Vec2::X, false, 1.0, &mut rng);
    planet.colonize(2);
    planet.army = Army::from([(Unit::Ship(Ship::LightFighter), 1)]).into();
    let mission = Mission::new_with_id(
        1,
        1,
        1,
        &origin,
        &planet,
        Icon::Attack,
        Army::from([(Unit::war_sun(), 1)]),
        BombingRaid::None,
        false,
        false,
        None,
    );
    let report = resolve_combat_with_rng(1, &mission, &planet, &mut rng);
    let report_id = report.id;
    let mut player = Player::new(1, 0);
    player.reports.push(report);
    let mut app = App::new();
    app.init_resource::<Time>()
        .init_resource::<Settings>()
        .insert_resource(player)
        .insert_resource(UiState {
            in_combat: Some(report_id),
            combat_view: CombatView::Cinematic,
            ..Default::default()
        })
        .add_message::<PlayAudioMsg>();
    app
}

fn audio(app: &mut App) -> Vec<&'static str> {
    app.world_mut().resource_mut::<Messages<PlayAudioMsg>>().drain().map(|cue| cue.name).collect()
}

fn advance(app: &mut App, seconds: f32) {
    app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs_f32(seconds));
    app.world_mut().run_system_once(advance_cinematic).unwrap();
}

#[test]
fn schematic_is_the_default_selection_and_cinematic_is_explicit() {
    let mut world = World::new();
    assert!(!world.run_system_once(cinematic_selected).unwrap());
    world.init_resource::<UiState>();
    assert_eq!(world.resource::<UiState>().combat_view, CombatView::Schematic);
    assert!(!world.run_system_once(cinematic_selected).unwrap());
    world.resource_mut::<UiState>().combat_view = CombatView::Cinematic;
    assert!(world.run_system_once(cinematic_selected).unwrap());
}

#[test]
fn setup_adds_movie_resources_without_schematic_entities_or_changing_the_report() {
    let mut app = app();
    let saved = serde_json::to_string(&app.world().resource::<Player>().reports).unwrap();
    app.world_mut().run_system_once(setup_cinematic).unwrap();
    assert!(app.world().contains_resource::<CinematicPlayback>());
    assert!(app.world().contains_resource::<CinematicSoundtrack>());
    let world = app.world_mut();
    assert_eq!(world.query_filtered::<Entity, With<CombatCmp>>().iter(world).count(), 0);
    assert_eq!(app.world().resource::<CinematicPlayback>().elapsed, 0.0);
    assert_eq!(serde_json::to_string(&app.world().resource::<Player>().reports).unwrap(), saved);
    assert_eq!(audio(&mut app), vec!["horn"]);
}

#[test]
fn shared_pause_and_speed_settings_control_the_one_movie_clock() {
    let mut app = app();
    app.world_mut().run_system_once(setup_cinematic).unwrap();
    audio(&mut app);
    app.world_mut().resource_mut::<Settings>().combat_speed = 2.0;
    advance(&mut app, 0.25);
    assert_eq!(app.world().resource::<CinematicPlayback>().elapsed, 0.5);
    app.world_mut().resource_mut::<Settings>().combat_paused = true;
    advance(&mut app, 2.0);
    assert_eq!(app.world().resource::<CinematicPlayback>().elapsed, 0.5);
    assert!(audio(&mut app).is_empty());
    {
        let mut settings = app.world_mut().resource_mut::<Settings>();
        settings.combat_paused = false;
        settings.combat_speed = 0.25;
    }
    advance(&mut app, 1.0);
    assert_eq!(app.world().resource::<CinematicPlayback>().elapsed, 0.75);
}

#[test]
fn completion_emits_one_result_and_drops_expired_weapon_sounds() {
    let mut app = app();
    app.world_mut().run_system_once(setup_cinematic).unwrap();
    audio(&mut app);
    let duration = app.world().resource::<CinematicPlayback>().timeline.duration;
    advance(&mut app, duration + 5.0);
    assert!(app.world().resource::<CinematicPlayback>().is_finished());
    assert_eq!(audio(&mut app), vec!["victory"]);
    advance(&mut app, 1.0);
    advance(&mut app, 1.0);
    assert!(audio(&mut app).is_empty());
}

#[test]
fn restart_rearms_audio_and_can_complete_a_second_time() {
    let mut app = app();
    app.world_mut().run_system_once(setup_cinematic).unwrap();
    let first_cue_at = app.world().resource::<CinematicSoundtrack>().cues[0].0;
    let duration = app.world().resource::<CinematicPlayback>().timeline.duration;
    audio(&mut app);
    advance(&mut app, duration + 1.0);
    assert_eq!(audio(&mut app), vec!["victory"]);
    app.world_mut().resource_mut::<CinematicPlayback>().restart();
    advance(&mut app, 0.0);
    assert_eq!(app.world().resource::<CinematicSoundtrack>().next, 0);
    advance(&mut app, first_cue_at);
    assert!(!audio(&mut app).is_empty());
    advance(&mut app, 0.0);
    assert!(audio(&mut app).is_empty());
    advance(&mut app, duration);
    assert_eq!(audio(&mut app), vec!["victory"]);
}

#[test]
fn exit_discards_playback_and_soundtrack_so_the_next_report_starts_fresh() {
    let mut app = app();
    app.world_mut().run_system_once(setup_cinematic).unwrap();
    advance(&mut app, 0.5);
    app.world_mut().run_system_once(exit_cinematic).unwrap();
    assert!(!app.world().contains_resource::<CinematicPlayback>());
    assert!(!app.world().contains_resource::<CinematicSoundtrack>());
    assert!(app.world().resource::<UiState>().in_combat.is_some());
    audio(&mut app);
    advance(&mut app, 1.0);
    assert!(audio(&mut app).is_empty());
    app.world_mut().run_system_once(setup_cinematic).unwrap();
    assert_eq!(app.world().resource::<CinematicPlayback>().elapsed, 0.0);
    assert_eq!(app.world().resource::<CinematicSoundtrack>().next, 0);
}

#[test]
fn missing_selected_report_creates_no_movie_and_emits_no_audio() {
    let mut app = app();
    app.world_mut().resource_mut::<UiState>().in_combat = None;
    app.world_mut().run_system_once(setup_cinematic).unwrap();
    assert!(!app.world().contains_resource::<CinematicPlayback>());
    assert!(!app.world().contains_resource::<CinematicSoundtrack>());
    advance(&mut app, 1.0);
    assert!(audio(&mut app).is_empty());
}
