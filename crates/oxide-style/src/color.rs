//! CSS color parsing (hex, rgb, rgba, and named colors).

/// Parses a CSS color string into an RGBA `[f32; 4]` with components in `[0.0, 1.0]`.
#[must_use]
pub fn parse_color(input: &str) -> Option<[f32; 4]> {
    let s = input.trim().to_ascii_lowercase();

    if s == "transparent" {
        return Some([0.0, 0.0, 0.0, 0.0]);
    }

    if let Some(hex) = s.strip_prefix('#') {
        return parse_hex_color(hex);
    }

    if s.starts_with("rgb(") && s.ends_with(')') {
        let inner = &s[4..s.len() - 1];
        return parse_rgb_values(inner, 1.0);
    }

    if s.starts_with("rgba(") && s.ends_with(')') {
        let inner = &s[5..s.len() - 1];
        return parse_rgba_values(inner);
    }

    // Named CSS colors
    match s.as_str() {
        "black" => Some([0.0, 0.0, 0.0, 1.0]),
        "white" => Some([1.0, 1.0, 1.0, 1.0]),
        "red" => Some([1.0, 0.0, 0.0, 1.0]),
        "green" => Some([0.0, 0.5, 0.0, 1.0]),
        "lime" => Some([0.0, 1.0, 0.0, 1.0]),
        "blue" => Some([0.0, 0.0, 1.0, 1.0]),
        "yellow" => Some([1.0, 1.0, 0.0, 1.0]),
        "cyan" | "aqua" => Some([0.0, 1.0, 1.0, 1.0]),
        "magenta" | "fuchsia" => Some([1.0, 0.0, 1.0, 1.0]),
        "gray" | "grey" => Some([0.5, 0.5, 0.5, 1.0]),
        "lightgray" | "lightgrey" => Some([0.83, 0.83, 0.83, 1.0]),
        "darkgray" | "darkgrey" => Some([0.66, 0.66, 0.66, 1.0]),
        "silver" => Some([0.75, 0.75, 0.75, 1.0]),
        "orange" => Some([1.0, 0.65, 0.0, 1.0]),
        "purple" => Some([0.5, 0.0, 0.5, 1.0]),
        "navy" => Some([0.0, 0.0, 0.5, 1.0]),
        "teal" => Some([0.0, 0.5, 0.5, 1.0]),
        "maroon" => Some([0.5, 0.0, 0.0, 1.0]),
        "olive" => Some([0.5, 0.5, 0.0, 1.0]),
        _ => None,
    }
}

fn parse_hex_color(hex: &str) -> Option<[f32; 4]> {
    match hex.len() {
        3 => {
            let r = u8::from_str_radix(&hex[0..1], 16).ok()?;
            let g = u8::from_str_radix(&hex[1..2], 16).ok()?;
            let b = u8::from_str_radix(&hex[2..3], 16).ok()?;
            Some([
                (r * 17) as f32 / 255.0,
                (g * 17) as f32 / 255.0,
                (b * 17) as f32 / 255.0,
                1.0,
            ])
        }
        6 => {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            Some([r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0])
        }
        8 => {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            let a = u8::from_str_radix(&hex[6..8], 16).ok()?;
            Some([
                r as f32 / 255.0,
                g as f32 / 255.0,
                b as f32 / 255.0,
                a as f32 / 255.0,
            ])
        }
        _ => None,
    }
}

fn parse_rgb_values(inner: &str, alpha: f32) -> Option<[f32; 4]> {
    let parts: Vec<&str> = inner.split(',').map(str::trim).collect();
    if parts.len() != 3 {
        return None;
    }

    let r = parts[0].parse::<f32>().ok()? / 255.0;
    let g = parts[1].parse::<f32>().ok()? / 255.0;
    let b = parts[2].parse::<f32>().ok()? / 255.0;

    Some([r.clamp(0.0, 1.0), g.clamp(0.0, 1.0), b.clamp(0.0, 1.0), alpha])
}

fn parse_rgba_values(inner: &str) -> Option<[f32; 4]> {
    let parts: Vec<&str> = inner.split(',').map(str::trim).collect();
    if parts.len() != 4 {
        return None;
    }

    let r = parts[0].parse::<f32>().ok()? / 255.0;
    let g = parts[1].parse::<f32>().ok()? / 255.0;
    let b = parts[2].parse::<f32>().ok()? / 255.0;
    let a = parts[3].parse::<f32>().ok()?;

    Some([r.clamp(0.0, 1.0), g.clamp(0.0, 1.0), b.clamp(0.0, 1.0), a.clamp(0.0, 1.0)])
}
