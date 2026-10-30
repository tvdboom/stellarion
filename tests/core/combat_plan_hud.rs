use super::*;

#[test]
#[ignore = "requires a GPU; saves full and compact combat-plan HUD previews"]
fn render_combat_plan_hud() {
    use crate::core::combat::plan::{BattlePlan, CombatTactic, ProtectionPosture, TargetPriority};
    use bevy::app::AppExit;
    use bevy::render::view::screenshot::{save_to_disk, Screenshot, ScreenshotCaptured};
    use bevy::winit::{WinitPlugin, WinitSettings};
    use bevy_egui::{EguiPlugin, EguiPrimaryContextPass};
    std::fs::create_dir_all("target/ui-battle-plan").unwrap();
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "Combat plan HUD".into(),
                    resolution: (1040, 800).into(),
                    visible: false,
                    ..Default::default()
                }),
                ..Default::default()
            })
            .set(WinitPlugin {
                run_on_any_thread: true,
            }),
    )
    .add_plugins(EguiPlugin::default())
    .insert_resource(WinitSettings::continuous())
    .add_systems(Startup, |mut commands: Commands| {
        commands.spawn(Camera2d);
    })
    .add_systems(
        EguiPrimaryContextPass,
        |mut contexts: EguiContexts,
         mut frame: Local<usize>,
         mut handles: Local<Vec<egui::TextureHandle>>,
         mut window: Single<&mut Window>,
         mut commands: Commands| {
            let Ok(context) = contexts.ctx_mut() else {
                return;
            };
            if *frame == 0 {
                context.add_font(FontInsert::new(
                    "combat identity",
                    egui::FontData::from_static(include_bytes!(
                        "../../assets/fonts/FiraMono-Medium.ttf"
                    )),
                    vec![InsertFontFamily {
                        family: egui::FontFamily::Name("combat identity".into()),
                        priority: FontPriority::Highest,
                    }],
                ));
                context.add_font(FontInsert::new(
                    "firasans",
                    egui::FontData::from_static(include_bytes!(
                        "../../assets/fonts/FiraSans-Bold.ttf"
                    )),
                    vec![InsertFontFamily {
                        family: egui::FontFamily::Proportional,
                        priority: FontPriority::Highest,
                    }],
                ));
                for file in std::fs::read_dir("assets/images/mission").unwrap() {
                    let path = file.unwrap().path();
                    let Some(key) = path.file_stem().and_then(|stem| stem.to_str()) else {
                        continue;
                    };
                    if !key.starts_with("tactic ")
                        && !key.starts_with("target ")
                        && !key.starts_with("protection ")
                    {
                        continue;
                    }
                    let pixels = image::open(&path).unwrap().into_rgba8();
                    handles.push(context.load_texture(
                        key,
                        egui::ColorImage::from_rgba_unmultiplied(
                            [pixels.width() as usize, pixels.height() as usize],
                            pixels.as_raw(),
                        ),
                        egui::TextureOptions::LINEAR,
                    ));
                }
                *frame += 1;
                return;
            }
            let images = ImageIds(
                handles.iter().map(|handle| (handle.name().to_owned(), handle.id())).collect(),
            );
            egui::Area::new(egui::Id::new("combat plan preview")).fixed_pos(egui::Pos2::ZERO).show(
                context,
                |ui| {
                    let rect = context.content_rect();
                    ui.painter().rect_filled(rect, 0.0, egui::Color32::from_rgb(5, 12, 21));
                    let names: Vec<(String, Color, u128)> = vec![
                        ("Player 1 · Fleet 1".into(), Color::srgb_u8(90, 200, 240), 20),
                        ("Player 1 · Fleet 2".into(), Color::srgb_u8(90, 200, 240), 10),
                        ("Player 2".into(), Color::srgb_u8(240, 180, 90), 30),
                        ("Player 3".into(), Color::srgb_u8(160, 220, 130), 15),
                    ];
                    let plans = [
                        CombatTactic::Swarm,
                        CombatTactic::Siege,
                        CombatTactic::Skirmish,
                        CombatTactic::Bastion,
                    ]
                    .into_iter()
                    .map(|tactic| CombatIdentityPlan {
                        plan: BattlePlan {
                            automatic_tactic: false,
                            tactic,
                            target_priority: TargetPriority::HeavyTargets,
                            protection: ProtectionPosture::Vanguard,
                        },
                        affinity: 83,
                        damage_bonuses: vec![(
                            CombatTactic::Bastion,
                            tactic.damage_bonus_basis_points(CombatTactic::Bastion, 83),
                        )],
                    })
                    .collect::<Vec<_>>();
                    let panel = draw_identity(
                        ui.painter(),
                        rect,
                        false,
                        "ATTACKER",
                        &names,
                        egui::Color32::GRAY,
                    );
                    draw_identity(
                        ui.painter(),
                        rect,
                        true,
                        "DEFENDER",
                        &names,
                        egui::Color32::GRAY,
                    );
                    let mut tooltip = egui::Tooltip::always_open(
                        context.clone(),
                        ui.layer_id(),
                        ui.id().with("preview details"),
                        panel,
                    )
                    .width(280.0_f32.min(rect.width() - 48.0));
                    tooltip.popup = tooltip
                        .popup
                        .frame(super::super::identity_ui::identity_tooltip_frame(ui.style()));
                    tooltip.show(|ui| {
                        super::super::identity_ui::draw_identity_details(ui, &plans, &images)
                    });
                },
            );
            *frame += 1;
            if *frame == 60 {
                window.resolution.set(360.0, 460.0);
            }
            if *frame == 40 || *frame == 100 {
                let compact = *frame == 100;
                let mut capture = commands.spawn(Screenshot::primary_window());
                capture.observe(save_to_disk(if compact {
                    "target/ui-battle-plan/combat-plans-compact.png"
                } else {
                    "target/ui-battle-plan/combat-plans.png"
                }));
                if compact {
                    capture.observe(
                        |_: On<ScreenshotCaptured>, mut exit: MessageWriter<AppExit>| {
                            exit.write(AppExit::Success);
                        },
                    );
                }
            }
        },
    );
    app.run();
}

#[test]
fn combat_identity_hover_card_fits_small_windows_and_scrolls_all_fleets() {
    use super::super::identity_ui::show_identity_tooltip;
    let context = egui::Context::default();
    context.global_style_mut(|style| {
        style.animation_time = 0.0;
        style.interaction.tooltip_delay = 0.0;
    });
    let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(360.0, 460.0));
    let panel = egui::Rect::from_min_size(egui::pos2(12.0, 40.0), egui::vec2(160.0, 54.0));
    let plans = (1..=12)
        .map(|affinity| CombatIdentityPlan {
            plan: super::super::plan::BattlePlan::default(),
            affinity,
            damage_bonuses: Vec::new(),
        })
        .collect::<Vec<_>>();
    let render = |pointer, scroll: bool| {
        let mut events = vec![egui::Event::PointerMoved(pointer)];
        if scroll {
            events.push(egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, -3000.0),
                phase: egui::TouchPhase::Move,
                modifiers: egui::Modifiers::NONE,
            });
        }
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(viewport),
                events,
                ..default()
            },
            |ui| {
                ui.set_min_size(viewport.size());
                show_identity_tooltip(
                    ui,
                    panel,
                    egui::Id::new("hover card"),
                    &plans,
                    &ImageIds::default(),
                );
            },
        );
        output.textures_delta.clear();
        output.shapes
    };
    for _ in 0..5 {
        render(panel.center(), false);
    }
    let shapes = render(panel.center(), false);
    fn popup_rect(shape: &egui::Shape) -> Option<egui::Rect> {
        match shape {
            egui::Shape::Rect(rect) if rect.fill == egui::Color32::from_rgb(10, 21, 33) => {
                Some(rect.rect)
            },
            egui::Shape::Vec(shapes) => shapes.iter().find_map(popup_rect),
            _ => None,
        }
    }
    let popup = shapes.iter().find_map(|shape| popup_rect(&shape.shape)).unwrap();
    assert!(viewport.contains_rect(popup), "hover card stays on screen: {popup:?}");
    let visible = |shapes: &[egui::epaint::ClippedShape], label| {
        shapes.iter().any(|shape| match &shape.shape {
            egui::Shape::Text(text) if text.galley.job.text == label => {
                shape.clip_rect.intersects(text.galley.rect.translate(text.pos.to_vec2()))
            },
            _ => false,
        })
    };
    assert!(visible(&shapes, "1% affinity"));
    assert!(!visible(&shapes, "12% affinity"));
    let pointer = popup.center();
    for _ in 0..3 {
        render(pointer, false);
    }
    for _ in 0..12 {
        render(pointer, true);
    }
    assert!(
        visible(&render(pointer, false), "12% affinity"),
        "the last fleet remains reachable from inside the tooltip"
    );
}

#[test]
fn combat_identity_bonuses_use_each_opponent_and_keep_the_opening_affinity() {
    use crate::core::combat::plan::{BattlePlan, CombatTactic};
    use crate::core::combat::report::{CombatReport, RoundReport};
    use crate::core::combat::resolution::CombatUnit;
    use crate::core::map::planet::Planet;
    use crate::core::missions::Mission;
    use crate::core::units::{ships::Ship, Army, Combat, Unit};

    let combatant = |id, owner, unit: Unit, tactic| CombatUnit {
        id,
        owner: Some(owner),
        fleet_id: 0,
        battle_plan: BattlePlan {
            automatic_tactic: false,
            tactic,
            ..Default::default()
        },
        unit,
        hull: unit.hull(),
        shield: unit.shield(),
        repairs: Vec::new(),
        shots: Vec::new(),
    };
    let mut attacker = (1..=5)
        .map(|id| {
            let mut fighter = combatant(id, 1, Unit::Ship(Ship::LightFighter), CombatTactic::Swarm);
            fighter.hull = 0;
            fighter
        })
        .collect::<Vec<_>>();
    let sun = combatant(36, 1, Unit::war_sun(), CombatTactic::Swarm);
    attacker.push(sun.clone());
    let defender = vec![
        combatant(37, 2, Unit::Ship(Ship::Cruiser), CombatTactic::Breakthrough),
        combatant(38, 3, Unit::Ship(Ship::Battleship), CombatTactic::Interception),
    ];
    let mut planet = Planet::new(1, "Target".into(), Vec2::ZERO, false, 1.0);
    planet.owned = Some(3);
    planet.controlled = Some(3);
    planet.army = Army::new().into();
    let mut report = crate::test_support::empty_report(Mission::default(), planet);
    report.combat_report = Some(CombatReport {
        rounds: vec![
            RoundReport {
                attacker,
                defender: defender.clone(),
                ..Default::default()
            },
            RoundReport {
                attacker: vec![sun],
                defender,
                ..Default::default()
            },
        ],
        ..Default::default()
    });

    let opening = combat_identity_plans(&report, &Side::Attacker, 0);
    assert_eq!(opening.len(), 1);
    assert_eq!(opening[0].affinity, 50, "five level-1 fighters balance one level-5 War Sun");
    assert_eq!(
        opening[0].damage_bonuses,
        vec![(CombatTactic::Breakthrough, 750), (CombatTactic::Interception, 0)]
    );
    let defenders = combat_identity_plans(&report, &Side::Defender, 0);
    assert_eq!(defenders[0].damage_bonuses, vec![(CombatTactic::Swarm, 1500)]);
    assert_eq!(defenders[1].damage_bonuses, vec![(CombatTactic::Swarm, 0)]);

    let next = combat_identity_plans(&report, &Side::Attacker, 1);
    assert_eq!(next[0].affinity, opening[0].affinity);
    assert_eq!(next[0].damage_bonuses, opening[0].damage_bonuses);
    assert_eq!(combat_identity_plans(&report, &Side::Attacker, 0)[0].affinity, 50);
}
