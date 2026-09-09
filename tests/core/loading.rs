use super::*;

use crate::core::combat::report::{CombatReport, MissionReport};
use crate::core::map::icon::Icon;
use crate::core::messages::MessageLevel;
use crate::core::missions::Mission;
use crate::core::simulation::{GameModel, GameRules, OrbitalStrike};
use crate::core::units::buildings::Building;
use crate::multiplayer::client::PendingTurnCommands;

#[test]
/// The loading lifecycle distinguishes deferred, in-flight, and ready groups.
fn loading_state_has_explicit_transitions() {
    assert_ne!(GameplayAssetState::Deferred, GameplayAssetState::Loading);
    assert_ne!(GameplayAssetState::Loading, GameplayAssetState::Ready);
    assert_ne!(GameplayAssetState::Loading, GameplayAssetState::Failed);
}

#[test]
fn a_new_turn_keeps_its_public_railgun_strike_for_playback() {
    let mut model = GameModel::new([44; 32], GameRules::default()).unwrap();
    model.start().unwrap();
    let origin = model.players[0].home_planet;
    let target = model.players[1].home_planet;
    model.orbital_strikes.push(OrbitalStrike {
        turn: model.turn,
        origins: vec![origin],
        target,
        chance_basis_points: 350,
        destroyed: false,
    });

    let mut pending = PendingTurnCommands::default();
    pending.reset(model.turn);
    let preview = gameplay_draft_projection(&model, model.players[0].id, &pending);

    assert!(preview.is_none(), "an empty new-turn draft must not clear a resolved strike");
    assert_eq!(preview.as_ref().unwrap_or(&model).orbital_strikes, model.orbital_strikes);
}

#[test]
fn newly_resolved_railgun_strike_warns_everyone_except_its_shooters_once() {
    let mut model = GameModel::new(
        [45; 32],
        GameRules {
            player_count: 3,
            ..default()
        },
    )
    .unwrap();
    model.start().unwrap();
    let shooter = model.players[0].id;
    let target_player = model.players[1].id;
    let observer = model.players[2].id;
    let origin = model.players[0].home_planet;
    let target = model.players[1].home_planet;
    let previous = model.map.clone();
    model.orbital_strikes.push(OrbitalStrike {
        turn: model.turn,
        origins: vec![origin],
        target,
        chance_basis_points: 500,
        destroyed: false,
    });
    let prior_turn = model.turn.saturating_sub(1);

    for player in [target_player, observer] {
        let notifications =
            orbital_strike_notifications(Some(&previous), &model, player, prior_turn);
        assert_eq!(notifications.len(), 1);
        assert_eq!(notifications[0].level, MessageLevel::Warning);
        assert_eq!(
            notifications[0].message,
            format!("Railgun shot fired on planet {}.", model.map.get(target).name)
        );
        assert_eq!(notifications[0].action, Some(MessageAction::FocusRailgunTarget(target)));
    }

    assert!(
        orbital_strike_notifications(Some(&previous), &model, shooter, prior_turn).is_empty(),
        "the firing player already received immediate order feedback"
    );
    assert!(
        orbital_strike_notifications(Some(&previous), &model, observer, model.turn).is_empty(),
        "refreshing the same canonical turn must not duplicate the warning"
    );
    assert!(orbital_strike_notifications(None, &model, observer, prior_turn).is_empty());
}

#[test]
fn destroyed_moon_railgun_warning_remains_focusable() {
    let mut model = GameModel::new([46; 32], GameRules::default()).unwrap();
    model.start().unwrap();
    let observer = model.players[1].id;
    let origin = model.players[0].home_planet;
    let target = model
        .map
        .planets
        .iter()
        .find(|planet| planet.is_moon())
        .map(|planet| planet.id)
        .expect("the generated map should contain a moon");
    let previous = model.map.clone();
    model.map.get_mut(target).is_destroyed = true;
    model.orbital_strikes.push(OrbitalStrike {
        turn: model.turn,
        origins: vec![origin],
        target,
        chance_basis_points: 500,
        destroyed: true,
    });

    let notifications = orbital_strike_notifications(
        Some(&previous),
        &model,
        observer,
        model.turn.saturating_sub(1),
    );

    assert_eq!(notifications.len(), 1);
    assert!(notifications[0].message.starts_with("Railgun shot fired on moon "));
    assert_eq!(notifications[0].action, Some(MessageAction::FocusRailgunTarget(target)));
}

#[test]
fn newly_completed_enemy_strategic_structures_create_focusable_notifications_once() {
    let mut model = GameModel::new(
        [31; 32],
        GameRules {
            player_count: 3,
            ..default()
        },
    )
    .unwrap();
    model.start().unwrap();
    let local_player_id = model.players[0].id;
    let enemy_planet = model.players[1].home_planet;
    let previous = model.map.clone();

    model.map.get_mut(enemy_planet).army.insert(Unit::space_dock(), 1);
    model.map.get_mut(enemy_planet).army.insert(Unit::Building(Building::OrbitalRailgun), 1);

    let notifications = public_structure_notifications(Some(&previous), &model, local_player_id);
    assert_eq!(notifications.len(), 2);
    assert!(notifications.iter().all(|(change, _)| {
        change.planet == enemy_planet
            && change.change == PublicStructureChange::Built
            && change.owner == model.players[1].id
    }));
    assert_eq!(
        notifications[0].1.message,
        format!("A Space Dock has been built on planet {}.", model.map.get(enemy_planet).name)
    );
    assert_eq!(notifications[0].1.action, Some(MessageAction::FocusPlanet(enemy_planet)));
    assert_eq!(
        notifications[1].1.message,
        format!(
            "An Orbital Railgun has been built on planet {}.",
            model.map.get(enemy_planet).name
        )
    );

    assert!(public_structure_notifications(Some(&model.map), &model, local_player_id).is_empty());
    assert!(public_structure_notifications(None, &model, local_player_id).is_empty());
    assert!(public_structure_notifications(Some(&previous), &model, model.players[1].id).is_empty());
}

#[test]
fn battle_destruction_notifies_only_nonparticipants_for_each_public_structure() {
    let mut model = GameModel::new(
        [32; 32],
        GameRules {
            player_count: 3,
            ..default()
        },
    )
    .unwrap();
    model.start().unwrap();
    let attacker = model.players[0].id;
    let defender = model.players[1].id;
    let observer = model.players[2].id;
    let target = model.players[1].home_planet;
    model.map.get_mut(target).army.insert(Unit::space_dock(), 1);
    model.map.get_mut(target).army.insert(Unit::Building(Building::OrbitalRailgun), 1);
    let previous = model.map.clone();
    model.map.get_mut(target).army.remove(&Unit::space_dock());
    model.map.get_mut(target).army.remove(&Unit::Building(Building::OrbitalRailgun));

    let report = MissionReport {
        id: 91,
        turn: usize::try_from(model.turn).unwrap(),
        mission: Mission {
            owner: attacker,
            destination: target,
            objective: Icon::Attack,
            ..default()
        },
        planet: previous.get(target).clone(),
        scout_probes: 0,
        surviving_attacker: Default::default(),
        surviving_defender: model.map.get(target).army.clone(),
        planet_colonized: false,
        planet_destroyed: false,
        destination_owned: model.map.get(target).owned,
        destination_controlled: model.map.get(target).controlled,
        combat_report: Some(CombatReport::default()),
        hidden: false,
    };
    model.player_mut(attacker).unwrap().push_report(report.clone());
    model.player_mut(defender).unwrap().push_report(report);

    let notifications = public_structure_notifications(Some(&previous), &model, observer);
    assert_eq!(notifications.len(), 2);
    assert!(notifications
        .iter()
        .all(|(change, _)| change.change == PublicStructureChange::Destroyed));
    assert_eq!(
        notifications[0].1.message,
        format!("A Space Dock has been destroyed on planet {}.", model.map.get(target).name)
    );
    assert_eq!(
        notifications[1].1.message,
        format!("An Orbital Railgun has been destroyed on planet {}.", model.map.get(target).name)
    );
    assert!(public_structure_notifications(Some(&previous), &model, attacker).is_empty());
    assert!(public_structure_notifications(Some(&previous), &model, defender).is_empty());

    for player in &mut model.players {
        player.reports.clear();
    }
    assert!(
        public_structure_notifications(Some(&previous), &model, observer).is_empty(),
        "a structure disappearing outside combat is not mislabeled as battle destruction"
    );
}
