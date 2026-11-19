//! Style computation engine applying UA styles, author rules, and inline styles with inheritance.

use crate::computed::{ComputedStyle, Display};
use crate::parser::{
    apply_declarations, apply_declarations_filtered, parse_declarations, parse_stylesheet,
    Combinator, ComplexSelector, CssRule, PseudoClass, Selector, SimpleSelector,
};
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

    // Inherit text & typography properties and CSS custom properties from parent
    if let Some(parent) = parent_style {
        style.color = parent.color;
        style.font_size = parent.font_size;
        style.font_weight = parent.font_weight;
        style.text_align = parent.text_align;
        style.line_height = parent.line_height;
        style.custom_props = parent.custom_props.clone();
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
                if matches_selector_in_doc(doc, node_id, &rule.selector, &tag_lower, attributes) {
                    apply_declarations(&mut style, &rule.declarations);
                }
            }

            // B. Collect matching Author rules and sort by (specificity, source_order)
            let mut matched: Vec<((u16, u16, u16), usize, &CssRule)> = author_rules
                .iter()
                .enumerate()
                .filter(|(_, r)| {
                    matches_selector_in_doc(doc, node_id, &r.selector, &tag_lower, attributes)
                })
                .map(|(idx, r)| (r.selector.specificity(), idx, r))
                .collect();
            matched.sort_by_key(|(spec, idx, _)| (*spec, *idx));

            for (_, _, rule) in &matched {
                apply_declarations_filtered(&mut style, &rule.declarations, Some(false));
            }

            let inline_decls = attributes.get("style").map(|s| parse_declarations(s));
            if let Some(decls) = &inline_decls {
                apply_declarations_filtered(&mut style, decls, Some(false));
            }
            for (_, _, rule) in &matched {
                apply_declarations_filtered(&mut style, &rule.declarations, Some(true));
            }
            if let Some(decls) = &inline_decls {
                apply_declarations_filtered(&mut style, decls, Some(true));
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

fn matches_selector_in_doc(
    doc: &Document,
    node_id: NodeId,
    selector: &Selector,
    tag_name: &str,
    attributes: &HashMap<String, String>,
) -> bool {
    match selector {
        Selector::Complex(c) => matches_complex(doc, node_id, c),
        other => matches_selector(other, tag_name, attributes),
    }
}

fn matches_complex(doc: &Document, node_id: NodeId, complex: &ComplexSelector) -> bool {
    let Some((last, _)) = complex.parts.last() else {
        return false;
    };
    if !matches_simple(doc, node_id, last) {
        return false;
    }
    matches_complex_prefix(doc, node_id, &complex.parts[..complex.parts.len() - 1])
}

fn matches_complex_prefix(
    doc: &Document,
    right_id: NodeId,
    prefix: &[(SimpleSelector, Option<Combinator>)],
) -> bool {
    let Some((sel, comb_opt)) = prefix.last() else {
        return true;
    };
    let rest = &prefix[..prefix.len() - 1];
    let comb = comb_opt.unwrap_or(Combinator::Descendant);

    match comb {
        Combinator::Child => {
            let Some(parent_id) = doc.get(right_id).and_then(|n| n.parent) else {
                return false;
            };
            matches_simple(doc, parent_id, sel) && matches_complex_prefix(doc, parent_id, rest)
        }
        Combinator::Descendant => {
            let mut cur = doc.get(right_id).and_then(|n| n.parent);
            while let Some(anc_id) = cur {
                if matches_simple(doc, anc_id, sel) && matches_complex_prefix(doc, anc_id, rest) {
                    return true;
                }
                cur = doc.get(anc_id).and_then(|n| n.parent);
            }
            false
        }
        Combinator::AdjacentSibling => {
            let Some(prev_id) = prev_element_sibling(doc, right_id) else {
                return false;
            };
            matches_simple(doc, prev_id, sel) && matches_complex_prefix(doc, prev_id, rest)
        }
        Combinator::GeneralSibling => {
            let mut cur = prev_element_sibling(doc, right_id);
            while let Some(sib_id) = cur {
                if matches_simple(doc, sib_id, sel) && matches_complex_prefix(doc, sib_id, rest) {
                    return true;
                }
                cur = prev_element_sibling(doc, sib_id);
            }
            false
        }
    }
}

fn prev_element_sibling(doc: &Document, node_id: NodeId) -> Option<NodeId> {
    let parent_id = doc.get(node_id)?.parent?;
    let children = &doc.get(parent_id)?.children;
    let pos = children.iter().position(|&id| id == node_id)?;
    children[..pos]
        .iter()
        .rev()
        .copied()
        .find(|&id| matches!(doc.get(id).map(|n| &n.data), Some(NodeData::Element { .. })))
}

fn element_index_in_parent(doc: &Document, node_id: NodeId) -> Option<usize> {
    let parent_id = doc.get(node_id)?.parent?;
    let children = &doc.get(parent_id)?.children;
    let mut idx = 0usize;
    for &cid in children {
        if matches!(doc.get(cid).map(|n| &n.data), Some(NodeData::Element { .. })) {
            idx += 1;
            if cid == node_id {
                return Some(idx);
            }
        }
    }
    None
}

fn matches_simple(doc: &Document, node_id: NodeId, sel: &SimpleSelector) -> bool {
    let Some(NodeData::Element {
        tag_name,
        attributes,
    }) = doc.get(node_id).map(|n| &n.data)
    else {
        return false;
    };
    if let Some(t) = &sel.tag
        && !t.eq_ignore_ascii_case(tag_name)
    {
        return false;
    }
    if let Some(id_val) = &sel.id
        && attributes.get("id").map(String::as_str) != Some(id_val.as_str())
    {
        return false;
    }
    for c in &sel.classes {
        let has = attributes
            .get("class")
            .is_some_and(|attr| attr.split_whitespace().any(|tok| tok.eq_ignore_ascii_case(c)));
        if !has {
            return false;
        }
    }
    for p in &sel.pseudos {
        match p {
            PseudoClass::NthChild(n) => {
                if element_index_in_parent(doc, node_id) != Some(*n) {
                    return false;
                }
            }
            PseudoClass::Not(inner) => {
                if matches_simple(doc, node_id, inner) {
                    return false;
                }
            }
            PseudoClass::Is(list) => {
                if !list.iter().any(|s| matches_simple(doc, node_id, s)) {
                    return false;
                }
            }
            PseudoClass::Hover | PseudoClass::Active | PseudoClass::Focus => {}
        }
    }
    true
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
        Selector::Complex(_) => false,
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
