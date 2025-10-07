use crate::core::assets::WorldAssets;
use crate::core::constants::{LABEL_TEXT_SIZE, SUBTITLE_TEXT_SIZE, TITLE_TEXT_SIZE};
use crate::core::map::map::MapCmp;
use crate::core::map::systems::ShowOnHoverCmp;
use crate::core::player::Player;
use crate::core::resources::ResourceName;
use crate::core::settings::Settings;
use crate::core::ui::utils::{add_root_node, add_text};
use crate::core::units::buildings::Building;
use crate::core::units::defense::Defense;
use crate::core::units::ships::Ship;
use crate::core::units::Description;
use crate::core::utils::{on_out, on_over, Hovered};
use crate::utils::NameFromEnum;
use bevy::ecs::relationship::RelatedSpawnerCommands;
use bevy::prelude::*;
use strum::IntoEnumIterator;

#[derive(Component)]
pub struct UiCmp;

#[derive(Component)]
pub struct CycleCmp;

#[derive(Component)]
pub struct UnitsPanelCmp;

fn add_hover_info(
    parent: &mut RelatedSpawnerCommands<ChildOf>,
    text: impl Into<String>,
    assets: &WorldAssets,
    window: &Window,
) {
    parent
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: Val::Percent(115.),
                left: Val::Percent(0.),
                width: Val::Px(window.width() * 0.2),
                padding: UiRect::all(Val::Px(15.)),
                ..default()
            },
            ImageNode::new(assets.image("panel")),
            Pickable::IGNORE,
            ShowOnHoverCmp,
        ))
        .with_children(|parent| {
            parent.spawn(add_text(text, "medium", LABEL_TEXT_SIZE, assets, window));
        });
}

fn add_units<T: NameFromEnum + IntoEnumIterator + Component>(
    parent: &mut RelatedSpawnerCommands<ChildOf>,
    assets: &WorldAssets,
    window: &Window,
) {
    parent
        .spawn(Node {
            flex_direction: FlexDirection::Column,
            margin: UiRect::ZERO.with_top(Val::Percent(2.)),
            ..default()
        })
        .with_children(|parent| {
            for unit in T::iter() {
                parent
                    .spawn(Node {
                        flex_direction: FlexDirection::Row,
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        ..default()
                    })
                    .with_children(|parent| {
                        parent.spawn((
                            Node {
                                height: Val::Percent(10.),
                                margin: UiRect::ZERO.with_right(Val::Percent(10.)),
                                ..default()
                            },
                            ImageNode::new(assets.image(unit.to_lowername().as_str())),
                            Pickable::IGNORE,
                        ));

                        parent.spawn((
                            add_text("0", "bold", TITLE_TEXT_SIZE, &assets, &window),
                            Pickable::IGNORE,
                            unit,
                        ));
                    });
            }
        });
}

pub fn draw_ui(
    mut commands: Commands,
    player: Res<Player>,
    settings: Res<Settings>,
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
                ImageNode::new(assets.image("thin_panel")),
                UiCmp,
                MapCmp,
            ))
            .with_children(|parent| {
                parent
                    .spawn((
                        Node {
                            flex_direction: FlexDirection::Row,
                            align_items: AlignItems::Center,
                            padding: UiRect::ZERO.with_top(Val::Percent(1.)),
                            margin: UiRect::ZERO.with_right(Val::Percent(15.)),
                            ..default()
                        },
                        UiCmp,
                    ))
                    .observe(on_over)
                    .observe(on_out)
                    .with_children(|parent| {
                        add_hover_info(parent, "Turn\n\nNumber of turns played.", &assets, &window);

                        parent.spawn((
                            Node {
                                height: Val::Percent(80.),
                                margin: UiRect::ZERO.with_right(Val::Percent(10.)),
                                ..default()
                            },
                            ImageNode::new(assets.image("turn")),
                            Pickable::IGNORE,
                        ));

                        parent.spawn((
                            add_text(
                                settings.turn.to_string(),
                                "bold",
                                TITLE_TEXT_SIZE,
                                &assets,
                                &window,
                            ),
                            Pickable::IGNORE,
                            CycleCmp,
                        ));
                    });

                for resource in ResourceName::iter() {
                    parent
                        .spawn((
                            Node {
                                flex_direction: FlexDirection::Row,
                                align_items: AlignItems::Center,
                                padding: UiRect::ZERO.with_top(Val::Percent(1.)),
                                margin: UiRect::ZERO.with_right(Val::Percent(5.)),
                                ..default()
                            },
                            Pickable::default(),
                            UiCmp,
                        ))
                        .observe(on_over)
                        .observe(on_out)
                        .with_children(|parent| {
                            add_hover_info(
                                parent,
                                format!("{}\n\n{}", resource.to_name(), resource.description()),
                                &assets,
                                &window,
                            );

                            parent.spawn((
                                Node {
                                    height: Val::Percent(80.),
                                    margin: UiRect::ZERO.with_right(Val::Percent(10.)),
                                    ..default()
                                },
                                ImageNode::new(assets.image(resource.to_lowername().as_str())),
                                Pickable::IGNORE,
                            ));

                            parent.spawn((
                                add_text(
                                    player.resources.get(&resource).to_string(),
                                    "bold",
                                    TITLE_TEXT_SIZE,
                                    &assets,
                                    &window,
                                ),
                                Pickable::IGNORE,
                                resource,
                            ));
                        });
                }
            });

        parent
            .spawn((
                Node {
                    position_type: PositionType::Absolute,
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    right: Val::Percent(2.),
                    width: Val::Percent(20.),
                    height: Val::Percent(70.),
                    padding: UiRect::ZERO.with_top(Val::Percent(4.)),
                    ..default()
                },
                ImageNode::new(assets.image("panel")),
                Pickable::IGNORE,
                UnitsPanelCmp,
            ))
            .with_children(|parent| {
                parent.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        top: Val::Percent(2.5),
                        ..default()
                    },
                    add_text("Overview", "bold", SUBTITLE_TEXT_SIZE, &assets, &window),
                    Pickable::IGNORE,
                ));

                add_units::<Ship>(parent, &assets, &window);
                // add_units::<Defense>(parent, &assets, &window);
                // add_units::<Building>(parent, &assets, &window);
            });
    });
}

pub fn update_ui(
    mut cycle_q: Query<&mut Text, With<CycleCmp>>,
    mut resource_q: Query<(&mut Text, &ResourceName), Without<CycleCmp>>,
    hover_q: Query<(Entity, Option<&Hovered>), With<UiCmp>>,
    mut show_q: Query<&mut Visibility, With<ShowOnHoverCmp>>,
    children_q: Query<&Children>,
    players: Res<Player>,
    settings: Res<Settings>,
) {
    // Update the cycle label
    cycle_q.single_mut().unwrap().0 = format!("{:.0}", settings.turn);

    // Update the resource labels
    for (mut text, resource) in &mut resource_q {
        text.0 = format!("{:.0}", players.resources.get(resource));
    }

    // Show on hover
    for (entity, hovered) in &hover_q {
        for child in children_q.iter_descendants(entity) {
            if let Ok(mut visibility) = show_q.get_mut(child) {
                *visibility = if hovered.is_some() && settings.show_hover {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                };
            }
        }
    }
}
