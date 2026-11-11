//! Vello GPU painter rendering layout trees, background rectangles, borders, and shaped glyphs.

use cosmic_text::{Attrs, Buffer, FontSystem, Metrics, Shaping};
use oxide_dom::{Document, NodeId};
use oxide_layout::{LayoutBox, LayoutTree};
use std::collections::HashMap;
use std::sync::Arc;
use vello::kurbo::{Affine, Rect as KurboRect, Stroke};
use vello::peniko::{Blob, Brush, Color, Fill, Font};
use vello::{AaConfig, AaSupport, Glyph, RenderParams, Renderer, RendererOptions, Scene};

/// GPU Paint Engine converting layout trees into Vello compute render commands.
pub struct PaintEngine {
    font_system: FontSystem,
    font_cache: HashMap<cosmic_text::fontdb::ID, Font>,
    renderer: Option<Renderer>,
}

impl Default for PaintEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl PaintEngine {
    #[must_use]
    pub fn new() -> Self {
        Self {
            font_system: FontSystem::new(),
            font_cache: HashMap::new(),
            renderer: None,
        }
    }

    /// Builds a Vello scene by traversing the layout tree with the specified vertical scroll offset.
    pub fn build_scene(&mut self, layout: &LayoutTree, scroll_y: f32) -> Scene {
        let mut scene = Scene::new();
        if let Some(root_box) = &layout.root {
            self.paint_box(&mut scene, root_box, scroll_y);
        }
        scene
    }

    fn paint_box(&mut self, scene: &mut Scene, layout_box: &LayoutBox, scroll_y: f32) {
        let rect = &layout_box.rect;
        let y = rect.y - scroll_y;

        // 1. Draw Background Rectangle
        if let Some(bg) = layout_box.style.background_color
            && bg[3] > 0.0
        {
            let kurbo_rect = KurboRect::new(
                rect.x as f64,
                y as f64,
                (rect.x + rect.width) as f64,
                (y + rect.height) as f64,
            );
            let color = Color::from_rgba8(
                (bg[0] * 255.0) as u8,
                (bg[1] * 255.0) as u8,
                (bg[2] * 255.0) as u8,
                (bg[3] * 255.0) as u8,
            );
            scene.fill(Fill::NonZero, Affine::IDENTITY, color, None, &kurbo_rect);
        }

        // 2. Draw Borders
        let bw = layout_box.style.border_width[0];
        let bc = layout_box.style.border_color;
        if bw > 0.0 && bc[3] > 0.0 {
            let kurbo_rect = KurboRect::new(
                rect.x as f64,
                y as f64,
                (rect.x + rect.width) as f64,
                (y + rect.height) as f64,
            );
            let border_color = Color::from_rgba8(
                (bc[0] * 255.0) as u8,
                (bc[1] * 255.0) as u8,
                (bc[2] * 255.0) as u8,
                (bc[3] * 255.0) as u8,
            );
            scene.stroke(
                &Stroke::new(bw as f64),
                Affine::IDENTITY,
                border_color,
                None,
                &kurbo_rect,
            );
        }

        // 3. Draw Text Glyphs
        if let Some(text) = &layout_box.text {
            let font_size = layout_box.style.font_size;
            let line_height = font_size * layout_box.style.line_height;
            let metrics = Metrics::new(font_size, line_height);

            let mut buffer = Buffer::new(&mut self.font_system, metrics);
            buffer.set_text(
                &mut self.font_system,
                text,
                Attrs::new(),
                Shaping::Advanced,
            );
            buffer.shape_until_scroll(&mut self.font_system, false);

            let text_color = Color::from_rgba8(
                (layout_box.style.color[0] * 255.0) as u8,
                (layout_box.style.color[1] * 255.0) as u8,
                (layout_box.style.color[2] * 255.0) as u8,
                (layout_box.style.color[3] * 255.0) as u8,
            );

            for run in buffer.layout_runs() {
                let baseline_y = y + run.line_y;

                for glyph in run.glyphs {
                    if let Some(font) = self.get_or_load_font(glyph.font_id) {
                        let glyph_run = [Glyph {
                            id: glyph.glyph_id as u32,
                            x: rect.x + glyph.x,
                            y: baseline_y,
                        }];
                        scene
                            .draw_glyphs(&font)
                            .font_size(font_size)
                            .brush(&Brush::Solid(text_color))
                            .draw(Fill::NonZero, glyph_run.into_iter());
                    }
                }
            }
        }

        // 4. Recurse down children in layout tree
        for child in &layout_box.children {
            self.paint_box(scene, child, scroll_y);
        }
    }

    fn get_or_load_font(&mut self, font_id: cosmic_text::fontdb::ID) -> Option<Font> {
        if let Some(cached) = self.font_cache.get(&font_id) {
            return Some(cached.clone());
        }

        let loaded = self
            .font_system
            .db()
            .with_face_data(font_id, |data, index| {
                Font::new(Blob::new(Arc::new(data.to_vec())), index)
            })?;

        self.font_cache.insert(font_id, loaded.clone());
        Some(loaded)
    }

    /// Initializes Vello compute renderer on the active GPU device.
    pub fn ensure_renderer(&mut self, device: &vello::wgpu::Device) -> Result<(), vello::Error> {
        if self.renderer.is_none() {
            let renderer = Renderer::new(
                device,
                RendererOptions {
                    surface_format: None,
                    use_cpu: false,
                    antialiasing_support: AaSupport::all(),
                    num_init_threads: None,
                },
            )?;
            self.renderer = Some(renderer);
        }
        Ok(())
    }

    /// Renders a Vello scene onto a target WGPU texture.
    pub fn render_to_texture(
        &mut self,
        device: &vello::wgpu::Device,
        queue: &vello::wgpu::Queue,
        scene: &Scene,
        texture_view: &vello::wgpu::TextureView,
        width: u32,
        height: u32,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.ensure_renderer(device)?;
        if let Some(renderer) = &mut self.renderer {
            renderer.render_to_texture(
                device,
                queue,
                scene,
                texture_view,
                &RenderParams {
                    base_color: Color::from_rgb8(15, 17, 23),
                    width,
                    height,
                    antialiasing_method: AaConfig::Area,
                },
            )?;
        }
        Ok(())
    }
}

/// Hit-tests a point `(x, y)` in page coordinates to find the deepest matching DOM node.
#[must_use]
pub fn hit_test(cur: &LayoutBox, x: f32, y: f32) -> Option<NodeId> {
    if !cur.rect.contains(x, y) {
        return None;
    }

    // Check children first (deepest matching in Z-order)
    for child in cur.children.iter().rev() {
        if let Some(hit) = hit_test(child, x, y) {
            return Some(hit);
        }
    }

    Some(cur.node_id)
}

/// Finds the link target URL if the hit node or any of its ancestors is an `<a>` element.
#[must_use]
pub fn find_link_target(doc: &Document, mut current_id: NodeId) -> Option<String> {
    while let Some(node) = doc.get(current_id) {
        if node.tag_name() == Some("a")
            && let Some(href) = node.attribute("href")
        {
            return Some(href.to_string());
        }
        if let Some(parent_id) = node.parent {
            current_id = parent_id;
        } else {
            break;
        }
    }
    None
}
