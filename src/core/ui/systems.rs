use crate::core::assets::WorldAssets;
use crate::core::constants::SUBTITLE_TEXT_SIZE;
use crate::core::game_settings::GameSettings;
use crate::core::map::map::MapCmp;
use crate::core::player::Player;
use crate::core::resources::ResourceCmp;
use crate::core::ui::utils::{add_root_node, add_text};
use crate::utils::NameFromEnum;
use bevy::prelude::*;
use strum::IntoEnumIterator;

#[derive(Component)]
pub struct UiCmp;

#[derive(Component)]
pub struct CycleCmp;

pub fn draw_ui(
    mut commands: Commands,
    player: Res<Player>,
    settings: Res<GameSettings>,
    assets: Local<WorldAssets>,
    window: Single<&Window>,
) {
    commands.spawn(add_root_node()).with_children(|parent| {
        parent
            .spawn((
                Node {
                    top: Val::Percent(3.),
                    width: Val::Percent(50.),
                    height: Val::Percent(6.),
                    position_type: PositionType::Absolute,
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    ..default()
                },
                ImageNode::new(assets.image("panel")),
                Pickable::IGNORE,
                UiCmp,
                MapCmp,
            ))
            .with_children(|parent| {
                parent
                    .spawn((
                        Node {
                            width: Val::Percent(5.),
                            margin: UiRect::all(Val::Percent(1.)).with_right(Val::Percent(0.)),
                            ..default()
                        },
                        ImageNode::new(assets.image("cycle")),
                    ))
                    .with_children(|parent| {
                        parent.spawn((
                            add_text(
                                format!("{:.0}", settings.cycle),
                                "bold",
                                SUBTITLE_TEXT_SIZE,
                                &assets,
                                &window,
                            ),
                            CycleCmp,
                        ));
                    });

                for resource in ResourceCmp::iter() {
                    parent
                        .spawn((
                            Node {
                                width: Val::Percent(5.),
                                margin: UiRect::all(Val::Percent(1.)).with_right(Val::Percent(1.5)),
                                ..default()
                            },
                            ImageNode::new(assets.image(resource.to_lowername().as_str())),
                        ))
                        .with_children(|parent| {
                            parent.spawn((
                                Node {
                                    align_self: AlignSelf::Center,
                                    margin: UiRect::ZERO.with_right(Val::Percent(3.)),
                                    ..default()
                                },
                                add_text(
                                    format!("{:.0}", player.resources.get(&resource)),
                                    "bold",
                                    SUBTITLE_TEXT_SIZE,
                                    &assets,
                                    &window,
                                ),
                                resource,
                            ));
                        });
                }
            });
    });
}

pub fn update_ui(
    mut cycle_q: Query<&mut Text, With<CycleCmp>>,
    mut resource_q: Query<(&mut Text, &ResourceCmp), Without<CycleCmp>>,
    players: Res<Player>,
    setting: Res<GameSettings>,
) {
    // Update the cycle label
    cycle_q.single_mut().unwrap().0 = format!("{:.0}", setting.cycle);

    // Update the resource labels
    for (mut text, resource) in &mut resource_q {
        text.0 = format!("{:.0}", players.resources.get(resource));
    }
}
