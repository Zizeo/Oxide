//! CSS rules and inline declarations parser.

use crate::color::parse_color;
use crate::computed::{
    AlignItems, BoxShadow, ComputedStyle, Display, FlexDirection, Gradient, JustifyContent,
    Overflow, Position, TextAlign, Transform2D,
};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Combinator {
    Descendant,
    Child,
    AdjacentSibling,
    GeneralSibling,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PseudoClass {
    Hover,
    Active,
    Focus,
    NthChild(usize),
    Not(Box<SimpleSelector>),
    Is(Vec<SimpleSelector>),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SimpleSelector {
    pub universal: bool,
    pub tag: Option<String>,
    pub id: Option<String>,
    pub classes: Vec<String>,
    pub pseudos: Vec<PseudoClass>,
}

impl SimpleSelector {
    pub fn specificity(&self) -> (u16, u16, u16) {
        let a = if self.id.is_some() { 1 } else { 0 };
        let b = self.classes.len() as u16 + self.pseudos.len() as u16;
        let c = if self.tag.is_some() { 1 } else { 0 };
        (a, b, c)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComplexSelector {
    /// Ordered left-to-right; each entry holds the simple selector and the
    /// combinator connecting it to the *next* entry.
    pub parts: Vec<(SimpleSelector, Option<Combinator>)>,
}

impl ComplexSelector {
    pub fn specificity(&self) -> (u16, u16, u16) {
        self.parts.iter().fold((0, 0, 0), |acc, (s, _)| {
            let (a, b, c) = s.specificity();
            (acc.0 + a, acc.1 + b, acc.2 + c)
        })
    }
}

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
    Complex(ComplexSelector),
}

impl Selector {
    pub fn specificity(&self) -> (u16, u16, u16) {
        match self {
            Selector::Universal => (0, 0, 0),
            Selector::Tag(_) => (0, 0, 1),
            Selector::Class(_) => (0, 1, 0),
            Selector::Id(_) => (1, 0, 0),
            Selector::Compound { tag, class, id } => (
                u16::from(id.is_some()),
                u16::from(class.is_some()),
                u16::from(tag.is_some()),
            ),
            Selector::Complex(c) => c.specificity(),
        }
    }
}

/// A parsed CSS rule consisting of a selector and declarations.
#[derive(Debug, Clone)]
pub struct CssRule {
    pub selector: Selector,
    pub declarations: HashMap<String, String>,
}

/// Parses an author stylesheet string using a default `1280x720` light viewport.
#[must_use]
pub fn parse_stylesheet(css: &str) -> Vec<CssRule> {
    parse_stylesheet_for_viewport(css, 1280.0, 720.0, false)
}

/// Parses an author stylesheet, evaluating `@media` blocks against viewport size
/// and `prefers-color-scheme`.
#[must_use]
pub fn parse_stylesheet_for_viewport(
    css: &str,
    viewport_width: f32,
    viewport_height: f32,
    dark_mode: bool,
) -> Vec<CssRule> {
    let clean = strip_comments(css);
    let mut rules = Vec::new();
    let mut i = 0;
    let bytes = clean.as_bytes();

    while i < bytes.len() {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }

        if clean[i..].starts_with("@media")
            && let Some(open_rel) = clean[i..].find('{')
        {
            let cond = clean[i + 6..i + open_rel].trim();
            let block_start = i + open_rel + 1;
            if let Some(block_end) = find_matching_brace(&clean, block_start) {
                if eval_media_query(cond, viewport_width, viewport_height, dark_mode) {
                    rules.extend(parse_stylesheet_for_viewport(
                        &clean[block_start..block_end],
                        viewport_width,
                        viewport_height,
                        dark_mode,
                    ));
                }
                i = block_end + 1;
                continue;
            }
        }

        let Some(open_rel) = clean[i..].find('{') else {
            break;
        };
        let sel_part = clean[i..i + open_rel].trim();
        let decl_start = i + open_rel + 1;
        let Some(decl_end) = find_matching_brace(&clean, decl_start) else {
            break;
        };
        let decl_part = &clean[decl_start..decl_end];
        if !sel_part.starts_with('@') {
            let decls = parse_declarations(decl_part);
            for single_sel in split_top_level(sel_part, ',') {
                if let Some(selector) = parse_single_selector(single_sel.trim()) {
                    rules.push(CssRule {
                        selector,
                        declarations: decls.clone(),
                    });
                }
            }
        }
        i = decl_end + 1;
    }

    rules
}

fn find_matching_brace(s: &str, after_open: usize) -> Option<usize> {
    let mut depth = 1usize;
    for (idx, ch) in s[after_open..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(after_open + idx);
                }
            }
            _ => {}
        }
    }
    None
}

fn eval_media_query(cond: &str, vw: f32, vh: f32, dark_mode: bool) -> bool {
    for part in cond.split("and") {
        let p = part.trim().trim_start_matches('(').trim_end_matches(')').trim();
        if let Some((k, v)) = p.split_once(':') {
            let k = k.trim().to_ascii_lowercase();
            let v = v.trim().to_ascii_lowercase();
            match k.as_str() {
                "min-width" => {
                    if parse_pixel_val(&v).is_some_and(|min| vw < min) {
                        return false;
                    }
                }
                "max-width" => {
                    if parse_pixel_val(&v).is_some_and(|max| vw > max) {
                        return false;
                    }
                }
                "min-height" => {
                    if parse_pixel_val(&v).is_some_and(|min| vh < min) {
                        return false;
                    }
                }
                "max-height" => {
                    if parse_pixel_val(&v).is_some_and(|max| vh > max) {
                        return false;
                    }
                }
                "prefers-color-scheme" => {
                    if (v == "dark" && !dark_mode) || (v == "light" && dark_mode) {
                        return false;
                    }
                }
                _ => {}
            }
        }
    }
    true
}

fn parse_single_selector(sel: &str) -> Option<Selector> {
    let trimmed = sel.trim();
    if trimmed.is_empty() {
        return None;
    }

    // Fast path for simple selectors without combinators or functional pseudos.
    if !trimmed.contains([' ', '>', '+', '~', ':']) {
        if trimmed == "*" {
            return Some(Selector::Universal);
        }
        if let Some(c) = trimmed.strip_prefix('.') {
            return Some(Selector::Class(c.trim().to_ascii_lowercase()));
        }
        if let Some(id) = trimmed.strip_prefix('#') {
            return Some(Selector::Id(id.trim().to_ascii_lowercase()));
        }
        if let Some((tag, class)) = trimmed.split_once('.')
            && !class.contains('.')
        {
            return Some(Selector::Compound {
                tag: Some(tag.trim().to_ascii_lowercase()),
                class: Some(class.trim().to_ascii_lowercase()),
                id: None,
            });
        }
        if let Some((tag, id)) = trimmed.split_once('#') {
            return Some(Selector::Compound {
                tag: Some(tag.trim().to_ascii_lowercase()),
                class: None,
                id: Some(id.trim().to_ascii_lowercase()),
            });
        }
        return Some(Selector::Tag(trimmed.to_ascii_lowercase()));
    }

    parse_complex_selector(trimmed).map(Selector::Complex)
}

pub(crate) fn parse_complex_selector(sel: &str) -> Option<ComplexSelector> {
    let mut parts: Vec<(SimpleSelector, Option<Combinator>)> = Vec::new();
    let chars: Vec<char> = sel.chars().collect();
    let mut i = 0;
    let mut pending_comb: Option<Combinator> = None;

    while i < chars.len() {
        while i < chars.len() && chars[i].is_ascii_whitespace() {
            if pending_comb.is_none() && !parts.is_empty() {
                pending_comb = Some(Combinator::Descendant);
            }
            i += 1;
        }
        if i >= chars.len() {
            break;
        }
        match chars[i] {
            '>' => {
                pending_comb = Some(Combinator::Child);
                i += 1;
                continue;
            }
            '+' => {
                pending_comb = Some(Combinator::AdjacentSibling);
                i += 1;
                continue;
            }
            '~' => {
                pending_comb = Some(Combinator::GeneralSibling);
                i += 1;
                continue;
            }
            _ => {}
        }

        let start = i;
        let mut paren_depth = 0usize;
        while i < chars.len() {
            let ch = chars[i];
            if ch == '(' {
                paren_depth += 1;
            } else if ch == ')' && paren_depth > 0 {
                paren_depth -= 1;
            } else if paren_depth == 0 && (ch.is_ascii_whitespace() || matches!(ch, '>' | '+' | '~'))
            {
                break;
            }
            i += 1;
        }

        let token: String = chars[start..i].iter().collect();
        let simple = parse_simple_selector(&token)?;
        if let Some(comb) = pending_comb.take()
            && let Some(prev) = parts.last_mut()
        {
            prev.1 = Some(comb);
        }
        parts.push((simple, None));
    }

    if parts.is_empty() {
        None
    } else {
        Some(ComplexSelector { parts })
    }
}

fn parse_simple_selector(tok: &str) -> Option<SimpleSelector> {
    if tok.is_empty() {
        return None;
    }
    let mut out = SimpleSelector::default();
    let chars: Vec<char> = tok.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        match chars[i] {
            '*' => {
                out.universal = true;
                i += 1;
            }
            '.' => {
                i += 1;
                let s = i;
                while i < chars.len() && !matches!(chars[i], '.' | '#' | ':') {
                    i += 1;
                }
                let name: String = chars[s..i].iter().collect();
                if !name.is_empty() {
                    out.classes.push(name.to_ascii_lowercase());
                }
            }
            '#' => {
                i += 1;
                let s = i;
                while i < chars.len() && !matches!(chars[i], '.' | '#' | ':') {
                    i += 1;
                }
                let name: String = chars[s..i].iter().collect();
                if !name.is_empty() {
                    out.id = Some(name.to_ascii_lowercase());
                }
            }
            ':' => {
                i += 1;
                if i < chars.len() && chars[i] == ':' {
                    i += 1; // ::before / ::after
                }
                let s = i;
                while i < chars.len() && !matches!(chars[i], '.' | '#' | ':' | '(') {
                    i += 1;
                }
                let name: String = chars[s..i].iter().collect();
                let mut arg = None;
                if i < chars.len() && chars[i] == '(' {
                    i += 1;
                    let arg_s = i;
                    let mut depth = 1usize;
                    while i < chars.len() && depth > 0 {
                        if chars[i] == '(' {
                            depth += 1;
                        } else if chars[i] == ')' {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        i += 1;
                    }
                    arg = Some(chars[arg_s..i].iter().collect::<String>());
                    if i < chars.len() && chars[i] == ')' {
                        i += 1;
                    }
                }
                match name.to_ascii_lowercase().as_str() {
                    "hover" => out.pseudos.push(PseudoClass::Hover),
                    "active" => out.pseudos.push(PseudoClass::Active),
                    "focus" => out.pseudos.push(PseudoClass::Focus),
                    "nth-child" => {
                        if let Some(n) = arg.and_then(|a| a.trim().parse::<usize>().ok()) {
                            out.pseudos.push(PseudoClass::NthChild(n));
                        }
                    }
                    "not" => {
                        if let Some(inner) = arg.and_then(|a| parse_simple_selector(a.trim())) {
                            out.pseudos.push(PseudoClass::Not(Box::new(inner)));
                        }
                    }
                    "is" => {
                        if let Some(a) = arg {
                            let list: Vec<SimpleSelector> = split_top_level(&a, ',')
                                .into_iter()
                                .filter_map(|p| parse_simple_selector(p.trim()))
                                .collect();
                            if !list.is_empty() {
                                out.pseudos.push(PseudoClass::Is(list));
                            }
                        }
                    }
                    _ => {}
                }
            }
            _ => {
                let s = i;
                while i < chars.len() && !matches!(chars[i], '.' | '#' | ':') {
                    i += 1;
                }
                let tag: String = chars[s..i].iter().collect();
                if !tag.is_empty() {
                    out.tag = Some(tag.to_ascii_lowercase());
                }
            }
        }
    }
    Some(out)
}

fn split_top_level(input: &str, sep: char) -> Vec<&str> {
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (idx, ch) in input.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' if depth > 0 => depth -= 1,
            c if c == sep && depth == 0 => {
                out.push(&input[start..idx]);
                start = idx + c.len_utf8();
            }
            _ => {}
        }
    }
    out.push(&input[start..]);
    out
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
            let key = k.trim();
            let key_norm = if key.starts_with("--") {
                key.to_string()
            } else {
                key.to_ascii_lowercase()
            };
            let val = v.trim().to_string();
            if !key_norm.is_empty() && !val.is_empty() {
                decls.insert(key_norm, val);
            }
        }
    }
    decls
}

/// Applies all declarations (both normal and `!important`) to `style`.
pub fn apply_declarations(style: &mut ComputedStyle, declarations: &HashMap<String, String>) {
    apply_declarations_filtered(style, declarations, None);
}

/// Applies declarations filtered by `!important` (`Some(false)` = normal only,
/// `Some(true)` = `!important` only, `None` = all).
pub fn apply_declarations_filtered(
    style: &mut ComputedStyle,
    declarations: &HashMap<String, String>,
    only_important: Option<bool>,
) {
    // First store any `--custom-prop` definitions so `var()` on the same rule resolves.
    for (prop, raw_val) in declarations {
        if prop.starts_with("--") {
            let (clean, is_imp) = strip_important(raw_val);
            if only_important.is_none_or(|want| want == is_imp) {
                style.custom_props.insert(prop.clone(), clean.to_string());
            }
        }
    }

    for (prop, raw_val) in declarations {
        if prop.starts_with("--") {
            continue;
        }
        let (clean, is_imp) = strip_important(raw_val);
        if only_important.is_some_and(|want| want != is_imp) {
            continue;
        }
        let resolved = resolve_css_vars(clean, &style.custom_props);
        apply_single_declaration(style, prop, &resolved);
    }
}

fn strip_important(val: &str) -> (&str, bool) {
    let trimmed = val.trim();
    if let Some(idx) = trimmed.to_ascii_lowercase().rfind("!important") {
        (trimmed[..idx].trim(), true)
    } else {
        (trimmed, false)
    }
}

fn resolve_css_vars(val: &str, props: &HashMap<String, String>) -> String {
    let mut out = val.to_string();
    for _ in 0..8 {
        let Some(start) = out.find("var(") else {
            break;
        };
        let mut depth = 1usize;
        let mut end = None;
        for (idx, ch) in out[start + 4..].char_indices() {
            match ch {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(start + 4 + idx);
                        break;
                    }
                }
                _ => {}
            }
        }
        let Some(end_idx) = end else {
            break;
        };
        let inner = &out[start + 4..end_idx];
        let (var_name, fallback) = match inner.split_once(',') {
            Some((n, f)) => (n.trim(), Some(f.trim())),
            None => (inner.trim(), None),
        };
        let replacement = props
            .get(var_name)
            .map(String::as_str)
            .or(fallback)
            .unwrap_or("")
            .to_string();
        out.replace_range(start..=end_idx, &replacement);
    }
    out
}

fn apply_single_declaration(style: &mut ComputedStyle, prop: &str, val: &str) {
    let v = val.trim();
    match prop {
        "display" => match v.to_ascii_lowercase().as_str() {
            "none" => style.display = Display::None,
            "block" => style.display = Display::Block,
            "inline" | "inline-block" => style.display = Display::Inline,
            "flex" => style.display = Display::Flex,
            "grid" => style.display = Display::Grid,
            _ => {}
        },
        "position" => match v.to_ascii_lowercase().as_str() {
            "static" => style.position = Position::Static,
            "relative" => style.position = Position::Relative,
            "absolute" => style.position = Position::Absolute,
            "fixed" => style.position = Position::Fixed,
            "sticky" => style.position = Position::Sticky,
            _ => {}
        },
        "top" => style.inset[0] = parse_pixel_val(v),
        "right" => style.inset[1] = parse_pixel_val(v),
        "bottom" => style.inset[2] = parse_pixel_val(v),
        "left" => style.inset[3] = parse_pixel_val(v),
        "z-index" => {
            if let Ok(z) = v.parse::<i32>() {
                style.z_index = z;
            }
        }
        "overflow" => match v.to_ascii_lowercase().as_str() {
            "visible" => style.overflow = Overflow::Visible,
            "hidden" => style.overflow = Overflow::Hidden,
            "scroll" => style.overflow = Overflow::Scroll,
            "auto" => style.overflow = Overflow::Auto,
            _ => {}
        },
        "text-overflow" => {
            style.text_overflow_ellipsis = v.eq_ignore_ascii_case("ellipsis");
        }
        "grid-template-columns" => {
            style.grid_template_columns = v.split_whitespace().map(str::to_string).collect();
        }
        "grid-template-rows" => {
            style.grid_template_rows = v.split_whitespace().map(str::to_string).collect();
        }
        "opacity" => {
            if let Ok(op) = v.parse::<f32>() {
                style.opacity = op.clamp(0.0, 1.0);
            }
        }
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
            if let Some(rect) = parse_rect_shorthand(v) {
                style.border_radius = rect[0];
                style.border_radii = rect;
            }
        }
        "border-top-left-radius" => {
            if let Some(px) = parse_pixel_val(v) {
                style.border_radii[0] = px;
            }
        }
        "border-top-right-radius" => {
            if let Some(px) = parse_pixel_val(v) {
                style.border_radii[1] = px;
            }
        }
        "border-bottom-right-radius" => {
            if let Some(px) = parse_pixel_val(v) {
                style.border_radii[2] = px;
            }
        }
        "border-bottom-left-radius" => {
            if let Some(px) = parse_pixel_val(v) {
                style.border_radii[3] = px;
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
        "background" | "background-color" | "background-image" => {
            if let Some(grad) = parse_gradient(v) {
                style.background_gradient = Some(grad);
            } else if let Some(c) = parse_color(v) {
                style.background_color = Some(c);
            }
        }
        "box-shadow" => {
            style.box_shadow = parse_box_shadow(v);
        }
        "transform" => {
            style.transform = parse_transform(v);
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

fn parse_box_shadow(v: &str) -> Option<BoxShadow> {
    if v.eq_ignore_ascii_case("none") {
        return None;
    }
    let mut nums = Vec::new();
    let mut color = [0.0, 0.0, 0.0, 0.25];
    for tok in v.split_whitespace() {
        if let Some(px) = parse_pixel_val(tok) {
            nums.push(px);
        } else if let Some(c) = parse_color(tok) {
            color = c;
        }
    }
    if nums.len() >= 2 {
        Some(BoxShadow {
            offset_x: nums[0],
            offset_y: nums[1],
            blur: nums.get(2).copied().unwrap_or(0.0),
            color,
        })
    } else {
        None
    }
}

fn parse_gradient(v: &str) -> Option<Gradient> {
    let lower = v.to_ascii_lowercase();
    let (is_linear, inner) = if let Some(rest) = lower.strip_prefix("linear-gradient(") {
        (true, rest.strip_suffix(')')?)
    } else if let Some(rest) = lower.strip_prefix("radial-gradient(") {
        (false, rest.strip_suffix(')')?)
    } else {
        return None;
    };

    let parts: Vec<&str> = split_top_level(inner, ',');
    let mut angle_deg = 180.0_f32;
    let mut start_idx = 0usize;
    if is_linear && let Some(first) = parts.first() {
        let f = first.trim();
        if let Some(deg) = f.strip_suffix("deg").and_then(|d| d.trim().parse::<f32>().ok()) {
            angle_deg = deg;
            start_idx = 1;
        } else if f.starts_with("to ") {
            angle_deg = match f {
                "to right" => 90.0,
                "to left" => 270.0,
                "to top" => 0.0,
                _ => 180.0,
            };
            start_idx = 1;
        }
    }

    let color_parts = &parts[start_idx..];
    let n = color_parts.len();
    if n < 2 {
        return None;
    }
    let mut stops = Vec::with_capacity(n);
    for (idx, raw) in color_parts.iter().enumerate() {
        let c = parse_color(raw.trim())?;
        stops.push((c, idx as f32 / (n - 1) as f32));
    }

    Some(if is_linear {
        Gradient::Linear { angle_deg, stops }
    } else {
        Gradient::Radial { stops }
    })
}

fn parse_transform(v: &str) -> Transform2D {
    let mut t = Transform2D::default();
    let lower = v.to_ascii_lowercase();
    for part in lower.split(')') {
        let p = part.trim();
        if let Some(args) = p.strip_prefix("translate(") {
            let vals: Vec<f32> = args.split(',').filter_map(parse_pixel_val).collect();
            if let Some(&x) = vals.first() {
                t.translate_x = x;
            }
            if let Some(&y) = vals.get(1) {
                t.translate_y = y;
            }
        } else if let Some(args) = p.strip_prefix("translatex(") {
            if let Some(x) = parse_pixel_val(args) {
                t.translate_x = x;
            }
        } else if let Some(args) = p.strip_prefix("translatey(") {
            if let Some(y) = parse_pixel_val(args) {
                t.translate_y = y;
            }
        } else if let Some(args) = p.strip_prefix("scale(") {
            let vals: Vec<f32> = args.split(',').filter_map(|s| s.trim().parse().ok()).collect();
            if let Some(&sx) = vals.first() {
                t.scale_x = sx;
                t.scale_y = vals.get(1).copied().unwrap_or(sx);
            }
        } else if let Some(args) = p.strip_prefix("rotate(")
            && let Some(deg) = args.trim().strip_suffix("deg").and_then(|d| d.trim().parse::<f32>().ok())
        {
            t.rotate_rad = deg.to_radians();
        }
    }
    t
}

/// Parses a pixel, rem, em, pt, or CSS math (`calc`, `min`, `max`, `clamp`) value.
pub fn parse_pixel_val(s: &str) -> Option<f32> {
    let trimmed = s.trim();
    if let Some(inner) = trimmed.strip_prefix("calc(").and_then(|r| r.strip_suffix(')')) {
        return eval_math_expr(inner);
    }
    if let Some(inner) = trimmed.strip_prefix("min(").and_then(|r| r.strip_suffix(')')) {
        return split_top_level(inner, ',')
            .into_iter()
            .filter_map(parse_pixel_val)
            .reduce(f32::min);
    }
    if let Some(inner) = trimmed.strip_prefix("max(").and_then(|r| r.strip_suffix(')')) {
        return split_top_level(inner, ',')
            .into_iter()
            .filter_map(parse_pixel_val)
            .reduce(f32::max);
    }
    if let Some(inner) = trimmed.strip_prefix("clamp(").and_then(|r| r.strip_suffix(')')) {
        let args: Vec<f32> = split_top_level(inner, ',')
            .into_iter()
            .filter_map(parse_pixel_val)
            .collect();
        if args.len() == 3 {
            return Some(args[1].clamp(args[0], args[2]));
        }
    }

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

fn eval_math_expr(expr: &str) -> Option<f32> {
    if let Some((l, r)) = expr.split_once(" + ") {
        return Some(parse_pixel_val(l)? + parse_pixel_val(r)?);
    }
    if let Some((l, r)) = expr.split_once(" - ") {
        return Some(parse_pixel_val(l)? - parse_pixel_val(r)?);
    }
    if let Some((l, r)) = expr.split_once(" * ") {
        return Some(parse_pixel_val(l)? * parse_pixel_val(r)?);
    }
    if let Some((l, r)) = expr.split_once(" / ") {
        let denom = parse_pixel_val(r)?;
        return (denom != 0.0).then(|| parse_pixel_val(l).map(|a| a / denom)).flatten();
    }
    parse_pixel_val(expr)
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
            chars.next();
            while let Some(inner) = chars.next() {
                if inner == '*' && chars.peek() == Some(&'/') {
                    chars.next();
                    break;
                }
            }
        } else {
            out.push(ch);
        }
    }

    out
}
