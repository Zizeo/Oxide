//! Bridges DOM and ComputedStyles into Taffy and cosmic-text layout.

use crate::layout_tree::{LayoutBox, LayoutTree, Rect as LayoutRect};
use cosmic_text::{Attrs, Buffer, FontSystem, Metrics, Shaping};
use oxide_dom::{Document, NodeData, NodeId as DomNodeId};
use oxide_style::{
    AlignItems, ComputedStyle, Display, FlexDirection, JustifyContent, StyleTree,
};
use taffy::prelude::{
    AlignItems as TaffyAlignItems, AvailableSpace, Dimension, Display as TaffyDisplay,
    FlexDirection as TaffyFlexDirection, JustifyContent as TaffyJustifyContent,
    LengthPercentage, LengthPercentageAuto, NodeId as TaffyNodeId, Rect as TaffyRect, Size, Style,
    TaffyTree,
};

/// Internal context linking a Taffy node back to its DOM source.
#[derive(Debug, Clone)]
enum TaffySource {
    Element {
        node_id: DomNodeId,
        style: ComputedStyle,
    },
    Text {
        node_id: DomNodeId,
        style: ComputedStyle,
        content: String,
    },
}

/// Computes the complete layout tree for an arena Document and its resolved styles.
#[must_use]
pub fn compute_layout(
    doc: &Document,
    styles: &StyleTree,
    viewport_width: f32,
    viewport_height: f32,
) -> LayoutTree {
    let mut layout_tree = LayoutTree::new(viewport_width, viewport_height);

    let Some(root_id) = doc.root else {
        return layout_tree;
    };

    let mut taffy_tree = TaffyTree::<TaffySource>::new();
    let mut font_system = FontSystem::new();

    let root_w = viewport_width.max(300.0) - 16.0;

    if let Some(root_taffy_id) = build_taffy_node(
        doc,
        styles,
        root_id,
        &mut taffy_tree,
        &mut font_system,
        root_w,
    ) {
        let available_space = Size {
            width: AvailableSpace::Definite(viewport_width),
            height: AvailableSpace::MinContent,
        };

        if taffy_tree.compute_layout(root_taffy_id, available_space).is_ok() {
            layout_tree.root = Some(build_layout_box(
                &taffy_tree,
                root_taffy_id,
                0.0,
                0.0,
            ));
        }
    }

    layout_tree
}

fn build_taffy_node(
    doc: &Document,
    styles: &StyleTree,
    node_id: DomNodeId,
    taffy: &mut TaffyTree<TaffySource>,
    font_system: &mut FontSystem,
    available_w: f32,
) -> Option<TaffyNodeId> {
    let node = doc.get(node_id)?;
    let style = styles.get(node_id).cloned().unwrap_or_default();

    if style.display == Display::None {
        return None;
    }

    match &node.data {
        NodeData::Document => {
            let mut children = Vec::new();
            for &child_id in &node.children {
                if let Some(child_taffy_id) =
                    build_taffy_node(doc, styles, child_id, taffy, font_system, available_w)
                {
                    children.push(child_taffy_id);
                }
            }

            let taffy_style = Style {
                display: TaffyDisplay::Block,
                size: Size {
                    width: Dimension::Auto,
                    height: Dimension::Auto,
                },
                ..Default::default()
            };

            let source = TaffySource::Element { node_id, style };
            let id = taffy.new_with_children(taffy_style, &children).ok()?;
            let _ = taffy.set_node_context(id, Some(source));
            Some(id)
        }
        NodeData::Element { .. } => {
            let child_w = (style.width.unwrap_or(available_w) - style.padding[1] - style.padding[3]).max(50.0);
            let mut children = Vec::new();
            for &child_id in &node.children {
                if let Some(child_taffy_id) =
                    build_taffy_node(doc, styles, child_id, taffy, font_system, child_w)
                {
                    children.push(child_taffy_id);
                }
            }

            let taffy_style = convert_computed_style(&style);
            let source = TaffySource::Element { node_id, style };
            let id = taffy.new_with_children(taffy_style, &children).ok()?;
            let _ = taffy.set_node_context(id, Some(source));
            Some(id)
        }
        NodeData::Text { content } => {
            let trimmed = content.trim();
            if trimmed.is_empty() {
                return None;
            }

            // Measure wrapped text with cosmic-text
            let (text_w, text_h) = measure_text(font_system, trimmed, style.font_size, style.line_height, available_w);

            let taffy_style = Style {
                display: TaffyDisplay::Block,
                size: Size {
                    width: Dimension::Length(text_w),
                    height: Dimension::Length(text_h),
                },
                margin: TaffyRect {
                    top: LengthPercentageAuto::Length(style.margin[0]),
                    right: LengthPercentageAuto::Length(style.margin[1]),
                    bottom: LengthPercentageAuto::Length(style.margin[2]),
                    left: LengthPercentageAuto::Length(style.margin[3]),
                },
                ..Default::default()
            };

            let source = TaffySource::Text {
                node_id,
                style,
                content: trimmed.to_string(),
            };

            let id = taffy.new_leaf(taffy_style).ok()?;
            let _ = taffy.set_node_context(id, Some(source));
            Some(id)
        }
        NodeData::DocType { .. } | NodeData::Comment { .. } => None,
    }
}

fn measure_text(
    font_system: &mut FontSystem,
    text: &str,
    font_size: f32,
    line_height_mult: f32,
    max_w: f32,
) -> (f32, f32) {
    let line_height = font_size * line_height_mult;
    let metrics = Metrics::new(font_size, line_height);
    let mut buffer = Buffer::new(font_system, metrics);
    buffer.set_size(font_system, Some(max_w.max(50.0)), None);
    buffer.set_text(font_system, text, Attrs::new(), Shaping::Advanced);
    buffer.shape_until_scroll(font_system, false);

    let mut width = 0.0_f32;
    let mut height = 0.0_f32;
    for run in buffer.layout_runs() {
        width = width.max(run.line_w);
        height += run.line_height;
    }

    (width.max(1.0).min(max_w), height.max(font_size))
}

fn convert_computed_style(style: &ComputedStyle) -> Style {
    let display = match style.display {
        Display::None => TaffyDisplay::None,
        Display::Flex => TaffyDisplay::Flex,
        Display::Block | Display::Inline => TaffyDisplay::Block,
    };

    let flex_direction = match style.flex_direction {
        FlexDirection::Row => TaffyFlexDirection::Row,
        FlexDirection::Column => TaffyFlexDirection::Column,
        FlexDirection::RowReverse => TaffyFlexDirection::RowReverse,
        FlexDirection::ColumnReverse => TaffyFlexDirection::ColumnReverse,
    };

    let justify_content = style.justify_content.map(|jc| match jc {
        JustifyContent::FlexStart => TaffyJustifyContent::FlexStart,
        JustifyContent::Center => TaffyJustifyContent::Center,
        JustifyContent::FlexEnd => TaffyJustifyContent::FlexEnd,
        JustifyContent::SpaceBetween => TaffyJustifyContent::SpaceBetween,
        JustifyContent::SpaceAround => TaffyJustifyContent::SpaceAround,
        JustifyContent::SpaceEvenly => TaffyJustifyContent::SpaceEvenly,
    });

    let align_items = style.align_items.map(|ai| match ai {
        AlignItems::FlexStart => TaffyAlignItems::FlexStart,
        AlignItems::Center => TaffyAlignItems::Center,
        AlignItems::FlexEnd => TaffyAlignItems::FlexEnd,
        AlignItems::Stretch => TaffyAlignItems::Stretch,
    });

    let width = style.width.map_or(Dimension::Auto, Dimension::Length);
    let height = style.height.map_or(Dimension::Auto, Dimension::Length);

    let margin = TaffyRect {
        top: if style.margin_auto[0] {
            LengthPercentageAuto::Auto
        } else {
            LengthPercentageAuto::Length(style.margin[0])
        },
        right: if style.margin_auto[1] {
            LengthPercentageAuto::Auto
        } else {
            LengthPercentageAuto::Length(style.margin[1])
        },
        bottom: if style.margin_auto[2] {
            LengthPercentageAuto::Auto
        } else {
            LengthPercentageAuto::Length(style.margin[2])
        },
        left: if style.margin_auto[3] {
            LengthPercentageAuto::Auto
        } else {
            LengthPercentageAuto::Length(style.margin[3])
        },
    };

    let padding = TaffyRect {
        top: LengthPercentage::Length(style.padding[0]),
        right: LengthPercentage::Length(style.padding[1]),
        bottom: LengthPercentage::Length(style.padding[2]),
        left: LengthPercentage::Length(style.padding[3]),
    };

    let gap = Size {
        width: LengthPercentage::Length(style.gap[1]),
        height: LengthPercentage::Length(style.gap[0]),
    };

    Style {
        display,
        flex_direction,
        justify_content,
        align_items,
        size: Size { width, height },
        margin,
        padding,
        gap,
        ..Default::default()
    }
}

fn build_layout_box(
    taffy: &TaffyTree<TaffySource>,
    taffy_node_id: TaffyNodeId,
    parent_abs_x: f32,
    parent_abs_y: f32,
) -> LayoutBox {
    let layout = taffy.layout(taffy_node_id).copied().unwrap_or_default();

    let abs_x = parent_abs_x + layout.location.x;
    let abs_y = parent_abs_y + layout.location.y;

    let rect = LayoutRect::new(abs_x, abs_y, layout.size.width, layout.size.height);

    let (node_id, style, text) = match taffy.get_node_context(taffy_node_id) {
        Some(TaffySource::Element { node_id, style }) => (*node_id, style.clone(), None),
        Some(TaffySource::Text {
            node_id,
            style,
            content,
        }) => (*node_id, style.clone(), Some(content.clone())),
        None => (
            DomNodeId::default(),
            ComputedStyle::default(),
            None,
        ),
    };

    let mut children = Vec::new();
    if let Ok(child_ids) = taffy.children(taffy_node_id) {
        for child_id in child_ids {
            children.push(build_layout_box(taffy, child_id, abs_x, abs_y));
        }
    }

    LayoutBox {
        node_id,
        rect,
        style,
        text,
        children,
    }
}
