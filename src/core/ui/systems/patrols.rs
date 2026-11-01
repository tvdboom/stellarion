//! Patrol destination selection and active-fleet rows.

use super::*;
use crate::core::patrols::{PatrolPhase, PATROL_RADIUS};

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
            let accent = Color32::from_rgb(112, 190, 255);
            egui::Frame::new()
                .fill(Color32::from_rgba_unmultiplied(28, 36, 48, 235))
                .stroke(Stroke::new(1.0, accent))
                .corner_radius(5.0)
                .inner_margin(egui::Margin::symmetric(12, 8))
                .show(ui, |ui| {
                    ui.label(
                        RichText::new("Click empty space to choose a patrol zone.")
                            .small()
                            .color(accent),
                    );
                    if ui.small_button("Cancel").clicked() || cancel {
                        state.cancel_patrol_selection();
                    }
                });
        },
    );
}

pub(super) fn draw_active(
    ui: &mut Ui,
    mission: &Mission,
    state: &mut UiState,
    map: &Map,
    player: &Player,
    recall: &mut MessageWriter<RecallMissionMsg>,
    editable: bool,
) {
    let Some(patrol) = &mission.patrol else {
        return;
    };
    if mission.owner != player.id {
        ui.small("Foreign fleet");
        let response = ui.small(format!(
            "Detected at ({:.1}, {:.1}) AU",
            mission.position.x / Planet::SIZE,
            mission.position.y / Planet::SIZE
        ));
        if response.hovered() {
            state.mission_hover = Some(mission.id);
            state.mission_hover_from_ui = true;
        }
        ui.label("");
        return;
    }
    ui.small("Patrol");
    let response = ui
        .vertical(|ui| {
            let status = match patrol.phase {
                PatrolPhase::Outbound => {
                    format!("Travelling to patrol zone · {} turns", mission.duration(map))
                },
                PatrolPhase::OnStation => {
                    "Patrolling · attacks all detected foreign fleets".to_owned()
                },
                PatrolPhase::Returning => format!(
                    "Returning to {} · {} turns",
                    map.get(mission.destination).name,
                    mission.duration(map).max(1)
                ),
            };
            ui.small(status);
            ui.small(format!(
                "Zone ({:.1}, {:.1}) AU · radius {:.0} AU",
                patrol.center.x / Planet::SIZE,
                patrol.center.y / Planet::SIZE,
                PATROL_RADIUS / Planet::SIZE
            ));
            if mission.owner == player.id {
                ui.small(format!(
                    "Strength: {} · Upkeep: {} deuterium/turn",
                    super::missions::fleet_strength(&mission.army),
                    mission.patrol_upkeep(),
                ));
                if patrol.phase != PatrolPhase::Returning {
                    let mut returning = mission.clone();
                    returning.return_patrol(map, mission.send);
                    ui.small(format!(
                        "Return: {} · {} turns",
                        map.get(returning.destination).name,
                        returning.duration(map).max(1)
                    ));
                }
            }
        })
        .response;
    if response.hovered() {
        state.mission_hover = Some(mission.id);
        state.mission_hover_from_ui = true;
    }
    if patrol.phase != PatrolPhase::Returning {
        if ui.add_enabled(editable, egui::Button::new("Recall")).clicked() {
            recall.write(RecallMissionMsg::new(mission.id));
        }
    } else {
        ui.label("");
    }
}
