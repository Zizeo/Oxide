//! Vello GPU painter rendering layout trees, background rectangles, borders, and shaped glyphs.

use cosmic_text::{Attrs, Buffer, FontSystem, Metrics, Shaping};
use oxide_dom::{Document, NodeId};
use oxide_layout::{LayoutBox, LayoutTree};
use oxide_style::{Gradient as CssGradient, Overflow};
use std::collections::HashMap;
use std::sync::Arc;
use vello::kurbo::{Affine, Point, Rect as KurboRect, RoundedRect, RoundedRectRadii, Stroke};
use vello::peniko::{
    Blob, Brush, Color, ColorStop, Fill, Font, Image as PenikoImage, ImageFormat, Mix,
};
use vello::{AaConfig, AaSupport, Glyph, RenderParams, Renderer, RendererOptions, Scene};

/// GPU Paint Engine converting layout trees into Vello compute render commands.
pub struct PaintEngine {
    font_system: FontSystem,
    font_cache: HashMap<cosmic_text::fontdb::ID, Font>,
    image_cache: HashMap<String, PenikoImage>,
    cached_scene: Option<(f32, Scene)>,
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
            image_cache: HashMap::new(),
            cached_scene: None,
            renderer: None,
        }
    }

    pub fn invalidate(&mut self) {
        self.cached_scene = None;
    }

    /// Registers a decoded RGBA8 image asset for `<img>` rendering.
    pub fn register_image(&mut self, key: String, rgba: Vec<u8>, width: u32, height: u32) {
        let img = PenikoImage::new(Blob::new(Arc::new(rgba)), ImageFormat::Rgba8, width, height);
        self.image_cache.insert(key, img);
        self.cached_scene = None;
    }

    /// Builds a Vello scene by traversing the layout tree with the specified vertical scroll offset.
    pub fn build_scene(&mut self, layout: &LayoutTree, scroll_y: f32) -> Scene {
        let mut scene = Scene::new();
        if let Some(root_box) = &layout.root {
            self.paint_box(&mut scene, root_box, scroll_y);
        }
        self.cached_scene = Some((scroll_y, scene.clone()));
        scene
    }

    fn paint_box(&mut self, scene: &mut Scene, layout_box: &LayoutBox, scroll_y: f32) {
        let rect = &layout_box.rect;
        let y = rect.y - scroll_y;
        let kurbo_rect = KurboRect::new(
            rect.x as f64,
            y as f64,
            (rect.x + rect.width) as f64,
            (y + rect.height) as f64,
        );
        let r = layout_box.style.border_radii;
        let rounded = RoundedRect::from_rect(
            kurbo_rect,
            RoundedRectRadii::new(r[0] as f64, r[1] as f64, r[2] as f64, r[3] as f64),
        );

        let tr = layout_box.style.transform;
        let affine = Affine::translate((tr.translate_x as f64, tr.translate_y as f64))
            * Affine::rotate(tr.rotate_rad as f64)
            * Affine::scale_non_uniform(tr.scale_x as f64, tr.scale_y as f64);

        let pushed_opacity = layout_box.style.opacity < 0.999;
        if pushed_opacity {
            scene.push_layer(Mix::Normal, layout_box.style.opacity, affine, &kurbo_rect);
        }
        let pushed_clip = matches!(
            layout_box.style.overflow,
            Overflow::Hidden | Overflow::Scroll | Overflow::Auto
        );
        if pushed_clip {
            scene.push_layer(Mix::Normal, 1.0, affine, &rounded);
        }

        // 0. Box Shadow
        if let Some(shadow) = &layout_box.style.box_shadow
            && shadow.color[3] > 0.0
        {
            let sh_rect = kurbo_rect + vello::kurbo::Vec2::new(shadow.offset_x as f64, shadow.offset_y as f64);
            let sh_color = Color::from_rgba8(
                (shadow.color[0] * 255.0) as u8,
                (shadow.color[1] * 255.0) as u8,
                (shadow.color[2] * 255.0) as u8,
                (shadow.color[3] * 255.0) as u8,
            );
            scene.fill(Fill::NonZero, affine, sh_color, None, &sh_rect);
        }

        // 1. Background (Gradient or Solid)
        if let Some(grad) = &layout_box.style.background_gradient {
            let brush = build_peniko_gradient(grad, kurbo_rect);
            scene.fill(Fill::NonZero, affine, &brush, None, &rounded);
        } else if let Some(bg) = layout_box.style.background_color
            && bg[3] > 0.0
        {
            let color = Color::from_rgba8(
                (bg[0] * 255.0) as u8,
                (bg[1] * 255.0) as u8,
                (bg[2] * 255.0) as u8,
                (bg[3] * 255.0) as u8,
            );
            scene.fill(Fill::NonZero, affine, color, None, &rounded);
        }

        // 2. Borders
        let bw = layout_box.style.border_width[0];
        let bc = layout_box.style.border_color;
        if bw > 0.0 && bc[3] > 0.0 {
            let border_color = Color::from_rgba8(
                (bc[0] * 255.0) as u8,
                (bc[1] * 255.0) as u8,
                (bc[2] * 255.0) as u8,
                (bc[3] * 255.0) as u8,
            );
            scene.stroke(&Stroke::new(bw as f64), affine, border_color, None, &rounded);
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

        // 4. Recurse down children sorted by z-index
        let mut order: Vec<usize> = (0..layout_box.children.len()).collect();
        order.sort_by_key(|&i| layout_box.children[i].style.z_index);
        for idx in order {
            self.paint_box(scene, &layout_box.children[idx], scroll_y);
        }

        if pushed_clip {
            scene.pop_layer();
        }
        if pushed_opacity {
            scene.pop_layer();
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
}

fn build_peniko_gradient(grad: &CssGradient, rect: KurboRect) -> Brush {
    match grad {
        CssGradient::Linear { angle_deg, stops } => {
            let rad = (*angle_deg - 90.0).to_radians() as f64;
            let cx = (rect.x0 + rect.x1) * 0.5;
            let cy = (rect.y0 + rect.y1) * 0.5;
            let half = ((rect.width().powi(2) + rect.height().powi(2)).sqrt() * 0.5).max(1.0);
            let dx = rad.cos() * half;
            let dy = rad.sin() * half;
            let g = vello::peniko::Gradient::new_linear(Point::new(cx - dx, cy - dy), Point::new(cx + dx, cy + dy))
                .with_stops(
                    stops
                        .iter()
                        .map(|(c, off)| ColorStop {
                            offset: *off,
                            color: vello::peniko::color::DynamicColor::from_alpha_color(
                                Color::from_rgba8(
                                    (c[0] * 255.0) as u8,
                                    (c[1] * 255.0) as u8,
                                    (c[2] * 255.0) as u8,
                                    (c[3] * 255.0) as u8,
                                ),
                            ),
                        })
                        .collect::<Vec<_>>()
                        .as_slice(),
                );
            Brush::Gradient(g)
        }
        CssGradient::Radial { stops } => {
            let cx = (rect.x0 + rect.x1) * 0.5;
            let cy = (rect.y0 + rect.y1) * 0.5;
            let r = (rect.width().max(rect.height()) * 0.5).max(1.0) as f32;
            let g = vello::peniko::Gradient::new_radial(Point::new(cx, cy), r).with_stops(
                stops
                    .iter()
                    .map(|(c, off)| ColorStop {
                        offset: *off,
                        color: vello::peniko::color::DynamicColor::from_alpha_color(
                            Color::from_rgba8(
                                (c[0] * 255.0) as u8,
                                (c[1] * 255.0) as u8,
                                (c[2] * 255.0) as u8,
                                (c[3] * 255.0) as u8,
                            ),
                        ),
                    })
                    .collect::<Vec<_>>()
                    .as_slice(),
            );
            Brush::Gradient(g)
        }
    }
}

impl PaintEngine {

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
