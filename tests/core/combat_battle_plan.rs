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
        plan(CombatTactic::Siege, ProtectionPosture::ProtectBombers).resolved(&fighters).tactic,
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
fn same_owner_source_fleets_keep_independent_bonuses_and_cannot_lend_protection() {
    let mut fighter = combatant(1, 1, Unit::Ship(Ship::LightFighter));
    fighter.fleet_id = 11;
    fighter.battle_plan = plan(CombatTactic::Swarm, ProtectionPosture::ScreenCapitals);
    let mut bomber = combatant(2, 1, Unit::Ship(Ship::Bomber));
    bomber.fleet_id = 12;
    bomber.battle_plan = plan(CombatTactic::Siege, ProtectionPosture::ProtectBombers);
    let sources = vec![fighter, bomber];
    let mut enemy = combatant(3, 2, Unit::Ship(Ship::Bomber));
    enemy.fleet_id = 13;
    enemy.battle_plan = plan(CombatTactic::Siege, ProtectionPosture::Standard);
    let ours = RoundBattlePlans::from_units(&sources);
    let theirs = RoundBattlePlans::from_units(&[enemy.clone()]);
    assert_eq!(ours.commanders.len(), 2);
    assert_eq!(
        ours.damage_for_fleet(
            100,
            &sources[0],
            enemy.owner,
            enemy.fleet_id,
            Some(enemy.unit),
            &theirs,
            &mut BTreeMap::new()
        ),
        115
    );
    assert_eq!(
        ours.damage_for_fleet(
            100,
            &sources[1],
            enemy.owner,
            enemy.fleet_id,
            Some(enemy.unit),
            &theirs,
            &mut BTreeMap::new()
        ),
        100
    );
    let weights = planned_target_weights(
        Unit::Ship(Ship::Cruiser),
        TargetPriority::Standard,
        &sources,
        &ours,
    );
    assert!(weights.iter().all(|(_, weight)| *weight == 10_000));
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
fn suitability_uses_resource_value_and_excludes_noncombat_units() {
    let swarm = plan(CombatTactic::Swarm, ProtectionPosture::Standard);
    let mut army = Army::from([(Unit::Ship(Ship::LightFighter), 35), (Unit::war_sun(), 1)]);
    // 35 fighters and one War Sun each cost 1,750 resources, with affinities 100 and 0.
    assert_eq!(swarm.suitability(&army), 50);
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
    let source = RoundBattlePlans::new(&units, |owner| {
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
    let enemy = RoundBattlePlans::new(&enemies, |owner| {
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
        RoundBattlePlans::new(&units, |_| plan(CombatTactic::Swarm, ProtectionPosture::Standard));
    let enemies = vec![combatant(3, 3, Unit::Ship(Ship::Cruiser))];
    let enemy = RoundBattlePlans::new(&enemies, |_| BattlePlan::default());
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
    let source = RoundBattlePlans::new(&units, |_| {
        plan(CombatTactic::Breakthrough, ProtectionPosture::Standard)
    });
    let enemy = RoundBattlePlans::new(&enemies, |_| {
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
fn suitability_and_cover_change_only_at_the_next_round_boundary() {
    let mut units =
        vec![combatant(1, 1, Unit::Ship(Ship::LightFighter)), combatant(2, 1, Unit::war_sun())];
    let orders = plan(CombatTactic::Swarm, ProtectionPosture::ScreenCapitals);
    let start = RoundBattlePlans::new(&units, |_| orders);
    let before = start.commanders[&(Some(1), 0)].suitability;
    assert!(before > 0);
    assert!(start.commanders[&(Some(1), 0)].covering_value > 0);
    units[0].hull = 0;
    assert_eq!(start.commanders[&(Some(1), 0)].suitability, before);
    let next = RoundBattlePlans::new(&units, |_| orders);
    assert_eq!(next.commanders[&(Some(1), 0)].suitability, 0);
    assert_eq!(next.commanders[&(Some(1), 0)].covering_value, 0);
}

#[test]
fn default_target_selection_preserves_the_original_seeded_stream() {
    let original = vec![
        combatant(1, 2, Unit::probe()),
        combatant(2, 2, Unit::space_dock()),
        combatant(3, 2, Unit::Defense(Defense::RocketLauncher)),
    ];
    let orders = RoundBattlePlans::new(&original, |_| BattlePlan::default());
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
fn target_preferences_bias_two_to_one_and_keep_bomber_eligibility() {
    let mut targets = vec![
        combatant(1, 2, Unit::Ship(Ship::LightFighter)),
        combatant(2, 2, Unit::war_sun()),
        combatant(3, 2, Unit::Defense(Defense::RocketLauncher)),
    ];
    let orders = RoundBattlePlans::new(&targets, |_| BattlePlan::default());
    let cruiser = Unit::Ship(Ship::Cruiser);
    assert_eq!(
        planned_target_weights(cruiser, TargetPriority::SmallShips, &targets, &orders),
        vec![(0, 20_000), (1, 10_000)]
    );
    assert_eq!(
        planned_target_weights(cruiser, TargetPriority::Defenses, &targets, &orders),
        vec![(0, 10_000), (1, 10_000), (2, 20_000)]
    );
    for preference in TargetPriority::ALL {
        let weights =
            planned_target_weights(Unit::Ship(Ship::Bomber), preference, &targets, &orders);
        assert_eq!(weights.len(), 1);
        assert_eq!(weights[0].0, 2);
    }
    let mut rng = rand_chacha::ChaCha8Rng::from_seed([34; 32]);
    let small = (0..6_000)
        .filter(|_| {
            choose_planned_combat_target(
                cruiser,
                TargetPriority::SmallShips,
                &mut targets,
                &orders,
                &mut rng,
            )
            .unwrap()
            .id == 1
        })
        .count();
    assert!((3_800..4_200).contains(&small), "twice-weighted target selected {small} times");
    targets[0].unit = Unit::space_dock();
    assert_eq!(
        planned_target_weights(cruiser, TargetPriority::CapitalShips, &targets, &orders),
        vec![(0, 20_000), (1, 20_000)]
    );
}

#[test]
fn protection_is_bounded_preserves_total_exposure_and_needs_real_same_owner_screens() {
    let orders = plan(CombatTactic::Swarm, ProtectionPosture::ScreenCapitals);
    let mut targets = vec![combatant(1, 2, Unit::war_sun())];
    targets.extend((2..42).map(|id| combatant(id, 2, Unit::Ship(Ship::LightFighter))));
    let context = RoundBattlePlans::new(&targets, |_| orders);
    let weights = planned_target_weights(
        Unit::Ship(Ship::Cruiser),
        TargetPriority::Standard,
        &targets,
        &context,
    );
    assert_eq!(weights[0].1, 7_500);
    assert_eq!(weights.iter().map(|(_, weight)| *weight).sum::<u128>(), 410_000);
    assert!(weights[1..].iter().all(|(_, weight)| *weight > 10_000));
    let focused = planned_target_weights(
        Unit::Ship(Ship::Cruiser),
        TargetPriority::CapitalShips,
        &targets,
        &context,
    );
    assert_eq!(focused[0].1, 15_000, "cover cannot erase a twofold capital preference");
    assert_eq!(focused.iter().map(|(_, weight)| *weight).sum::<u128>(), 420_000);

    let lone_screen = &targets[..2];
    let context = RoundBattlePlans::new(lone_screen, |_| orders);
    let weights = planned_target_weights(
        Unit::Ship(Ship::Cruiser),
        TargetPriority::Standard,
        lone_screen,
        &context,
    );
    assert!(weights[0].1 > 9_900, "one inexpensive fighter cannot fully screen a War Sun");
    for unit in &mut targets[1..] {
        unit.owner = Some(3);
    }
    let context = RoundBattlePlans::new(&targets, |_| orders);
    assert!(planned_target_weights(
        Unit::Ship(Ship::Cruiser),
        TargetPriority::Standard,
        &targets,
        &context
    )
    .iter()
    .all(|(_, weight)| *weight == 10_000));
    for unit in &mut targets[1..] {
        unit.owner = Some(2);
        unit.unit = Unit::probe();
    }
    let context = RoundBattlePlans::new(&targets, |_| orders);
    assert!(planned_target_weights(
        Unit::Ship(Ship::Cruiser),
        TargetPriority::Standard,
        &targets,
        &context
    )
    .iter()
    .all(|(_, weight)| *weight == 10_000));
    assert!(ProtectionPosture::ALL.into_iter().all(|posture| !posture.covers(Unit::space_dock())
        && !posture.protects(Unit::space_dock())
        && !posture.covers(Unit::colony_ship())));
}

#[test]
fn every_nondefault_posture_transfers_exposure_to_its_covering_ships() {
    for (posture, protected, cover) in [
        (ProtectionPosture::ScreenCapitals, Unit::war_sun(), Unit::Ship(Ship::Destroyer)),
        (ProtectionPosture::HeavyCover, Unit::Ship(Ship::Cruiser), Unit::war_sun()),
        (ProtectionPosture::ProtectBombers, Unit::Ship(Ship::Bomber), Unit::Ship(Ship::Cruiser)),
    ] {
        let units = vec![combatant(1, 2, protected), combatant(2, 2, cover)];
        let context = RoundBattlePlans::new(&units, |_| plan(CombatTactic::Breakthrough, posture));
        let weights = planned_target_weights(
            Unit::Ship(Ship::Cruiser),
            TargetPriority::Standard,
            &units,
            &context,
        );
        assert!(weights[0].1 < 10_000 && weights[0].1 >= 7_500);
        assert!(weights[1].1 > 10_000);
        assert_eq!(weights.iter().map(|(_, weight)| *weight).sum::<u128>(), 20_000);
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
    destination.battle_plan = plan(CombatTactic::Bastion, ProtectionPosture::HeavyCover);
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
        target_priority: TargetPriority::Defenses,
        protection: ProtectionPosture::ProtectBombers,
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
