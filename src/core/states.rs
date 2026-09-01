//! Application, gameplay-overlay, combat-animation, and audio state machines.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;

#[derive(States, EnumIter, Debug, Clone, Copy, Eq, PartialEq, Hash, Default)]
/// Top-level boot, menu, loading, lobby, and gameplay application states.
pub enum AppState {
    /// Public configuration, anonymous authentication, and menu assets are loading.
    #[default]
    Boot,
    /// Top-level navigation after authentication completes.
    MainMenu,
    /// Debug-only one-player practice setup; release builds redirect to the main menu.
    SinglePlayerMenu,
    /// Multiplayer action chooser.
    MultiPlayerMenu,
    /// Creator settings and display-name form.
    CreateGame,
    /// Game-code and display-name form.
    JoinGame,
    /// Game/recovery-code replacement-identity form.
    RecoverPlayer,
    /// Authenticated user's list of persisted games.
    ResumeGame,
    /// Selected game waiting for all configured members.
    Lobby,
    /// Global preferences screen.
    Settings,
    /// Gameplay state and deferred world assets are loading.
    LoadingGame,
    /// Map rendering and gameplay input are active.
    Game,
}

#[derive(States, Debug, Clone, Copy, Eq, PartialEq, Hash, Default)]
/// In-game map, combat, pause, settings, and completion overlay states.
pub enum GameState {
    /// Normal map interaction and simultaneous-turn drafting.
    #[default]
    Playing,
    /// Pre-combat report selection.
    CombatMenu,
    /// Animated combat playback.
    Combat,
    /// Pause/options overlay.
    GameMenu,
    /// In-game settings overlay.
    Settings,
    /// Victory, defeat, or draw overlay.
    EndGame,
}

#[derive(States, Debug, Clone, Copy, Eq, PartialEq, Hash, Default)]
/// Ordered phases of combat animation playback.
pub enum CombatState {
    /// Creates combat animation entities.
    #[default]
    Setup,
    /// Resolves anti-ballistic interception.
    AntiBallistic,
    /// Displays the next combat round.
    DisplayRound,
    /// Animates weapon fire.
    Fire,
    /// Applies repair effects.
    Repair,
    /// Applies bomber effects.
    Bomb,
    /// Applies the death-ray effect.
    DeathRay,
    /// Cleans up the current combat.
    EndCombat,
}

#[derive(States, Debug, Clone, Copy, Eq, PartialEq, Hash, Default, Serialize, Deserialize)]
/// User-selectable music and effect playback modes.
pub enum AudioState {
    /// All audio is disabled.
    Mute,
    /// Effects play but background music does not.
    #[default]
    NoMusic,
    /// Effects and background music play.
    Sound,
}
