//! Bridges deferred asset completion and canonical multiplayer state into Bevy resources.

use bevy::prelude::*;

use crate::core::assets::{GameplayAssetState, WorldAssets};
use crate::core::missions::Missions;
use crate::core::settings::Settings;
use crate::core::states::{AppState, GameState};
use crate::core::turns::{filter_missions, PreviousEndTurnState, StartTurnMsg};
use crate::core::ui::systems::UiState;
use crate::multiplayer::client::{
    ConnectionStatus, MultiplayerSession, PendingTurnCommands, RefreshGameplayProjection,
};

/// Leaves boot only after anonymous authentication and the minimal menu group are ready.
pub fn finish_boot(
    server: Res<AssetServer>,
    assets: Res<WorldAssets>,
    session: Res<MultiplayerSession>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    if session.connection != ConnectionStatus::Initializing && assets.menu_ready(&server) {
        next_state.set(AppState::MainMenu);
    }
}

/// Requests the world/unit/effect/audio groups on entry to the explicit loading state.
pub fn begin_gameplay_loading(
    server: Res<AssetServer>,
    mut assets: ResMut<WorldAssets>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
) {
    assets.begin_gameplay_loading(&server, &mut layouts);
}

/// Installs the selected player's ECS projection after every gameplay asset is ready.
pub fn finish_gameplay_loading(
    mut commands: Commands,
    server: Res<AssetServer>,
    mut assets: ResMut<WorldAssets>,
    session: Res<MultiplayerSession>,
    mut pending: ResMut<PendingTurnCommands>,
    mut settings: ResMut<Settings>,
    mut next_app_state: ResMut<NextState<AppState>>,
    mut next_game_state: ResMut<NextState<GameState>>,
    mut start_turn: MessageWriter<StartTurnMsg>,
) {
    if assets.refresh_gameplay_state(&server) != GameplayAssetState::Ready {
        return;
    }
    if install_gameplay_projection(
        &mut commands,
        &session,
        &mut pending,
        &mut settings,
        &mut next_game_state,
        &mut start_turn,
        true,
        true,
    ) {
        next_app_state.set(AppState::Game);
    }
}

/// Replaces an already-visible turn projection without leaving the gameplay state.
pub fn refresh_gameplay_projection(
    mut refresh: MessageReader<RefreshGameplayProjection>,
    mut commands: Commands,
    session: Res<MultiplayerSession>,
    mut pending: ResMut<PendingTurnCommands>,
    mut settings: ResMut<Settings>,
    mut next_game_state: ResMut<NextState<GameState>>,
    mut start_turn: MessageWriter<StartTurnMsg>,
) {
    if refresh.read().count() == 0 {
        return;
    }
    install_gameplay_projection(
        &mut commands,
        &session,
        &mut pending,
        &mut settings,
        &mut next_game_state,
        &mut start_turn,
        false,
        false,
    );
}

/// Copies the canonical multiplayer snapshot into the live gameplay resources.
fn install_gameplay_projection(
    commands: &mut Commands,
    session: &MultiplayerSession,
    pending: &mut PendingTurnCommands,
    settings: &mut Settings,
    next_game_state: &mut NextState<GameState>,
    start_turn: &mut MessageWriter<StartTurnMsg>,
    skip_battle: bool,
    skip_end_game: bool,
) -> bool {
    let (Some(record), Some(membership)) = (&session.active_game, &session.membership) else {
        return false;
    };
    let model = &record.persisted.state;
    let Ok(player) = model.player(membership.player_id) else {
        return false;
    };
    let Ok(turn) = usize::try_from(model.turn) else {
        return false;
    };

    settings.turn = turn;
    settings.n_planets = model.rules.planets_per_player;
    settings.p_colonizable = model.rules.colonizable_percent;
    settings.p_moons = model.rules.moons_percent;
    pending.reset(model.turn);
    commands.insert_resource(model.map.clone());
    commands.insert_resource(player.clone());
    commands.insert_resource(Missions(filter_missions(&model.missions, &model.map, player)));
    commands.insert_resource(UiState::default());
    commands.insert_resource(PreviousEndTurnState::default());
    start_turn.write(StartTurnMsg::new(skip_battle, skip_end_game));
    next_game_state.set(GameState::Playing);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// The loading lifecycle distinguishes deferred, in-flight, and ready groups.
    fn loading_state_has_explicit_transitions() {
        assert_ne!(GameplayAssetState::Deferred, GameplayAssetState::Loading);
        assert_ne!(GameplayAssetState::Loading, GameplayAssetState::Ready);
    }
}
