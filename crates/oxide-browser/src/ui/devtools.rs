use egui::{
    Color32, CornerRadius, Margin, RichText, ScrollArea, Stroke, Ui, Vec2,
};
use oxide_dom::{Document, NodeData, NodeId};
use crate::browser::{BrowserState, ConsoleLevel, DevToolsTab};

pub fn render_devtools(ui: &mut Ui, state: &mut BrowserState) {
    // Top bar of DevTools
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(4.0, 0.0);

        ui.label(
            RichText::new("🛠 DevTools")
                .strong()
                .size(13.0)
                .color(Color32::from_rgb(56, 189, 248)),
        );

        ui.separator();

        // DevTools Tabs: Elements (DOM), Network, Source, Console
        let tabs = [
            (DevToolsTab::Elements, "🌳 Elements (DOM)"),
            (DevToolsTab::Network, "🌐 Network"),
            (DevToolsTab::Source, "📄 Raw Source"),
            (DevToolsTab::Console, "💻 Console"),
        ];

        for (tab_type, label) in tabs {
            let is_selected = state.devtools_tab == tab_type;
            let btn = ui.add(
                egui::Button::new(
                    RichText::new(label)
                        .size(12.0)
                        .color(if is_selected {
                            Color32::WHITE
                        } else {
                            Color32::from_rgb(156, 163, 175)
                        }),
                )
                .fill(if is_selected {
                    Color32::from_rgb(45, 48, 58)
                } else {
                    Color32::TRANSPARENT
                })
                .corner_radius(CornerRadius::same(4)),
            );

            if btn.clicked() {
                state.devtools_tab = tab_type;
            }
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add(
                    egui::Button::new(
                        RichText::new("×")
                            .size(12.0)
                            .color(Color32::from_rgb(156, 163, 175)),
                    )
                    .frame(false),
                )
                .clicked()
            {
                state.show_devtools = false;
            }
        });
    });

    ui.separator();

    // DevTools Content
    match state.devtools_tab {
        DevToolsTab::Elements => render_dom_inspector(ui, state),
        DevToolsTab::Network => render_network_tab(ui, state),
        DevToolsTab::Source => render_source_tab(ui, state),
        DevToolsTab::Console => render_console_tab(ui, state),
    }
}

/// Interactive DOM Tree Inspector rendering the parsed arena Document hierarchy.
fn render_dom_inspector(ui: &mut Ui, state: &mut BrowserState) {
    let active_tab = match state.active_tab_mut() {
        Some(t) => t,
        None => {
            ui.label(RichText::new("No active tab.").color(Color32::GRAY));
            return;
        }
    };

    let doc = match &active_tab.document {
        Some(d) => d.clone(),
        None => {
            ui.vertical_centered(|ui| {
                ui.add_space(30.0);
                ui.label(
                    RichText::new("No DOM Document available.")
                        .size(14.0)
                        .color(Color32::from_rgb(156, 163, 175)),
                );
                ui.label(
                    RichText::new("Navigate to an HTML page or data:text/html URL to inspect the live DOM tree.")
                        .size(12.0)
                        .color(Color32::from_rgb(107, 114, 128)),
                );
            });
            return;
        }
    };

    // Filter toolbar & DOM Statistics
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(8.0, 0.0);

        ui.label(
            RichText::new("🔍 Filter:")
                .size(11.5)
                .color(Color32::from_rgb(156, 163, 175)),
        );

        let search_edit = egui::TextEdit::singleline(&mut active_tab.dom_search_query)
            .hint_text("Filter by tag, attribute, or text...")
            .desired_width(220.0);
        ui.add(search_edit);

        if !active_tab.dom_search_query.is_empty() && ui.button("Clear").clicked() {
            active_tab.dom_search_query.clear();
        }

        ui.separator();

        ui.label(
            RichText::new(format!("Total Nodes: {}", doc.len()))
                .size(11.0)
                .strong()
                .color(Color32::from_rgb(74, 222, 128)),
        );

        if let Some(selected) = active_tab.selected_dom_node {
            ui.separator();
            if let Some(node) = doc.get(selected) {
                let tag = node.tag_name().unwrap_or("node");
                ui.label(
                    RichText::new(format!("Selected: <{}>", tag))
                        .size(11.0)
                        .color(Color32::from_rgb(56, 189, 248)),
                );
            }
            if ui.button("Deselect").clicked() {
                active_tab.selected_dom_node = None;
            }
        }
    });

    ui.separator();

    let mut selected_node = active_tab.selected_dom_node;
    let search_filter = active_tab.dom_search_query.trim().to_ascii_lowercase();

    // Two-column layout: Left = Interactive DOM Tree, Right = Selected Node Details
    ui.columns(2, |columns| {
        // Left Column: Interactive DOM Tree Hierarchy
        columns[0].vertical(|ui| {
            ui.label(
                RichText::new("DOM Tree Hierarchy")
                    .size(12.0)
                    .strong()
                    .color(Color32::from_rgb(209, 213, 219)),
            );
            ui.add_space(4.0);

            ScrollArea::both()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if let Some(root_id) = doc.root {
                        render_dom_tree_node(
                            ui,
                            &doc,
                            root_id,
                            0,
                            &mut selected_node,
                            &search_filter,
                        );
                    }
                });
        });

        // Right Column: Node Details & Computed Attributes
        columns[1].vertical(|ui| {
            ui.label(
                RichText::new("Node Details & Attributes")
                    .size(12.0)
                    .strong()
                    .color(Color32::from_rgb(209, 213, 219)),
            );
            ui.add_space(4.0);

            ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if let Some(node_id) = selected_node {
                        render_node_details(
                            ui,
                            &doc,
                            node_id,
                            active_tab.style_tree.as_ref(),
                            active_tab.layout_tree.as_ref(),
                        );
                    } else {
                        ui.centered_and_justified(|ui| {
                            ui.label(
                                RichText::new("Click a node in the tree to inspect its attributes, text content, and DOM properties.")
                                    .size(12.0)
                                    .color(Color32::from_rgb(107, 114, 128))
                                    .italics(),
                            );
                        });
                    }
                });
        });
    });

    active_tab.selected_dom_node = selected_node;
}

/// Recursively renders a DOM tree node with interactive collapsing headers.
fn render_dom_tree_node(
    ui: &mut Ui,
    doc: &Document,
    node_id: NodeId,
    depth: usize,
    selected: &mut Option<NodeId>,
    filter: &str,
) {
    let Some(node) = doc.get(node_id) else {
        return;
    };

    // Filter matching heuristic: checks tag name, attributes, or text
    let matches_filter = if filter.is_empty() {
        true
    } else {
        node_matches_filter(doc, node_id, filter)
    };

    if !matches_filter && !has_matching_descendants(doc, node_id, filter) {
        return;
    }

    let is_selected = *selected == Some(node_id);

    match &node.data {
        NodeData::Document => {
            ui.horizontal(|ui| {
                let label = ui.selectable_label(
                    is_selected,
                    RichText::new("#document")
                        .size(12.0)
                        .color(Color32::from_rgb(147, 197, 253))
                        .monospace(),
                );
                if label.clicked() {
                    *selected = Some(node_id);
                }
            });
            for &child_id in &node.children {
                render_dom_tree_node(ui, doc, child_id, depth + 1, selected, filter);
            }
        }
        NodeData::DocType { name, .. } => {
            ui.horizontal(|ui| {
                ui.add_space(depth as f32 * 12.0);
                let label = ui.selectable_label(
                    is_selected,
                    RichText::new(format!("<!DOCTYPE {}>", name))
                        .size(11.5)
                        .color(Color32::from_rgb(192, 132, 252))
                        .monospace(),
                );
                if label.clicked() {
                    *selected = Some(node_id);
                }
            });
        }
        NodeData::Comment { content } => {
            ui.horizontal(|ui| {
                ui.add_space(depth as f32 * 12.0);
                let text = if content.len() > 30 {
                    format!("<!-- {}... -->", &content[..27])
                } else {
                    format!("<!-- {} -->", content)
                };
                let label = ui.selectable_label(
                    is_selected,
                    RichText::new(text)
                        .size(11.0)
                        .color(Color32::from_rgb(110, 120, 130))
                        .monospace(),
                );
                if label.clicked() {
                    *selected = Some(node_id);
                }
            });
        }
        NodeData::Text { content } => {
            let trimmed = content.trim();
            if !trimmed.is_empty() {
                ui.horizontal(|ui| {
                    ui.add_space(depth as f32 * 12.0);
                    let display_text = if trimmed.len() > 35 {
                        format!("\"{}...\"", &trimmed[..32])
                    } else {
                        format!("\"{}\"", trimmed)
                    };
                    let label = ui.selectable_label(
                        is_selected,
                        RichText::new(display_text)
                            .size(11.0)
                            .color(Color32::from_rgb(209, 213, 219))
                            .monospace(),
                    );
                    if label.clicked() {
                        *selected = Some(node_id);
                    }
                });
            }
        }
        NodeData::Element {
            tag_name,
            attributes,
        } => {
            let has_children = !node.children.is_empty();
            let mut attrs_str = String::new();
            if let Some(id_val) = attributes.get("id") {
                attrs_str.push_str(&format!(" id=\"{}\"", id_val));
            }
            if let Some(class_val) = attributes.get("class") {
                attrs_str.push_str(&format!(" class=\"{}\"", class_val));
            }
            for (k, v) in attributes {
                if k != "id" && k != "class" && attrs_str.len() < 40 {
                    attrs_str.push_str(&format!(" {}=\"{}\"", k, v));
                }
            }

            ui.horizontal(|ui| {
                ui.add_space(depth as f32 * 12.0);

                let tag_display = if attrs_str.is_empty() {
                    format!("<{}>", tag_name)
                } else {
                    format!("<{tag_name}{attrs_str}>")
                };

                let tag_color = if is_selected {
                    Color32::WHITE
                } else {
                    Color32::from_rgb(56, 189, 248)
                };

                let label = ui.selectable_label(
                    is_selected,
                    RichText::new(tag_display)
                        .size(12.0)
                        .strong()
                        .color(tag_color)
                        .monospace(),
                );

                if label.clicked() {
                    *selected = Some(node_id);
                }

                if has_children {
                    ui.label(
                        RichText::new(format!("({} children)", node.children.len()))
                            .size(10.0)
                            .color(Color32::from_rgb(107, 114, 128)),
                    );
                }
            });

            // Recurse into children
            for &child_id in &node.children {
                render_dom_tree_node(ui, doc, child_id, depth + 1, selected, filter);
            }
        }
    }
}

/// Renders detailed properties and attributes for the selected DOM node.
fn render_node_details(
    ui: &mut Ui,
    doc: &Document,
    node_id: NodeId,
    styles: Option<&oxide_style::StyleTree>,
    layout: Option<&oxide_layout::LayoutTree>,
) {
    let Some(node) = doc.get(node_id) else {
        return;
    };

    egui::Frame::NONE
        .fill(Color32::from_rgb(20, 22, 28))
        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(45, 48, 58)))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::same(12))
        .show(ui, |ui| {
            ui.vertical(|ui| {
                // Header badge
                ui.horizontal(|ui| {
                    match &node.data {
                        NodeData::Element { tag_name, .. } => {
                            ui.label(
                                RichText::new(format!("<{}>", tag_name.to_ascii_uppercase()))
                                    .size(16.0)
                                    .strong()
                                    .color(Color32::from_rgb(56, 189, 248)),
                            );
                            ui.label(
                                RichText::new("HTML Element")
                                    .size(12.0)
                                    .color(Color32::from_rgb(156, 163, 175)),
                            );
                        }
                        NodeData::Text { .. } => {
                            ui.label(
                                RichText::new("Text Node")
                                    .size(15.0)
                                    .strong()
                                    .color(Color32::from_rgb(209, 213, 219)),
                            );
                        }
                        NodeData::Comment { .. } => {
                            ui.label(
                                RichText::new("Comment Node")
                                    .size(15.0)
                                    .strong()
                                    .color(Color32::from_rgb(110, 120, 130)),
                            );
                        }
                        NodeData::DocType { name, .. } => {
                            ui.label(
                                RichText::new(format!("DocType: {}", name))
                                    .size(15.0)
                                    .strong()
                                    .color(Color32::from_rgb(192, 132, 252)),
                            );
                        }
                        NodeData::Document => {
                            ui.label(
                                RichText::new("Document Root")
                                    .size(15.0)
                                    .strong()
                                    .color(Color32::from_rgb(147, 197, 253)),
                            );
                        }
                    }
                });

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(4.0);

                // Quick metadata
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Children:").strong().color(Color32::from_rgb(156, 163, 175)));
                    ui.label(node.children.len().to_string());

                    ui.separator();

                    ui.label(RichText::new("Parent:").strong().color(Color32::from_rgb(156, 163, 175)));
                    ui.label(if node.parent.is_some() { "Present" } else { "None (Root)" });
                });

                // Computed Layout Coordinates if available
                if let Some(l_tree) = layout
                    && let Some(root_box) = &l_tree.root
                    && let Some(box_match) = find_layout_box(root_box, node_id)
                {
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new("📐 Computed Layout (Taffy)")
                            .strong()
                            .size(12.5)
                            .color(Color32::from_rgb(74, 222, 128)),
                    );

                    egui::Frame::NONE
                        .fill(Color32::from_rgb(15, 23, 42))
                        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(30, 41, 59)))
                        .corner_radius(CornerRadius::same(4))
                        .inner_margin(Margin::same(6))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(format!("Position: ({:.1}, {:.1})", box_match.rect.x, box_match.rect.y)).monospace().size(11.5));
                                ui.separator();
                                ui.label(RichText::new(format!("Size: {:.1} × {:.1} px", box_match.rect.width, box_match.rect.height)).monospace().size(11.5));
                            });
                        });
                }

                // Computed CSS Styles if available
                if let Some(s_tree) = styles
                    && let Some(comp_style) = s_tree.get(node_id)
                {
                    ui.add_space(8.0);
                        ui.label(
                            RichText::new("🎨 Computed CSS Styles")
                                .strong()
                                .size(12.5)
                                .color(Color32::from_rgb(251, 191, 36)),
                        );

                        egui::Grid::new("computed_styles_grid")
                            .striped(true)
                            .min_col_width(90.0)
                            .spacing(Vec2::new(10.0, 3.0))
                            .show(ui, |ui| {
                                ui.label(RichText::new("display").monospace().color(Color32::from_rgb(147, 197, 253)));
                                ui.label(RichText::new(format!("{:?}", comp_style.display)).monospace());
                                ui.end_row();

                                ui.label(RichText::new("font-size").monospace().color(Color32::from_rgb(147, 197, 253)));
                                ui.label(RichText::new(format!("{:.1}px", comp_style.font_size)).monospace());
                                ui.end_row();

                                ui.label(RichText::new("font-weight").monospace().color(Color32::from_rgb(147, 197, 253)));
                                ui.label(RichText::new(format!("{:.0}", comp_style.font_weight)).monospace());
                                ui.end_row();

                                ui.label(RichText::new("margin").monospace().color(Color32::from_rgb(147, 197, 253)));
                                ui.label(RichText::new(format!("[{:.1}, {:.1}, {:.1}, {:.1}]", comp_style.margin[0], comp_style.margin[1], comp_style.margin[2], comp_style.margin[3])).monospace());
                                ui.end_row();

                                ui.label(RichText::new("padding").monospace().color(Color32::from_rgb(147, 197, 253)));
                                ui.label(RichText::new(format!("[{:.1}, {:.1}, {:.1}, {:.1}]", comp_style.padding[0], comp_style.padding[1], comp_style.padding[2], comp_style.padding[3])).monospace());
                                ui.end_row();

                                if let Some(bg) = comp_style.background_color {
                                    ui.label(RichText::new("background").monospace().color(Color32::from_rgb(147, 197, 253)));
                                    ui.label(RichText::new(format!("rgba({:.0}, {:.0}, {:.0}, {:.2})", bg[0]*255.0, bg[1]*255.0, bg[2]*255.0, bg[3])).monospace());
                                    ui.end_row();
                                }
                            });
                }

                ui.add_space(8.0);

                // Attributes table (if Element)
                if let NodeData::Element { attributes, .. } = &node.data {
                    ui.label(
                        RichText::new("HTML Attributes:")
                            .strong()
                            .size(12.5)
                            .color(Color32::from_rgb(226, 232, 240)),
                    );

                    if attributes.is_empty() {
                        ui.label(
                            RichText::new("No attributes on this element.")
                                .size(11.5)
                                .color(Color32::GRAY)
                                .italics(),
                        );
                    } else {
                        egui::Grid::new("element_attributes_grid")
                            .striped(true)
                            .min_col_width(90.0)
                            .spacing(Vec2::new(10.0, 4.0))
                            .show(ui, |ui| {
                                ui.label(RichText::new("Name").strong().color(Color32::from_rgb(147, 197, 253)));
                                ui.label(RichText::new("Value").strong().color(Color32::from_rgb(74, 222, 128)));
                                ui.end_row();

                                for (name, val) in attributes {
                                    ui.label(RichText::new(name).monospace().color(Color32::from_rgb(251, 191, 36)));
                                    ui.label(RichText::new(format!("\"{}\"", val)).monospace().color(Color32::from_rgb(226, 232, 240)));
                                    ui.end_row();
                                }
                            });
                    }
                }

                ui.add_space(8.0);

                // Text Content
                let text = doc.text_content(node_id);
                let trimmed_text = text.trim();
                if !trimmed_text.is_empty() {
                    ui.label(
                        RichText::new("Text Content:")
                            .strong()
                            .size(12.5)
                            .color(Color32::from_rgb(226, 232, 240)),
                    );

                    egui::Frame::NONE
                        .fill(Color32::from_rgb(15, 17, 22))
                        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(35, 38, 48)))
                        .corner_radius(CornerRadius::same(4))
                        .inner_margin(Margin::same(8))
                        .show(ui, |ui| {
                            ui.label(
                                RichText::new(trimmed_text)
                                    .size(12.0)
                                    .color(Color32::from_rgb(209, 213, 219)),
                            );
                        });
                }
            });
        });
}

fn find_layout_box(cur: &oxide_layout::LayoutBox, target: NodeId) -> Option<&oxide_layout::LayoutBox> {
    if cur.node_id == target {
        return Some(cur);
    }
    for child in &cur.children {
        if let Some(found) = find_layout_box(child, target) {
            return Some(found);
        }
    }
    None
}

fn node_matches_filter(doc: &Document, node_id: NodeId, filter: &str) -> bool {
    let Some(node) = doc.get(node_id) else {
        return false;
    };

    match &node.data {
        NodeData::Element {
            tag_name,
            attributes,
        } => {
            if tag_name.to_ascii_lowercase().contains(filter) {
                return true;
            }
            for (k, v) in attributes {
                if k.to_ascii_lowercase().contains(filter) || v.to_ascii_lowercase().contains(filter) {
                    return true;
                }
            }
        }
        NodeData::Text { content } if content.to_ascii_lowercase().contains(filter) => {
            return true;
        }
        _ => {}
    }

    false
}

fn has_matching_descendants(doc: &Document, node_id: NodeId, filter: &str) -> bool {
    let Some(node) = doc.get(node_id) else {
        return false;
    };

    for &child_id in &node.children {
        if node_matches_filter(doc, child_id, filter) || has_matching_descendants(doc, child_id, filter) {
            return true;
        }
    }

    false
}

fn render_network_tab(ui: &mut Ui, state: &mut BrowserState) {
    let active_tab = match state.active_tab() {
        Some(t) => t.clone(),
        None => return,
    };

    ScrollArea::vertical().show(ui, |ui| {
        if let Some(resp) = &active_tab.response {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("Request URL:")
                        .strong()
                        .color(Color32::from_rgb(209, 213, 219)),
                );
                ui.label(RichText::new(&resp.url).monospace().color(Color32::from_rgb(56, 189, 248)));
            });

            ui.add_space(4.0);

            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("HTTP Status:")
                        .strong()
                        .color(Color32::from_rgb(209, 213, 219)),
                );
                ui.label(RichText::new(resp.status.to_string()).strong().color(Color32::from_rgb(74, 222, 128)));

                ui.separator();

                ui.label(
                    RichText::new("Fetch Latency:")
                        .strong()
                        .color(Color32::from_rgb(209, 213, 219)),
                );
                ui.label(format!("{} ms", resp.duration_ms));

                ui.separator();

                ui.label(
                    RichText::new("Body Size:")
                        .strong()
                        .color(Color32::from_rgb(209, 213, 219)),
                );
                ui.label(format!("{} bytes", resp.body.len()));
            });

            ui.add_space(8.0);
            ui.label(
                RichText::new("Response Headers:")
                    .strong()
                    .color(Color32::from_rgb(226, 232, 240)),
            );

            egui::Frame::NONE
                .fill(Color32::from_rgb(18, 19, 24))
                .stroke(Stroke::new(1.0_f32, Color32::from_rgb(38, 42, 54)))
                .corner_radius(CornerRadius::same(4))
                .inner_margin(Margin::symmetric(10, 8))
                .show(ui, |ui| {
                    if resp.headers.is_empty() {
                        ui.label(
                            RichText::new("No headers recorded (data or local URI)")
                                .italics()
                                .color(Color32::GRAY),
                        );
                    } else {
                        for (k, v) in &resp.headers {
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(format!("{}:", k))
                                        .monospace()
                                        .color(Color32::from_rgb(147, 197, 253)),
                                );
                                ui.label(
                                    RichText::new(v)
                                        .monospace()
                                        .color(Color32::from_rgb(226, 232, 240)),
                                );
                            });
                        }
                    }
                });
        } else {
            ui.label(
                RichText::new("No active network request for this tab.")
                    .italics()
                    .color(Color32::GRAY),
            );
        }
    });
}

fn render_source_tab(ui: &mut Ui, state: &mut BrowserState) {
    let active_tab = match state.active_tab() {
        Some(t) => t.clone(),
        None => return,
    };

    if let Some(resp) = &active_tab.response {
        ui.horizontal(|ui| {
            ui.label(format!("Lines: {}", resp.body.lines().count()));
            ui.separator();
            ui.label(format!("Characters: {}", resp.body.len()));

            if ui.button("📋 Copy Source").clicked() {
                ui.ctx().copy_text(resp.body.clone());
            }
        });

        ui.add_space(4.0);

        ScrollArea::vertical().show(ui, |ui| {
            let mut code = resp.body.as_str();
            ui.add(
                egui::TextEdit::multiline(&mut code)
                    .font(egui::TextStyle::Monospace)
                    .desired_width(f32::INFINITY)
                    .interactive(false),
            );
        });
    } else {
        ui.label(RichText::new("No document loaded.").italics().color(Color32::GRAY));
    }
}

fn render_console_tab(ui: &mut Ui, state: &mut BrowserState) {
    let mut script_to_run: Option<String> = None;

    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format!("Log Entries ({})", state.console_logs.len()))
                .strong()
                .color(Color32::from_rgb(209, 213, 219)),
        );

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("🗑 Clear Console").clicked() {
                state.console_logs.clear();
            }
        });
    });

    ui.separator();

    // Scrollable Console logs
    ScrollArea::vertical()
        .max_height(ui.available_height() - 40.0)
        .stick_to_bottom(true)
        .show(ui, |ui| {
            for entry in &state.console_logs {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(&entry.timestamp)
                            .monospace()
                            .size(11.0)
                            .color(Color32::from_rgb(107, 114, 128)),
                    );

                    let (level_str, level_color) = match entry.level {
                        ConsoleLevel::Info => ("[INFO]", Color32::from_rgb(56, 189, 248)),
                        ConsoleLevel::Warn => ("[WARN]", Color32::from_rgb(251, 191, 36)),
                        ConsoleLevel::Error => ("[ERROR]", Color32::from_rgb(239, 68, 68)),
                        ConsoleLevel::Net => ("[NET]", Color32::from_rgb(74, 222, 128)),
                    };

                    ui.label(
                        RichText::new(level_str)
                            .monospace()
                            .size(11.0)
                            .strong()
                            .color(level_color),
                    );

                    ui.label(
                        RichText::new(&entry.message)
                            .monospace()
                            .size(12.0)
                            .color(Color32::from_rgb(226, 232, 240)),
                    );
                });
            }
        });

    ui.separator();

    // Interactive JavaScript REPL Input Bar
    if let Some(active_tab) = state.active_tab_mut() {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(6.0, 0.0);
            ui.label(
                RichText::new("JS >")
                    .monospace()
                    .strong()
                    .size(13.0)
                    .color(Color32::from_rgb(56, 189, 248)),
            );

            let repl_width = ui.available_width() - 80.0;
            let text_edit = egui::TextEdit::singleline(&mut active_tab.js_repl_input)
                .desired_width(repl_width.max(100.0))
                .hint_text("Enter JavaScript expression (e.g. document.title = 'New Title', console.log(1+2))...")
                .font(egui::TextStyle::Monospace);

            let resp = ui.add(text_edit);
            let enter_pressed = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));

            let eval_btn = ui.add(
                egui::Button::new(
                    RichText::new("▶ Run")
                        .size(12.0)
                        .color(Color32::from_rgb(56, 189, 248)),
                )
                .fill(Color32::from_rgb(30, 41, 59))
                .corner_radius(CornerRadius::same(4)),
            );

            if (enter_pressed || eval_btn.clicked()) && !active_tab.js_repl_input.trim().is_empty() {
                script_to_run = Some(active_tab.js_repl_input.trim().to_string());
                active_tab.js_repl_input.clear();
            }
        });
    }

    // Execute REPL script and re-run pipeline if DOM was mutated
    if let Some(script) = script_to_run {
        state.log(ConsoleLevel::Info, format!("> {}", script));

        let eval_outcome = if let Some(tab) = state.active_tab_mut() {
            if let Some(js_cell) = &tab.js_runtime {
                let mut js = js_cell.borrow_mut();
                let res = js.eval(&script);
                let mutated = js.take_mutation();
                Some((res, mutated))
            } else {
                None
            }
        } else {
            None
        };

        match eval_outcome {
            Some((Ok(val), did_mutate)) => {
                state.log(ConsoleLevel::Net, format!("← {}", val));
                if did_mutate {
                    if let Some(tab) = state.active_tab_mut()
                        && let Some(doc) = &tab.document
                    {
                        let styles = oxide_style::resolve_styles(doc);
                        let layout = oxide_layout::compute_layout(doc, &styles, 1000.0, 700.0);
                        tab.style_tree = Some(styles);
                        tab.layout_tree = Some(layout);
                    }
                    state.log(
                        ConsoleLevel::Warn,
                        "DOM mutated by script: Re-ran style -> layout -> paint pipeline.",
                    );
                }
            }
            Some((Err(err), _)) => {
                state.log(ConsoleLevel::Error, format!("← Error: {}", err));
            }
            None => {
                state.log(
                    ConsoleLevel::Error,
                    "No JavaScript runtime initialized for active tab.",
                );
            }
        }
    }
}
