use super::*;
use crate::core::simulation::{
    add_trade_agreement_immediately, preview_commands, resolve_turn, GameModel, GameRules,
    TurnCommand, TurnSubmission,
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
fn completed_trading_posts_are_visible_only_through_owned_post_range() {
    let mut model = GameModel::new(
        [31; 32],
        GameRules {
            player_count: 4,
            ..Default::default()
        },
    )
    .unwrap();
    let adjacent = model.players[0].home_planet;
    let distant = model.players[2].home_planet;
    let post = model.players[3].home_planet;
    for planet in &mut model.map.planets {
        planet.owned = None;
        planet.controlled = None;
    }
    model.map.get_mut(adjacent).owned = Some(1);
    model.map.get_mut(adjacent).controlled = Some(2);
    model.map.get_mut(adjacent).position =
        bevy::math::Vec2::X * Planet::SIZE * TRADING_POST_RANGE_PER_LEVEL;
    model.map.get_mut(distant).owned = Some(3);
    model.map.get_mut(distant).controlled = Some(3);
    model.map.get_mut(distant).position =
        bevy::math::Vec2::Y * Planet::SIZE * (TRADING_POST_RANGE_PER_LEVEL + 0.01);
    let target = model.map.get_mut(post);
    target.position = bevy::math::Vec2::ZERO;
    target.owned = Some(4);
    target.controlled = Some(3);
    target.buy.push(Unit::Building(Building::TradingPost));

    for viewer in 1..=4 {
        assert_eq!(visible_trading_post_owner(&model.map, viewer, model.map.get(post)), None);
    }
    model.map.get_mut(post).buy.clear();
    model.map.get_mut(post).army.insert(Unit::Building(Building::TradingPost), 1);

    assert_eq!(visible_trading_post_owner(&model.map, 4, model.map.get(post)), Some(4));
    for viewer in [1, 2, 3] {
        assert_eq!(visible_trading_post_owner(&model.map, viewer, model.map.get(post)), None);
    }
    model.map.get_mut(adjacent).army.insert(Unit::Building(Building::TradingPost), 1);
    for viewer in [1, 4] {
        assert_eq!(
            visible_trading_post_owner(&model.map, viewer, model.map.get(post)),
            Some(4),
            "only an owned completed post reveals a foreign post"
        );
    }
    assert_eq!(visible_trading_post_owner(&model.map, 2, model.map.get(post)), None);
    assert_eq!(visible_trading_post_owner(&model.map, 3, model.map.get(post)), None);
    assert!(trading_posts_are_adjacent(&model.map, 1, adjacent, 4, post));

    model.map.get_mut(adjacent).is_destroyed = true;
    for viewer in [1, 2] {
        assert_eq!(visible_trading_post_owner(&model.map, viewer, model.map.get(post)), None);
    }
    assert_eq!(visible_trading_post_owner(&model.map, 4, model.map.get(post)), Some(4));
}

#[test]
fn trading_post_broadcast_tracks_current_ownership_and_destruction() {
    let mut model = trading_game();
    let post = model.players[1].home_planet;
    assert_eq!(visible_trading_post_owner(&model.map, 1, model.map.get(post)), Some(2));

    model.map.get_mut(post).controlled = Some(1);
    assert_eq!(visible_trading_post_owner(&model.map, 1, model.map.get(post)), Some(2));
    model.map.get_mut(post).owned = Some(1);
    assert_eq!(visible_trading_post_owner(&model.map, 1, model.map.get(post)), Some(1));
    model.map.get_mut(post).owned = None;
    assert_eq!(visible_trading_post_owner(&model.map, 1, model.map.get(post)), None);
    model.map.get_mut(post).owned = Some(2);
    model.map.get_mut(post).army.remove(&Unit::Building(Building::TradingPost));
    assert_eq!(visible_trading_post_owner(&model.map, 1, model.map.get(post)), None);
    model.map.get_mut(post).army.insert(Unit::Building(Building::TradingPost), 1);
    model.map.get_mut(post).is_destroyed = true;
    assert_eq!(visible_trading_post_owner(&model.map, 1, model.map.get(post)), None);
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
fn trading_posts_use_either_range_and_keep_each_senders_capacity() {
    let mut model = trading_game();
    let first = model.players[0].home_planet;
    let second = model.players[1].home_planet;
    model.map.get_mut(first).army.insert(Unit::Building(Building::TradingPost), 5);
    assert_eq!(trading_post_capacity(model.map.get(first), 1), 2_500);
    assert_eq!(trading_post_capacity(model.map.get(second), 2), 1_000);
    for (distance, expected) in [(3.0, true), (3.01, true), (7.5, true), (7.51, false)] {
        model.map.get_mut(second).position = bevy::math::Vec2::X * Planet::SIZE * distance;
        assert_eq!(trading_posts_are_adjacent(&model.map, 1, first, 2, second), expected);
        assert_eq!(trading_posts_are_adjacent(&model.map, 2, second, 1, first), expected);
    }
    model.map.get_mut(second).position = bevy::math::Vec2::X * Planet::SIZE * 6.0;
    assert_eq!(visible_trading_post_owner(&model.map, 1, model.map.get(second)), Some(2));
    assert_eq!(visible_trading_post_owner(&model.map, 2, model.map.get(first)), None);
    add_trade_agreement_immediately(
        &mut model,
        TradeAgreement {
            id: 93,
            turn: 1,
            parties: [
                TradeParty {
                    player_id: 1,
                    planet_id: first,
                    resources: Resources::new(2_000, 500, 0),
                },
                TradeParty {
                    player_id: 2,
                    planet_id: second,
                    resources: Resources::new(1_000, 0, 0),
                },
            ],
        },
    )
    .unwrap();
    assert_eq!(model.trades.len(), 1);
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
    let before = serde_json::to_value(&model).unwrap();
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
    assert_eq!(serde_json::to_value(&model).unwrap(), before);
}

fn add_owned_trading_post(model: &mut GameModel, level: usize) -> PlanetId {
    let planet_id = model
        .map
        .planets
        .iter()
        .find(|planet| !planet.is_moon() && planet.owned.is_none())
        .unwrap()
        .id;
    let planet = model.map.get_mut(planet_id);
    planet.owned = Some(1);
    planet.controlled = Some(1);
    planet.army.insert(Unit::Building(Building::TradingPost), level);
    planet_id
}

#[test]
fn each_owned_trading_post_can_hold_one_independent_resource_hub_loan() {
    let model = trading_game();
    let mut model = model;
    let first = model.players[0].home_planet;
    let second = add_owned_trading_post(&mut model, 1);
    let commands = vec![
        TurnCommand::BorrowResources {
            planet_id: first,
            resources: Resources::new(1_000, 250, 250),
            term: ResourceLoanTerm::Short,
        },
        TurnCommand::BorrowResources {
            planet_id: second,
            resources: Resources::new(100, 200, 200),
            term: ResourceLoanTerm::Long,
        },
    ];
    let projected = preview_commands(&model, 1, &commands).unwrap();
    assert_eq!(projected.resource_loans.len(), 2);
    assert_eq!(projected.players[0].resources, Resources::new(3_100, 2_450, 2_450));
    assert_eq!(projected.resource_loans[0].due_turn, 2);
    assert_eq!(projected.resource_loans[1].due_turn, 4);

    let duplicate = preview_commands(
        &model,
        1,
        &[
            commands[0].clone(),
            TurnCommand::BorrowResources {
                planet_id: first,
                resources: Resources::new(1, 0, 0),
                term: ResourceLoanTerm::Medium,
            },
        ],
    );
    assert!(duplicate.is_err());
}

#[test]
fn testing_boost_trading_post_can_borrow_in_the_same_draft() {
    let mut model = trading_game();
    let player_id = model.players[0].id;
    let other_player_id = model.players[1].id;
    let post = model.players[0].home_planet;
    model.map.get_mut(post).army.remove(&Unit::Building(Building::TradingPost));
    let commands = vec![
        TurnCommand::PracticeBoost,
        TurnCommand::BorrowResources {
            planet_id: post,
            resources: Resources::new(100, 200, 300),
            term: ResourceLoanTerm::Medium,
        },
    ];

    let projected = preview_commands(&model, player_id, &commands).unwrap();
    assert_eq!(
        projected.map.get(post).army.amount(&Unit::Building(Building::TradingPost)),
        Building::MAX_LEVEL
    );
    assert_eq!(projected.resource_loans.len(), 1);

    resolve_turn(
        &mut model,
        &[
            TurnSubmission::new(player_id, 1, commands),
            TurnSubmission::new(other_player_id, 1, Vec::new()),
        ],
    )
    .unwrap();
    assert_eq!(model.resource_loans.len(), 1);
    assert_eq!(model.resource_loans[0].planet_id, post);
}

#[test]
fn resource_hub_repayment_uses_per_resource_ceiling_and_fixed_term_premiums() {
    for (term, expected) in [
        (ResourceLoanTerm::Short, Resources::new(2, 11, 13)),
        (ResourceLoanTerm::Medium, Resources::new(2, 13, 15)),
        (ResourceLoanTerm::Long, Resources::new(2, 15, 17)),
    ] {
        let loan = ResourceLoan {
            player_id: 1,
            planet_id: 0,
            issued_turn: 1,
            due_turn: 1 + term.turns(),
            principal: Resources::new(1, 10, 11),
            term,
        };
        assert_eq!(loan.repayment(), expected);
    }
}

#[test]
fn due_resource_hub_loans_repay_after_production_and_unaffordable_loans_defer() {
    let mut model = trading_game();
    let post = model.players[0].home_planet;
    resolve_turn(
        &mut model,
        &[
            TurnSubmission::new(
                1,
                1,
                vec![TurnCommand::BorrowResources {
                    planet_id: post,
                    resources: Resources::new(100, 200, 300),
                    term: ResourceLoanTerm::Short,
                }],
            ),
            TurnSubmission::new(2, 1, Vec::new()),
        ],
    )
    .unwrap();
    assert_eq!(model.turn, 2);
    assert_eq!(model.players[0].resources, Resources::new(2_100, 2_200, 2_300));
    assert_eq!(model.resource_hub_repayment_due(1), Resources::new(110, 220, 330));
    assert!(!model.resource_hub_borrowing_blocked(1));

    model.players[0].resources = Resources::new(109, 10_000, 10_000);
    resolve_turn(
        &mut model,
        &[TurnSubmission::new(1, 2, Vec::new()), TurnSubmission::new(2, 2, Vec::new())],
    )
    .unwrap();
    assert_eq!(model.turn, 3);
    assert_eq!(model.players[0].resources, Resources::new(109, 10_000, 10_000));
    assert_eq!(model.resource_loans.len(), 1);
    assert_eq!(model.resource_hub_repayment_due(1), Resources::new(110, 220, 330));
    assert!(model.resource_hub_borrowing_blocked(1));

    let second_post = add_owned_trading_post(&mut model, 1);
    assert!(preview_commands(
        &model,
        1,
        &[TurnCommand::BorrowResources {
            planet_id: second_post,
            resources: Resources::new(100, 0, 0),
            term: ResourceLoanTerm::Short,
        }],
    )
    .is_err());

    let other_post = model.players[1].home_planet;
    let turn = model.turn;
    add_trade_agreement_immediately(
        &mut model,
        TradeAgreement {
            id: 99,
            turn,
            parties: [
                TradeParty {
                    player_id: 1,
                    planet_id: post,
                    resources: Resources::new(0, 1, 0),
                },
                TradeParty {
                    player_id: 2,
                    planet_id: other_post,
                    resources: Resources::new(1, 0, 0),
                },
            ],
        },
    )
    .unwrap();

    model.players[0].resources = Resources::new(110, 220, 330);
    model.trades.clear();
    resolve_turn(
        &mut model,
        &[TurnSubmission::new(1, 3, Vec::new()), TurnSubmission::new(2, 3, Vec::new())],
    )
    .unwrap();
    assert_eq!(model.players[0].resources, Resources::default());
    assert!(model.resource_loans.is_empty());
    assert!(!model.resource_hub_borrowing_blocked(1));
}

#[test]
fn simultaneous_due_loans_are_repaid_as_one_complete_empire_bundle() {
    let mut model = trading_game();
    let first = model.players[0].home_planet;
    let second = add_owned_trading_post(&mut model, 1);
    resolve_turn(
        &mut model,
        &[
            TurnSubmission::new(
                1,
                1,
                vec![
                    TurnCommand::BorrowResources {
                        planet_id: first,
                        resources: Resources::new(100, 0, 0),
                        term: ResourceLoanTerm::Short,
                    },
                    TurnCommand::BorrowResources {
                        planet_id: second,
                        resources: Resources::new(100, 0, 0),
                        term: ResourceLoanTerm::Short,
                    },
                ],
            ),
            TurnSubmission::new(2, 1, Vec::new()),
        ],
    )
    .unwrap();
    assert_eq!(model.resource_hub_repayment_due(1), Resources::new(220, 0, 0));

    model.players[0].resources = Resources::new(110, 0, 0);
    resolve_turn(
        &mut model,
        &[TurnSubmission::new(1, 2, Vec::new()), TurnSubmission::new(2, 2, Vec::new())],
    )
    .unwrap();
    assert_eq!(model.players[0].resources, Resources::new(110, 0, 0));
    assert_eq!(model.resource_loans.len(), 2);

    model.players[0].resources = Resources::new(220, 0, 0);
    resolve_turn(
        &mut model,
        &[TurnSubmission::new(1, 3, Vec::new()), TurnSubmission::new(2, 3, Vec::new())],
    )
    .unwrap();
    assert_eq!(model.players[0].resources, Resources::default());
    assert!(model.resource_loans.is_empty());
}

#[test]
fn early_resource_hub_repayment_requires_the_complete_bundle() {
    let mut model = trading_game();
    let post = model.players[0].home_planet;
    resolve_turn(
        &mut model,
        &[
            TurnSubmission::new(
                1,
                1,
                vec![TurnCommand::BorrowResources {
                    planet_id: post,
                    resources: Resources::new(100, 200, 300),
                    term: ResourceLoanTerm::Long,
                }],
            ),
            TurnSubmission::new(2, 1, Vec::new()),
        ],
    )
    .unwrap();
    let repayment = Resources::new(150, 300, 450);
    let before = model.clone();
    model.players[0].resources = Resources::new(149, 10_000, 10_000);
    assert!(preview_commands(
        &model,
        1,
        &[TurnCommand::RepayResourceLoanEarly {
            planet_id: post
        }],
    )
    .is_err());
    assert_eq!(model.resource_loans, before.resource_loans);

    model.players[0].resources = repayment;
    let projected = preview_commands(
        &model,
        1,
        &[TurnCommand::RepayResourceLoanEarly {
            planet_id: post,
        }],
    )
    .unwrap();
    assert_eq!(projected.players[0].resources, Resources::default());
    assert!(projected.resource_loans.is_empty());
}
