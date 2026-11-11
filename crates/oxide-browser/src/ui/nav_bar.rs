use egui::{Color32, CornerRadius, Key, Margin, RichText, Stroke, Ui, Vec2};
use crate::browser::BrowserState;

pub fn render_nav_bar(ui: &mut Ui, state: &mut BrowserState) {
    let active_tab = match state.active_tab() {
        Some(t) => t.clone(),
        None => return,
    };

    // Ctrl+L (see `ui::shortcuts`) asks the omnibox to take focus on the next
    // frame; consume the flag here so it fires exactly once.
    let focus_omnibox = std::mem::take(&mut state.focus_omnibox_request);

    let mut navigate_to = None;
    let mut do_back = false;
    let mut do_forward = false;
    let mut do_reload = false;
    let mut do_stop = false;

    // Omnibox Bar Frame
    egui::Frame::NONE
        .fill(Color32::from_rgb(45, 47, 54))
        .inner_margin(Margin::symmetric(10, 6))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(8.0, 0.0);

                // Back Button '◀'
                let back_enabled = active_tab.can_go_back();
                let back_btn = ui.add_enabled(
                    back_enabled,
                    egui::Button::new(
                        RichText::new("◀")
                            .size(14.0)
                            .color(if back_enabled {
                                Color32::WHITE
                            } else {
                                Color32::from_rgb(90, 95, 105)
                            }),
                    )
                    .fill(Color32::from_rgb(35, 37, 42))
                    .corner_radius(CornerRadius::same(6)),
                );
                if back_btn.clicked() {
                    do_back = true;
                }

                // Forward Button '▶'
                let fwd_enabled = active_tab.can_go_forward();
                let fwd_btn = ui.add_enabled(
                    fwd_enabled,
                    egui::Button::new(
                        RichText::new("▶")
                            .size(14.0)
                            .color(if fwd_enabled {
                                Color32::WHITE
                            } else {
                                Color32::from_rgb(90, 95, 105)
                            }),
                    )
                    .fill(Color32::from_rgb(35, 37, 42))
                    .corner_radius(CornerRadius::same(6)),
                );
                if fwd_btn.clicked() {
                    do_forward = true;
                }

                // Reload / Stop Button
                if active_tab.loading {
                    let stop_btn = ui.add(
                        egui::Button::new(
                            RichText::new("■")
                                .size(11.0)
                                .color(Color32::from_rgb(239, 68, 68)),
                        )
                        .fill(Color32::from_rgb(35, 37, 42))
                        .corner_radius(CornerRadius::same(6)),
                    );
                    if stop_btn.clicked() {
                        do_stop = true;
                    }
                } else {
                    let reload_btn = ui.add(
                        egui::Button::new(
                            RichText::new("⟳")
                                .size(15.0)
                                .color(Color32::WHITE),
                        )
                        .fill(Color32::from_rgb(35, 37, 42))
                        .corner_radius(CornerRadius::same(6)),
                    );
                    if reload_btn.clicked() {
                        do_reload = true;
                    }
                }

                // Home Button '🏠'
                let home_btn = ui.add(
                    egui::Button::new(
                        RichText::new("🏠")
                            .size(14.0)
                            .color(Color32::WHITE),
                    )
                    .fill(Color32::from_rgb(35, 37, 42))
                    .corner_radius(CornerRadius::same(6)),
                );
                if home_btn.clicked() {
                    navigate_to = Some("oxide://welcome".to_string());
                }

                // Security / Scheme Badge
                let (badge_text, badge_color) = if active_tab.error.is_some() {
                    ("🛡 BLOCKED", Color32::from_rgb(239, 68, 68))
                } else if active_tab.url.starts_with("https://") {
                    ("🔒 HTTPS", Color32::from_rgb(74, 222, 128))
                } else if active_tab.url.starts_with("oxide://") {
                    ("⚡ OXIDE", Color32::from_rgb(56, 189, 248))
                } else {
                    ("⚠ HTTP", Color32::from_rgb(251, 191, 36))
                };

                ui.label(
                    RichText::new(badge_text)
                        .color(badge_color)
                        .size(11.0)
                        .strong(),
                );

                // Omnibox URL / Google Search Text Input
                let omnibox_width = ui.available_width() - 185.0;
                let active_tab_mut = state.active_tab_mut().unwrap();

                let text_edit = egui::TextEdit::singleline(&mut active_tab_mut.omnibox_input)
                    .hint_text("Search Google or type a URL...")
                    .desired_width(omnibox_width.max(120.0))
                    .margin(Margin::symmetric(8, 6))
                    .font(egui::TextStyle::Monospace);

                let text_res = ui.add(text_edit);

    // Ctrl+L: focus the omnibox and select the whole URL, like Chrome.
    if focus_omnibox {
        text_res.request_focus();
        if let Some(mut edit_state) = egui::TextEdit::load_state(ui.ctx(), text_res.id) {
            let end = egui::text::CCursor::new(active_tab_mut.omnibox_input.chars().count());
            edit_state.cursor.set_char_range(Some(egui::text::CCursorRange::two(
                egui::text::CCursor::new(0),
                end,
            )));
            edit_state.store(ui.ctx(), text_res.id);
        }
    }

                // Detect Enter key press in Omnibox
                if text_res.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                    navigate_to = Some(active_tab_mut.omnibox_input.clone());
                }

                // Go Button '→'
                let go_btn = ui.add(
                    egui::Button::new(
                        RichText::new("➡")
                            .size(14.0)
                            .color(Color32::from_rgb(56, 189, 248)),
                    )
                    .fill(Color32::from_rgb(35, 37, 42))
                    .corner_radius(CornerRadius::same(6)),
                );
                if go_btn.clicked() {
                    navigate_to = Some(active_tab_mut.omnibox_input.clone());
                }

                // DevTools / DOM Inspector Toggle Button
                let devtools_active = state.show_devtools;
                let devtools_btn = ui.add(
                    egui::Button::new(
                        RichText::new(if devtools_active { "🛠 DevTools ⏷" } else { "🛠 DevTools ⏶" })
                            .size(12.0)
                            .color(if devtools_active {
                                Color32::from_rgb(56, 189, 248)
                            } else {
                                Color32::from_rgb(156, 163, 175)
                            }),
                    )
                    .fill(if devtools_active {
                        Color32::from_rgb(20, 30, 45)
                    } else {
                        Color32::from_rgb(35, 37, 42)
                    })
                    .stroke(if devtools_active {
                        Stroke::new(1.0_f32, Color32::from_rgb(56, 189, 248))
                    } else {
                        Stroke::NONE
                    })
                    .corner_radius(CornerRadius::same(6)),
                );

                if devtools_btn.clicked() {
                    state.show_devtools = !state.show_devtools;
                    if state.show_devtools {
                        state.devtools_tab = crate::browser::DevToolsTab::Elements;
                        state.fetch_active_tab_for_inspector();
                    }
                }
            });
        });

    // Execute state changes
    if let Some(url) = navigate_to {
        state.navigate_active_tab(url);
    } else if do_back {
        state.go_back();
    } else if do_forward {
        state.go_forward();
    } else if do_reload {
        state.reload_active_tab();
    } else if do_stop {
        state.stop_active_tab();
    }
}
