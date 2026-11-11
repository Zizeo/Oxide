//! Style computation engine applying UA styles, author rules, and inline styles with inheritance.

use crate::computed::{ComputedStyle, Display};
use crate::parser::{apply_declarations, parse_declarations, parse_stylesheet, CssRule, Selector};
use oxide_dom::{Document, NodeData, NodeId};
use std::collections::HashMap;

/// Resolved map of computed styles per DOM node.
#[derive(Debug, Clone, Default)]
pub struct StyleTree {
    pub styles: HashMap<NodeId, ComputedStyle>,
}

impl StyleTree {
    #[must_use]
    pub fn get(&self, id: NodeId) -> Option<&ComputedStyle> {
        self.styles.get(&id)
    }
}

/// Resolves the full computed styles for an arena Document.
#[must_use]
pub fn resolve_styles(doc: &Document) -> StyleTree {
    // 1. Gather all <style> blocks from the document
    let mut author_rules = Vec::new();
    let style_elements = doc.find_elements_by_tag("style");
    for style_id in style_elements {
        let css_text = doc.text_content(style_id);
        let rules = parse_stylesheet(&css_text);
        author_rules.extend(rules);
    }

    resolve_styles_with_rules(doc, &author_rules)
}

/// Resolves computed styles with an explicit list of author CSS rules (e.g. including external stylesheets).
#[must_use]
pub fn resolve_styles_with_rules(doc: &Document, author_rules: &[CssRule]) -> StyleTree {
    let mut tree = StyleTree::default();
    let ua_rules = default_ua_rules();

    // Recursively resolve styles starting from root with inheritance
    if let Some(root_id) = doc.root {
        resolve_node_styles(
            doc,
            root_id,
            None,
            &ua_rules,
            author_rules,
            &mut tree,
        );
    }

    tree
}

fn resolve_node_styles(
    doc: &Document,
    node_id: NodeId,
    parent_style: Option<&ComputedStyle>,
    ua_rules: &[CssRule],
    author_rules: &[CssRule],
    tree: &mut StyleTree,
) {
    let Some(node) = doc.get(node_id) else {
        return;
    };

    let mut style = ComputedStyle::default();

    // Inherit text & typography properties from parent if available
    if let Some(parent) = parent_style {
        style.color = parent.color;
        style.font_size = parent.font_size;
        style.font_weight = parent.font_weight;
        style.text_align = parent.text_align;
        style.line_height = parent.line_height;
    }

    match &node.data {
        NodeData::Document => {
            style.display = Display::Block;
        }
        NodeData::DocType { .. } | NodeData::Comment { .. } => {
            style.display = Display::None;
        }
        NodeData::Text { .. } => {
            // Text nodes inherit inline display and parent typography
            style.display = Display::Inline;
        }
        NodeData::Element {
            tag_name,
            attributes,
        } => {
            let tag_lower = tag_name.to_ascii_lowercase();

            // A. Apply matching UA rules
            for rule in ua_rules {
                if matches_selector(&rule.selector, &tag_lower, attributes) {
                    apply_declarations(&mut style, &rule.declarations);
                }
            }

            // B. Apply matching Author rules (<style> blocks)
            for rule in author_rules {
                if matches_selector(&rule.selector, &tag_lower, attributes) {
                    apply_declarations(&mut style, &rule.declarations);
                }
            }

            // C. Apply inline style="..." attribute (highest priority)
            if let Some(inline_style_str) = attributes.get("style") {
                let inline_decls = parse_declarations(inline_style_str);
                apply_declarations(&mut style, &inline_decls);
            }
        }
    }

    // Save computed style for current node
    tree.styles.insert(node_id, style.clone());

    // Recurse to children with current computed style as parent
    for &child_id in &node.children {
        resolve_node_styles(
            doc,
            child_id,
            Some(&style),
            ua_rules,
            author_rules,
            tree,
        );
    }
}

fn matches_selector(
    selector: &Selector,
    tag_name: &str,
    attributes: &HashMap<String, String>,
) -> bool {
    match selector {
        Selector::Universal => true,
        Selector::Tag(t) => t.eq_ignore_ascii_case(tag_name),
        Selector::Class(c) => {
            if let Some(class_attr) = attributes.get("class") {
                class_attr
                    .split_whitespace()
                    .any(|tok| tok.eq_ignore_ascii_case(c))
            } else {
                false
            }
        }
        Selector::Id(id_val) => {
            if let Some(elem_id) = attributes.get("id") {
                elem_id == id_val
            } else {
                false
            }
        }
        Selector::Compound { tag, class, id } => {
            if let Some(t) = tag
                && !t.eq_ignore_ascii_case(tag_name)
            {
                return false;
            }
            if let Some(c) = class {
                if let Some(class_attr) = attributes.get("class") {
                    if !class_attr
                        .split_whitespace()
                        .any(|tok| tok.eq_ignore_ascii_case(c))
                    {
                        return false;
                    }
                } else {
                    return false;
                }
            }
            if let Some(id_val) = id {
                if let Some(elem_id) = attributes.get("id") {
                    if elem_id != id_val {
                        return false;
                    }
                } else {
                    return false;
                }
            }
            true
        }
    }
}

/// Generates the standard User-Agent stylesheet rules.
#[must_use]
pub fn default_ua_rules() -> Vec<CssRule> {
    let ua_css = r#"
        html, body { display: block; margin: 8px; font-size: 16px; color: #1a1a1a; background-color: #ffffff; }
        head, script, style, title, meta, link { display: none; }
        
        h1 { display: block; font-size: 32px; font-weight: bold; margin: 21.44px 0; color: #111827; }
        h2 { display: block; font-size: 24px; font-weight: bold; margin: 19.92px 0; color: #1f2937; }
        h3 { display: block; font-size: 18.72px; font-weight: bold; margin: 18.72px 0; color: #374151; }
        h4 { display: block; font-size: 16px; font-weight: bold; margin: 21.28px 0; color: #374151; }
        h5 { display: block; font-size: 13.28px; font-weight: bold; margin: 22.18px 0; }
        h6 { display: block; font-size: 10.72px; font-weight: bold; margin: 24.98px 0; }
        
        p { display: block; margin: 16px 0; line-height: 1.5; font-size: 16px; color: #1f2937; }
        div, section, article, header, footer, nav, main, aside { display: block; }
        span { display: inline; }
        
        a { display: inline; color: #1a0dab; text-decoration: underline; }
        b, strong { display: inline; font-weight: bold; }
        i, em { display: inline; }
        
        ul, ol { display: block; margin: 16px 0; padding-left: 32px; }
        li { display: block; margin: 4px 0; }
        
        pre { display: block; font-size: 13px; margin: 16px 0; background: #f8fafc; padding: 12px; border: 1px solid #e2e8f0; color: #0f172a; }
        code { display: inline; font-size: 13px; background: #f1f5f9; padding: 2px 4px; border: 1px solid #e2e8f0; color: #0f172a; }
        
        table { display: block; border-collapse: collapse; margin: 16px 0; }
        tr { display: flex; flex-direction: row; }
        th, td { display: block; padding: 6px 12px; border: 1px solid #e5e7eb; }
        th { font-weight: bold; background-color: #f9fafb; }
        
        button { display: inline; padding: 6px 14px; font-size: 14px; background: #e5e7eb; color: #111827; border: 1px solid #d1d5db; }
        input { display: inline; padding: 6px 10px; font-size: 14px; background: #ffffff; color: #111827; border: 1px solid #d1d5db; }
        hr { display: block; height: 1px; margin: 16px 0; background-color: #e5e7eb; }
    "#;

    parse_stylesheet(ua_css)
}
