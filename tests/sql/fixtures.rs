//! Generates current Rust snapshots for disposable SQL contract tests.

use stellarion::core::simulation::{
    resolve_turn, GameModel, GameRules, PersistedGame, TurnSubmission,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let rules = GameRules {
        planets_per_player: 5,
        moons_percent: 0,
        colonizable_percent: 50,
        player_count: 4,
        practice_mode: false,
    };
    let lobby = PersistedGame::new(GameModel::new([7; 32], rules.clone())?);
    let mut active = GameModel::new(
        [8; 32],
        GameRules {
            player_count: 2,
            ..rules
        },
    )?;
    active.start()?;
    let mut resolved = active.clone();
    resolve_turn(
        &mut resolved,
        &[TurnSubmission::new(1, 1, vec![]), TurnSubmission::new(2, 1, vec![])],
    )?;
    println!(
        "{}",
        serde_json::json!({
            "lobby": lobby,
            "active": PersistedGame::new(active),
            "resolved": PersistedGame::new(resolved),
        })
    );
    Ok(())
}
