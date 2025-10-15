use crate::core::map::icon::Icon;
use crate::core::map::map::Map;
use crate::core::player::Player;
use crate::core::settings::Settings;
use crate::core::ui::systems::UiState;
use crate::core::units::buildings::Building;
use crate::core::units::ships::Ship;
use crate::core::units::Unit;
use bevy::prelude::*;

#[derive(Message)]
pub struct NextTurnMsg;

pub fn next_turn(
    mut next_turn_ev: MessageReader<NextTurnMsg>,
    mut state: ResMut<UiState>,
    mut map: ResMut<Map>,
    mut player: ResMut<Player>,
    mut settings: ResMut<Settings>,
) {
    for _ in next_turn_ev.read() {
        settings.turn += 1;

        // Produce resources
        let production = player.resource_production(&map.planets);
        player.resources += production;

        // Apply purchases
        map.planets.iter_mut().for_each(|p| p.produce());

        // Resolve missions
        let id = player.id;
        player.missions.retain_mut(|mission| {
            mission.advance(&map);

            if mission.has_reached_destination(&map) {
                let destination = map.get_mut(mission.destination);

                // If the destination planet is friendly, the fleet stops there
                // (the planet could have been colonized by another mission)
                // Missile strikes always attack the destination planet
                if destination.owner != Some(id) || mission.objective == Icon::MissileStrike {
                    // combat !! do as if won with all units for now

                    match mission.objective {
                        Icon::Colonize => {
                            *mission.army.entry(Unit::Ship(Ship::ColonyShip)).or_insert(1) -= 1;
                            destination.owner = Some(id);

                            // If the planet has no buildings, build a level 1 mine
                            if destination.complex.is_empty() {
                                destination.complex.insert(Building::Mine, 1);
                            }
                        },
                        _ => (),
                    }
                }

                // Dock surviving fleet on the planet
                destination.dock(mission.army.clone());

                false
            } else {
                true
            }
        });

        *state = UiState::default()
    }
}
