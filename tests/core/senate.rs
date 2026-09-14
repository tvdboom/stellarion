use super::*;
use crate::core::orders::OrderError;

fn senate_model(level: usize) -> (GameModel, PlanetId, PlanetId) {
    let mut model = started_model(2);
    let home = model.players[0].home_planet;
    let colony = model
        .map
        .planets
        .iter()
        .find(|planet| !planet.is_moon() && planet.owned.is_none())
        .unwrap()
        .id;
    model.map.get_mut(colony).colonize(1);
    model.map.get_mut(home).army.insert(Unit::Building(Building::Senate), level);
    for id in [home, colony] {
        let planet = model.map.get_mut(id);
        planet.army.insert(Unit::Building(Building::Shipyard), 1);
        planet.army.insert(Unit::Building(Building::Factory), 1);
    }
    model.players[0].resources = Resources::new(1_000_000, 1_000_000, 1_000_000);
    (model, home, colony)
}

fn select(home: PlanetId, policy: SenatePolicy) -> TurnCommand {
    TurnCommand::SetSenatePolicy {
        planet_id: home,
        policy,
    }
}

fn buy(planet_id: PlanetId, unit: Unit, count: usize) -> TurnCommand {
    TurnCommand::BuyUnits {
        planet_id,
        unit,
        count,
    }
}

#[test]
fn senate_production_scales_per_level_on_each_owned_planet_only() {
    let (mut model, home, colony) = senate_model(3);
    let expected_colony_limit = colony_limit(&model, 1).unwrap();
    let moon = model.map.moons()[0].id;
    model.map.get_mut(moon).controlled = Some(1);
    let enemy = model.players[1].home_planet;
    let outpost = model
        .map
        .planets
        .iter()
        .find(|planet| !planet.is_moon() && planet.owned.is_none())
        .unwrap()
        .id;
    model.map.get_mut(outpost).controlled = Some(1);

    for policy in SenatePolicy::ALL {
        apply_senate_policy(&mut model, 1, home, policy).unwrap();
        let support = model.players[0].senate_support(&model.map);
        for id in [home, colony] {
            let planet = model.map.get(id);
            assert_eq!(
                support.bonus(planet, policy),
                if policy == SenatePolicy::Expansion {
                    6
                } else {
                    15
                }
            );
            assert_eq!(
                support.fleet_capacity(planet),
                if policy == SenatePolicy::Expansion {
                    11
                } else {
                    5
                }
            );
            assert_eq!(
                support.defense_capacity(planet),
                if policy == SenatePolicy::Consolidation {
                    20
                } else {
                    5
                }
            );
        }
        for id in [moon, enemy, outpost] {
            assert_eq!(support.bonus(model.map.get(id), policy), 0);
        }
        assert_eq!(colony_limit(&model, 1).unwrap(), expected_colony_limit);
    }
    // Neither a queued upgrade nor a Senate on a different world contributes.
    model.map.get_mut(home).buy.push(Unit::Building(Building::Senate));
    model.map.get_mut(colony).army.insert(Unit::Building(Building::Senate), 5);
    assert_eq!(model.players[0].senate_support(&model.map).level, 3);
    model.map.get_mut(home).owned = None;
    assert_eq!(model.players[0].senate_support(&model.map).level, 0);
}

#[test]
fn senate_bonus_does_not_unlock_units_or_expand_missile_storage() {
    let (mut model, home, colony) = senate_model(3);
    let player = &model.players[0];
    let support = player.senate_support(&model.map);
    assert_eq!(
        purchase_limit(player, model.map.get(colony), Unit::Ship(Ship::LightFighter), 5, support),
        Ok(11)
    );
    assert_eq!(
        purchase_limit(player, model.map.get(colony), Unit::Ship(Ship::Cruiser), 5, support),
        Err(OrderError::Production)
    );
    apply_senate_policy(&mut model, 1, home, SenatePolicy::Consolidation).unwrap();
    let planet = model.map.get_mut(colony);
    planet.army.insert(Unit::Building(Building::MissileSilo), 1);
    let player = &model.players[0];
    let support = player.senate_support(&model.map);
    let planet = model.map.get(colony);
    assert_eq!(
        purchase_limit(player, planet, Unit::Defense(Defense::RocketLauncher), 5, support),
        Ok(20)
    );
    assert_eq!(
        purchase_limit(player, planet, Unit::Defense(Defense::PlasmaTurret), 5, support),
        Err(OrderError::Production)
    );
    assert_eq!(
        purchase_limit(player, planet, Unit::antiballistic_missile(), 5, support),
        Ok(planet.max_missile_capacity())
    );
}

#[test]
fn senate_policy_switches_cannot_spend_both_bonuses_on_any_owned_world() {
    let (model, home, colony) = senate_model(1);
    let ship = Unit::Ship(Ship::LightFighter);
    let defense = Unit::Defense(Defense::RocketLauncher);
    for id in [home, colony] {
        assert!(preview_commands(
            &model,
            1,
            &[buy(id, ship, 7), select(home, SenatePolicy::Consolidation)]
        )
        .is_err());
        assert!(preview_commands(
            &model,
            1,
            &[
                select(home, SenatePolicy::Consolidation),
                buy(id, defense, 7),
                select(home, SenatePolicy::Expansion)
            ]
        )
        .is_err());
        assert!(preview_commands(
            &model,
            1,
            &[select(home, SenatePolicy::Consolidation), buy(id, ship, 7)]
        )
        .is_err());
    }
    // Base ship production remains usable alongside the chosen defense bonus.
    let commands = vec![
        buy(colony, ship, 5),
        select(home, SenatePolicy::Consolidation),
        buy(colony, defense, 7),
    ];
    let before_ships = model.map.get(colony).army.amount(&ship);
    let before_defense = model.map.get(colony).army.amount(&defense);
    let mut resolved = model;
    resolve_turn(
        &mut resolved,
        &[TurnSubmission::new(1, 1, commands), TurnSubmission::new(2, 1, vec![])],
    )
    .unwrap();
    assert_eq!(resolved.map.get(colony).army.amount(&ship), before_ships + 5);
    assert_eq!(resolved.map.get(colony).army.amount(&defense), before_defense + 7);
}

#[test]
fn senate_choices_remain_editable_until_turn_end_then_lock_the_next_three_turns() {
    let (model, home, _) = senate_model(1);
    let mut model = preview_commands(
        &model,
        1,
        &[
            select(home, SenatePolicy::Consolidation),
            select(home, SenatePolicy::Expansion),
            select(home, SenatePolicy::Consolidation),
        ],
    )
    .unwrap();
    assert_eq!(model.map.get(home).operations.senate_locked_until, 0);
    assert!(model.map.get(home).operations.senate_selection_pending);
    model = PersistedGame::from_json(PersistedGame::new(model).to_json().unwrap()).unwrap().state;
    apply_senate_policy(&mut model, 1, home, SenatePolicy::Expansion).unwrap();
    apply_senate_policy(&mut model, 1, home, SenatePolicy::Consolidation).unwrap();
    let unlock = model.turn + 4;
    empty_turn(&mut model);
    model = PersistedGame::from_json(PersistedGame::new(model).to_json().unwrap()).unwrap().state;
    for _ in 0..3 {
        assert_eq!(model.map.get(home).operations.senate_locked_until, unlock);
        assert!(apply_senate_policy(&mut model, 1, home, SenatePolicy::Expansion).is_err());
        apply_senate_policy(&mut model, 1, home, SenatePolicy::Consolidation).unwrap();
        empty_turn(&mut model);
    }
    assert_eq!(model.turn, unlock);
    apply_senate_policy(&mut model, 1, home, SenatePolicy::Expansion).unwrap();
}

#[test]
fn senate_upgrades_contribute_only_after_they_complete() {
    let (mut model, home, colony) = senate_model(0);
    model.map.get_mut(home).buy.push(Unit::Building(Building::Senate));
    assert_eq!(
        model.players[0].senate_support(&model.map).fleet_capacity(model.map.get(colony)),
        5
    );
    assert!(apply_senate_policy(&mut model, 1, home, SenatePolicy::Consolidation).is_err());
    empty_turn(&mut model);
    let support = model.players[0].senate_support(&model.map);
    assert_eq!(support.level, 1);
    assert_eq!(support.policy, SenatePolicy::Expansion);
    assert_eq!(support.fleet_capacity(model.map.get(colony)), 7);
}

#[test]
fn senate_and_dock_choices_share_the_same_remaining_ship_capacity() {
    let (mut model, home, colony) = senate_model(1);
    model.map.get_mut(colony).army.insert(Unit::space_dock(), 1);
    let ship = Unit::Ship(Ship::LightFighter);
    let bastion = TurnCommand::SetSpaceDockMode {
        planet_id: colony,
        mode: SpaceDockMode::Bastion,
    };
    let draft = preview_commands(&model, 1, &[buy(colony, ship, 7), bastion.clone()]).unwrap();
    assert_eq!(draft.map.get(colony).operations.space_dock, SpaceDockMode::Bastion);
    assert!(preview_commands(&draft, 1, &[select(home, SenatePolicy::Consolidation)]).is_err());
    assert!(preview_commands(
        &model,
        1,
        &[buy(colony, ship, 7), select(home, SenatePolicy::Consolidation), bastion]
    )
    .is_err());
}

#[test]
fn senate_policy_orders_require_the_owned_home_world_and_valid_commitment_state() {
    let (mut model, home, colony) = senate_model(1);
    let policy = SenatePolicy::Consolidation;
    assert!(apply_senate_policy(&mut model, 2, home, policy).is_err());
    model.map.get_mut(colony).army.insert(Unit::Building(Building::Senate), 1);
    assert!(apply_senate_policy(&mut model, 1, colony, policy).is_err());
    model.map.get_mut(home).operations.senate_locked_until = model.turn + 3;
    model.map.get_mut(home).operations.senate_selection_pending = true;
    assert!(model.validate().is_err());
}
