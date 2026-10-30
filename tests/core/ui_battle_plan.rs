use super::*;
use crate::core::combat::plan::CombatTactic;

#[test]
fn mission_source_changes_inherit_standing_orders_without_resetting_draft_edits() {
    let mut origin = Planet::new(0, "Origin".into(), Vec2::ZERO, false, 1.0);
    origin.controlled = Some(1);
    origin.battle_plan.automatic_tactic = false;
    origin.battle_plan.tactic = CombatTactic::Siege;
    let mut mission = Mission::default();
    let mut initialized = None;
    initialize_mission_plan(&mut mission, &origin, 1, &mut initialized);
    assert_eq!(mission.battle_plan.tactic, CombatTactic::Siege);
    mission.battle_plan.tactic = CombatTactic::Swarm;
    initialize_mission_plan(&mut mission, &origin, 1, &mut initialized);
    assert_eq!(mission.battle_plan.tactic, CombatTactic::Swarm);
    origin.id = 3;
    origin.battle_plan.tactic = CombatTactic::Bastion;
    initialize_mission_plan(&mut mission, &origin, 1, &mut initialized);
    assert_eq!(mission.battle_plan.tactic, CombatTactic::Bastion);
    assert_eq!(initialized, Some(3));
}

#[test]
fn standing_plan_editing_only_exposes_the_viewers_own_force() {
    let mut planet = Planet::new(0, "World".into(), Vec2::ZERO, false, 1.0);
    planet.owned = Some(1);
    planet.controlled = Some(1);
    let controller = Army::from([(Unit::Ship(Ship::Dreadnought), 3)]);
    let protector = Army::from([(Unit::Ship(Ship::LightFighter), 12)]);
    planet.army = crate::core::map::planet::Garrison::from_parts(
        controller.clone(),
        BTreeMap::from([(2, protector.clone())]),
    );
    assert_eq!(commandable_garrison(&planet, &Player::new(1, 0)), Some(&controller));
    assert_eq!(commandable_garrison(&planet, &Player::new(2, 1)), Some(&protector));
    assert!(commandable_garrison(&planet, &Player::new(3, 2)).is_none());
    let mut spectator = Player::new(2, 1);
    spectator.spectator = true;
    assert!(commandable_garrison(&planet, &spectator).is_none());
    planet.is_destroyed = true;
    assert!(commandable_garrison(&planet, &Player::new(1, 0)).is_none());
}

#[test]
fn battle_plan_controls_wrap_within_a_narrow_detail_column() {
    let context = egui::Context::default();
    context.set_global_style(NordDark.custom_style());
    let mut plan = BattlePlan::default();
    let army = Army::from([(Unit::Ship(Ship::Cruiser), 2)]);
    let mut bounds = egui::Rect::NOTHING;
    let mut output = context.run_ui(Default::default(), |ui| {
        ui.set_width(240.0);
        ui.add_enabled_ui(false, |ui| {
            assert!(!draw_battle_plan_editor(ui, &ImageIds::default(), &mut plan, &army));
            bounds = ui.min_rect();
        });
    });
    output.textures_delta.clear();
    assert!(bounds.width() <= 241.0, "controls overflowed: {bounds:?}");
    assert_eq!(plan, BattlePlan::default());
}

#[test]
fn planet_header_edits_our_own_force_on_our_world_or_a_friendly_world() {
    let text = |output: &egui::FullOutput, label: &str| {
        output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == label => {
                    Some(egui::Rect::from_min_size(text.pos, text.galley.size()))
                },
                _ => None,
            })
            .unwrap()
    };
    for host in [1, 2] {
        let context = egui::Context::default();
        context.set_global_style(NordDark.custom_style());
        let mut planet = Planet::new(0, "World".into(), Vec2::ZERO, false, 1.0);
        planet.controlled = Some(host);
        planet.owned = Some(host);
        let ours = Army::from([(Unit::Ship(Ship::LightFighter), 4)]);
        if host == 1 {
            planet.army = ours.into();
        } else {
            planet.army = crate::core::map::planet::Garrison::from_parts(
                Army::from([(Unit::Ship(Ship::Battleship), 10)]),
                BTreeMap::from([(1, ours)]),
            );
        }
        let images =
            ImageIds(HashMap::from([("tactic swarm".to_owned(), egui::TextureId::User(78))]));
        let mut state = UiState::default();
        let mut time = 1.0;
        let mut frame = |events| {
            time += 0.05;
            let mut output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1000.0, 850.0),
                    )),
                    time: Some(time),
                    events,
                    ..default()
                },
                |ui| {
                    ui.set_width(518.0);
                    draw_overview(
                        ui,
                        &planet,
                        &Player::new(1, 0),
                        &MultiplayerSession::default(),
                        &mut state,
                        &images,
                        true,
                    );
                },
            );
            output.textures_delta.clear();
            output
        };
        let output = frame(vec![]);
        let icon = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Mesh(mesh) if mesh.texture_id == egui::TextureId::User(78) => {
                    Some(mesh.calc_bounds())
                },
                _ => None,
            })
            .unwrap();
        let pointer = |pos, pressed: Option<bool>| {
            let mut events = vec![egui::Event::PointerMoved(pos)];
            if let Some(pressed) = pressed {
                events.push(egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                });
            }
            events
        };
        frame(pointer(icon.center(), None));
        let output = frame(pointer(icon.center(), None));
        let tactic = text(&output, "Tactic");
        let target = text(&output, "Priority");
        let protection = text(&output, "Protection");
        assert!((tactic.center().y - target.center().y).abs() < 0.1);
        assert!((target.center().y - protection.center().y).abs() < 0.1);
        text(&output, "Auto");
        let card = text(&output, "Swarm").center();
        frame(pointer(card, Some(true)));
        frame(pointer(card, Some(false)));
        let output = frame(pointer(card, None));
        let choice = text(&output, "Siege").center();
        frame(pointer(choice, Some(true)));
        frame(pointer(choice, Some(false)));
        assert_eq!(state.garrison_battle_plan_hover, Some(planet.id));
        assert_eq!(state.garrison_battle_plan_changes.len(), 1);
        assert_eq!(state.garrison_battle_plan_changes[0].0, planet.id);
        assert_eq!(state.garrison_battle_plan_changes[0].1.tactic, CombatTactic::Siege);
        assert_eq!(
            planet.battle_plan,
            BattlePlan::default(),
            "the hover UI queues commands instead of editing another commander's garrison"
        );
    }
}
