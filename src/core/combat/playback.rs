//! Playback shortcuts and early completion; recorded reports remain authoritative.

use std::collections::HashMap;

use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;
use bevy_tweening::TweenAnim;

use super::effects::{PendingImpact, Wreck};
use super::report::{MissionReport, Side};
use super::systems::{
    restore_combat_camera, setup_combat, BackgroundImageCmp, CombatCmp, CombatFormationState,
    CombatUnitCmp, FireState, IndividualCombatUnitCmp, SpawnShotMsg,
};
use crate::core::assets::WorldAssets;
use crate::core::audio::{MuteAudioMsg, PlayAudioMsg};
use crate::core::map::icon::Icon;
use crate::core::player::Player;
use crate::core::settings::Settings;
use crate::core::states::CombatState;
use crate::core::ui::systems::UiState;
use crate::core::units::{Amount, Unit};

#[derive(Component)]
pub(crate) struct CombatCardHome(pub Vec3);

#[derive(Resource)]
/// Requests a short round banner after a playback shortcut.
pub struct CombatRoundJump;

fn combatant(unit: Unit) -> bool {
    !unit.is_building() && !unit.is_missile() && unit != Unit::colony_ship()
}

fn conclusion_phase(report: &MissionReport) -> CombatState {
    if report.defender_salvage() == default() {
        CombatState::EndCombat
    } else {
        CombatState::Salvage
    }
}

/// Returns the presentation boundary for a round restored by backward navigation.
fn replay_phase(index: usize) -> CombatState {
    if index == 0 {
        // The first round owns the mission fly-in. Re-enter Setup so no weapon can be selected
        // until those restored entry tweens finish, exactly as on initial playback.
        CombatState::Setup
    } else {
        CombatState::DisplayRound
    }
}

/// Returns whether backward navigation should restart the selected round before moving earlier.
fn current_round_has_started(world: &mut World, phase: CombatState) -> bool {
    match phase {
        CombatState::Setup | CombatState::DisplayRound => false,
        CombatState::AntiBallistic | CombatState::Fire => {
            world
                .query::<&CombatUnitCmp>()
                .iter(world)
                .any(|card| !matches!(card.fire, FireState::Idle) || card.outcome_visible)
                || world
                    .query_filtered::<(), Or<(With<PendingImpact>, With<Wreck>)>>()
                    .iter(world)
                    .next()
                    .is_some()
        },
        CombatState::Repair
        | CombatState::Bomb
        | CombatState::DeathRay
        | CombatState::Salvage
        | CombatState::EndCombat => true,
    }
}

/// Reconstructs a card from a round boundary, including hull retained from prior rounds.
fn snapshot_card(
    report: &MissionReport,
    index: usize,
    finished: bool,
    unit: Unit,
    side: Side,
) -> Option<CombatUnitCmp> {
    if unit == Unit::colony_ship() {
        return None;
    }
    let combat = report.combat_report.as_ref()?;
    let round = combat.rounds.get(index)?;
    let previous = index.checked_sub(1).and_then(|i| combat.rounds.get(i));
    if side == Side::Defender && unit.is_ship() {
        if let Some(retreat) =
            combat.defender_retreat.as_ref().filter(|retreat| retreat.ships.amount(&unit) > 0)
        {
            let departure = retreat.after_round.unwrap_or(0);
            if index > departure || (finished && index >= departure) {
                return None;
            }
            if retreat.after_round.is_none() {
                let count = retreat.ships.amount(&unit);
                return Some(CombatUnitCmp {
                    unit,
                    side: side.clone(),
                    hull: count * report.unit_hull(unit, &side).max(1),
                    max_hull: count * report.unit_hull(unit, &side).max(1),
                    shield: count * report.unit_shield(unit, &side),
                    max_shield: count * report.unit_shield(unit, &side),
                    fire: FireState::Idle,
                    outcome_visible: false,
                });
            }
        }
    }
    let (hull, max_hull, shield, max_shield) = if unit.is_building() {
        let boundary = if finished {
            Some(round)
        } else {
            previous
        };
        let count = boundary.map_or_else(
            || report.planet.army.amount(&unit),
            |snapshot| snapshot.buildings.amount(&unit),
        );
        let shield = if unit == Unit::planetary_shield() {
            boundary.map_or_else(
                || report.initial_planetary_shield(),
                |snapshot| snapshot.planetary_shield,
            )
        } else {
            0
        };
        if unit == Unit::planetary_shield() && shield == 0 {
            return None;
        }
        (
            count,
            count,
            shield,
            if unit == Unit::planetary_shield() {
                report.initial_planetary_shield()
            } else {
                0
            },
        )
    } else {
        // IDs survive casualties and reordering; index once instead of scanning per survivor.
        let previous_hulls: HashMap<_, _> = previous
            .filter(|_| !finished)
            .into_iter()
            .flat_map(|snapshot| snapshot.units(&side))
            .map(|record| (record.id, record.hull))
            .collect();
        let (count, hull, shield) = round
            .units(&side)
            .iter()
            .filter(|record| record.unit == unit)
            .fold((0, 0, 0), |(count, hull, shield), record| {
                let (unit_hull, unit_shield) = if finished {
                    (record.hull, record.shield)
                } else {
                    (
                        previous_hulls
                            .get(&record.id)
                            .copied()
                            .unwrap_or_else(|| report.unit_hull(unit, &side)),
                        report.unit_shield(unit, &side),
                    )
                };
                (count + 1, hull + unit_hull, shield + unit_shield)
            });
        (
            hull,
            count * report.unit_hull(unit, &side),
            shield,
            count * report.unit_shield(unit, &side),
        )
    };
    (hull > 0 || unit.is_missile()).then_some(CombatUnitCmp {
        unit,
        side,
        hull,
        max_hull,
        shield,
        max_shield,
        fire: if finished {
            FireState::Fired
        } else {
            FireState::Idle
        },
        outcome_visible: finished,
    })
}

/// Rebuilds presentation from saved data so backward jumps also restore destroyed cards.
fn seek(
    world: &mut World,
    report: &MissionReport,
    index: usize,
    finished: bool,
    ending_phase: CombatState,
    preserved_individual_positions: Option<&HashMap<(bool, u64), Vec3>>,
) {
    if let Err(error) = world.run_system_once(restore_combat_camera) {
        warn!("Could not restore combat camera: {error}");
    }
    let entities =
        world.query_filtered::<Entity, With<CombatCmp>>().iter(world).collect::<Vec<_>>();
    for entity in entities {
        if world.get_entity(entity).is_ok() {
            world.despawn(entity);
        }
    }
    world.resource_mut::<Messages<SpawnShotMsg>>().clear();
    world.resource_mut::<UiState>().combat_round = index;
    if let Err(error) = world.run_system_once(setup_combat) {
        warn!("Could not rebuild combat playback: {error}");
        return;
    }
    let cards = world
        .query::<(Entity, &CombatUnitCmp, &CombatCardHome)>()
        .iter(world)
        .map(|(entity, card, home)| (entity, card.unit, card.side.clone(), home.0))
        .collect::<Vec<_>>();
    for (entity, unit, side, position) in cards {
        let card = snapshot_card(report, index, finished, unit, side.clone()).or_else(|| {
            (ending_phase == CombatState::Bomb
                && unit.is_building()
                && unit != Unit::planetary_shield())
            .then_some(CombatUnitCmp {
                unit,
                side,
                hull: 0,
                max_hull: 0,
                shield: 0,
                max_shield: 0,
                fire: FireState::Fired,
                outcome_visible: true,
            })
        });
        if let Some(mut card) = card {
            if (ending_phase == CombatState::AntiBallistic
                && card.unit == Unit::antiballistic_missile())
                || (card.side == Side::Attacker
                    && ((ending_phase == CombatState::DeathRay && card.unit == Unit::war_sun())
                        || (ending_phase == CombatState::Bomb
                            && card.unit == Unit::Ship(crate::core::units::ships::Ship::Bomber))))
            {
                card.fire = FireState::Select;
            }
            if ending_phase == CombatState::Bomb
                && card.unit.is_building()
                && card.unit != Unit::planetary_shield()
            {
                let losses = report.combat_report.as_ref().map_or(0, |combat| {
                    combat.rounds[index]
                        .attacker
                        .iter()
                        .flat_map(|unit| &unit.shots)
                        .filter(|shot| shot.unit == Some(card.unit) && shot.killed && !shot.missed)
                        .count()
                });
                card.hull = card.hull.saturating_add(losses);
                card.max_hull = card.hull;
            }
            if card.hull == 0 && !card.unit.is_missile() {
                world.despawn(entity);
                continue;
            }
            if ending_phase == CombatState::Setup {
                // Round one replays the mission entry. Keep the transform and tween created by
                // setup_combat so Setup can gate weapon selection on their completion.
                world.entity_mut(entity).insert(card);
            } else {
                world
                    .entity_mut(entity)
                    .insert((card, Transform::from_translation(position)))
                    .remove::<TweenAnim>();
            }
        } else {
            world.despawn(entity);
        }
    }
    if finished {
        if let Some(round) =
            report.combat_report.as_ref().and_then(|combat| combat.rounds.get(index))
        {
            let individuals = world
                .query::<(Entity, &IndividualCombatUnitCmp)>()
                .iter(world)
                .map(|(entity, card)| {
                    let key = card.id.map(|id| (card.side == Side::Defender, id));
                    let home = key
                        .and_then(|key| preserved_individual_positions?.get(&key).copied())
                        .unwrap_or_else(|| card.home());
                    (entity, card.id, card.side.clone(), home)
                })
                .collect::<Vec<_>>();
            for (entity, id, side, home) in individuals {
                world
                    .entity_mut(entity)
                    .insert(Transform::from_translation(home))
                    .remove::<TweenAnim>();
                let Some(id) = id else {
                    world.despawn(entity);
                    continue;
                };
                let Some(record) = round.units(&side).iter().find(|record| record.id == id) else {
                    world.despawn(entity);
                    continue;
                };
                if record.hull == 0 && !record.unit.is_missile() {
                    world.despawn(entity);
                    continue;
                }
                if let Some(mut card) = world.get_mut::<IndividualCombatUnitCmp>(entity) {
                    card.hull = record.hull;
                    card.shield = record.shield;
                }
            }
        }
    }
    if ending_phase == CombatState::EndCombat && report.planet_destroyed {
        let image = world.resource::<WorldAssets>().image("destroyed bg");
        for mut sprite in
            world.query_filtered::<&mut Sprite, With<BackgroundImageCmp>>().iter_mut(world)
        {
            sprite.image = image.clone();
        }
    }
    // Stop stale shots and suppress the entry horn when navigating within a replay.
    world.resource_mut::<Messages<PlayAudioMsg>>().clear();
    world.resource_mut::<Messages<MuteAudioMsg>>().write(MuteAudioMsg);
    world.insert_resource(CombatRoundJump);
    world.resource_mut::<NextState<CombatState>>().set(ending_phase);
}

/// Handles Ctrl+Shift+arrows and ends weapon playback when a combat army is eliminated.
pub fn control_combat_playback(world: &mut World) {
    let index = world.resource::<UiState>().combat_round;
    let phase = *world.resource::<State<CombatState>>().get();
    let shortcut = world.get_resource::<ButtonInput<KeyCode>>().and_then(|keys| {
        if !keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight])
            || !keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight])
        {
            return None;
        }
        match (keys.just_pressed(KeyCode::ArrowLeft), keys.just_pressed(KeyCode::ArrowRight)) {
            (true, false) => Some(false),
            (false, true) if phase != CombatState::EndCombat => Some(true),
            _ => None,
        }
    });
    let active = matches!(phase, CombatState::Fire | CombatState::Repair);
    if shortcut.is_none() && (!active || world.resource::<Settings>().combat_paused) {
        return;
    }
    let rewind_current = shortcut == Some(false) && current_round_has_started(world, phase);
    let mut alive = [false; 2];
    let mut destroyed_card = false;
    for card in world.query::<&CombatUnitCmp>().iter(world) {
        if combatant(card.unit) {
            if card.hull > 0 {
                alive[usize::from(card.side == Side::Defender)] = true;
            } else {
                destroyed_card = true;
            }
        }
    }
    let unresolved_effects = world
        .query_filtered::<(), Or<(With<PendingImpact>, With<Wreck>)>>()
        .iter(world)
        .next()
        .is_some();
    let state = world.resource::<UiState>();
    let Some(report) = state
        .in_combat
        .and_then(|id| world.resource::<Player>().reports.iter().find(|r| r.id == id))
    else {
        return;
    };
    let Some(combat) = report.combat_report.as_ref() else {
        return;
    };
    let Some(last) = combat.rounds.len().checked_sub(1) else {
        return;
    };
    let destination = if let Some(forward) = shortcut {
        if forward {
            (
                index.saturating_add(1).min(last),
                index >= last,
                if index >= last {
                    if phase == CombatState::Salvage {
                        CombatState::EndCombat
                    } else {
                        conclusion_phase(report)
                    }
                } else {
                    CombatState::DisplayRound
                },
            )
        } else {
            let destination = if rewind_current {
                index
            } else {
                index.saturating_sub(1)
            };
            (destination, false, replay_phase(destination))
        }
    } else {
        // Withdrawal has its own recorded departure boundary. Early-completion shortcuts could
        // otherwise skip the flyaway or leave the last volley before every projectile arrives.
        if combat.defender_retreat.is_some() {
            return;
        }
        // Empty, unguarded planets must still play bombing/destruction missions.
        let defending_army = report.planet.army.combined();
        let started_with_both = [&report.mission.army, &defending_army]
            .into_iter()
            .all(|army| army.iter().any(|(unit, count)| *count > 0 && combatant(*unit)));
        if !started_with_both
            || alive.into_iter().all(|present| present)
            || report.mission.objective == Icon::MissileStrike
            // Hull reaches zero on impact, one frame before the normal state machine creates and
            // completes the wreck sequence. Do not rebuild the conclusion over those explosions.
            || destroyed_card
            || unresolved_effects
        {
            return;
        }
        // Keep the mission's planet shot after the weapons finish; its outcome is recorded too.
        let death_ray = alive[0]
            && report.mission.objective == Icon::Destroy
            && combat.rounds[last].destroy_probability > 0.
            && combat.rounds[last]
                .attacker
                .iter()
                .any(|unit| unit.unit == Unit::war_sun() && unit.hull > 0);
        let bombing = alive[0]
            && combat.rounds[last].attacker.iter().any(|unit| {
                unit.unit == Unit::Ship(crate::core::units::ships::Ship::Bomber)
                    && unit.hull > 0
                    && unit.shots.iter().any(|shot| shot.is_bombing())
            });
        (
            last,
            true,
            if bombing {
                CombatState::Bomb
            } else if death_ray {
                CombatState::DeathRay
            } else {
                conclusion_phase(report)
            },
        )
    };
    let report = report.clone();
    // Automatic early completion rebuilds the report at its final boundary. Keep the exact
    // survivor slots from the played formation instead of compacting them around casualties.
    let preserved_individual_positions = (shortcut.is_none()
        && world
            .get_resource::<CombatFormationState>()
            .is_some_and(|formation| formation.individual()))
    .then(|| {
        world
            .query::<&IndividualCombatUnitCmp>()
            .iter(world)
            .filter_map(|card| card.id.map(|id| ((card.side == Side::Defender, id), card.home())))
            .collect::<HashMap<_, _>>()
    });
    seek(
        world,
        &report,
        destination.0,
        destination.1,
        destination.2,
        preserved_individual_positions.as_ref(),
    );
}

#[cfg(test)]
#[path = "../../../tests/core/combat_playback.rs"]
mod tests;
