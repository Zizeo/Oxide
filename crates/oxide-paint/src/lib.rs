//! 2D GPU Painting using Vello for Oxide.

pub mod painter;

pub use painter::{find_link_target, hit_test, PaintEngine};

#[cfg(test)]
mod tests {
    use super::*;
    use oxide_dom::Document;
    use oxide_layout::compute_layout;
    use oxide_style::resolve_styles;

    #[test]
    fn test_paint_engine_build_scene() {
        let html = r#"<!DOCTYPE html>
        <html>
        <head>
            <style>
                .card { background-color: #1e293b; padding: 20px; border: 2px solid #38bdf8; }
            </style>
        </head>
        <body>
            <div class="card">
                <h1>Title</h1>
                <p>Paragraph with text</p>
                <a href="https://example.com">Click Me</a>
            </div>
        </body>
        </html>"#;

        let doc = Document::parse(html);
        let styles = resolve_styles(&doc);
        let layout = compute_layout(&doc, &styles, 800.0, 600.0);

        let mut engine = PaintEngine::new();
        let _scene = engine.build_scene(&layout, 0.0);

        // Test link hit testing
        let a_nodes = doc.find_elements_by_tag("a");
        assert_eq!(a_nodes.len(), 1);
        let a_id = a_nodes[0];
        let link_target = find_link_target(&doc, a_id);
        assert_eq!(link_target, Some("https://example.com".to_string()));
    }
}
