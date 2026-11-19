//! Computed style definitions used for layout and rendering.

use std::collections::HashMap;

/// Layout display property.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Display {
    #[default]
    Block,
    Inline,
    Flex,
    Grid,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Position {
    #[default]
    Static,
    Relative,
    Absolute,
    Fixed,
    Sticky,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Overflow {
    #[default]
    Visible,
    Hidden,
    Scroll,
    Auto,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BoxShadow {
    pub offset_x: f32,
    pub offset_y: f32,
    pub blur: f32,
    pub color: [f32; 4],
}

#[derive(Debug, Clone, PartialEq)]
pub enum Gradient {
    Linear {
        angle_deg: f32,
        stops: Vec<([f32; 4], f32)>,
    },
    Radial {
        stops: Vec<([f32; 4], f32)>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform2D {
    pub translate_x: f32,
    pub translate_y: f32,
    pub scale_x: f32,
    pub scale_y: f32,
    pub rotate_rad: f32,
}

impl Default for Transform2D {
    fn default() -> Self {
        Self {
            translate_x: 0.0,
            translate_y: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            rotate_rad: 0.0,
        }
    }
}

/// Flexbox direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FlexDirection {
    #[default]
    Row,
    Column,
    RowReverse,
    ColumnReverse,
}

/// Flexbox content alignment along main axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JustifyContent {
    FlexStart,
    Center,
    FlexEnd,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

/// Flexbox items alignment along cross axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlignItems {
    FlexStart,
    Center,
    FlexEnd,
    Stretch,
}

/// Horizontal text alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
}

/// Fully resolved and computed style for a DOM node.
#[derive(Debug, Clone, PartialEq)]
pub struct ComputedStyle {
    pub display: Display,
    pub flex_direction: FlexDirection,
    pub justify_content: Option<JustifyContent>,
    pub align_items: Option<AlignItems>,
    pub gap: [f32; 2], // row_gap, column_gap
    pub width: Option<f32>,
    pub height: Option<f32>,
    pub margin: [f32; 4],       // top, right, bottom, left
    pub margin_auto: [bool; 4], // top, right, bottom, left
    pub padding: [f32; 4],      // top, right, bottom, left
    pub border_width: [f32; 4], // top, right, bottom, left
    pub border_color: [f32; 4],
    pub border_radius: f32,
    pub background_color: Option<[f32; 4]>,
    pub color: [f32; 4],
    pub font_size: f32,
    pub font_weight: f32,
    pub line_height: f32,
    pub text_align: TextAlign,
    pub position: Position,
    pub inset: [Option<f32>; 4], // top, right, bottom, left
    pub z_index: i32,
    pub overflow: Overflow,
    pub text_overflow_ellipsis: bool,
    pub grid_template_columns: Vec<String>,
    pub grid_template_rows: Vec<String>,
    pub border_radii: [f32; 4], // tl, tr, br, bl
    pub opacity: f32,
    pub box_shadow: Option<BoxShadow>,
    pub background_gradient: Option<Gradient>,
    pub transform: Transform2D,
    pub custom_props: HashMap<String, String>,
}

impl Default for ComputedStyle {
    fn default() -> Self {
        Self {
            display: Display::Block,
            flex_direction: FlexDirection::Row,
            justify_content: None,
            align_items: None,
            gap: [0.0, 0.0],
            width: None,
            height: None,
            margin: [0.0, 0.0, 0.0, 0.0],
            margin_auto: [false, false, false, false],
            padding: [0.0, 0.0, 0.0, 0.0],
            border_width: [0.0, 0.0, 0.0, 0.0],
            border_color: [0.0, 0.0, 0.0, 0.0],
            border_radius: 0.0,
            background_color: None,
            color: [0.1, 0.12, 0.15, 1.0], // Default dark charcoal text for web pages
            font_size: 16.0,
            font_weight: 400.0,
            line_height: 1.3,
            text_align: TextAlign::Left,
            position: Position::Static,
            inset: [None, None, None, None],
            z_index: 0,
            overflow: Overflow::Visible,
            text_overflow_ellipsis: false,
            grid_template_columns: Vec::new(),
            grid_template_rows: Vec::new(),
            border_radii: [0.0, 0.0, 0.0, 0.0],
            opacity: 1.0,
            box_shadow: None,
            background_gradient: None,
            transform: Transform2D::default(),
            custom_props: HashMap::new(),
        }
    }
}
