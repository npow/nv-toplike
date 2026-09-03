// SPDX-License-Identifier: Apache-2.0

use ratatui::style::Color;

pub const CYAN: Color = Color::Rgb(90, 220, 255);
pub const NEON_GREEN: Color = Color::Rgb(120, 255, 170);
pub const GREEN: Color = Color::Rgb(120, 255, 170);
pub const AMBER: Color = Color::Rgb(255, 200, 90);
pub const GOLD: Color = Color::Rgb(255, 215, 80);
pub const PINK: Color = Color::Rgb(255, 120, 205);
pub const PURPLE: Color = Color::Rgb(170, 130, 255);
pub const ORANGE: Color = Color::Rgb(255, 150, 80);
pub const RED: Color = Color::Rgb(255, 90, 90);
pub const MUTED: Color = Color::Rgb(120, 130, 155);
pub const DARK_GRAY: Color = Color::Rgb(70, 75, 95);
pub const WHITE: Color = Color::Rgb(240, 245, 255);

/// Convert HSV (h in 0..360, s in 0..1, v in 0..1) to Ratatui `Color::Rgb`.
pub fn hsv_to_rgb(h: f64, s: f64, v: f64) -> Color {
    let h = (h % 360.0 + 360.0) % 360.0;
    let s = s.clamp(0.0, 1.0);
    let v = v.clamp(0.0, 1.0);

    if s <= 0.001 {
        let gray = (v * 255.0).round() as u8;
        return Color::Rgb(gray, gray, gray);
    }

    let c = v * s;
    let hp = h / 60.0;
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let m = v - c;

    let (r1, g1, b1) = match hp as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };

    let r = ((r1 + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    let g = ((g1 + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    let b = ((b1 + m) * 255.0).round().clamp(0.0, 255.0) as u8;

    Color::Rgb(r, g, b)
}

/// Map temperature in Celsius to hue (0..360) for HSV color cycling:
/// Cold (<40°C): 180° (Cyan)
/// Normal (40-60°C): 180° down to 60° (Green-Yellow)
/// Warm (60-80°C): 60° down to 30° (Orange)
/// Hot (>80°C): 0° (Red)
pub fn temp_to_hue(temp_c: f64) -> f64 {
    if temp_c < 40.0 {
        180.0
    } else if temp_c < 60.0 {
        180.0 - ((temp_c - 40.0) / 20.0) * 120.0
    } else if temp_c < 80.0 {
        60.0 - ((temp_c - 60.0) / 20.0) * 30.0
    } else {
        0.0
    }
}

/// Dynamic heat color based on temperature.
pub fn heat_color(temperature: f64) -> Color {
    if temperature >= 85.0 {
        RED
    } else if temperature >= 70.0 {
        ORANGE
    } else if temperature >= 55.0 {
        AMBER
    } else if temperature >= 40.0 {
        GREEN
    } else {
        CYAN
    }
}

/// Dynamic utilization color based on ratio 0.0..1.0.
pub fn util_color(ratio: f64) -> Color {
    let r = ratio.clamp(0.0, 1.0);
    if r >= 0.85 {
        RED
    } else if r >= 0.65 {
        PINK
    } else if r >= 0.40 {
        AMBER
    } else if r >= 0.15 {
        GREEN
    } else {
        CYAN
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hsv_to_rgb_produces_valid_colors() {
        assert_eq!(hsv_to_rgb(0.0, 1.0, 1.0), Color::Rgb(255, 0, 0));
        assert_eq!(hsv_to_rgb(120.0, 1.0, 1.0), Color::Rgb(0, 255, 0));
        assert_eq!(hsv_to_rgb(240.0, 1.0, 1.0), Color::Rgb(0, 0, 255));
        assert_eq!(hsv_to_rgb(0.0, 0.0, 0.5), Color::Rgb(128, 128, 128));
    }

    #[test]
    fn temp_to_hue_gradient_is_monotonic() {
        assert_eq!(temp_to_hue(30.0), 180.0);
        assert!(temp_to_hue(50.0) < temp_to_hue(40.0));
        assert!(temp_to_hue(70.0) < temp_to_hue(50.0));
        assert_eq!(temp_to_hue(90.0), 0.0);
    }
}
