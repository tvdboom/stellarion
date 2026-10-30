use rand::SeedableRng;

use super::*;
use crate::core::combat::plan::{CombatTactic, ProtectionPosture};
use crate::core::units::defense::Defense;

fn combatant(id: u64, owner: u64, unit: Unit) -> CombatUnit {
    CombatUnit {
        fleet_id: 0,
        battle_plan: Default::default(),
        id,
        owner: Some(owner),
        unit,
        hull: unit.hull(),
        shield: unit.shield(),
        repairs: Vec::new(),
        shots: Vec::new(),
    }
}

fn plan(tactic: CombatTactic, protection: ProtectionPosture) -> BattlePlan {
    BattlePlan {
        automatic_tactic: false,
        tactic,
        protection,
        ..Default::default()
    }
}

#[test]
fn automatic_tactics_choose_highest_affinity_and_manual_choices_remain_fixed() {
    let fighters = Army::from([(Unit::Ship(Ship::LightFighter), 20)]);
    let defenses = Army::from([(Unit::Defense(Defense::PlasmaTurret), 3)]);
    assert_eq!(BattlePlan::default().resolved(&fighters).tactic, CombatTactic::Swarm);
    assert_eq!(BattlePlan::default().resolved(&defenses).tactic, CombatTactic::Bastion);
    assert_eq!(BattlePlan::default().resolved(&Army::new()).tactic, CombatTactic::Breakthrough);
    assert_eq!(
        plan(CombatTactic::Siege, ProtectionPosture::BattleLine).resolved(&fighters).tactic,
        CombatTactic::Siege
    );
    let mixed = Army::from([(Unit::Ship(Ship::LightFighter), 20), (Unit::Ship(Ship::Cruiser), 2)]);
    let best = BattlePlan::default().resolved(&mixed);
    assert!(CombatTactic::ALL.into_iter().all(|tactic| BattlePlan {
        automatic_tactic: false,
        tactic,
        ..Default::default()
    }
    .suitability(&mixed)
        <= best.suitability(&mixed)));
}

#[test]
fn automatic_intent_has_canonical_json_but_battle_orders_keep_the_actual_tactic() {
    let army = Army::from([(Unit::Ship(Ship::LightFighter), 5)]);
    let automatic = BattlePlan::default().resolved(&army);
    assert_eq!(automatic.tactic, CombatTactic::Swarm);
    assert_eq!(automatic, BattlePlan::default());
    let encoded = serde_json::to_value(automatic).unwrap();
    assert_eq!(encoded["tactic"], "Breakthrough");
    let restored: BattlePlan = serde_json::from_value(encoded).unwrap();
    assert_eq!(restored.resolved(&army).tactic, CombatTactic::Swarm);
    let frozen = automatic.frozen(&army);
    assert!(!frozen.automatic_tactic);
    assert_eq!(serde_json::to_value(frozen).unwrap()["tactic"], "Swarm");
}

#[test]
fn same_owner_source_fleets_share_composition_and_protection() {
    let mut fighter = combatant(1, 1, Unit::Ship(Ship::LightFighter));
    fighter.fleet_id = 11;
    fighter.battle_plan = plan(CombatTactic::Swarm, ProtectionPosture::Vanguard);
    let mut capital = combatant(2, 1, Unit::war_sun());
    capital.fleet_id = 12;
    capital.battle_plan = fighter.battle_plan;
    let sources = vec![fighter, capital];
    let ours = CombatBattlePlans::from_units(&sources);
    assert_eq!(ours.commanders.len(), 1);
    assert!(ours.commanders[&Some(1)].suitability < 100);
    let weights = planned_target_weights(
        Unit::Ship(Ship::Cruiser),
        TargetPriority::Standard,
        &sources,
        &ours,
    );
    assert!(weights[0].1 > 10_000, "fighters cover capitals from another source");
    assert!(weights[1].1 < 10_000);
    assert_eq!(weights.iter().map(|(_, weight)| weight).sum::<u128>(), 20_000);
    let mut allies = sources;
    allies[1].owner = Some(2);
    let separate = CombatBattlePlans::from_units(&allies);
    assert_eq!(separate.commanders.len(), 2);
    assert!(planned_target_weights(
        Unit::Ship(Ship::Cruiser),
        TargetPriority::Standard,
        &allies,
        &separate,
    )
    .iter()
    .all(|(_, weight)| *weight == 10_000));
}

#[test]
fn tactics_have_two_counters_two_losses_and_one_other_neutral() {
    for tactic in CombatTactic::ALL {
        assert!(!tactic.counters(tactic));
        let others =
            CombatTactic::ALL.into_iter().filter(|other| *other != tactic).collect::<Vec<_>>();
        assert_eq!(others.iter().filter(|other| tactic.counters(**other)).count(), 2);
        assert_eq!(others.iter().filter(|other| other.counters(tactic)).count(), 2);
        assert_eq!(
            others
                .iter()
                .filter(|other| !tactic.counters(**other) && !other.counters(tactic))
                .count(),
            1
        );
        assert!(others.iter().all(|other| !(tactic.counters(*other) && other.counters(tactic))));
    }
    assert!(CombatTactic::Siege.counters(CombatTactic::Bastion));
    assert!(CombatTactic::Interception.counters(CombatTactic::Swarm));
    assert!(!CombatTactic::Breakthrough.counters(CombatTactic::Bastion));
}

#[test]
fn suitability_uses_production_levels_and_excludes_noncombat_units() {
    let swarm = plan(CombatTactic::Swarm, ProtectionPosture::Standard);
    let mut army = Army::from([(Unit::Ship(Ship::LightFighter), 5), (Unit::war_sun(), 1)]);
    // Five level-1 fighters balance one level-5 War Sun, with affinities 100 and 0.
    assert_eq!(swarm.suitability(&army), 50);
    assert_eq!(
        swarm
            .suitability(&Army::from([(Unit::Ship(Ship::LightFighter), 35), (Unit::war_sun(), 1)])),
        87,
    );
    assert_eq!(
        plan(CombatTactic::Bastion, ProtectionPosture::Standard).suitability(&Army::from([
            (Unit::Defense(Defense::RocketLauncher), 1),
            (Unit::space_dock(), 1),
        ])),
        91,
    );
    for unit in [
        Unit::probe(),
        Unit::colony_ship(),
        Unit::crawler(),
        Unit::repair_truck(),
        Unit::planetary_shield(),
        Unit::interplanetary_missile(),
        Unit::Fauna(SpaceFauna::ElderStarDragon),
    ] {
        army.insert(unit, usize::MAX);
    }
    assert_eq!(swarm.suitability(&army), 50);
    assert_eq!(swarm.suitability(&Army::new()), 0);
    assert_eq!(swarm.suitability(&Army::from([(Unit::Ship(Ship::LightFighter), usize::MAX)])), 100);
}

#[test]
fn damage_matches_each_commander_and_never_penalizes_a_countered_tactic() {
    let units = vec![
        combatant(1, 1, Unit::Ship(Ship::LightFighter)),
        combatant(2, 2, Unit::Ship(Ship::Cruiser)),
    ];
    let source = CombatBattlePlans::new(&units, |owner| {
        plan(
            if owner == Some(1) {
                CombatTactic::Swarm
            } else {
                CombatTactic::Skirmish
            },
            ProtectionPosture::Standard,
        )
    });
    let enemies = vec![
        combatant(3, 3, Unit::Ship(Ship::Cruiser)),
        combatant(4, 4, Unit::Ship(Ship::Cruiser)),
    ];
    let enemy = CombatBattlePlans::new(&enemies, |owner| {
        plan(
            if owner == Some(3) {
                CombatTactic::Breakthrough
            } else {
                CombatTactic::Interception
            },
            ProtectionPosture::Standard,
        )
    });
    let mut carry = BTreeMap::new();
    assert_eq!(
        source.damage(100, &units[0], Some(3), Some(enemies[0].unit), &enemy, &mut carry),
        115
    );
    assert_eq!(
        source.damage(100, &units[0], Some(4), Some(enemies[1].unit), &enemy, &mut carry),
        100
    );
    assert_eq!(
        source.damage(100, &units[1], Some(3), Some(enemies[0].unit), &enemy, &mut carry),
        100
    );
    assert_eq!(source.damage(100, &units[0], None, Some(enemies[0].unit), &enemy, &mut carry), 100);
    assert_eq!(
        source.damage(
            100,
            &units[0],
            Some(3),
            Some(Unit::Fauna(SpaceFauna::ElderStarDragon)),
            &enemy,
            &mut carry,
        ),
        100
    );
    assert_eq!(
        source.damage(usize::MAX, &units[0], Some(3), Some(enemies[0].unit), &enemy, &mut carry),
        usize::MAX
    );
}

#[test]
fn small_shots_accumulate_their_earned_fraction_without_sharing_it() {
    let units = vec![
        combatant(1, 1, Unit::Ship(Ship::LightFighter)),
        combatant(2, 1, Unit::Ship(Ship::LightFighter)),
    ];
    let source =
        CombatBattlePlans::new(&units, |_| plan(CombatTactic::Swarm, ProtectionPosture::Standard));
    let enemies = vec![combatant(3, 3, Unit::Ship(Ship::Cruiser))];
    let enemy = CombatBattlePlans::new(&enemies, |_| BattlePlan::default());
    let mut carry = BTreeMap::new();
    assert_eq!(
        source.damage(10, &units[0], Some(3), Some(enemies[0].unit), &enemy, &mut carry),
        11
    );
    assert_eq!(
        source.damage(10, &units[1], Some(3), Some(enemies[0].unit), &enemy, &mut carry),
        11
    );
    assert_eq!(
        source.damage(10, &units[0], Some(3), Some(enemies[0].unit), &enemy, &mut carry),
        12
    );
    assert_eq!(carry[&units[0].id], 0);
    assert_eq!(carry[&units[1].id], 5_000);

    // 25% suitability still earns 3.75% extra damage, even though any single shot's
    // fractional bonus would otherwise be rounded down to zero.
    let source = CombatBattlePlans::new(&units, |_| {
        plan(CombatTactic::Breakthrough, ProtectionPosture::Standard)
    });
    let enemy = CombatBattlePlans::new(&enemies, |_| {
        plan(CombatTactic::Skirmish, ProtectionPosture::Standard)
    });
    let mut carry = BTreeMap::new();
    let damage = (0..8)
        .map(|_| source.damage(10, &units[0], Some(3), Some(enemies[0].unit), &enemy, &mut carry))
        .sum::<usize>();
    assert_eq!(damage, 83);
    assert_eq!(carry[&units[0].id], 0);
    // A neutral matchup neither earns nor spends a fraction left by a countered target.
    carry.insert(units[0].id, 7_500);
    assert_eq!(source.damage(10, &units[0], None, Some(enemies[0].unit), &enemy, &mut carry), 10);
    assert_eq!(carry[&units[0].id], 7_500);
}

#[test]
fn opening_affinity_and_damage_bonus_survive_casualties() {
    let mut units =
        vec![combatant(1, 1, Unit::Ship(Ship::LightFighter)), combatant(2, 1, Unit::war_sun())];
    let orders = plan(CombatTactic::Swarm, ProtectionPosture::Vanguard);
    let start = CombatBattlePlans::new(&units, |_| orders);
    let before = start.commanders[&Some(1)].suitability;
    assert!(before > 0);
    let enemies = vec![combatant(3, 2, Unit::Ship(Ship::Cruiser))];
    let enemy = CombatBattlePlans::new(&enemies, |_| {
        plan(CombatTactic::Breakthrough, ProtectionPosture::Standard)
    });
    let damage = start.damage(
        1_000,
        &units[1],
        Some(2),
        Some(enemies[0].unit),
        &enemy,
        &mut BTreeMap::new(),
    );
    units[0].hull = 0;
    units.retain(|unit| unit.hull > 0);
    assert_eq!(start.commanders[&Some(1)].suitability, before);
    assert_eq!(orders.suitability(&Army::from([(Unit::war_sun(), 1)])), 0);
    assert_eq!(
        start.damage(
            1_000,
            &units[0],
            Some(2),
            Some(enemies[0].unit),
            &enemy,
            &mut BTreeMap::new()
        ),
        damage,
    );
}

#[test]
fn combat_keeps_the_opening_bonus_after_fighters_are_destroyed() {
    let origin = Planet::new(0, "Origin".into(), Vec2::ZERO, false, 1.0);
    let mut destination = Planet::new(1, "Target".into(), Vec2::X, false, 1.0);
    destination.colonize(2);
    destination.army =
        Army::from([(Unit::Defense(Defense::GaussCannon), 4), (Unit::planetary_shield(), 10_000)])
            .into();
    destination.battle_plan = plan(CombatTactic::Breakthrough, ProtectionPosture::Standard);
    let mut mission = Mission::new_with_id(
        1,
        1,
        1,
        &origin,
        &destination,
        Icon::Attack,
        Army::from([(Unit::Ship(Ship::LightFighter), 1), (Unit::war_sun(), 1)]),
        BombingRaid::None,
        false,
        false,
        None,
    );
    mission.battle_plan = plan(CombatTactic::Swarm, ProtectionPosture::Standard);
    let report = resolve_combat_with_rng(
        1,
        &mission,
        &destination,
        &mut rand_chacha::ChaCha8Rng::from_seed([39; 32]),
    );
    let rounds = &report.combat_report.as_ref().unwrap().rounds;
    let casualty_round = rounds
        .iter()
        .position(|round| {
            round
                .attacker
                .iter()
                .any(|unit| unit.unit == Unit::Ship(Ship::LightFighter) && unit.hull == 0)
        })
        .expect("the fighter must be destroyed while the War Sun survives");
    let later_shots = rounds[casualty_round + 1..]
        .iter()
        .flat_map(|round| &round.attacker)
        .filter(|unit| unit.unit == Unit::war_sun())
        .flat_map(|unit| &unit.shots)
        .collect::<Vec<_>>();
    assert!(!later_shots.is_empty(), "combat must continue after the fighter is lost");
    // Opening affinity is 16%, so the 180-damage War Sun keeps its 2.4% bonus,
    // even though its own Swarm fit is zero. Fractional damage carries between shots.
    for shot in later_shots {
        assert_eq!(shot.unit, Some(Unit::planetary_shield()));
        assert!((184..=185).contains(&shot.planetary_shield_damage));
    }
}

#[test]
fn balanced_fire_matches_unweighted_selection_and_its_seeded_stream() {
    let original = vec![
        combatant(1, 2, Unit::probe()),
        combatant(2, 2, Unit::space_dock()),
        combatant(3, 2, Unit::Defense(Defense::RocketLauncher)),
    ];
    let orders = CombatBattlePlans::new(&original, |_| BattlePlan::default());
    for attacker in
        [Unit::Ship(Ship::Cruiser), Unit::Ship(Ship::Bomber), Unit::Defense(Defense::PlasmaTurret)]
    {
        let mut legacy = original.clone();
        let mut planned = original.clone();
        let mut legacy_rng = rand_chacha::ChaCha8Rng::from_seed([33; 32]);
        let mut planned_rng = legacy_rng.clone();
        for _ in 0..100 {
            assert_eq!(
                choose_combat_target(attacker, &mut legacy, &mut legacy_rng).unwrap().id,
                choose_planned_combat_target(
                    attacker,
                    TargetPriority::Standard,
                    &mut planned,
                    &orders,
                    &mut planned_rng
                )
                .unwrap()
                .id
            );
        }
        assert_eq!(legacy_rng.random::<u64>(), planned_rng.random::<u64>());
    }
}

#[test]
fn light_target_selection_favors_fighters_and_light_defenses_equally() {
    let mut targets = vec![
        combatant(1, 2, Unit::Ship(Ship::LightFighter)),
        combatant(2, 2, Unit::war_sun()),
        combatant(3, 2, Unit::Defense(Defense::RocketLauncher)),
    ];
    let orders = CombatBattlePlans::new(&targets, |_| BattlePlan::default());
    let cruiser = Unit::Ship(Ship::Cruiser);
    assert_eq!(
        planned_target_weights(cruiser, TargetPriority::LightTargets, &targets, &orders),
        vec![(0, 20_000), (1, 10_000), (2, 20_000)]
    );
    for preference in TargetPriority::ALL {
        let weights =
            planned_target_weights(Unit::Ship(Ship::Bomber), preference, &targets, &orders);
        assert_eq!(weights.len(), 1);
        assert_eq!(weights[0].0, 2);
    }
    let mut rng = rand_chacha::ChaCha8Rng::from_seed([34; 32]);
    let mut selected = [0; 3];
    for _ in 0..6_000 {
        let id = choose_planned_combat_target(
            cruiser,
            TargetPriority::LightTargets,
            &mut targets,
            &orders,
            &mut rng,
        )
        .unwrap()
        .id;
        selected[id as usize - 1] += 1;
    }
    assert!((2_250..2_550).contains(&selected[0]), "{selected:?}");
    assert!((1_050..1_350).contains(&selected[1]), "{selected:?}");
    assert!((2_250..2_550).contains(&selected[2]), "{selected:?}");
    targets[0].unit = Unit::space_dock();
    assert_eq!(
        planned_target_weights(cruiser, TargetPriority::HeavyTargets, &targets, &orders),
        vec![(0, 20_000), (1, 20_000), (2, 10_000)]
    );
}

#[test]
fn target_groups_weight_ships_and_defenses_in_one_pool_and_exclude_missiles() {
    let groups = [
        (
            TargetPriority::LightTargets,
            vec![
                Unit::Ship(Ship::LightFighter),
                Unit::Ship(Ship::HeavyFighter),
                Unit::Ship(Ship::Destroyer),
                Unit::Defense(Defense::Crawler),
                Unit::Defense(Defense::RepairTruck),
                Unit::Defense(Defense::RocketLauncher),
                Unit::Defense(Defense::LightLaser),
            ],
        ),
        (
            TargetPriority::MediumTargets,
            vec![
                Unit::Ship(Ship::Cruiser),
                Unit::Ship(Ship::Bomber),
                Unit::Ship(Ship::Battleship),
                Unit::Defense(Defense::HeavyLaser),
                Unit::Defense(Defense::GaussCannon),
            ],
        ),
        (
            TargetPriority::HeavyTargets,
            vec![
                Unit::Ship(Ship::Dreadnought),
                Unit::war_sun(),
                Unit::space_dock(),
                Unit::Defense(Defense::IonCannon),
                Unit::Defense(Defense::PlasmaTurret),
            ],
        ),
    ];
    let mut targets = groups
        .iter()
        .flat_map(|(_, units)| units)
        .enumerate()
        .map(|(index, unit)| combatant(index as u64, 2, *unit))
        .collect::<Vec<_>>();
    targets.extend([
        combatant(17, 2, Unit::probe()),
        combatant(18, 2, Unit::Defense(Defense::AntiballisticMissile)),
        combatant(19, 2, Unit::Defense(Defense::InterplanetaryMissile)),
    ]);
    let orders = CombatBattlePlans::new(&targets, |_| BattlePlan::default());
    for shooter in [Unit::Ship(Ship::Cruiser), Unit::Defense(Defense::PlasmaTurret)] {
        for priority in TargetPriority::ALL {
            let weights = planned_target_weights(shooter, priority, &targets, &orders);
            assert_eq!(weights.len(), 18);
            for (index, weight) in weights {
                let preferred = groups.iter().any(|(group, units)| {
                    *group == priority && units.contains(&targets[index].unit)
                });
                assert_eq!(
                    weight,
                    if preferred {
                        20_000
                    } else {
                        10_000
                    },
                    "{shooter:?}, {priority:?}, {:?}",
                    targets[index].unit
                );
            }
        }
    }
}

#[test]
fn bombers_weight_defenses_first_then_fall_back_to_weighted_ships() {
    let mut targets = vec![
        combatant(1, 2, Unit::Ship(Ship::LightFighter)),
        combatant(2, 2, Unit::Ship(Ship::Battleship)),
        combatant(3, 2, Unit::war_sun()),
        combatant(4, 2, Unit::Defense(Defense::RocketLauncher)),
        combatant(5, 2, Unit::Defense(Defense::GaussCannon)),
        combatant(6, 2, Unit::Defense(Defense::PlasmaTurret)),
    ];
    for priority in TargetPriority::ALL {
        let orders = CombatBattlePlans::new(&targets, |_| BattlePlan::default());
        let weights = planned_target_weights(Unit::Ship(Ship::Bomber), priority, &targets, &orders);
        assert_eq!(weights.len(), 3);
        assert!(weights.iter().all(|(index, _)| targets[*index].unit.is_defense()));
        let doubled = match priority {
            TargetPriority::Standard => None,
            TargetPriority::LightTargets => Some(3),
            TargetPriority::MediumTargets => Some(4),
            TargetPriority::HeavyTargets => Some(5),
        };
        for (index, weight) in weights {
            assert_eq!(
                weight,
                if Some(index) == doubled {
                    20_000
                } else {
                    10_000
                }
            );
        }
    }
    targets.truncate(3);
    let orders = CombatBattlePlans::new(&targets, |_| BattlePlan::default());
    assert_eq!(
        planned_target_weights(
            Unit::Ship(Ship::Bomber),
            TargetPriority::MediumTargets,
            &targets,
            &orders
        ),
        vec![(0, 10_000), (1, 20_000), (2, 10_000)]
    );
}

#[test]
fn formation_bias_is_flat_preserves_total_exposure_and_needs_other_ship_classes() {
    let orders = plan(CombatTactic::Swarm, ProtectionPosture::Vanguard);
    let mut targets =
        vec![combatant(1, 2, Unit::Ship(Ship::LightFighter)), combatant(2, 2, Unit::war_sun())];
    let context = CombatBattlePlans::new(&targets, |_| orders);
    let weights = planned_target_weights(
        Unit::Ship(Ship::Cruiser),
        TargetPriority::Standard,
        &targets,
        &context,
    );
    assert_eq!(weights, vec![(0, 11_111), (1, 8_889)]);
    assert_eq!(weights.iter().map(|(_, weight)| *weight).sum::<u128>(), 20_000);
    let focused = planned_target_weights(
        Unit::Ship(Ship::Cruiser),
        TargetPriority::LightTargets,
        &targets,
        &context,
    );
    assert_eq!(
        focused,
        vec![(0, 21_428), (1, 8_572)],
        "outgoing priority and incoming formation combine"
    );
    assert_eq!(focused.iter().map(|(_, weight)| *weight).sum::<u128>(), 30_000);

    targets[1].owner = Some(3);
    let context = CombatBattlePlans::new(&targets, |_| orders);
    assert!(planned_target_weights(
        Unit::Ship(Ship::Cruiser),
        TargetPriority::Standard,
        &targets,
        &context
    )
    .iter()
    .all(|(_, weight)| *weight == 10_000));
    targets[1].owner = Some(2);
    targets[1].unit = Unit::Ship(Ship::HeavyFighter);
    let context = CombatBattlePlans::new(&targets, |_| orders);
    assert!(planned_target_weights(
        Unit::Ship(Ship::Cruiser),
        TargetPriority::Standard,
        &targets,
        &context
    )
    .iter()
    .all(|(_, weight)| *weight == 10_000));
}

#[test]
fn every_formation_uses_its_ship_class_and_preserves_ground_support_and_allied_exposure() {
    let classes = [
        (Unit::Ship(Ship::LightFighter), ProtectionPosture::Vanguard),
        (Unit::Ship(Ship::HeavyFighter), ProtectionPosture::Vanguard),
        (Unit::Ship(Ship::Destroyer), ProtectionPosture::Vanguard),
        (Unit::Ship(Ship::Cruiser), ProtectionPosture::BattleLine),
        (Unit::Ship(Ship::Bomber), ProtectionPosture::BattleLine),
        (Unit::Ship(Ship::Battleship), ProtectionPosture::BattleLine),
        (Unit::Ship(Ship::Dreadnought), ProtectionPosture::Bulwark),
        (Unit::war_sun(), ProtectionPosture::Bulwark),
        (Unit::space_dock(), ProtectionPosture::Bulwark),
    ];
    let mut units = classes
        .iter()
        .enumerate()
        .map(|(index, (unit, _))| combatant(index as u64, 2, *unit))
        .collect::<Vec<_>>();
    units.extend([
        combatant(9, 2, Unit::Defense(Defense::Crawler)),
        combatant(10, 2, Unit::Defense(Defense::RepairTruck)),
        combatant(11, 2, Unit::Defense(Defense::RocketLauncher)),
        combatant(12, 2, Unit::Defense(Defense::LightLaser)),
        combatant(13, 2, Unit::Defense(Defense::HeavyLaser)),
        combatant(14, 2, Unit::Defense(Defense::GaussCannon)),
        combatant(15, 2, Unit::Defense(Defense::IonCannon)),
        combatant(16, 2, Unit::Defense(Defense::PlasmaTurret)),
        combatant(17, 2, Unit::probe()),
        combatant(18, 2, Unit::colony_ship()),
        combatant(19, 3, Unit::Ship(Ship::LightFighter)),
    ]);
    for posture in ProtectionPosture::ALL {
        let context = CombatBattlePlans::new(&units, |_| plan(CombatTactic::Breakthrough, posture));
        for priority in TargetPriority::ALL {
            let weights =
                planned_target_weights(Unit::Ship(Ship::Cruiser), priority, &units, &context);
            let mut original_total = 0;
            for (index, weight) in &weights {
                let baseline = if priority.prefers(units[*index].unit) {
                    20_000
                } else {
                    10_000
                };
                original_total += baseline;
                if *index >= classes.len() || posture == ProtectionPosture::Standard {
                    assert_eq!(
                        *weight, baseline,
                        "unchanged {:?} under {posture:?}/{priority:?}",
                        units[*index].unit
                    );
                } else if posture == classes[*index].1 {
                    assert!(
                        *weight > baseline && *weight <= baseline * 125 / 100,
                        "draws fire: {:?} under {posture:?}/{priority:?}",
                        units[*index].unit
                    );
                } else {
                    assert!(
                        *weight < baseline && *weight >= baseline * 80 / 100,
                        "covered: {:?} under {posture:?}/{priority:?}",
                        units[*index].unit
                    );
                }
            }
            assert_eq!(weights.iter().map(|(_, weight)| *weight).sum::<u128>(), original_total);
        }
    }
}

#[test]
fn battle_line_draws_fire_without_any_bombers() {
    let units = vec![
        combatant(1, 2, Unit::Ship(Ship::Battleship)),
        combatant(2, 2, Unit::Ship(Ship::Destroyer)),
    ];
    let orders = CombatBattlePlans::new(&units, |_| {
        plan(CombatTactic::Breakthrough, ProtectionPosture::BattleLine)
    });
    assert_eq!(
        planned_target_weights(
            Unit::Ship(Ship::Cruiser),
            TargetPriority::Standard,
            &units,
            &orders
        ),
        vec![(0, 11_111), (1, 8_889)]
    );
}

#[test]
fn formations_cannot_redirect_bomber_fire_onto_ineligible_ships() {
    let units = vec![
        combatant(1, 2, Unit::Ship(Ship::LightFighter)),
        combatant(2, 2, Unit::Ship(Ship::Battleship)),
        combatant(3, 2, Unit::war_sun()),
        combatant(4, 2, Unit::space_dock()),
        combatant(5, 2, Unit::Defense(Defense::RocketLauncher)),
    ];
    let open = CombatBattlePlans::new(&units, |_| BattlePlan::default());
    for posture in ProtectionPosture::ALL {
        let formed = CombatBattlePlans::new(&units, |_| plan(CombatTactic::Siege, posture));
        for priority in TargetPriority::ALL {
            assert_eq!(
                planned_target_weights(Unit::Ship(Ship::Bomber), priority, &units, &formed),
                planned_target_weights(Unit::Ship(Ship::Bomber), priority, &units, &open)
            );
        }
    }
}

#[test]
fn modified_plans_resolve_deterministically_and_respect_planetary_shields() {
    let mut origin = Planet::new(0, "Origin".into(), Vec2::ZERO, false, 1.0);
    origin.colonize(1);
    let mut destination = Planet::new(1, "Destination".into(), Vec2::X, false, 1.0);
    destination.colonize(2);
    destination.army = Army::from([
        (Unit::planetary_shield(), 5),
        (Unit::Ship(Ship::LightFighter), 15),
        (Unit::Defense(Defense::RocketLauncher), 20),
    ])
    .into();
    destination.battle_plan = plan(CombatTactic::Bastion, ProtectionPosture::Bulwark);
    let mut mission = Mission::new_with_id(
        1,
        1,
        1,
        &origin,
        &destination,
        Icon::Attack,
        Army::from([(Unit::Ship(Ship::Bomber), 5), (Unit::Ship(Ship::Cruiser), 12)]),
        BombingRaid::Industrial,
        false,
        false,
        None,
    );
    mission.battle_plan = BattlePlan {
        automatic_tactic: false,
        tactic: CombatTactic::Siege,
        target_priority: TargetPriority::LightTargets,
        protection: ProtectionPosture::BattleLine,
    };
    let report = resolve_combat_with_rng(
        1,
        &mission,
        &destination,
        &mut rand_chacha::ChaCha8Rng::from_seed([35; 32]),
    );
    let repeat = resolve_combat_with_rng(
        1,
        &mission,
        &destination,
        &mut rand_chacha::ChaCha8Rng::from_seed([35; 32]),
    );
    assert_eq!(serde_json::to_value(&report).unwrap(), serde_json::to_value(&repeat).unwrap());
    let mut shield = EnergyGrid::default().planetary_shield(5, false);
    let mut absorbed = 0;
    for round in &report.combat_report.as_ref().unwrap().rounds {
        for shot in round.attacker.iter().flat_map(|unit| &unit.shots) {
            shield = shield.saturating_sub(shot.planetary_shield_damage);
            absorbed += shot.planetary_shield_damage;
            if shot.unit.is_some_and(|unit| unit.is_defense() && unit != Unit::space_dock()) {
                assert_eq!(shield, 0);
            }
        }
        assert_eq!(shield, round.planetary_shield);
    }
    assert!(absorbed > 0);
}

#[test]
fn a_planetary_shield_retains_its_controllers_tactic_without_armed_defenders() {
    let mut origin = Planet::new(0, "Origin".into(), Vec2::ZERO, false, 1.0);
    origin.colonize(1);
    let mut destination = Planet::new(1, "Shield".into(), Vec2::X, false, 1.0);
    destination.colonize(2);
    destination.army = Army::from([(Unit::planetary_shield(), 5)]).into();
    destination.battle_plan = plan(CombatTactic::Bastion, ProtectionPosture::Standard);
    let mut mission = Mission::new_with_id(
        1,
        1,
        1,
        &origin,
        &destination,
        Icon::Attack,
        Army::from([(Unit::Ship(Ship::Bomber), 1)]),
        BombingRaid::Industrial,
        false,
        false,
        None,
    );
    mission.battle_plan = plan(CombatTactic::Skirmish, ProtectionPosture::Standard);
    let report = resolve_combat_with_rng(
        1,
        &mission,
        &destination,
        &mut rand_chacha::ChaCha8Rng::from_seed([36; 32]),
    );
    let combat = report.combat_report.unwrap();
    assert_eq!(combat.rounds[0].attacker[0].shots[0].planetary_shield_damage, 72);
}

#[test]
fn merged_automatic_attack_freezes_the_combined_tactic_for_every_source_ship() {
    let mut destination = Planet::new(1, "Target".into(), Vec2::X, false, 1.0);
    destination.controlled = Some(2);
    destination.army.insert(Unit::Defense(Defense::RocketLauncher), 1);
    destination.army.insert(Unit::planetary_shield(), 100);
    let mut mission = Mission {
        id: 1,
        owner: 1,
        objective: Icon::Attack,
        army: Army::from([(Unit::Ship(Ship::LightFighter), 4)]),
        ..Default::default()
    };
    mission.merge(&Mission {
        id: 2,
        owner: 1,
        objective: Icon::Attack,
        army: Army::from([(Unit::Ship(Ship::Bomber), 1)]),
        battle_plan: plan(CombatTactic::Bastion, ProtectionPosture::Bulwark),
        ..Default::default()
    });
    let report = resolve_combat_with_rng(
        1,
        &mission,
        &destination,
        &mut rand_chacha::ChaCha8Rng::from_seed([37; 32]),
    );
    let rounds = &report.combat_report.as_ref().unwrap().rounds;
    assert!(rounds[0].attacker.iter().any(|unit| unit.fleet_id == 1));
    assert!(rounds[0].attacker.iter().any(|unit| unit.fleet_id == 2));
    for unit in rounds.iter().flat_map(|round| &round.attacker) {
        assert!(!unit.battle_plan.automatic_tactic);
        assert_eq!(unit.battle_plan.tactic, CombatTactic::Swarm);
    }
}

#[test]
fn allied_attackers_and_protectors_keep_a_separate_plan_per_player_in_combat() {
    use crate::core::missions::{FleetCombatOrders, JointAttackMission};
    let fighters = Army::from([(Unit::Ship(Ship::LightFighter), 4)]);
    let bombers = Army::from([(Unit::Ship(Ship::Bomber), 1)]);
    let swarm = plan(CombatTactic::Swarm, ProtectionPosture::Standard);
    let siege = plan(CombatTactic::Siege, ProtectionPosture::BattleLine);
    let mut destination = Planet::new(1, "Target".into(), Vec2::X, false, 1.0);
    destination.controlled = Some(3);
    destination.army =
        Army::from([(Unit::Defense(Defense::RocketLauncher), 1), (Unit::planetary_shield(), 100)])
            .into();
    destination.battle_plan = plan(CombatTactic::Bastion, ProtectionPosture::Standard);
    destination.dock_protecting_fleet_with_battle_plan(4, fighters.clone(), swarm);
    let mission = Mission {
        id: 1,
        owner: 1,
        objective: Icon::Attack,
        army: fighters.iter().chain(bombers.iter()).map(|(unit, count)| (*unit, *count)).collect(),
        battle_plan: swarm,
        joint_attack: Some(JointAttackMission {
            attackers: BTreeMap::from([(1, fighters), (2, bombers)]),
            combat_orders: BTreeMap::from([
                (
                    1,
                    FleetCombatOrders {
                        battle_plan: swarm,
                        ..Default::default()
                    },
                ),
                (
                    2,
                    FleetCombatOrders {
                        battle_plan: siege,
                        ..Default::default()
                    },
                ),
            ]),
            ..Default::default()
        }),
        ..Default::default()
    };
    let report = resolve_combat_with_rng(
        1,
        &mission,
        &destination,
        &mut rand_chacha::ChaCha8Rng::from_seed([38; 32]),
    );
    let round = &report.combat_report.as_ref().unwrap().rounds[0];
    for unit in round.attacker.iter().chain(&round.defender) {
        let expected = match unit.owner.unwrap() {
            1 | 4 => CombatTactic::Swarm,
            2 => CombatTactic::Siege,
            3 => CombatTactic::Bastion,
            owner => panic!("unexpected commander {owner}"),
        };
        assert_eq!(unit.battle_plan.tactic, expected);
    }
    assert_eq!(report.battle_plan_fleets(&Side::Attacker).len(), 2);
    assert_eq!(report.battle_plan_fleets(&Side::Defender).len(), 2);
}

#[test]
fn battle_plan_json_rejects_unknown_fields_and_invalid_choices() {
    let encoded = serde_json::to_value(BattlePlan::default()).unwrap();
    assert_eq!(encoded["tactic"], "Breakthrough");
    assert_eq!(
        serde_json::from_value::<BattlePlan>(encoded.clone()).unwrap(),
        BattlePlan::default()
    );
    let mut invalid = encoded.clone();
    invalid["tactic"] = "Unknown".into();
    assert!(serde_json::from_value::<BattlePlan>(invalid).is_err());
    let mut extra = encoded;
    extra["bonus"] = 100.into();
    assert!(serde_json::from_value::<BattlePlan>(extra).is_err());
}
