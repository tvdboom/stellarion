//! Moving-target selection and validation inside the shared mission panel.

use super::*;
use crate::core::interception::target_available;

pub(super) fn visible_contact(target: &Mission, map: &Map, player: &Player) -> bool {
    !player.spectator
        && target.owner != player.id
        && (target.is_seen_by_phalanx(map, player).is_some()
            || target.is_seen_by_radar(map, player).is_some())
}

impl UiState {
    pub(crate) fn begin_intercept_selection(&mut self) {
        if self.mission_info.objective != Icon::Intercept {
            self.intercept_previous_destination = Some(self.mission_info.destination);
            self.intercept_target = None;
        }
        self.mission_info.objective = Icon::Intercept;
        self.mission_info.patrol = None;
        self.mission_info.intercept = None;
        self.mission_info.jump_gate = false;
        self.mission_info.deep_cover = false;
        self.mission_info.bombing = BombingRaid::None;
        self.selecting_patrol_zone = false;
        self.selecting_intercept = true;
        self.allied_mission = false;
        self.joint_attack_invitees.clear();
        self.joint_attack_draft_id = None;
        self.mission = false;
        self.planet_selected = None;
        self.planet_hover = None;
        self.mission_hover = None;
        self.mission_planet_hover = None;
    }

    /// Accepts visible contacts even when unreachable, so the draft can explain the restriction.
    pub(crate) fn choose_intercept_target(
        &mut self,
        map: &Map,
        player: &Player,
        turn: usize,
        target: &Mission,
    ) -> bool {
        if !visible_contact(target, map, player) {
            return false;
        }
        let origin = self
            .planet_selected
            .and_then(|id| map.try_get(id))
            .filter(|planet| planet.can_launch_mission(player.id))
            .or_else(|| {
                map.try_get(self.mission_info.origin)
                    .filter(|planet| planet.can_launch_mission(player.id))
            })
            .unwrap_or_else(|| map.get(player.home_planet));
        let army = self
            .mission_info
            .army
            .iter()
            .filter(|(unit, _)| unit.is_ship())
            .map(|(unit, count)| (*unit, *count))
            .collect();
        if self.mission_info.objective != Icon::Intercept {
            self.intercept_previous_destination = Some(self.mission_info.destination);
        }
        self.mission_info = Mission::new_with_id(
            rand::random::<u64>().max(1),
            turn,
            player.id,
            origin,
            origin,
            Icon::Intercept,
            army,
            BombingRaid::None,
            true,
            false,
            None,
        );
        self.intercept_target = Some(target.id);
        self.selecting_intercept = false;
        self.selecting_patrol_zone = false;
        self.mission = true;
        self.mission_tab = MissionTab::NewMission;
        self.combat_report = None;
        self.planet_selected = None;
        self.planet_hover = None;
        self.mission_planet_hover = None;
        self.mission_hover = None;
        self.allied_mission = false;
        self.joint_attack_draft_id = None;
        self.joint_attack_invitees.clear();
        self.mission_battle_plan_origin = None;
        true
    }
}

pub(crate) fn draw_selection_toast(context: &egui::Context, state: &mut UiState, cancel: bool) {
    if !state.selecting_intercept {
        return;
    }
    crate::core::messages::show_notification_area(
        context,
        "intercept target selection",
        true,
        560.0,
        |ui| {
            joint_attack_toast(ui, "Select a visible enemy fleet to plot an interception.", |ui| {
                if joint_attack_toast_button(ui, "Cancel", true).clicked() || cancel {
                    state.selecting_intercept = false;
                    if state.intercept_target.is_none() {
                        state.mission_info.objective = Icon::Attack;
                        if let Some(destination) = state.intercept_previous_destination {
                            state.mission_info.destination = destination;
                        }
                    }
                    state.mission = true;
                }
            });
        },
    );
}

pub(super) const TARGET_LABEL_WIDTH: f32 = 160.0;

fn contact_label(target: &Mission, session: &MultiplayerSession) -> RichText {
    let name = session
        .player_name(target.owner)
        .map(str::to_owned)
        .unwrap_or_else(|| format!("Player {}", target.owner));
    RichText::new(format!("{name}'s fleet"))
        .color(session.player_color(target.owner).color().to_color32())
}

/// Shows the selected contact without a button, matching the Deep space destination text.
pub(super) fn draw_target_label(
    ui: &mut Ui,
    selected: Option<MissionId>,
    contacts: &[Mission],
    map: &Map,
    player: &Player,
    session: &MultiplayerSession,
    center_y: f32,
) {
    let target = selected.and_then(|id| {
        contacts.iter().find(|target| target.id == id && visible_contact(target, map, player))
    });
    let label = target
        .map_or_else(|| RichText::new("Enemy mission"), |target| contact_label(target, session));
    let mut job = std::sync::Arc::unwrap_or_clone(egui::WidgetText::from(label).into_layout_job(
        ui.style(),
        TextStyle::Button.into(),
        Align::Center,
    ));
    job.wrap.max_width = TARGET_LABEL_WIDTH;
    job.halign = Align::Center;
    let label = ui.fonts_mut(|fonts| fonts.layout_job(job));
    let height = label.size().y;
    let (rect, _) = ui
        .allocate_exact_size(egui::vec2(TARGET_LABEL_WIDTH, 70.0_f32.max(height)), Sense::hover());
    ui.put(
        egui::Rect::from_center_size(
            egui::pos2(rect.center().x, center_y.max(rect.top() + height * 0.5)),
            egui::vec2(rect.width(), height),
        ),
        egui::Label::new(label).selectable(false),
    );
}

/// Replans after origin, target or fleet edits, without changing the ordinary panel layout.
pub(super) fn refresh_preview(
    state: &mut UiState,
    contacts: &[Mission],
    map: &Map,
    player: &Player,
    turn: usize,
) {
    if state.mission_info.objective != Icon::Intercept {
        return;
    }
    let origin = map.get(state.mission_info.origin);
    let plan = state.mission_info.battle_plan;
    let id = state.mission_info.id.max(1);
    let army = state.mission_info.army.clone();
    let planned = state
        .intercept_target
        .and_then(|id| contacts.iter().find(|target| target.id == id))
        .filter(|target| target_available(target, map, player))
        .and_then(|target| {
            Mission::plan_intercept(id, turn, player.id, origin, target, army.clone(), map)
        });
    state.mission_info = planned
        .unwrap_or_else(|| {
            Mission::new_with_id(
                id,
                turn,
                player.id,
                origin,
                origin,
                Icon::Intercept,
                army,
                BombingRaid::None,
                true,
                false,
                None,
            )
        })
        .with_battle_plan(plan);
}

/// Target-specific blockers take priority over fleet/fuel errors on the disabled Send button.
pub(super) fn unavailable_reason(
    state: &UiState,
    contacts: &[Mission],
    map: &Map,
    player: &Player,
) -> Option<&'static str> {
    if state.mission_info.objective != Icon::Intercept {
        return None;
    }
    let Some(target) =
        state.intercept_target.and_then(|id| contacts.iter().find(|target| target.id == id))
    else {
        return Some("Select a visible enemy mission to intercept. The previous contact may no longer be visible.");
    };
    if !visible_contact(target, map, player) {
        return Some("This enemy mission is no longer visible.");
    }
    if target.deep_cover || target.jump_gate || target.objective == Icon::MissileStrike {
        return Some("Deep Cover, missiles and jump-gate travel cannot be intercepted.");
    }
    if target.is_patrolling() {
        return Some("Use a patrol to engage an operating enemy patrol.");
    }
    if target.duration(map) == 0 {
        return Some("This enemy mission has already reached its destination.");
    }
    if !Icon::Intercept.accepts_army(&state.mission_info.army) {
        return Some("Select at least one combat ship for interception.");
    }
    if state.mission_info.intercept.is_none() {
        return Some("The selected fleet cannot reach a meeting point along the enemy's flight path before landing. Choose a closer launch world or a faster fleet; its slowest ship sets the speed.");
    }
    None
}

#[cfg(test)]
#[path = "../../../../tests/core/ui_interception.rs"]
mod tests;
