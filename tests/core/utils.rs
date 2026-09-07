use super::*;

#[test]
fn compact_numbers_keep_their_scale_at_abbreviation_boundaries() {
    for (amount, expected) in [
        (999, "999"),
        (1_000, "1.0k"),
        (100_000, "100k"),
        (150_000, "150k"),
        (999_000, "999k"),
        (1_000_000, "1.00M"),
        (1_250_000, "1.25M"),
    ] {
        assert_eq!(FmtNumb::fmt(amount), expected);
    }
}

#[test]
fn title_casing_handles_empty_multibyte_and_expanding_initial_letters() {
    assert_eq!("".to_title(), "");
    assert_eq!("étoile".to_title(), "Étoile");
    assert_eq!("ßtern".to_title(), "SStern");
    assert_eq!("LightFighter".to_title(), "Light fighter");
}

#[test]
fn duration_scaling_preserves_identity_and_handles_invalid_or_extreme_factors() {
    let duration = Duration::new(16_777_217, 123);
    assert_eq!(scale_duration(duration, 1.0), duration);
    assert_eq!(scale_duration(Duration::from_secs(1), 0.25), Duration::from_millis(250));
    assert_eq!(scale_duration(duration, f32::NAN), Duration::ZERO);
    assert_eq!(scale_duration(duration, -1.0), Duration::ZERO);
    assert_eq!(scale_duration(duration, f32::INFINITY), Duration::MAX);
    assert_eq!(scale_duration(Duration::MAX, 2.0), Duration::MAX);
    assert_eq!(scale_duration(Duration::ZERO, f32::INFINITY), Duration::ZERO);
}

#[test]
fn thousands_grouping_preserves_all_digits() {
    for (amount, expected) in
        [(0, "0"), (12, "12"), (999, "999"), (1_000, "1.000"), (1_234_567, "1.234.567")]
    {
        assert_eq!(format_thousands(amount), expected);
    }
    assert_eq!(format_thousands(usize::MAX).replace('.', ""), usize::MAX.to_string());
}

#[test]
fn transparent_bevy_colors_do_not_become_emissive_egui_colors() {
    assert_eq!(Color::srgba(1.0, 0.5, 1.0, 0.0).to_color32(), egui::Color32::TRANSPARENT);
    assert_eq!(Color::srgba(1.0, 0.0, 0.0, 1.0).to_color32(), egui::Color32::RED);
    let translucent = Color::srgba(1.0, 0.0, 0.0, 0.5).to_color32();
    assert_eq!(translucent.a(), 127);
    assert!(translucent.r() < 255);
}
