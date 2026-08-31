//! Shared naming, arithmetic, formatting, and color conversion helpers.

use std::fmt::Debug;
use std::time::Duration;

use bevy::prelude::Color;
use bevy_egui::egui;

/// Scale a Duration by a factor
pub fn scale_duration(duration: Duration, scale: f32) -> Duration {
    let sec = (duration.as_secs() as f32 + duration.subsec_nanos() as f32 * 1e-9) * scale;
    Duration::new(sec.trunc() as u64, (sec.fract() * 1e9) as u32)
}

/// Add dots to thousands
pub fn format_thousands(n: usize) -> String {
    let s = n.to_string();
    let chars: Vec<char> = s.chars().rev().collect();
    let mut result = Vec::new();

    for (i, c) in chars.iter().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push('.');
        }
        result.push(*c);
    }

    result.iter().rev().collect()
}

/// Helper function to extract only the variant name (removes tuple/struct fields)
fn extract_variant_name(text: String) -> String {
    text.split_once('(')
        .or_else(|| text.split_once('{'))
        .map(|(variant, _)| variant)
        .unwrap_or(&text)
        .trim_matches(&['"', ' '][..])
        .to_string()
}

/// Trait to get the text of an enum variant
pub trait NameFromEnum {
    /// Returns the human-readable display name.
    fn to_name(&self) -> String;
    /// Returns the lowercase asset-key form of the name.
    fn to_lowername(&self) -> String;
    /// Returns the title-cased display form of the name.
    fn to_title(&self) -> String;
}

impl<T: Debug> NameFromEnum for T {
    /// Returns the human-readable display name.
    fn to_name(&self) -> String {
        let text = extract_variant_name(format!("{:?}", self));
        let mut output = String::with_capacity(text.len() + 4);
        let mut previous_was_lowercase = false;
        for character in text.chars() {
            if character.is_uppercase() && previous_was_lowercase {
                output.push(' ');
            }
            previous_was_lowercase = character.is_lowercase();
            output.push(character);
        }
        output
    }

    /// Returns the lowercase asset-key form of the name.
    fn to_lowername(&self) -> String {
        self.to_name().to_lowercase()
    }

    /// Returns the title-cased display form of the name.
    fn to_title(&self) -> String {
        let mut name = self.to_lowername();

        // Capitalize only the first letter
        name.replace_range(0..1, &name[0..1].to_uppercase());

        name
    }
}

/// Trait to safely divide by zero
pub trait SafeDiv: Sized + PartialEq + Copy {
    /// Divides while returning zero for a zero denominator.
    fn safe_div(self, b: Self) -> Self;
}

impl SafeDiv for f32 {
    #[inline]
    /// Divides while returning zero for a zero denominator.
    fn safe_div(self, b: Self) -> Self {
        if b == 0.0 {
            0.0
        } else {
            self / b
        }
    }
}

/// Trait to convert a large number to a nice formatted string
pub trait FmtNumb {
    /// Formats this value for user-facing or diagnostic output.
    fn fmt(self) -> String;
}

impl FmtNumb for usize {
    /// Formats this value for user-facing or diagnostic output.
    fn fmt(self) -> String {
        match self {
            n if n > 1_000_000 => format!("{:.2}M", self as f32 / 1_000_000.),
            n if n > 100_000 => format!("{:.0}k", self as f32 / 100_000.),
            n if n >= 1_000 => format!("{:.1}k", self as f32 / 1_000.),
            _ => self.to_string(),
        }
    }
}

/// Trait to convert from bevy's Color to Egui's Color32
pub trait ToColor32 {
    /// Converts this color into the equivalent egui color.
    fn to_color32(self) -> egui::Color32;
}

impl ToColor32 for Color {
    /// Converts this color into the equivalent egui color.
    fn to_color32(self) -> egui::Color32 {
        let c = self.to_srgba();
        egui::Color32::from_rgba_premultiplied(
            (c.red * 255.0) as u8,
            (c.green * 255.0) as u8,
            (c.blue * 255.0) as u8,
            (c.alpha * 255.0) as u8,
        )
    }
}
