//! Shared patrol and expedition space-destination selection.

use super::*;

use crate::core::expeditions::valid_expedition_target;
use crate::core::messages::show_notification_area;
use crate::core::patrols::valid_patrol_target;

impl UiState {
    /// Targets an owned operating fleet's strategic center, independent of its animated sprite.
    pub(crate) fn choose_patrol_reinforcement(
        &mut self,
        map: &Map,
        player: &Player,
        turn: usize,
        target: &Mission,
    ) -> bool {
        if target.owner != player.id || !target.is_patrolling() || player.spectator {
            return false;
        }
        if self.choosing_expedition_destination() {
            return false;
        }
        let Some(patrol) = &target.patrol else {
            return false;
        };
        if let Some(origin) = self
            .planet_selected
            .and_then(|id| map.try_get(id))
            .filter(|planet| planet.can_launch_mission(player.id))
        {
            self.mission_info.origin = origin.id;
        }
        self.mission_info.objective = Icon::Patrol;
        self.mission_info.expedition = None;
        self.choose_patrol_zone(map, player, turn, patrol.center)
    }

    /// Temporarily hides the ordinary draft while its space destination is picked.
    pub(crate) fn begin_patrol_selection(&mut self) {
        self.begin_space_selection(Icon::Patrol);
    }

    /// Keeps a valid space target when changing objectives; selecting it again picks a new point.
    pub(crate) fn select_space_objective(
        &mut self,
        map: &Map,
        player: &Player,
        turn: usize,
        objective: Icon,
    ) {
        let center = if self.mission_info.objective == objective {
            None
        } else {
            match self.mission_info.objective {
                Icon::Patrol => self.mission_info.patrol.as_ref().map(|patrol| patrol.center),
                Icon::Expedition => self.mission_info.expedition.as_ref().map(|site| site.center),
                _ => None,
            }
        };
        let battle_plan = self.mission_info.battle_plan;
        if objective == Icon::Patrol {
            self.begin_patrol_selection();
        } else {
            self.begin_space_selection(objective);
        }
        if center.is_some_and(|center| self.choose_patrol_zone(map, player, turn, center)) {
            self.mission_info.battle_plan = battle_plan;
        }
    }

    pub(crate) fn begin_space_selection(&mut self, objective: Icon) {
        if !matches!(self.mission_info.objective, Icon::Patrol | Icon::Expedition) {
            self.patrol_previous_destination = Some(self.mission_info.destination);
        }
        self.mission_info.intercept = None;
        self.mission_info.sabotage = None;
        self.mission_info.objective = objective;
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
        self.mission_hover = None;
        self.mission_hover_from_ui = false;
    }

    /// Returns whether map clicks should place an expedition destination.
    pub(crate) fn choosing_expedition_destination(&self) -> bool {
        self.selecting_patrol_zone && self.mission_info.objective == Icon::Expedition
    }

    /// Opens a normal mission draft at a valid space destination.
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
        let expedition = self.mission_info.objective == Icon::Expedition;
        if if expedition {
            !valid_expedition_target(map, origin.id, center)
        } else {
            !valid_patrol_target(map, origin.id, center, None)
        } {
            return false;
        }
        if !matches!(self.mission_info.objective, Icon::Patrol | Icon::Expedition) {
            self.patrol_previous_destination = Some(self.mission_info.destination);
        }
        let army = self
            .mission_info
            .army
            .iter()
            .filter(|(unit, _)| unit.is_ship())
            .map(|(unit, count)| (*unit, *count))
            .collect();
        let id = if self.mission_info.id == 0 {
            rand::random::<u64>().max(1)
        } else {
            self.mission_info.id
        };
        self.mission_info = if expedition {
            Mission::new_expedition(id, turn, player.id, origin, center, army)
        } else {
            Mission::new_patrol(id, turn, player.id, origin, center, army, None)
        };
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
        if self.mission_info.expedition.is_some() {
            self.mission_info.objective = Icon::Expedition;
        } else if self.mission_info.patrol.is_some() {
            self.mission_info.objective = Icon::Patrol;
        } else {
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
    show_notification_area(context, "patrol zone selection", true, 560.0, |ui| {
        super::missions::joint_attack_toast(
            ui,
            if state.mission_info.objective == Icon::Expedition {
                "Click a point on the map to set the expedition destination."
            } else {
                "Click empty space for a patrol zone, or an active patrol to reinforce it."
            },
            |ui| {
                if super::missions::joint_attack_toast_button(ui, "Cancel", true).clicked()
                    || cancel
                {
                    state.cancel_patrol_selection();
                }
            },
        );
    });
}
