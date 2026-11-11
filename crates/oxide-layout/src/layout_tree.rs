//! Positioned and sized layout tree representation.

use oxide_dom::NodeId;
use oxide_style::ComputedStyle;

/// A 2D bounding box in viewport coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    #[must_use]
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self { x, y, width, height }
    }

    #[must_use]
    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && px <= self.x + self.width && py >= self.y && py <= self.y + self.height
    }
}

/// A positioned layout box ready for rendering and hit-testing.
#[derive(Debug, Clone)]
pub struct LayoutBox {
    pub node_id: NodeId,
    pub rect: Rect,
    pub style: ComputedStyle,
    pub text: Option<String>,
    pub children: Vec<LayoutBox>,
}

impl LayoutBox {
    /// Recursively counts all layout boxes in the subtree.
    #[must_use]
    pub fn count_boxes(&self) -> usize {
        1 + self.children.iter().map(LayoutBox::count_boxes).sum::<usize>()
    }
}

/// The complete positioned layout tree for a document.
#[derive(Debug, Default, Clone)]
pub struct LayoutTree {
    pub root: Option<LayoutBox>,
    pub viewport_width: f32,
    pub viewport_height: f32,
}

impl LayoutTree {
    #[must_use]
    pub fn new(viewport_width: f32, viewport_height: f32) -> Self {
        Self {
            root: None,
            viewport_width,
            viewport_height,
        }
    }
}
