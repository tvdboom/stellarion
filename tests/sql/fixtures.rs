//! Generates current Rust snapshots for disposable SQL contract tests.

use rand::SeedableRng;
use stellarion::core::combat::plan::{BattlePlan, CombatTactic, ProtectionPosture, TargetPriority};
use stellarion::core::combat::resolution::resolve_combat_with_rng;
use stellarion::core::map::icon::Icon;
use stellarion::core::missions::{BombingRaid, Mission};
use stellarion::core::simulation::{
    resolve_turn, GameModel, GameRules, PersistedGame, TurnSubmission,
};
use stellarion::core::trading::{trading_post_capacity, trading_posts_are_adjacent};
use stellarion::core::units::{buildings::Building, ships::Ship, Army, Unit};

fn trade_route_fixtures(model: &GameModel) -> Vec<serde_json::Value> {
    let mut map = model.map.clone();
    let first = &model.players[0];
    let second = &model.players[1];
    map.get_mut(first.home_planet).position = bevy::math::Vec2::ZERO;
    let mut routes = Vec::new();
    for first_level in 0..=Building::MAX_LEVEL + 1 {
        for second_level in 0..=Building::MAX_LEVEL + 1 {
            for distance in [
                0.0, 150.0, 150.01, 300.0, 300.01, 400.0, 450.0, 450.01, 600.0, 600.01, 750.0,
                750.01,
            ] {
                map.get_mut(first.home_planet)
                    .army
                    .insert(Unit::Building(Building::TradingPost), first_level);
                let other = map.get_mut(second.home_planet);
                other.army.insert(Unit::Building(Building::TradingPost), second_level);
                other.position = bevy::math::Vec2::X * distance;
                routes.push(serde_json::json!({
                    "first": map.get(first.home_planet),
                    "second": map.get(second.home_planet),
                    "first_capacity": trading_post_capacity(map.get(first.home_planet), first.id),
                    "second_capacity": trading_post_capacity(map.get(second.home_planet), second.id),
                    "valid": trading_posts_are_adjacent(
                        &map, first.id, first.home_planet, second.id, second.home_planet,
                    ),
                }));
            }
        }
    }
    routes
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let battle_plans = CombatTactic::ALL
        .into_iter()
        .flat_map(|tactic| {
            TargetPriority::ALL.into_iter().flat_map(move |target_priority| {
                ProtectionPosture::ALL.into_iter().map(move |protection| BattlePlan {
                    automatic_tactic: false,
                    tactic,
                    target_priority,
                    protection,
                })
            })
        })
        .flat_map(|plan| {
            [
                plan,
                BattlePlan {
                    automatic_tactic: true,
                    ..plan
                },
            ]
        })
        .collect::<Vec<_>>();
    let rules = GameRules {
        planets_per_player: 5,
        moons_percent: 0,
        space_fauna_percent: 15,
        independent_populations: false,
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
    let battle_mission = Mission::new_with_id(
        7,
        1,
        1,
        active.map.get(active.players[0].home_planet),
        active.map.get(active.players[1].home_planet),
        Icon::Attack,
        Army::from([(Unit::Ship(Ship::LightFighter), 3)]),
        BombingRaid::None,
        false,
        false,
        None,
    );
    let battle_report = resolve_combat_with_rng(
        1,
        &battle_mission,
        active.map.get(active.players[1].home_planet),
        &mut rand_chacha::ChaCha8Rng::seed_from_u64(7),
    );
    let mut active_three = GameModel::new(
        [9; 32],
        GameRules {
            player_count: 3,
            ..rules.clone()
        },
    )?;
    active_three.start()?;
    let mut resolved = active.clone();
    resolve_turn(
        &mut resolved,
        &[TurnSubmission::new(1, 1, vec![]), TurnSubmission::new(2, 1, vec![])],
    )?;
    println!(
        "{}",
        serde_json::json!({
            "battle_plan": BattlePlan::default(),
            "battle_plans": battle_plans,
            "battle_report": battle_report,
            "trade_routes": trade_route_fixtures(&active),
            "lobby": lobby,
            "active": PersistedGame::new(active),
            "active_three": PersistedGame::new(active_three),
            "resolved": PersistedGame::new(resolved),
        })
    );
    Ok(())
}
