use super::*;

#[test]
fn saved_preferences_leave_game_rules_and_transient_playback_state_untouched() {
    let mut settings = Settings {
        n_planets: 20,
        p_colonizable: 50,
        p_moons: 10,
        turn: 7,
        combat_paused: true,
        ..Settings::default()
    };
    let mut preferences = GamePreferences {
        show_cells: false,
        combat_speed: 8.0,
        combat_view: CombatView::Cinematic,
        ..GamePreferences::default()
    };
    preferences.audio = AudioState::Mute;
    preferences.volume = 0.0;
    preferences.unmuted_audio = AudioState::Sound;
    preferences.unmuted_volume = 0.35;
    let json = serde_json::to_string(&preferences).unwrap();
    settings.apply_game_preferences(serde_json::from_str(&json).unwrap());
    assert_eq!(settings.game_preferences(), preferences);
    assert_eq!(
        (settings.n_planets, settings.p_colonizable, settings.p_moons, settings.turn),
        (20, 50, 10, 7)
    );
    assert!(settings.combat_paused);
    settings.set_audio_mode(settings.restored_audio_mode());
    assert_eq!(settings.audio, AudioState::Sound);
    assert_eq!(settings.volume, 0.35);
}

#[test]
fn preference_validation_rejects_nonfinite_and_out_of_range_values() {
    for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -0.01, 1.01] {
        assert!(GamePreferences {
            volume: value,
            ..GamePreferences::default()
        }
        .validate()
        .is_err());
    }
    for value in [f32::NAN, 0.0, -1.0, 1.01] {
        assert!(GamePreferences {
            unmuted_volume: value,
            ..GamePreferences::default()
        }
        .validate()
        .is_err());
    }
    for value in [f32::NAN, f32::INFINITY, 0.0, 0.24, 64.01] {
        assert!(GamePreferences {
            combat_speed: value,
            ..GamePreferences::default()
        }
        .validate()
        .is_err());
    }
    for value in [0.25, 1.0, 64.0] {
        assert!(GamePreferences {
            combat_speed: value,
            ..GamePreferences::default()
        }
        .validate()
        .is_ok());
    }
}
