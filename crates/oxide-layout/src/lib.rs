//! Layout engine bridging DOM and computed styles into Taffy and Cosmic-Text.

pub mod engine;
pub mod layout_tree;

pub use engine::compute_layout;
pub use layout_tree::{LayoutBox, LayoutTree, Rect};

#[cfg(test)]
mod tests {
    use super::*;
    use oxide_dom::Document;
    use oxide_style::resolve_styles;

    #[test]
    fn test_example_com_layout() {
        let html = r#"<!doctype html>
<html>
<head>
    <title>Example Domain</title>
    <style type="text/css">
    body {
        background-color: #f0f0f2;
        margin: 0;
        padding: 0;
    }
    div {
        width: 600px;
        margin: 5em auto;
        padding: 2em;
        background-color: #fdfdff;
        border-radius: 0.5em;
        box-shadow: 2px 3px 7px 2px rgba(0,0,0,0.02);
    }
    h1 {
        color: #38bdf8;
    }
    a:link, a:visited {
        color: #38488f;
        text-decoration: none;
    }
    </style>    
</head>

<body>
<div>
    <h1>Example Domain</h1>
    <p>This domain is for use in illustrative examples in documents. You may use this
    domain in literature without prior coordination or asking for permission.</p>
    <p><a href="https://www.iana.org/domains/example">More information...</a></p>
</div>
</body>
</html>"#;

        let doc = Document::parse(html);
        let style_tree = resolve_styles(&doc);
        let layout = compute_layout(&doc, &style_tree, 1200.0, 800.0);

        println!("Layout root: {:#?}", layout.root);
        assert!(layout.root.is_some());
        let root = layout.root.unwrap();
        assert!(root.count_boxes() > 1);
    }
}
