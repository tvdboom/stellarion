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
                    let names = vec![
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
                    .map(|tactic| {
                        (
                            BattlePlan {
                                automatic_tactic: false,
                                tactic,
                                target_priority: TargetPriority::CapitalShips,
                                protection: ProtectionPosture::ScreenCapitals,
                            },
                            83,
                        )
                    })
                    .collect::<Vec<_>>();
                    draw_identity(
                        ui.painter(),
                        rect,
                        false,
                        "ATTACKER",
                        &names,
                        &plans,
                        &images,
                        egui::Color32::GRAY,
                    );
                    draw_identity(
                        ui.painter(),
                        rect,
                        true,
                        "DEFENDER",
                        &names,
                        &plans,
                        &images,
                        egui::Color32::GRAY,
                    );
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
