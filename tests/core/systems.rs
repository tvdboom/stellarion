use bevy::ecs::system::RunSystemOnce;

use super::*;

#[test]
fn modal_menus_block_map_picking_clear_hover_and_restore_input_on_resume() {
    let mut app = App::new();
    app.insert_resource(UiState {
        planet_hover: Some(1),
        planet_selected: Some(2),
        mission_hover: Some(5),
        mission_hover_from_ui: true,
        ..default()
    });
    let window = app
        .world_mut()
        .spawn((Window::default(), CursorIcon::from(SystemCursorIcon::Pointer)))
        .id();
    app.add_systems(Update, suspend_gameplay_interactions);

    app.update();
    app.update(); // Switching from the pause menu to Settings must not duplicate the blocker.

    let mut blockers =
        app.world_mut().query_filtered::<(Entity, &Pickable), With<GameplayInputBlocker>>();
    let (blocker, pickable) = blockers.single(app.world()).unwrap();
    assert!(pickable.should_block_lower);
    assert!(!pickable.is_hoverable);
    let state = app.world().resource::<UiState>();
    assert_eq!(state.planet_hover, None);
    assert_eq!(state.planet_selected, Some(2), "persistent selection is preserved");
    assert_eq!(state.mission_hover, None);
    assert!(!state.mission_hover_from_ui);
    assert_eq!(
        app.world().get::<CursorIcon>(window),
        Some(&CursorIcon::from(SystemCursorIcon::Default))
    );

    app.world_mut().run_system_once(resume_gameplay_interactions).unwrap();
    assert!(app.world().get_entity(blocker).is_err());
}
