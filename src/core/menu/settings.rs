use crate::core::assets::WorldAssets;
use crate::core::audio::ChangeAudioMsg;
use crate::core::constants::*;
use crate::core::menu::utils::add_text;
use crate::core::settings::Settings;
use crate::core::states::AudioState;
use crate::utils::NameFromEnum;
use bevy::prelude::*;
use std::fmt::Debug;

#[derive(Component, Clone, Debug, PartialEq)]
pub enum SettingsBtn {
    Five,
    Ten,
    Twenty,
    Mute,
    NoMusic,
    Sound,
}

fn match_setting(setting: &SettingsBtn, game_settings: &Settings) -> bool {
    match setting {
        SettingsBtn::Five => game_settings.n_planets == 5,
        SettingsBtn::Ten => game_settings.n_planets == 10,
        SettingsBtn::Twenty => game_settings.n_planets == 20,
        SettingsBtn::Mute => game_settings.audio == AudioState::Mute,
        SettingsBtn::NoMusic => game_settings.audio == AudioState::NoMusic,
        SettingsBtn::Sound => game_settings.audio == AudioState::Sound,
    }
}

pub fn recolor_label<E: Debug + Clone + Reflect>(
    color: Color,
) -> impl Fn(On<Pointer<E>>, Query<(&mut BackgroundColor, &SettingsBtn)>, ResMut<Settings>) {
    move |ev, mut bgcolor_q, game_settings| {
        if let Ok((mut bgcolor, setting)) = bgcolor_q.get_mut(ev.entity) {
            // Don't change the color of selected buttons
            if !match_setting(&setting, &game_settings) {
                bgcolor.0 = color;
            }
        };
    }
}

pub fn on_click_label_button(
    event: On<Pointer<Click>>,
    mut btn_q: Query<(&mut BackgroundColor, &SettingsBtn)>,
    mut game_settings: ResMut<Settings>,
    mut change_audio_ev: MessageWriter<ChangeAudioMsg>,
) {
    match btn_q.get(event.entity).unwrap().1 {
        SettingsBtn::Five => game_settings.n_planets = 5,
        SettingsBtn::Ten => game_settings.n_planets = 10,
        SettingsBtn::Twenty => game_settings.n_planets = 20,
        SettingsBtn::Mute => {
            game_settings.audio = AudioState::Mute;
            change_audio_ev.write(ChangeAudioMsg(Some(AudioState::Mute)));
        },
        SettingsBtn::NoMusic => {
            game_settings.audio = AudioState::NoMusic;
            change_audio_ev.write(ChangeAudioMsg(Some(AudioState::NoMusic)));
        },
        SettingsBtn::Sound => {
            game_settings.audio = AudioState::Sound;
            change_audio_ev.write(ChangeAudioMsg(Some(AudioState::Sound)));
        },
    }

    // Reset the color of the other buttons
    for (mut bgcolor, setting) in &mut btn_q {
        if !match_setting(setting, &game_settings) {
            bgcolor.0 = NORMAL_BUTTON_COLOR;
        }
    }
}

pub fn spawn_label(
    parent: &mut ChildSpawnerCommands,
    title: &str,
    buttons: Vec<SettingsBtn>,
    game_settings: &Settings,
    assets: &WorldAssets,
    window: &Window,
) {
    parent.spawn(add_text(title, "bold", TITLE_TEXT_SIZE, &assets, &window));

    parent
        .spawn(Node {
            width: Val::Percent(100.),
            height: Val::Percent(100.),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            flex_direction: FlexDirection::Row,
            padding: UiRect {
                top: Val::Percent(3.),
                left: Val::Percent(5.),
                right: Val::Percent(5.),
                bottom: Val::Percent(7.),
            },
            ..default()
        })
        .with_children(|parent| {
            for item in buttons.iter() {
                parent
                    .spawn((
                        Node {
                            width: Val::Percent(30.),
                            height: Val::Percent(100.),
                            align_items: AlignItems::Center,
                            justify_content: JustifyContent::Center,
                            margin: UiRect::all(Val::Percent(1.)),
                            ..default()
                        },
                        BackgroundColor(if match_setting(item, game_settings) {
                            PRESSED_BUTTON_COLOR
                        } else {
                            NORMAL_BUTTON_COLOR
                        }),
                        item.clone(),
                        Button,
                    ))
                    .observe(recolor_label::<Over>(HOVERED_BUTTON_COLOR))
                    .observe(recolor_label::<Out>(NORMAL_BUTTON_COLOR))
                    .observe(recolor_label::<Press>(PRESSED_BUTTON_COLOR))
                    .observe(recolor_label::<Release>(HOVERED_BUTTON_COLOR))
                    .observe(on_click_label_button)
                    .with_children(|parent| {
                        parent.spawn(add_text(
                            item.to_title(),
                            "bold",
                            SUBTITLE_TEXT_SIZE,
                            assets,
                            window,
                        ));
                    });
            }
        });
}
