//! Computed style definitions used for layout and rendering.

/// Layout display property.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Display {
    #[default]
    Block,
    Inline,
    Flex,
    None,
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
        }
    }
}
