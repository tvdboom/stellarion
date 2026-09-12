use super::*;
use crate::core::simulation::{
    add_trade_agreement_immediately, preview_commands, resolve_turn, GameModel, GameRules,
    TurnSubmission,
};

fn trading_game() -> GameModel {
    let mut model = GameModel::new([31; 32], GameRules::default()).unwrap();
    model.start().unwrap();
    let first = model.players[0].home_planet;
    let second = model.players[1].home_planet;
    model.map.get_mut(first).position = bevy::math::Vec2::ZERO;
    model.map.get_mut(second).position = bevy::math::Vec2::X * Planet::SIZE * 2.5;
    model.map.get_mut(first).army.insert(Unit::Building(Building::TradingPost), 3);
    model.map.get_mut(second).army.insert(Unit::Building(Building::TradingPost), 2);
    for planet_id in [first, second] {
        for building in [Building::MetalMine, Building::CrystalMine, Building::DeuteriumSynthesizer]
        {
            model.map.get_mut(planet_id).army.remove(&Unit::Building(building));
        }
    }
    model.players[0].resources = Resources::new(2_000, 2_000, 2_000);
    model.players[1].resources = Resources::new(2_000, 2_000, 2_000);
    model
}

#[test]
fn each_post_level_supplies_its_owners_independent_capacity() {
    let model = trading_game();
    let first = model.players[0].home_planet;
    let second = model.players[1].home_planet;
    assert_eq!(trading_post_capacity(model.map.get(first), 1), 1_500);
    assert_eq!(trading_post_capacity(model.map.get(second), 2), 1_000);
    assert!(trading_posts_are_adjacent(&model.map, 1, first, 2, second));
}

#[test]
fn finalized_trade_reserves_now_and_delivers_only_at_resolution() {
    let mut model = trading_game();
    let first = model.players[0].home_planet;
    let second = model.players[1].home_planet;
    add_trade_agreement_immediately(
        &mut model,
        TradeAgreement {
            id: 91,
            turn: 1,
            parties: [
                TradeParty {
                    player_id: 1,
                    planet_id: first,
                    resources: Resources::new(800, 400, 200),
                },
                TradeParty {
                    player_id: 2,
                    planet_id: second,
                    resources: Resources::new(100, 300, 500),
                },
            ],
        },
    )
    .unwrap();

    let projected = preview_commands(&model, 1, &[]).unwrap();
    assert_eq!(projected.players[0].resources, Resources::new(1_200, 1_600, 1_800));
    assert_eq!(model.players[0].resources, Resources::new(2_000, 2_000, 2_000));
    assert_eq!(model.trade_incoming(1), Resources::new(100, 300, 500));

    resolve_turn(
        &mut model,
        &[TurnSubmission::new(1, 1, Vec::new()), TurnSubmission::new(2, 1, Vec::new())],
    )
    .unwrap();
    assert_eq!(model.players[0].resources, Resources::new(1_300, 1_900, 2_300));
    assert_eq!(model.players[1].resources, Resources::new(2_700, 2_100, 1_700));
    assert!(model.trades.is_empty());
}

#[test]
fn a_trade_above_either_posts_capacity_is_rejected() {
    let mut model = trading_game();
    let first = model.players[0].home_planet;
    let second = model.players[1].home_planet;
    let error = add_trade_agreement_immediately(
        &mut model,
        TradeAgreement {
            id: 92,
            turn: 1,
            parties: [
                TradeParty {
                    player_id: 1,
                    planet_id: first,
                    resources: Resources::new(1_500, 0, 0),
                },
                TradeParty {
                    player_id: 2,
                    planet_id: second,
                    resources: Resources::new(1_001, 0, 0),
                },
            ],
        },
    );
    assert!(error.is_err());
    assert!(model.trades.is_empty());
}
