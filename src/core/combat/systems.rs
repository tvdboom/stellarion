use bevy::ecs::children;
use bevy::image::TextureAtlas;
use bevy::picking::Pickable;
use bevy::prelude::*;
use bevy::window::SystemCursorIcon;

use crate::core::assets::WorldAssets;
use crate::core::audio::PlayAudioMsg;
use crate::core::constants::BUTTON_TEXT_SIZE;
use crate::core::map::map::MapCmp;
use crate::core::map::systems::EndTurnButtonCmp;
use crate::core::map::utils::{cursor, set_button_index};
use crate::core::menu::utils::add_root_node;
use crate::core::states::GameState;
use crate::core::turns::StartTurnMsg;

#[derive(Component)]
pub struct CombatCmp;

pub fn setup_in_combat(
    mut commands: Commands,
    mut play_audio_ev: MessageWriter<PlayAudioMsg>,
    assets: Local<WorldAssets>,
) {
    play_audio_ev.write(PlayAudioMsg::new("horn"));

    let texture = assets.texture("long button");
    commands
        .spawn((add_root_node(true), ImageNode::new(assets.image("combat")), CombatCmp))
        .with_children(|parent| {
            parent
                .spawn(Node {
                    width: Val::Percent(100.),
                    height: Val::Percent(100.),
                    align_items: AlignItems::End,
                    justify_content: JustifyContent::FlexEnd,
                    padding: UiRect::all(Val::Percent(3.)),
                    ..default()
                })
                .with_children(|parent| {
                    parent.spawn((
                        Node {
                            position_type: PositionType::Absolute,
                            bottom: Val::Px(30.),
                            right: Val::Px(50.),
                            width: Val::Px(200.),
                            height: Val::Px(40.),
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            ..default()
                        },
                        ImageNode::from_atlas_image(
                            texture.image.clone(),
                            TextureAtlas {
                                layout: texture.atlas.layout.clone(),
                                index: 0,
                            },
                        ),
                        Pickable::default(),
                        EndTurnButtonCmp,
                        MapCmp,
                        children![(
                            Text::new("Continue"),
                            TextFont {
                                font: assets.font("bold"),
                                font_size: BUTTON_TEXT_SIZE,
                                ..default()
                            },
                        )],
                    ))
                        .observe(cursor::<Over>(SystemCursorIcon::Pointer))
                        .observe(cursor::<Out>(SystemCursorIcon::Default))
                        .observe(
                            |_: On<Pointer<Over>>, button_q: Single<&mut ImageNode, With<EndTurnButtonCmp>>| {
                                set_button_index(&mut button_q.into_inner(), 1);
                            },
                        )
                        .observe(|_: On<Pointer<Out>>, button_q: Single<&mut ImageNode, With<EndTurnButtonCmp>>| {
                            set_button_index(&mut button_q.into_inner(), 0);
                        })
                        .observe(
                            |_: On<Pointer<Press>>, button_q: Single<&mut ImageNode, With<EndTurnButtonCmp>>| {
                                set_button_index(&mut button_q.into_inner(), 0);
                            },
                        )
                        .observe(
                            |_: On<Pointer<Release>>, button_q: Single<&mut ImageNode, With<EndTurnButtonCmp>>| {
                                set_button_index(&mut button_q.into_inner(), 1);
                            },
                        )
                        .observe(|_: On<Pointer<Click>>, mut start_turn_msg: MessageWriter<StartTurnMsg>, mut next_game_state: ResMut<NextState<GameState>>| {
                            start_turn_msg.write(StartTurnMsg::new(true, false));
                            next_game_state.set(GameState::Playing);
                        });
                });
        });
}
