//! Shared naming, arithmetic, formatting, and color conversion helpers.

use std::fmt::Debug;
use std::time::Duration;

use bevy::prelude::Color;
use bevy_egui::egui;

/// Scales a duration, treating negative/NaN factors as zero and saturating overflow.
pub fn scale_duration(duration: Duration, scale: f32) -> Duration {
    if scale.is_nan() || scale <= 0.0 || duration.is_zero() {
        return Duration::ZERO;
    }
    if scale == 1.0 {
        return duration;
    }
    Duration::try_from_secs_f64(duration.as_secs_f64() * f64::from(scale)).unwrap_or(Duration::MAX)
}

/// Formats an integer with dots between groups of three decimal digits.
pub fn format_thousands(n: usize) -> String {
    let digits = n.to_string();
    let mut result = String::with_capacity(digits.len() + (digits.len() - 1) / 3);
    for (index, digit) in digits.bytes().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            result.push('.');
        }
        result.push(char::from(digit));
    }
    result
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

        // Debug names may be empty or start with a multibyte Unicode letter.
        if let Some(first) = name.chars().next() {
            name.replace_range(..first.len_utf8(), &first.to_uppercase().to_string());
        }

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
    /// Abbreviates thousands as `k` and millions as `M`, keeping the numeric scale consistent.
    fn fmt(self) -> String {
        match self {
            n if n >= 1_000_000 => format!("{:.2}M", self as f32 / 1_000_000.),
            n if n >= 100_000 => format!("{:.0}k", self as f32 / 1_000.),
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
        egui::Color32::from_rgba_unmultiplied(
            (c.red * 255.0) as u8,
            (c.green * 255.0) as u8,
            (c.blue * 255.0) as u8,
            (c.alpha * 255.0) as u8,
        )
    }
}

#[cfg(test)]
#[path = "../tests/core/utils.rs"]
mod tests;
