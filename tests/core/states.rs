use super::*;

#[test]
fn only_in_game_menus_are_modal_over_the_map() {
    for state in [GameState::Playing, GameState::CombatMenu, GameState::Combat, GameState::EndGame]
    {
        assert!(!state.is_modal_menu(), "{state:?}");
    }
    for state in [GameState::GameMenu, GameState::Settings] {
        assert!(state.is_modal_menu(), "{state:?}");
    }
}
