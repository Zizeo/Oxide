//! HTML parsing and arena-backed DOM tree subsystem for Oxide.

pub mod arena;
pub mod parser;

pub use arena::{node_key_id, Document, Node, NodeData, NodeId};
pub use parser::parse_html;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_html() {
        let html = r#"<!DOCTYPE html>
        <html>
        <head>
            <title>Test Page Title</title>
        </head>
        <body>
            <h1 id="heading" class="title">Hello World</h1>
            <p>This is a paragraph with <a href="https://example.com">a link</a>.</p>
        </body>
        </html>"#;

        let doc = Document::parse(html);
        assert!(!doc.is_empty());
        assert!(doc.root.is_some());

        // Test title extraction
        assert_eq!(doc.title(), Some("Test Page Title".to_string()));

        // Test find by tag
        let h1_nodes = doc.find_elements_by_tag("h1");
        assert_eq!(h1_nodes.len(), 1);
        let h1_node = doc.get(h1_nodes[0]).unwrap();
        assert_eq!(h1_node.tag_name(), Some("h1"));
        assert_eq!(h1_node.attribute("id"), Some("heading"));
        assert_eq!(h1_node.attribute("class"), Some("title"));
        assert_eq!(doc.text_content(h1_nodes[0]), "Hello World");

        // Test get element by ID
        let found_by_id = doc.get_element_by_id("heading");
        assert_eq!(found_by_id, Some(h1_nodes[0]));

        // Test anchor tag
        let a_nodes = doc.find_elements_by_tag("a");
        assert_eq!(a_nodes.len(), 1);
        let a_node = doc.get(a_nodes[0]).unwrap();
        assert_eq!(a_node.attribute("href"), Some("https://example.com"));
        assert_eq!(doc.text_content(a_nodes[0]), "a link");
    }

    #[test]
    fn test_malformed_html_recovery() {
        // html5ever handles malformed tags seamlessly
        let html = "<p>Unclosed paragraph<div>Nested without closing";
        let doc = Document::parse(html);
        assert!(!doc.is_empty());
        let p_nodes = doc.find_elements_by_tag("p");
        assert!(!p_nodes.is_empty());
    }

    #[test]
    fn test_form_controls_focus_typing_and_radio_toggle() {
        let html = r#"<form>
            <input id="u" type="text" value="a">
            <input id="c" type="checkbox">
            <input id="r1" type="radio" name="grp" checked>
            <input id="r2" type="radio" name="grp">
        </form>"#;
        let mut doc = Document::parse(html);
        let u = doc.get_element_by_id("u").unwrap();
        let c = doc.get_element_by_id("c").unwrap();
        let r1 = doc.get_element_by_id("r1").unwrap();
        let r2 = doc.get_element_by_id("r2").unwrap();

        assert_eq!(doc.focus_next(false), Some(u));
        doc.type_char('b');
        assert_eq!(doc.get(u).unwrap().attribute("value"), Some("ab"));
        doc.backspace();
        assert_eq!(doc.get(u).unwrap().attribute("value"), Some("a"));

        doc.toggle_control(c);
        assert!(doc.get(c).unwrap().attribute("checked").is_some());
        doc.toggle_control(r2);
        assert!(doc.get(r1).unwrap().attribute("checked").is_none());
        assert!(doc.get(r2).unwrap().attribute("checked").is_some());
    }
}
