//! Playback shortcuts and early completion; recorded reports remain authoritative.

use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;
use bevy_tweening::TweenAnim;

use super::report::{MissionReport, Side};
use super::systems::{
    restore_combat_camera, setup_combat, BackgroundImageCmp, CombatCmp, CombatUnitCmp, FireState,
    SpawnShotMsg,
};
use crate::core::assets::WorldAssets;
use crate::core::audio::{MuteAudioMsg, PlayAudioMsg};
use crate::core::constants::PS_SHIELD_PER_LEVEL;
use crate::core::map::icon::Icon;
use crate::core::player::Player;
use crate::core::settings::Settings;
use crate::core::states::CombatState;
use crate::core::ui::systems::UiState;
use crate::core::units::{Amount, Combat, Unit};

#[derive(Component)]
pub(crate) struct CombatCardHome(pub Vec3);

#[derive(Resource)]
/// Requests a short round banner after a playback shortcut.
pub struct CombatRoundJump;

fn combatant(unit: Unit) -> bool {
    !unit.is_building() && !unit.is_missile() && unit != Unit::colony_ship()
}

/// Reconstructs a card from a round boundary, including hull retained from prior rounds.
fn snapshot_card(
    report: &MissionReport,
    index: usize,
    finished: bool,
    unit: Unit,
    side: Side,
) -> Option<CombatUnitCmp> {
    let combat = report.combat_report.as_ref()?;
    let round = combat.rounds.get(index)?;
    let previous = index.checked_sub(1).and_then(|i| combat.rounds.get(i));
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
            boundary.map_or(count * PS_SHIELD_PER_LEVEL, |snapshot| snapshot.planetary_shield)
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
                report.planet.army.amount(&unit) * PS_SHIELD_PER_LEVEL
            } else {
                0
            },
        )
    } else {
        let records = round.units(&side);
        let count = records.iter().filter(|record| record.unit == unit).count();
        let hull = records
            .iter()
            .filter(|record| record.unit == unit)
            .map(|record| {
                if finished {
                    record.hull
                } else {
                    previous
                        .and_then(|snapshot| {
                            snapshot.units(&side).iter().find(|old| old.id == record.id)
                        })
                        .map_or(unit.hull(), |old| old.hull)
                }
            })
            .sum();
        let shield = if finished {
            records.iter().filter(|record| record.unit == unit).map(|record| record.shield).sum()
        } else {
            count * unit.shield()
        };
        (hull, count * unit.hull(), shield, count * unit.shield())
    };
    (hull > 0).then_some(CombatUnitCmp {
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
    })
}

/// Rebuilds presentation from saved data so backward jumps also restore destroyed cards.
fn seek(
    world: &mut World,
    report: &MissionReport,
    index: usize,
    finished: bool,
    ending_phase: CombatState,
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
            })
        });
        if let Some(mut card) = card {
            if card.side == Side::Attacker
                && ((ending_phase == CombatState::DeathRay && card.unit == Unit::war_sun())
                    || (ending_phase == CombatState::Bomb
                        && card.unit == Unit::Ship(crate::core::units::ships::Ship::Bomber)))
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
            if card.hull == 0 {
                world.despawn(entity);
                continue;
            }
            world
                .entity_mut(entity)
                .insert((card, Transform::from_translation(position)))
                .remove::<TweenAnim>();
        } else {
            world.despawn(entity);
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
            (true, false) if index > 0 => Some(false),
            (false, true) if phase != CombatState::EndCombat => Some(true),
            _ => None,
        }
    });
    let active = matches!(phase, CombatState::Fire | CombatState::Repair);
    if shortcut.is_none() && (!active || world.resource::<Settings>().combat_paused) {
        return;
    }
    let mut alive = [false; 2];
    for card in world.query::<&CombatUnitCmp>().iter(world) {
        if combatant(card.unit) && card.hull > 0 {
            alive[usize::from(card.side == Side::Defender)] = true;
        }
    }
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
                    CombatState::EndCombat
                } else {
                    CombatState::DisplayRound
                },
            )
        } else {
            (index - 1, false, CombatState::DisplayRound)
        }
    } else {
        // Empty, unguarded planets must still play bombing/destruction missions.
        let started_with_both = [&report.mission.army, &report.planet.army]
            .into_iter()
            .all(|army| army.iter().any(|(unit, count)| *count > 0 && combatant(*unit)));
        if !started_with_both
            || alive.into_iter().all(|present| present)
            || report.mission.objective == Icon::MissileStrike
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
                CombatState::EndCombat
            },
        )
    };
    let report = report.clone();
    seek(world, &report, destination.0, destination.1, destination.2);
}

#[cfg(test)]
#[path = "../../../tests/core/combat_playback.rs"]
mod tests;
