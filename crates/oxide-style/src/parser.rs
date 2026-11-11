//! CSS rules and inline declarations parser.

use crate::color::parse_color;
use crate::computed::{
    AlignItems, ComputedStyle, Display, FlexDirection, JustifyContent, TextAlign,
};
use std::collections::HashMap;

/// CSS Selector kinds for style rule matching.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Selector {
    Tag(String),
    Class(String),
    Id(String),
    Compound {
        tag: Option<String>,
        class: Option<String>,
        id: Option<String>,
    },
    Universal,
}

/// A parsed CSS rule consisting of a selector and declarations.
#[derive(Debug, Clone)]
pub struct CssRule {
    pub selector: Selector,
    pub declarations: HashMap<String, String>,
}

/// Parses an author stylesheet string (e.g. from `<style>` tags) into rules.
#[must_use]
pub fn parse_stylesheet(css: &str) -> Vec<CssRule> {
    let mut rules = Vec::new();
    let clean = strip_comments(css);

    for block in clean.split('}') {
        let trimmed = block.trim();
        if trimmed.is_empty() {
            continue;
        }

        if let Some((sel_part, decl_part)) = trimmed.split_once('{') {
            let decls = parse_declarations(decl_part);
            for single_sel in sel_part.split(',') {
                let sel_str = single_sel.trim();
                if let Some(selector) = parse_single_selector(sel_str) {
                    rules.push(CssRule {
                        selector,
                        declarations: decls.clone(),
                    });
                }
            }
        }
    }

    rules
}

fn parse_single_selector(sel: &str) -> Option<Selector> {
    // Strip pseudo-classes like :link, :visited, :hover, :active, :focus, ::before, ::after
    let clean = sel.split(':').next().unwrap_or(sel).trim();
    if clean.is_empty() {
        return None;
    }

    if clean == "*" {
        return Some(Selector::Universal);
    }

    if let Some(class_name) = clean.strip_prefix('.') {
        return Some(Selector::Class(class_name.trim().to_ascii_lowercase()));
    }

    if let Some(id_name) = clean.strip_prefix('#') {
        return Some(Selector::Id(id_name.trim().to_ascii_lowercase()));
    }

    // Check compound selector like tag.class or tag#id
    if let Some((tag, class)) = clean.split_once('.') {
        return Some(Selector::Compound {
            tag: Some(tag.trim().to_ascii_lowercase()),
            class: Some(class.trim().to_ascii_lowercase()),
            id: None,
        });
    }

    if let Some((tag, id)) = clean.split_once('#') {
        return Some(Selector::Compound {
            tag: Some(tag.trim().to_ascii_lowercase()),
            class: None,
            id: Some(id.trim().to_ascii_lowercase()),
        });
    }

    Some(Selector::Tag(clean.to_ascii_lowercase()))
}

/// Parses a semicolon-separated string of CSS declarations (like `style="..."`).
#[must_use]
pub fn parse_declarations(input: &str) -> HashMap<String, String> {
    let mut decls = HashMap::new();
    for statement in input.split(';') {
        let stmt = statement.trim();
        if stmt.is_empty() {
            continue;
        }
        if let Some((k, v)) = stmt.split_once(':') {
            let key = k.trim().to_ascii_lowercase();
            let val = v.trim().to_string();
            if !key.is_empty() && !val.is_empty() {
                decls.insert(key, val);
            }
        }
    }
    decls
}

/// Applies a set of CSS declaration key-value pairs to mutate a `ComputedStyle`.
pub fn apply_declarations(style: &mut ComputedStyle, declarations: &HashMap<String, String>) {
    for (prop, val) in declarations {
        apply_single_declaration(style, prop, val);
    }
}

fn apply_single_declaration(style: &mut ComputedStyle, prop: &str, val: &str) {
    let v = val.trim();
    match prop {
        "display" => match v.to_ascii_lowercase().as_str() {
            "none" => style.display = Display::None,
            "block" => style.display = Display::Block,
            "inline" | "inline-block" => style.display = Display::Inline,
            "flex" => style.display = Display::Flex,
            _ => {}
        },
        "flex-direction" => match v.to_ascii_lowercase().as_str() {
            "row" => style.flex_direction = FlexDirection::Row,
            "column" => style.flex_direction = FlexDirection::Column,
            "row-reverse" => style.flex_direction = FlexDirection::RowReverse,
            "column-reverse" => style.flex_direction = FlexDirection::ColumnReverse,
            _ => {}
        },
        "justify-content" => match v.to_ascii_lowercase().as_str() {
            "flex-start" | "start" => style.justify_content = Some(JustifyContent::FlexStart),
            "center" => style.justify_content = Some(JustifyContent::Center),
            "flex-end" | "end" => style.justify_content = Some(JustifyContent::FlexEnd),
            "space-between" => style.justify_content = Some(JustifyContent::SpaceBetween),
            "space-around" => style.justify_content = Some(JustifyContent::SpaceAround),
            "space-evenly" => style.justify_content = Some(JustifyContent::SpaceEvenly),
            _ => {}
        },
        "align-items" => match v.to_ascii_lowercase().as_str() {
            "flex-start" | "start" => style.align_items = Some(AlignItems::FlexStart),
            "center" => style.align_items = Some(AlignItems::Center),
            "flex-end" | "end" => style.align_items = Some(AlignItems::FlexEnd),
            "stretch" => style.align_items = Some(AlignItems::Stretch),
            _ => {}
        },
        "gap" => {
            let parts: Vec<f32> = v.split_whitespace().filter_map(parse_pixel_val).collect();
            if parts.len() == 1 {
                style.gap = [parts[0], parts[0]];
            } else if parts.len() >= 2 {
                style.gap = [parts[0], parts[1]];
            }
        }
        "width" => {
            if v == "auto" {
                style.width = None;
            } else if let Some(px) = parse_pixel_val(v) {
                style.width = Some(px);
            }
        }
        "height" => {
            if v == "auto" {
                style.height = None;
            } else if let Some(px) = parse_pixel_val(v) {
                style.height = Some(px);
            }
        }
        "margin" => {
            parse_margin_shorthand(v, &mut style.margin, &mut style.margin_auto);
        }
        "border-radius" => {
            if let Some(px) = parse_pixel_val(v) {
                style.border_radius = px;
            }
        }
        "margin-top" => {
            if let Some(px) = parse_pixel_val(v) {
                style.margin[0] = px;
            }
        }
        "margin-right" => {
            if let Some(px) = parse_pixel_val(v) {
                style.margin[1] = px;
            }
        }
        "margin-bottom" => {
            if let Some(px) = parse_pixel_val(v) {
                style.margin[2] = px;
            }
        }
        "margin-left" => {
            if let Some(px) = parse_pixel_val(v) {
                style.margin[3] = px;
            }
        }
        "padding" => {
            if let Some(rect) = parse_rect_shorthand(v) {
                style.padding = rect;
            }
        }
        "padding-top" => {
            if let Some(px) = parse_pixel_val(v) {
                style.padding[0] = px;
            }
        }
        "padding-right" => {
            if let Some(px) = parse_pixel_val(v) {
                style.padding[1] = px;
            }
        }
        "padding-bottom" => {
            if let Some(px) = parse_pixel_val(v) {
                style.padding[2] = px;
            }
        }
        "padding-left" => {
            if let Some(px) = parse_pixel_val(v) {
                style.padding[3] = px;
            }
        }
        "background" | "background-color" => {
            if let Some(c) = parse_color(v) {
                style.background_color = Some(c);
            }
        }
        "color" => {
            if let Some(c) = parse_color(v) {
                style.color = c;
            }
        }
        "font-size" => {
            if let Some(px) = parse_pixel_val(v) {
                style.font_size = px;
            }
        }
        "font-weight" => match v.to_ascii_lowercase().as_str() {
            "bold" | "bolder" | "700" | "800" | "900" => style.font_weight = 700.0,
            "normal" | "lighter" | "400" | "300" | "200" | "100" => style.font_weight = 400.0,
            _ => {
                if let Ok(num) = v.parse::<f32>() {
                    style.font_weight = num;
                }
            }
        },
        "line-height" => {
            if let Ok(num) = v.parse::<f32>() {
                style.line_height = num;
            } else if let Some(px) = parse_pixel_val(v) {
                style.line_height = px / style.font_size.max(1.0);
            }
        }
        "text-align" => match v.to_ascii_lowercase().as_str() {
            "center" => style.text_align = TextAlign::Center,
            "right" => style.text_align = TextAlign::Right,
            "left" => style.text_align = TextAlign::Left,
            _ => {}
        },
        "border" => {
            // Shorthand e.g. "1px solid #38bdf8"
            for token in v.split_whitespace() {
                if let Some(px) = parse_pixel_val(token) {
                    style.border_width = [px, px, px, px];
                } else if let Some(c) = parse_color(token) {
                    style.border_color = c;
                }
            }
        }
        "border-width" => {
            if let Some(rect) = parse_rect_shorthand(v) {
                style.border_width = rect;
            }
        }
        "border-color" => {
            if let Some(c) = parse_color(v) {
                style.border_color = c;
            }
        }
        _ => {}
    }
}

/// Parses a pixel, rem, or em value into a scalar f32 in pixels.
fn parse_pixel_val(s: &str) -> Option<f32> {
    let trimmed = s.trim();
    if let Some(px_str) = trimmed.strip_suffix("px") {
        px_str.trim().parse::<f32>().ok()
    } else if let Some(rem_str) = trimmed.strip_suffix("rem") {
        rem_str.trim().parse::<f32>().ok().map(|r| r * 16.0)
    } else if let Some(em_str) = trimmed.strip_suffix("em") {
        em_str.trim().parse::<f32>().ok().map(|e| e * 16.0)
    } else if let Some(pt_str) = trimmed.strip_suffix("pt") {
        pt_str.trim().parse::<f32>().ok().map(|pt| pt * 1.333)
    } else if trimmed == "0" {
        Some(0.0)
    } else {
        trimmed.parse::<f32>().ok()
    }
}

/// Expands CSS box 1..4 value shorthands into `[top, right, bottom, left]`.
fn parse_rect_shorthand(s: &str) -> Option<[f32; 4]> {
    let parts: Vec<f32> = s.split_whitespace().filter_map(parse_pixel_val).collect();
    match parts.len() {
        1 => Some([parts[0], parts[0], parts[0], parts[0]]),
        2 => Some([parts[0], parts[1], parts[0], parts[1]]),
        3 => Some([parts[0], parts[1], parts[2], parts[1]]),
        4 => Some([parts[0], parts[1], parts[2], parts[3]]),
        _ => None,
    }
}

/// Parses margin shorthand with support for `auto` centering.
fn parse_margin_shorthand(s: &str, margin: &mut [f32; 4], margin_auto: &mut [bool; 4]) {
    let tokens: Vec<&str> = s.split_whitespace().collect();
    let parse_token = |t: &str| -> (f32, bool) {
        if t == "auto" {
            (0.0, true)
        } else {
            (parse_pixel_val(t).unwrap_or(0.0), false)
        }
    };

    let parsed: Vec<(f32, bool)> = tokens.iter().map(|&t| parse_token(t)).collect();
    match parsed.len() {
        1 => {
            let (val, auto) = parsed[0];
            *margin = [val, val, val, val];
            *margin_auto = [auto, auto, auto, auto];
        }
        2 => {
            let (v_val, v_auto) = parsed[0];
            let (h_val, h_auto) = parsed[1];
            *margin = [v_val, h_val, v_val, h_val];
            *margin_auto = [v_auto, h_auto, v_auto, h_auto];
        }
        3 => {
            let (top_val, top_auto) = parsed[0];
            let (h_val, h_auto) = parsed[1];
            let (bot_val, bot_auto) = parsed[2];
            *margin = [top_val, h_val, bot_val, h_val];
            *margin_auto = [top_auto, h_auto, bot_auto, h_auto];
        }
        4 => {
            *margin = [parsed[0].0, parsed[1].0, parsed[2].0, parsed[3].0];
            *margin_auto = [parsed[0].1, parsed[1].1, parsed[2].1, parsed[3].1];
        }
        _ => {}
    }
}

fn strip_comments(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '/' && chars.peek() == Some(&'*') {
            chars.next(); // consume '*'
            while let Some(inner) = chars.next() {
                if inner == '*' && chars.peek() == Some(&'/') {
                    chars.next(); // consume '/'
                    break;
                }
            }
        } else {
            out.push(ch);
        }
    }

    out
}
