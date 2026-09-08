//! Bridges deferred asset completion and canonical multiplayer state into Bevy resources.

use bevy::prelude::*;

use crate::core::assets::{GameplayAssetState, WorldAssets};
use crate::core::combat::report::MissionReport;
use crate::core::identity::PlayerId;
use crate::core::map::model::Map;
use crate::core::map::orbital_railgun::OrbitalStrikes;
use crate::core::map::planet::PlanetId;
use crate::core::messages::{MessageAction, MessageMsg};
use crate::core::missions::Missions;
use crate::core::settings::Settings;
use crate::core::states::{AppState, GameState};
use crate::core::turns::{filter_missions, StartTurnMsg};
use crate::core::ui::systems::UiState;
use crate::core::units::buildings::Building;
use crate::core::units::{Amount, Unit};
use crate::multiplayer::client::{
    ConnectionStatus, MultiplayerSession, PendingTurnCommands, RefreshGameplayProjection,
    RefreshTurnDraft,
};

/// A strategic structure whose existence publicly identifies a planet's owner.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum PublicStructure {
    SpaceDock,
    OrbitalRailgun,
}

impl PublicStructure {
    pub(crate) fn unit(self) -> Unit {
        match self {
            Self::SpaceDock => Unit::space_dock(),
            Self::OrbitalRailgun => Unit::Building(Building::OrbitalRailgun),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::SpaceDock => "Space Dock",
            Self::OrbitalRailgun => "Orbital Railgun",
        }
    }

    fn article(self) -> &'static str {
        match self {
            Self::SpaceDock => "A",
            Self::OrbitalRailgun => "An",
        }
    }
}

/// Whether a public structure has appeared or been lost since the prior turn.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PublicStructureChange {
    Built,
    Destroyed,
}

/// Announces a public strategic-structure change to map presentation systems.
#[derive(Message, Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PublicStructureChangeMsg {
    pub(crate) planet: PlanetId,
    pub(crate) structure: PublicStructure,
    pub(crate) change: PublicStructureChange,
    pub(crate) owner: PlayerId,
}

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
    mut session: ResMut<MultiplayerSession>,
    mut pending: ResMut<PendingTurnCommands>,
    mut settings: ResMut<Settings>,
    mut next_app_state: ResMut<NextState<AppState>>,
    mut next_game_state: ResMut<NextState<GameState>>,
    mut start_turn: MessageWriter<StartTurnMsg>,
) {
    match assets.refresh_gameplay_state(&server) {
        GameplayAssetState::Ready => {},
        GameplayAssetState::Failed => {
            if session.menu_error.is_none() {
                let failure = assets
                    .gameplay_error()
                    .unwrap_or("A gameplay asset or one of its dependencies failed to load.");
                let recovery = if cfg!(debug_assertions) {
                    "Run `just assets` and restart Stellarion."
                } else {
                    "Reinstall the game or restore its runtime assets, then restart Stellarion."
                };
                session.menu_error = Some(format!("{failure} {recovery}"));
            }
            return;
        },
        GameplayAssetState::Deferred | GameplayAssetState::Loading => return,
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
    previous_map: Option<Res<Map>>,
    mut pending: ResMut<PendingTurnCommands>,
    mut settings: ResMut<Settings>,
    mut next_game_state: ResMut<NextState<GameState>>,
    mut start_turn: MessageWriter<StartTurnMsg>,
    mut messages: MessageWriter<MessageMsg>,
    mut structure_changes: MessageWriter<PublicStructureChangeMsg>,
) {
    if refresh.read().count() == 0 {
        return;
    }
    let previous_turn = settings.turn as u64;
    let structure_notifications = session
        .active_game
        .as_ref()
        .zip(session.membership.as_ref())
        .map_or_else(Vec::new, |(record, membership)| {
            public_structure_notifications(
                previous_map.as_deref(),
                &record.persisted.state,
                membership.player_id,
            )
        });
    let railgun_notifications = session
        .active_game
        .as_ref()
        .zip(session.membership.as_ref())
        .map_or_else(Vec::new, |(record, membership)| {
            orbital_strike_notifications(
                previous_map.as_deref(),
                &record.persisted.state,
                membership.player_id,
                previous_turn,
            )
        });
    if install_gameplay_projection(
        &mut commands,
        &session,
        &mut pending,
        &mut settings,
        &mut next_game_state,
        &mut start_turn,
        false,
        false,
    ) {
        for notification in railgun_notifications {
            messages.write(notification);
        }
        for (change, notification) in structure_notifications {
            messages.write(notification);
            structure_changes.write(change);
        }
    }
}

/// Warns observers and targets once when a newly resolved public Railgun strike is installed.
fn orbital_strike_notifications(
    previous: Option<&Map>,
    current: &crate::core::simulation::GameModel,
    local_player_id: PlayerId,
    previously_displayed_turn: u64,
) -> Vec<MessageMsg> {
    let Some(previous) = previous.filter(|_| current.turn > previously_displayed_turn) else {
        return Vec::new();
    };
    current
        .orbital_strikes
        .iter()
        .filter(|strike| strike.turn == current.turn)
        .filter(|strike| {
            !strike.origins.iter().any(|origin| {
                previous.try_get(*origin).and_then(|planet| planet.owned) == Some(local_player_id)
            })
        })
        .filter_map(|strike| {
            let target = current.map.try_get(strike.target)?;
            let world = if target.is_moon() {
                "moon"
            } else {
                "planet"
            };
            let notification =
                MessageMsg::warning(format!("Railgun shot fired on {world} {}.", target.name));
            Some(if target.is_destroyed {
                notification
            } else {
                notification.with_action(MessageAction::FocusPlanet(target.id))
            })
        })
        .collect()
}

fn battle_that_destroyed(
    model: &crate::core::simulation::GameModel,
    planet: PlanetId,
    unit: Unit,
) -> Option<&MissionReport> {
    model.players.iter().flat_map(|player| &player.reports).find(|report| {
        usize::try_from(model.turn).is_ok_and(|turn| report.turn == turn)
            && report.mission.destination == planet
            && report.combat_report.is_some()
            && report.planet.army.amount(&unit) > 0
            && report.surviving_defender.amount(&unit) == 0
    })
}

/// Announces public structures that appeared, or were destroyed in battle, this turn.
fn public_structure_notifications(
    previous: Option<&Map>,
    current: &crate::core::simulation::GameModel,
    local_player_id: PlayerId,
) -> Vec<(PublicStructureChangeMsg, MessageMsg)> {
    let Some(previous) = previous else {
        return Vec::new();
    };
    current
        .map
        .planets
        .iter()
        .filter_map(|planet| previous.try_get(planet.id).map(|prior| (planet, prior)))
        .flat_map(|(planet, prior)| {
            [PublicStructure::SpaceDock, PublicStructure::OrbitalRailgun].into_iter().filter_map(
                move |structure| {
                    let unit = structure.unit();
                    let had_structure = prior.army.amount(&unit) > 0;
                    let has_structure = planet.army.amount(&unit) > 0;
                    let (change, owner) = if !had_structure && has_structure {
                        let owner = planet.owned?;
                        if owner == local_player_id {
                            return None;
                        }
                        (PublicStructureChange::Built, owner)
                    } else if had_structure && !has_structure {
                        let report = battle_that_destroyed(current, planet.id, unit)?;
                        if report.mission.owner == local_player_id
                            || report.planet.controlled == Some(local_player_id)
                        {
                            return None;
                        }
                        (PublicStructureChange::Destroyed, report.planet.owned.or(prior.owned)?)
                    } else {
                        return None;
                    };
                    let verb = match change {
                        PublicStructureChange::Built => "built",
                        PublicStructureChange::Destroyed => "destroyed",
                    };
                    let mut notification = MessageMsg::info(format!(
                        "{} {} has been {verb} on planet {}.",
                        structure.article(),
                        structure.label(),
                        planet.name
                    ));
                    if !planet.is_destroyed {
                        notification =
                            notification.with_action(MessageAction::FocusPlanet(planet.id));
                    }
                    Some((
                        PublicStructureChangeMsg {
                            planet: planet.id,
                            structure,
                            change,
                            owner,
                        },
                        notification,
                    ))
                },
            )
        })
        .collect()
}

/// Builds a local draft only when it belongs to the canonical turn being displayed.
fn gameplay_draft_projection(
    state: &crate::core::simulation::GameModel,
    player_id: PlayerId,
    pending: &PendingTurnCommands,
) -> Option<crate::core::simulation::GameModel> {
    (pending.turn == state.turn && !pending.commands.is_empty())
        .then(|| crate::core::simulation::preview_commands(state, player_id, &pending.commands))
        .and_then(Result::ok)
}

/// Rebuilds recovered orders while preserving the planet/mission the player just opened.
pub(crate) fn refresh_turn_draft(
    mut refresh: MessageReader<RefreshTurnDraft>,
    mut commands: Commands,
    session: Res<MultiplayerSession>,
    pending: Res<PendingTurnCommands>,
    mut messages: MessageWriter<crate::core::messages::MessageMsg>,
) {
    if refresh.read().count() == 0 {
        return;
    }
    let (Some(record), Some(member)) = (&session.active_game, &session.membership) else {
        return;
    };
    if pending.turn != record.persisted.state.turn {
        return;
    }
    match crate::core::simulation::preview_commands(
        &record.persisted.state,
        member.player_id,
        &pending.commands,
    ) {
        Ok(model) => {
            if let Ok(player) = model.player(member.player_id) {
                commands.insert_resource(model.map.clone());
                commands.insert_resource(player.clone());
                commands.insert_resource(Missions(filter_missions(
                    &model.missions,
                    &model.map,
                    player,
                )));
                commands.insert_resource(OrbitalStrikes(model.orbital_strikes.clone()));
            }
        },
        Err(error) => {
            messages.write(crate::core::messages::MessageMsg::error(format!(
                "Could not restore turn orders: {error}"
            )));
        },
    }
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
    // A newly advanced canonical turn carries the public Railgun outcome that must be animated.
    // Previewing an empty draft clears that history, so only build a draft projection when the
    // local commands actually belong to the canonical turn being installed.
    let preview = gameplay_draft_projection(&record.persisted.state, membership.player_id, pending);
    let model = preview.as_ref().unwrap_or(&record.persisted.state);
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
    if pending.turn != model.turn {
        pending.reset(model.turn);
    }
    commands.insert_resource(model.map.clone());
    commands.insert_resource(player.clone());
    commands.insert_resource(Missions(filter_missions(&model.missions, &model.map, player)));
    commands.insert_resource(OrbitalStrikes(model.orbital_strikes.clone()));
    commands.insert_resource(UiState::default());
    start_turn.write(StartTurnMsg::new(skip_battle, skip_end_game));
    next_game_state.set(GameState::Playing);
    true
}

#[cfg(test)]
#[path = "../../tests/core/loading.rs"]
mod tests;
