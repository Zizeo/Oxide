//! HTML5 parser integration converting html5ever DOM into arena-allocated Document.

use crate::arena::{Document, NodeData, NodeId};
use html5ever::tendril::TendrilSink;
use html5ever::tokenizer::TokenizerOpts;
use html5ever::tree_builder::TreeBuilderOpts;
use html5ever::ParseOpts;
use markup5ever_rcdom::{Handle, NodeData as RcNodeData, RcDom};
use std::collections::HashMap;

/// Parses an HTML string into an arena Document using `html5ever`.
#[must_use]
pub fn parse_html(html: &str) -> Document {
    let opts = ParseOpts {
        tokenizer: TokenizerOpts::default(),
        tree_builder: TreeBuilderOpts::default(),
    };

    let dom = html5ever::parse_document(RcDom::default(), opts)
        .from_utf8()
        .one(html.as_bytes());

    let mut document = Document::new();
    let root_id = convert_rc_node(&dom.document, None, &mut document, 0);
    document.root = Some(root_id);

    document
}

const MAX_DOM_DEPTH: usize = 64;

fn convert_rc_node(
    handle: &Handle,
    parent: Option<NodeId>,
    doc: &mut Document,
    depth: usize,
) -> NodeId {
    let node_data = match &handle.data {
        RcNodeData::Document => NodeData::Document,
        RcNodeData::Doctype {
            name,
            public_id,
            system_id,
        } => NodeData::DocType {
            name: name.to_string(),
            public_id: public_id.to_string(),
            system_id: system_id.to_string(),
        },
        RcNodeData::Text { contents } => {
            let text = contents.borrow().to_string();
            NodeData::Text { content: text }
        }
        RcNodeData::Comment { contents } => NodeData::Comment {
            content: contents.to_string(),
        },
        RcNodeData::Element { name, attrs, .. } => {
            let tag_name = name.local.to_string();
            let mut attributes = HashMap::new();
            for attr in attrs.borrow().iter() {
                attributes.insert(attr.name.local.to_string(), attr.value.to_string());
            }
            NodeData::Element {
                tag_name,
                attributes,
            }
        }
        RcNodeData::ProcessingInstruction { .. } => NodeData::Comment {
            content: "processing instruction".to_string(),
        },
    };

    let node_id = doc.insert_node(parent, node_data);

    if depth < MAX_DOM_DEPTH {
        let children = handle.children.borrow();
        for child in children.iter() {
            let child_id = convert_rc_node(child, Some(node_id), doc, depth + 1);
            doc.append_child(node_id, child_id);
        }
    }

    node_id
}
