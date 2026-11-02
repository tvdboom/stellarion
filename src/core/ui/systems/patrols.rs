//! Patrol destination selection.

use super::*;

impl UiState {
    /// Temporarily hides the ordinary draft while its space destination is picked.
    pub(crate) fn begin_patrol_selection(&mut self) {
        if self.mission_info.objective != Icon::Patrol {
            self.patrol_previous_destination = Some(self.mission_info.destination);
        }
        self.mission_info.objective = Icon::Patrol;
        self.mission_info.jump_gate = false;
        self.mission_info.deep_cover = false;
        self.mission_info.bombing = BombingRaid::None;
        self.allied_mission = false;
        self.joint_attack_invitees.clear();
        self.selecting_patrol_zone = true;
        self.mission = false;
        self.planet_selected = None;
        self.planet_hover = None;
        self.mission_planet_hover = None;
    }

    /// Opens a normal mission draft at a valid empty-space destination.
    pub(crate) fn choose_patrol_zone(
        &mut self,
        map: &Map,
        player: &Player,
        turn: usize,
        center: Vec2,
    ) -> bool {
        let origin = map
            .try_get(self.mission_info.origin)
            .filter(|planet| planet.can_launch_mission(player.id))
            .unwrap_or_else(|| map.get(player.home_planet));
        if !crate::core::patrols::valid_patrol_target(map, origin.id, center, None) {
            return false;
        }
        if self.mission_info.objective != Icon::Patrol {
            self.patrol_previous_destination = Some(self.mission_info.destination);
        }
        let army = self
            .mission_info
            .army
            .iter()
            .filter(|(unit, _)| unit.is_ship())
            .map(|(unit, count)| (*unit, *count))
            .collect();
        self.mission_info = Mission::new_patrol(
            if self.mission_info.id == 0 {
                rand::random::<u64>().max(1)
            } else {
                self.mission_info.id
            },
            turn,
            player.id,
            origin,
            center,
            army,
            None,
        );
        self.patrol_center = Some(center);
        self.selecting_patrol_zone = false;
        self.mission = true;
        self.mission_tab = MissionTab::NewMission;
        self.combat_report = None;
        self.planet_selected = None;
        self.planet_hover = None;
        self.mission_planet_hover = None;
        self.allied_mission = false;
        self.joint_attack_draft_id = None;
        self.joint_attack_invitees.clear();
        true
    }

    pub(crate) fn cancel_patrol_selection(&mut self) {
        self.selecting_patrol_zone = false;
        self.mission = true;
        self.mission_tab = MissionTab::NewMission;
        if self.mission_info.patrol.is_none() {
            self.mission_info.objective = Icon::Attack;
            if let Some(destination) = self.patrol_previous_destination {
                self.mission_info.destination = destination;
            }
        }
    }
}

pub(super) fn draw_selection_toast(context: &egui::Context, state: &mut UiState, cancel: bool) {
    if !state.selecting_patrol_zone {
        return;
    }
    crate::core::messages::show_notification_area(
        context,
        "patrol zone selection",
        true,
        560.0,
        |ui| {
            super::missions::joint_attack_toast(
                ui,
                "Click empty space to choose a patrol zone.",
                |ui| {
                    if super::missions::joint_attack_toast_button(ui, "Cancel", true).clicked()
                        || cancel
                    {
                        state.cancel_patrol_selection();
                    }
                },
            );
        },
    );
}
