use slotmap::{new_key_type, SlotMap};
use std::collections::HashMap;

new_key_type! {
    /// Strongly-typed arena index for DOM nodes.
    pub struct NodeId;
}

/// The type and content of a DOM node.
#[derive(Debug, Clone)]
pub enum NodeData {
    Document,
    DocType {
        name: String,
        public_id: String,
        system_id: String,
    },
    Element {
        tag_name: String,
        attributes: HashMap<String, String>,
    },
    Text {
        content: String,
    },
    Comment {
        content: String,
    },
}

/// A node within the arena-backed DOM tree.
#[derive(Debug, Clone)]
pub struct Node {
    pub id: NodeId,
    pub parent: Option<NodeId>,
    pub children: Vec<NodeId>,
    pub data: NodeData,
}

impl Node {
    #[must_use]
    pub fn is_element(&self) -> bool {
        matches!(self.data, NodeData::Element { .. })
    }

    #[must_use]
    pub fn is_text(&self) -> bool {
        matches!(self.data, NodeData::Text { .. })
    }

    #[must_use]
    pub fn tag_name(&self) -> Option<&str> {
        match &self.data {
            NodeData::Element { tag_name, .. } => Some(tag_name.as_str()),
            _ => None,
        }
    }

    #[must_use]
    pub fn attribute(&self, name: &str) -> Option<&str> {
        match &self.data {
            NodeData::Element { attributes, .. } => attributes.get(name).map(|s| s.as_str()),
            _ => None,
        }
    }

    #[must_use]
    pub fn text(&self) -> Option<&str> {
        match &self.data {
            NodeData::Text { content } => Some(content.as_str()),
            _ => None,
        }
    }
}

/// Arena-allocated DOM Document.
#[derive(Debug, Clone, Default)]
pub struct Document {
    pub(crate) nodes: SlotMap<NodeId, Node>,
    pub root: Option<NodeId>,
}

impl Document {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Parse HTML string into an arena-backed Document using html5ever.
    #[must_use]
    pub fn parse(html: &str) -> Self {
        crate::parser::parse_html(html)
    }

    pub fn insert_node(&mut self, parent: Option<NodeId>, data: NodeData) -> NodeId {
        self.nodes.insert_with_key(|id| Node {
            id,
            parent,
            children: Vec::new(),
            data,
        })
    }

    pub fn append_child(&mut self, parent_id: NodeId, child_id: NodeId) {
        if let Some(parent_node) = self.nodes.get_mut(parent_id) {
            parent_node.children.push(child_id);
        }
        if let Some(child_node) = self.nodes.get_mut(child_id) {
            child_node.parent = Some(parent_id);
        }
    }

    #[must_use]
    pub fn get(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(id)
    }

    pub fn get_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        self.nodes.get_mut(id)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Sets or updates an attribute on an Element node.
    pub fn set_attribute(&mut self, id: NodeId, name: impl Into<String>, value: impl Into<String>) {
        if let Some(node) = self.nodes.get_mut(id)
            && let NodeData::Element { attributes, .. } = &mut node.data
        {
            attributes.insert(name.into(), value.into());
        }
    }

    /// Sets text content on a node, replacing any existing child nodes with a single text node.
    pub fn set_text_content(&mut self, id: NodeId, new_text: &str) {
        // Clear children
        if let Some(node) = self.nodes.get_mut(id) {
            node.children.clear();
        }

        // Insert new text node child
        let text_child = self.insert_node(
            Some(id),
            NodeData::Text {
                content: new_text.to_string(),
            },
        );
        self.append_child(id, text_child);
    }

    /// Replaces the inner HTML of an element by parsing a fragment and attaching its children.
    pub fn set_inner_html(&mut self, id: NodeId, html: &str) {
        let fragment_doc = crate::parser::parse_html(html);

        if let Some(node) = self.nodes.get_mut(id) {
            node.children.clear();
        }

        // Attach nodes from fragment_doc's body or root
        if let Some(frag_root) = fragment_doc.root {
            // Find body or direct children of root
            let body_nodes = fragment_doc.find_elements_by_tag("body");
            let source_container = body_nodes.first().copied().unwrap_or(frag_root);

            if let Some(source_node) = fragment_doc.get(source_container) {
                for &child_id in &source_node.children {
                    let cloned_child = self.clone_node_from(&fragment_doc, child_id, Some(id));
                    self.append_child(id, cloned_child);
                }
            }
        }
    }

    fn clone_node_from(&mut self, source_doc: &Document, source_id: NodeId, parent: Option<NodeId>) -> NodeId {
        let Some(source_node) = source_doc.get(source_id) else {
            return self.insert_node(parent, NodeData::Comment { content: String::new() });
        };

        let new_id = self.insert_node(parent, source_node.data.clone());
        for &child_id in &source_node.children {
            let cloned_child = self.clone_node_from(source_doc, child_id, Some(new_id));
            self.append_child(new_id, cloned_child);
        }
        new_id
    }

    /// Recursively collects all text content within a node and its descendants.
    #[must_use]
    pub fn text_content(&self, id: NodeId) -> String {
        let mut result = String::new();
        self.collect_text_content(id, &mut result);
        result
    }

    fn collect_text_content(&self, id: NodeId, out: &mut String) {
        if let Some(node) = self.get(id) {
            match &node.data {
                NodeData::Text { content } => {
                    out.push_str(content);
                }
                _ => {
                    for &child_id in &node.children {
                        self.collect_text_content(child_id, out);
                    }
                }
            }
        }
    }

    /// Extracts page title from `<title>` element if present.
    #[must_use]
    pub fn title(&self) -> Option<String> {
        let titles = self.find_elements_by_tag("title");
        titles.first().map(|&id| self.text_content(id).trim().to_string())
    }

    /// Finds all linked stylesheet URLs `<link rel="stylesheet" href="...">`.
    #[must_use]
    pub fn find_stylesheet_links(&self) -> Vec<String> {
        let mut links = Vec::new();
        let link_elements = self.find_elements_by_tag("link");
        for link_id in link_elements {
            if let Some(node) = self.get(link_id)
                && let Some(rel) = node.attribute("rel")
                && rel.eq_ignore_ascii_case("stylesheet")
                && let Some(href) = node.attribute("href")
            {
                links.push(href.to_string());
            }
        }
        links
    }

    /// Finds all element nodes with the specified tag name (case-insensitive).
    #[must_use]
    pub fn find_elements_by_tag(&self, tag: &str) -> Vec<NodeId> {
        let mut results = Vec::new();
        let tag_lower = tag.to_ascii_lowercase();
        if let Some(root_id) = self.root {
            self.find_elements_by_tag_recursive(root_id, &tag_lower, &mut results);
        }
        results
    }

    fn find_elements_by_tag_recursive(&self, id: NodeId, tag: &str, out: &mut Vec<NodeId>) {
        if let Some(node) = self.get(id) {
            if let NodeData::Element { tag_name, .. } = &node.data
                && tag_name.eq_ignore_ascii_case(tag)
            {
                out.push(id);
            }
            for &child_id in &node.children {
                self.find_elements_by_tag_recursive(child_id, tag, out);
            }
        }
    }

    /// Finds the first element with the given `id` attribute.
    #[must_use]
    pub fn get_element_by_id(&self, target_id: &str) -> Option<NodeId> {
        if let Some(root_id) = self.root {
            self.find_by_id_recursive(root_id, target_id)
        } else {
            None
        }
    }

    fn find_by_id_recursive(&self, current: NodeId, target_id: &str) -> Option<NodeId> {
        if let Some(node) = self.get(current) {
            if let Some(id_val) = node.attribute("id")
                && id_val == target_id
            {
                return Some(current);
            }
            for &child_id in &node.children {
                if let Some(found) = self.find_by_id_recursive(child_id, target_id) {
                    return Some(found);
                }
            }
        }
        None
    }

    /// Finds all elements matching the given CSS class.
    #[must_use]
    pub fn find_elements_by_class(&self, class_name: &str) -> Vec<NodeId> {
        let mut results = Vec::new();
        if let Some(root_id) = self.root {
            self.find_elements_by_class_recursive(root_id, class_name, &mut results);
        }
        results
    }

    fn find_elements_by_class_recursive(&self, id: NodeId, class_name: &str, out: &mut Vec<NodeId>) {
        if let Some(node) = self.get(id) {
            if let Some(class_attr) = node.attribute("class")
                && class_attr.split_whitespace().any(|c| c == class_name)
            {
                out.push(id);
            }
            for &child_id in &node.children {
                self.find_elements_by_class_recursive(child_id, class_name, out);
            }
        }
    }

    /// Basic query selector finding the first matching node for `#id`, `.class`, or `tag`.
    #[must_use]
    pub fn query_selector(&self, selector: &str) -> Option<NodeId> {
        let s = selector.trim();
        if let Some(id_name) = s.strip_prefix('#') {
            self.get_element_by_id(id_name)
        } else if let Some(class_name) = s.strip_prefix('.') {
            self.find_elements_by_class(class_name).first().copied()
        } else {
            self.find_elements_by_tag(s).first().copied()
        }
    }
}
