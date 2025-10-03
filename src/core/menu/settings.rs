use crate::core::assets::WorldAssets;
use crate::core::audio::ChangeAudioEv;
use crate::core::constants::*;
use crate::core::game_settings::GameSettings;
use crate::core::states::AudioState;
use crate::core::ui::utils::add_text;
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

fn match_setting(setting: &SettingsBtn, game_settings: &GameSettings) -> bool {
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
) -> impl Fn(Trigger<E>, Query<(&mut BackgroundColor, &SettingsBtn)>, ResMut<GameSettings>) {
    move |ev, mut bgcolor_q, game_settings| {
        if let Ok((mut bgcolor, setting)) = bgcolor_q.get_mut(ev.target()) {
            // Don't change the color of selected buttons
            if !match_setting(&setting, &game_settings) {
                bgcolor.0 = color;
            }
        };
    }
}

pub fn on_click_label_button(
    trigger: Trigger<Pointer<Click>>,
    mut btn_q: Query<(&mut BackgroundColor, &SettingsBtn)>,
    mut game_settings: ResMut<GameSettings>,
    mut change_audio_ev: EventWriter<ChangeAudioEv>,
) {
    match btn_q.get(trigger.target()).unwrap().1 {
        SettingsBtn::Five => game_settings.n_planets = 5,
        SettingsBtn::Ten => game_settings.n_planets = 10,
        SettingsBtn::Twenty => game_settings.n_planets = 20,
        SettingsBtn::Mute => {
            game_settings.audio = AudioState::Mute;
            change_audio_ev.write(ChangeAudioEv(Some(AudioState::Mute)));
        }
        SettingsBtn::NoMusic => {
            game_settings.audio = AudioState::NoMusic;
            change_audio_ev.write(ChangeAudioEv(Some(AudioState::NoMusic)));
        }
        SettingsBtn::Sound => {
            game_settings.audio = AudioState::Sound;
            change_audio_ev.write(ChangeAudioEv(Some(AudioState::Sound)));
        }
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
    game_settings: &GameSettings,
    assets: &WorldAssets,
    window: &Window,
) {
    parent.spawn(add_text(
        title,
        "bold",
        SUBTITLE_TEXT_SIZE,
        &assets,
        &window,
    ));

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
                    .observe(recolor_label::<Pointer<Over>>(HOVERED_BUTTON_COLOR))
                    .observe(recolor_label::<Pointer<Out>>(NORMAL_BUTTON_COLOR))
                    .observe(recolor_label::<Pointer<Pressed>>(PRESSED_BUTTON_COLOR))
                    .observe(recolor_label::<Pointer<Released>>(HOVERED_BUTTON_COLOR))
                    .observe(on_click_label_button)
                    .with_children(|parent| {
                        parent.spawn(add_text(
                            item.to_title(),
                            "bold",
                            LABEL_TEXT_SIZE,
                            assets,
                            window,
                        ));
                    });
            }
        });
}
