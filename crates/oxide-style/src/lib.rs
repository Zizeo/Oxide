//! CSS parsing and style computation engine for Oxide.

pub mod color;
pub mod computed;
pub mod engine;
pub mod parser;

pub use color::parse_color;
pub use computed::{
    AlignItems, ComputedStyle, Display, FlexDirection, JustifyContent, TextAlign,
};
pub use engine::{default_ua_rules, resolve_styles, resolve_styles_with_rules, StyleTree};
pub use parser::{apply_declarations, parse_declarations, parse_stylesheet, CssRule, Selector};

#[cfg(test)]
mod tests {
    use super::*;
    use oxide_dom::Document;

    #[test]
    fn test_color_parsing() {
        assert_eq!(parse_color("#ffffff"), Some([1.0, 1.0, 1.0, 1.0]));
        assert_eq!(parse_color("#000"), Some([0.0, 0.0, 0.0, 1.0]));
        assert_eq!(parse_color("transparent"), Some([0.0, 0.0, 0.0, 0.0]));
        assert_eq!(parse_color("rgb(255, 0, 0)"), Some([1.0, 0.0, 0.0, 1.0]));
    }

    #[test]
    fn test_ua_stylesheet_and_inheritance() {
        let html = r#"<!DOCTYPE html>
        <html>
        <head>
            <style>
                .custom-card { background-color: #1e293b; padding: 20px; }
                #special { color: #f43f5e; font-size: 28px; }
            </style>
        </head>
        <body style="color: #10b981;">
            <h1>Title</h1>
            <div class="custom-card">
                <p id="special" style="font-weight: bold;">Styled Paragraph</p>
                <span>Inherited green text</span>
            </div>
        </body>
        </html>"#;

        let doc = Document::parse(html);
        let style_tree = resolve_styles(&doc);

        // H1 should have UA font size 32px
        let h1_nodes = doc.find_elements_by_tag("h1");
        assert_eq!(h1_nodes.len(), 1);
        let h1_style = style_tree.get(h1_nodes[0]).unwrap();
        assert_eq!(h1_style.font_size, 32.0);
        assert_eq!(h1_style.font_weight, 700.0);

        // div.custom-card should have author padding and background
        let div_nodes = doc.find_elements_by_tag("div");
        assert_eq!(div_nodes.len(), 1);
        let div_style = style_tree.get(div_nodes[0]).unwrap();
        assert_eq!(div_style.padding, [20.0, 20.0, 20.0, 20.0]);
        assert!(div_style.background_color.is_some());

        // p#special should have ID selector color #f43f5e and inline font-weight 700
        let p_nodes = doc.find_elements_by_tag("p");
        assert_eq!(p_nodes.len(), 1);
        let p_style = style_tree.get(p_nodes[0]).unwrap();
        assert_eq!(p_style.font_size, 28.0);
        assert_eq!(p_style.font_weight, 700.0);

        // span should inherit body's color #10b981
        let span_nodes = doc.find_elements_by_tag("span");
        assert_eq!(span_nodes.len(), 1);
        let span_style = style_tree.get(span_nodes[0]).unwrap();
        assert_eq!(span_style.color, parse_color("#10b981").unwrap());

        // head and style elements should have display: none
        let style_elem = doc.find_elements_by_tag("style");
        assert_eq!(style_elem.len(), 1);
        assert_eq!(style_tree.get(style_elem[0]).unwrap().display, Display::None);
    }
}
