//! Optional screenshots of the real objective widget, rendered offscreen without a game session.

use super::*;
use crate::core::basis_texture::BasisTexturePlugin;
use bevy::asset::RenderAssetUsages;
use bevy::camera::RenderTarget;
use bevy::ecs::system::RunSystemOnce;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
use bevy::render::view::screenshot::{save_to_disk, Screenshot};
use bevy::render::RenderPlugin;
use bevy::window::ExitCondition;
use bevy::winit::WinitPlugin;
use bevy_egui::{EguiPlugin, EguiPrimaryContextPass, EguiTextureHandle, PrimaryEguiContext};

#[derive(Resource)]
struct ObjectivePreview {
    selected: Icon,
    buttons: HashMap<Icon, egui::Rect>,
    handles: Vec<(Icon, Handle<Image>)>,
}

fn draw_preview(
    mut contexts: EguiContexts,
    images: Res<ImageIds>,
    mut preview: ResMut<ObjectivePreview>,
) {
    let Ok(context) = contexts.ctx_mut() else {
        return;
    };
    egui::Area::new(egui::Id::new("objective preview")).fixed_pos(egui::Pos2::ZERO).show(
        context,
        |ui| {
            egui::Frame::new().fill(Color32::from_rgb(11, 16, 20)).inner_margin(14.0).show(
                ui,
                |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = MISSION_OBJECTIVE_BUTTON_GAP;
                        for icon in Icon::objectives(false, false, false, false) {
                            ui.add_enabled_ui(icon != Icon::Spy, |ui| {
                                let response = draw_mission_objective_button(
                                    ui,
                                    images.get(icon.asset_key()),
                                    icon,
                                    preview.selected == icon,
                                );
                                preview.buttons.insert(icon, response.rect);
                                if response.clicked() {
                                    preview.selected = icon;
                                }
                            });
                        }
                    });
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.small("🎯 Objective:");
                        ui.add_image(images.get(preview.selected.asset_key()), [20.0; 2]);
                        ui.small(preview.selected.to_name());
                    });
                },
            );
        },
    );
}

#[test]
#[ignore = "offscreen GPU review; writes PNGs under target/objective-preview"]
fn render_objective_button_preview() {
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(AssetPlugin {
                file_path: format!("{}/assets-runtime", env!("CARGO_MANIFEST_DIR")),
                meta_check: bevy::asset::AssetMetaCheck::Never,
                ..default()
            })
            .set(WindowPlugin {
                primary_window: None,
                exit_condition: ExitCondition::DontExit,
                ..default()
            })
            .set(RenderPlugin {
                synchronous_pipeline_compilation: true,
                ..default()
            })
            .disable::<WinitPlugin>(),
    )
    .add_plugins((EguiPlugin::default(), BasisTexturePlugin))
    .insert_resource(ClearColor(Color::srgb_u8(11, 16, 20)))
    .insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
        std::time::Duration::from_secs_f64(1.0 / 60.0),
    ))
    .init_resource::<ImageIds>()
    .add_systems(EguiPrimaryContextPass, draw_preview);
    app.finish();
    app.cleanup();
    let handles = Icon::objectives(false, false, false, false)
        .into_iter()
        .map(|icon| {
            let handle = app
                .world()
                .resource::<AssetServer>()
                .load(format!("images/mission/{}.basisu.ktx2", icon.asset_key()));
            (icon, handle)
        })
        .collect();
    app.insert_resource(ObjectivePreview {
        selected: Icon::Attack,
        buttons: HashMap::new(),
        handles,
    });
    let mut target = Image::new_uninit(
        Extent3d {
            width: 360,
            height: 140,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    target.texture_descriptor.usage |= TextureUsages::RENDER_ATTACHMENT;
    let target = app.world_mut().resource_mut::<Assets<Image>>().add(target);
    let camera = app
        .world_mut()
        .spawn((Camera2d, PrimaryEguiContext, RenderTarget::Image(target.clone().into())))
        .id();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(45);
    loop {
        app.update();
        if app.world().resource::<ObjectivePreview>().handles.iter().all(|(_, handle)| {
            app.world().resource::<AssetServer>().is_loaded_with_dependencies(handle.id())
        }) {
            break;
        }
        assert!(std::time::Instant::now() < deadline, "run just assets before objective preview");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    app.world_mut()
        .run_system_once(
            |mut contexts: EguiContexts,
             mut images: ResMut<ImageIds>,
             preview: Res<ObjectivePreview>| {
                contexts.ctx_mut().unwrap().set_global_style(NordDark.custom_style());
                for (icon, handle) in &preview.handles {
                    images.0.insert(
                        icon.asset_key(),
                        contexts.add_image(EguiTextureHandle::Strong(handle.clone())),
                    );
                }
            },
        )
        .unwrap();
    for _ in 0..12 {
        app.update();
    }
    std::fs::create_dir_all("target/objective-preview").unwrap();
    let center = app.world().resource::<ObjectivePreview>().buttons[&Icon::Raid].center();
    for (name, events) in [
        ("idle", vec![egui::Event::PointerGone]),
        ("hover", vec![egui::Event::PointerMoved(center)]),
        (
            "pressed",
            vec![egui::Event::PointerButton {
                pos: center,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            }],
        ),
        (
            "selected",
            vec![egui::Event::PointerButton {
                pos: center,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        ),
    ] {
        for event in events {
            app.world_mut().write_message(bevy_egui::input::EguiInputEvent {
                context: camera,
                event,
            });
        }
        for _ in 0..12 {
            app.update();
        }
        if name == "selected" {
            assert_eq!(app.world().resource::<ObjectivePreview>().selected, Icon::Raid);
            app.world_mut().write_message(bevy_egui::input::EguiInputEvent {
                context: camera,
                event: egui::Event::PointerGone,
            });
            for _ in 0..12 {
                app.update();
            }
        }
        let path = format!("{}/target/objective-preview/{name}.png", env!("CARGO_MANIFEST_DIR"));
        app.world_mut().spawn(Screenshot::image(target.clone())).observe(save_to_disk(path));
        for _ in 0..12 {
            app.update();
        }
    }
}
