use crate::browser::{BrowserState, DownloadStatus, PendingMenu};
use crate::ui::menu::MenuKind;
use egui::{
    pos2, vec2, Align, Color32, CornerRadius, Key, Layout, Margin, Rect, RichText, ScrollArea,
    Sense, Stroke, Ui, Vec2,
};

const BAR_BG: Color32 = Color32::from_rgb(43, 45, 49);
const BTN_BG: Color32 = Color32::from_rgb(56, 58, 64);
const PILL_BG: Color32 = Color32::from_rgb(30, 31, 34);
const PILL_BORDER: Color32 = Color32::from_rgb(56, 58, 64);
const PILL_FOCUS_BORDER: Color32 = Color32::from_rgb(102, 157, 246);
const CHROME_BLUE: Color32 = Color32::from_rgb(102, 157, 246);

fn nav_icon_btn(ui: &mut Ui, enabled: bool, glyph: &str, color: Color32, tip: &str) -> egui::Response {
    ui.add_enabled_ui(enabled, |ui| {
        ui.add_sized(
            Vec2::new(28.0, 28.0),
            egui::Button::new(
                RichText::new(glyph).size(14.5).color(if enabled {
                    color
                } else {
                    Color32::from_rgb(95, 99, 104)
                }),
            )
            .fill(Color32::TRANSPARENT)
            .corner_radius(CornerRadius::same(14)),
        )
    })
    .inner
    .on_hover_text(tip)
}

/// Compact vector site-identity icon button on the left of the omnibox.
fn site_identity_btn(ui: &mut Ui, url: &str, has_error: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(24.0, 24.0), Sense::click());
    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    let bg = if resp.hovered() {
        Color32::from_rgb(52, 54, 60)
    } else {
        Color32::from_rgb(40, 42, 47)
    };
    let painter = ui.painter();
    painter.circle_filled(rect.center(), 11.0, bg);

    let c = rect.center();
    if has_error {
        let color = Color32::from_rgb(248, 113, 113);
        painter.circle_stroke(c, 5.5, Stroke::new(1.4_f32, color));
        painter.line_segment([pos2(c.x, c.y - 2.5), pos2(c.x, c.y + 0.5)], Stroke::new(1.4_f32, color));
        painter.circle_filled(pos2(c.x, c.y + 2.6), 0.8, color);
    } else if url.starts_with("https://") {
        let color = Color32::from_rgb(154, 160, 166);
        let body = Rect::from_center_size(pos2(c.x, c.y + 1.5), vec2(8.0, 6.0));
        painter.rect_stroke(body, CornerRadius::same(1), Stroke::new(1.3_f32, color), egui::StrokeKind::Middle);
        painter.line_segment(
            [pos2(c.x - 2.2, c.y - 1.5), pos2(c.x - 2.2, c.y - 4.0)],
            Stroke::new(1.3_f32, color),
        );
        painter.line_segment(
            [pos2(c.x + 2.2, c.y - 1.5), pos2(c.x + 2.2, c.y - 4.0)],
            Stroke::new(1.3_f32, color),
        );
        painter.line_segment(
            [pos2(c.x - 2.2, c.y - 4.0), pos2(c.x + 2.2, c.y - 4.0)],
            Stroke::new(1.3_f32, color),
        );
    } else if url.starts_with("oxide://") {
        painter.circle_filled(c, 4.0, CHROME_BLUE);
    } else {
        let color = Color32::from_rgb(251, 191, 36);
        painter.circle_stroke(c, 5.5, Stroke::new(1.3_f32, color));
        painter.circle_filled(pos2(c.x, c.y - 2.2), 0.8, color);
        painter.line_segment([pos2(c.x, c.y - 0.5), pos2(c.x, c.y + 2.8)], Stroke::new(1.3_f32, color));
    }

    resp.on_hover_text("View site information")
}

pub fn render_nav_bar(ui: &mut Ui, state: &mut BrowserState) {
    let active_tab = match state.active_tab() {
        Some(t) => t.clone(),
        None => return,
    };

    let focus_omnibox = std::mem::take(&mut state.focus_omnibox_request);

    let mut navigate_to = None;
    let mut switch_to_tab = None;
    let mut do_back = false;
    let mut do_forward = false;
    let mut do_reload = false;
    let mut do_stop = false;
    let mut do_reset_zoom = false;
    let mut do_toggle_bookmark = false;
    let mut open_menu: Option<PendingMenu> = None;
    let mut omnibox_rect = Rect::NOTHING;
    let mut omnibox_focused = false;
    let mut omnibox_changed = false;
    let mut omnibox_escape = false;
    let mut enter_submitted = false;

    let pill_focused_id = ui.make_persistent_id("oxide_omnibox_focused");
    let dropdown_rect_id = egui::Id::new("oxide_omnibox_dropdown_rect");
    let dropdown_sel_id = egui::Id::new("oxide_omnibox_dropdown_sel");
    let dropdown_dismissed_id = egui::Id::new("oxide_omnibox_dropdown_dismissed");

    let was_focused = ui
        .ctx()
        .data(|d| d.get_temp::<bool>(pill_focused_id))
        .unwrap_or(false);
    let mut pressed_down = false;
    let mut pressed_up = false;
    if was_focused && state.omnibox_suggestions_open {
        ui.ctx().input_mut(|i| {
            pressed_down = i.consume_key(egui::Modifiers::NONE, Key::ArrowDown);
            pressed_up = i.consume_key(egui::Modifiers::NONE, Key::ArrowUp);
        });
    }

    egui::Frame::NONE
        .fill(BAR_BG)
        .inner_margin(Margin::symmetric(10, 5))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(2.0, 0.0);

                // 1. Classic Chrome navigation cluster: Back, Forward, Reload/Stop
                if nav_icon_btn(
                    ui,
                    active_tab.can_go_back(),
                    "←",
                    Color32::from_rgb(232, 234, 237),
                    "Click to go back",
                )
                .clicked()
                {
                    do_back = true;
                }
                if nav_icon_btn(
                    ui,
                    active_tab.can_go_forward(),
                    "→",
                    Color32::from_rgb(232, 234, 237),
                    "Click to go forward",
                )
                .clicked()
                {
                    do_forward = true;
                }
                if active_tab.loading {
                    if nav_icon_btn(
                        ui,
                        true,
                        "×",
                        Color32::from_rgb(232, 234, 237),
                        "Stop loading this page",
                    )
                    .clicked()
                    {
                        do_stop = true;
                    }
                } else if nav_icon_btn(
                    ui,
                    true,
                    "↻",
                    Color32::from_rgb(232, 234, 237),
                    "Reload this page (Ctrl+R)",
                )
                .clicked()
                {
                    do_reload = true;
                }

                ui.add_space(6.0);

                // 2. Right cluster (right_to_left first so nothing ever clips)
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(6.0, 0.0);

                    let menu_btn = ui
                        .add(
                            egui::Button::new(
                                RichText::new("⋮")
                                    .size(15.5)
                                    .color(Color32::from_rgb(232, 234, 237)),
                            )
                            .min_size(Vec2::new(28.0, 28.0))
                            .fill(Color32::TRANSPARENT)
                            .corner_radius(CornerRadius::same(14)),
                        )
                        .on_hover_text("Customize and control Oxide");
                    if menu_btn.clicked() {
                        open_menu = Some(PendingMenu {
                            kind: MenuKind::App,
                            pos: menu_btn.rect.left_bottom(),
                        });
                    }

                    if state.is_incognito {
                        egui::Frame::NONE
                            .fill(Color32::from_rgb(66, 39, 90))
                            .corner_radius(CornerRadius::same(14))
                            .inner_margin(Margin::symmetric(9, 4))
                            .show(ui, |ui| {
                                ui.label(
                                    RichText::new("Incognito")
                                        .size(11.0)
                                        .strong()
                                        .color(Color32::from_rgb(216, 180, 254)),
                                );
                            });
                    }

                    if !state.downloads.is_empty() {
                        let active_dl = state
                            .downloads
                            .iter()
                            .filter(|d| matches!(d.status, DownloadStatus::InProgress))
                            .count();
                        let dl_label = if active_dl > 0 {
                            format!("↓ {active_dl}")
                        } else {
                            "↓".to_string()
                        };
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new(dl_label)
                                        .size(12.5)
                                        .color(Color32::from_rgb(232, 234, 237)),
                                )
                                .min_size(Vec2::new(28.0, 28.0))
                                .fill(BTN_BG)
                                .corner_radius(CornerRadius::same(14)),
                            )
                            .on_hover_text("Downloads (Ctrl+J)")
                            .clicked()
                        {
                            navigate_to = Some("oxide://downloads".to_string());
                        }
                    }

                    // 3. Chrome Pill Omnibox filling remaining center width
                    let focus_t = ui.ctx().animate_bool_responsive(pill_focused_id, was_focused);
                    let border_color = Color32::from_rgb(
                        (PILL_BORDER.r() as f32
                            + (PILL_FOCUS_BORDER.r() as f32 - PILL_BORDER.r() as f32) * focus_t)
                            as u8,
                        (PILL_BORDER.g() as f32
                            + (PILL_FOCUS_BORDER.g() as f32 - PILL_BORDER.g() as f32) * focus_t)
                            as u8,
                        (PILL_BORDER.b() as f32
                            + (PILL_FOCUS_BORDER.b() as f32 - PILL_BORDER.b() as f32) * focus_t)
                            as u8,
                    );
                    let pill_bg = if was_focused {
                        Color32::from_rgb(24, 25, 28)
                    } else {
                        PILL_BG
                    };

                    let is_bookmarked = state.is_bookmarked(&active_tab.url);

                    let pill_res = egui::Frame::NONE
                        .fill(pill_bg)
                        .stroke(Stroke::new(if was_focused { 1.5_f32 } else { 1.0_f32 }, border_color))
                        .corner_radius(CornerRadius::same(16))
                        .inner_margin(Margin::symmetric(5, 3))
                        .show(ui, |ui| {
                            ui.set_min_height(26.0);
                            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                                ui.spacing_mut().item_spacing = Vec2::new(6.0, 0.0);

                                let badge_res = site_identity_btn(
                                    ui,
                                    &active_tab.url,
                                    active_tab.error.is_some(),
                                );
                                if badge_res.clicked() {
                                    open_menu = Some(PendingMenu {
                                        kind: MenuKind::SiteInfo,
                                        pos: badge_res.rect.left_bottom(),
                                    });
                                }

                                let zoom_changed = (active_tab.zoom - 1.0).abs() > 0.001;
                                let zoom_pct = active_tab.zoom_percent();

                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    ui.spacing_mut().item_spacing = Vec2::new(4.0, 0.0);

                                    // Bookmark star (Chrome-style inside omnibox right edge)
                                    let star_btn = ui
                                        .add(
                                            egui::Button::new(
                                                RichText::new(if is_bookmarked { "★" } else { "☆" })
                                                    .size(14.0)
                                                    .color(if is_bookmarked {
                                                        CHROME_BLUE
                                                    } else {
                                                        Color32::from_rgb(154, 160, 166)
                                                    }),
                                            )
                                            .min_size(Vec2::new(24.0, 22.0))
                                            .fill(Color32::TRANSPARENT)
                                            .corner_radius(CornerRadius::same(11)),
                                        )
                                        .on_hover_text(if is_bookmarked {
                                            "Remove bookmark (Ctrl+D)"
                                        } else {
                                            "Bookmark this tab (Ctrl+D)"
                                        });
                                    if star_btn.clicked() {
                                        do_toggle_bookmark = true;
                                    }

                                    if zoom_changed
                                        && ui
                                            .add(
                                                egui::Button::new(
                                                    RichText::new(format!("{zoom_pct}%"))
                                                        .size(11.0)
                                                        .color(CHROME_BLUE),
                                                )
                                                .fill(Color32::from_rgb(40, 42, 47))
                                                .corner_radius(CornerRadius::same(10)),
                                            )
                                            .on_hover_text("Reset zoom to 100% (Ctrl+0)")
                                            .clicked()
                                    {
                                        do_reset_zoom = true;
                                    }

                                    let input_w = ui.available_width().max(24.0);
                                    let url_before_edit = active_tab.url.clone();
                                    let active_tab_mut = state.active_tab_mut().unwrap();
                                    let text_edit = egui::TextEdit::singleline(
                                        &mut active_tab_mut.omnibox_input,
                                    )
                                    .frame(false)
                                    .hint_text("Search Google or type a URL")
                                    .desired_width(input_w);

                                    let text_res = ui.add(text_edit);
                                    omnibox_changed = text_res.changed();
                                    omnibox_focused = text_res.has_focus();
                                    ui.ctx()
                                        .data_mut(|d| d.insert_temp(pill_focused_id, omnibox_focused));

                                    if focus_omnibox {
                                        text_res.request_focus();
                                        omnibox_focused = true;
                                        ui.ctx().data_mut(|d| {
                                            d.insert_temp(pill_focused_id, true);
                                            d.insert_temp(dropdown_dismissed_id, false);
                                            d.remove::<usize>(dropdown_sel_id);
                                        });
                                        let mut edit_state =
                                            egui::TextEdit::load_state(ui.ctx(), text_res.id)
                                                .unwrap_or_default();
                                        let end = egui::text::CCursor::new(
                                            active_tab_mut.omnibox_input.chars().count(),
                                        );
                                        edit_state.cursor.set_char_range(Some(
                                            egui::text::CCursorRange::two(
                                                egui::text::CCursor::new(0),
                                                end,
                                            ),
                                        ));
                                        edit_state.store(ui.ctx(), text_res.id);
                                    }

                                    if omnibox_focused && ui.input(|i| i.key_pressed(Key::Escape)) {
                                        omnibox_escape = true;
                                        active_tab_mut.omnibox_input = url_before_edit;
                                        text_res.surrender_focus();
                                        omnibox_focused = false;
                                        ui.ctx().data_mut(|d| {
                                            d.insert_temp(pill_focused_id, false);
                                            d.insert_temp(dropdown_dismissed_id, true);
                                            d.remove::<usize>(dropdown_sel_id);
                                        });
                                    } else if text_res.lost_focus()
                                        && ui.input(|i| i.key_pressed(Key::Enter))
                                    {
                                        enter_submitted = true;
                                    }
                                });
                            });
                        });
                    omnibox_rect = pill_res.response.rect;
                    if active_tab.loading {
                        let t = ui.input(|i| i.time) as f32;
                        let phase = (t * 0.9) % 1.0;
                        let usable_w = (omnibox_rect.width() - 28.0).max(20.0);
                        let bar_w = (usable_w * 0.28).max(24.0);
                        let start_x = omnibox_rect.left() + 14.0 + (usable_w - bar_w) * phase;
                        let y = omnibox_rect.bottom() - 1.5;
                        ui.painter().line_segment(
                            [pos2(start_x, y), pos2(start_x + bar_w, y)],
                            Stroke::new(2.0_f32, CHROME_BLUE),
                        );
                        ui.ctx().request_repaint();
                    }
                });
            });
        });

    // Omnibox autocomplete suggestions dropdown
    if omnibox_changed {
        ui.ctx().data_mut(|d| {
            d.insert_temp(dropdown_dismissed_id, false);
            d.remove::<usize>(dropdown_sel_id);
        });
    }
    state.omnibox_focused = omnibox_focused;

    let dismissed = ui
        .ctx()
        .data(|d| d.get_temp::<bool>(dropdown_dismissed_id))
        .unwrap_or(false);
    let last_dropdown_rect = ui.ctx().data(|d| d.get_temp::<Rect>(dropdown_rect_id));
    let interacting_dropdown = last_dropdown_rect.is_some_and(|r| {
        ui.input(|i| {
            (i.pointer.any_down() || i.pointer.any_released())
                && i.pointer.interact_pos().is_some_and(|p| r.contains(p))
        })
    });

    let current_input = state
        .active_tab()
        .map(|t| t.omnibox_input.clone())
        .unwrap_or_default();
    let query = current_input.trim();
    let keep_for_interaction = state.omnibox_suggestions_open && interacting_dropdown;
    let show_suggestions = !omnibox_escape
        && !dismissed
        && (omnibox_focused || keep_for_interaction)
        && !query.is_empty()
        && query != active_tab.url;

    let suggestions = if show_suggestions || enter_submitted {
        if query.is_empty() {
            Vec::new()
        } else {
            collect_suggestions(state, query)
        }
    } else {
        Vec::new()
    };

    let mut selected_idx = ui
        .ctx()
        .data(|d| d.get_temp::<usize>(dropdown_sel_id))
        .filter(|&i| i < suggestions.len());
    if show_suggestions && !suggestions.is_empty() {
        if pressed_down {
            selected_idx = Some(match selected_idx {
                Some(i) => (i + 1) % suggestions.len(),
                None => 0,
            });
        } else if pressed_up {
            selected_idx = Some(match selected_idx {
                Some(0) | None => suggestions.len() - 1,
                Some(i) => i - 1,
            });
        }
        ui.ctx().data_mut(|d| match selected_idx {
            Some(i) => d.insert_temp(dropdown_sel_id, i),
            None => d.remove::<usize>(dropdown_sel_id),
        });
    }

    if enter_submitted {
        if let Some(item) = selected_idx.and_then(|i| suggestions.get(i)) {
            match &item.target {
                SuggestionTarget::Tab(id) => switch_to_tab = Some(*id),
                SuggestionTarget::Url(u) => navigate_to = Some(u.clone()),
            }
        } else if !query.is_empty() {
            navigate_to = Some(current_input.clone());
        }
        state.omnibox_suggestions_open = false;
        ui.ctx().data_mut(|d| {
            d.remove::<Rect>(dropdown_rect_id);
            d.remove::<usize>(dropdown_sel_id);
        });
    } else {
        state.omnibox_suggestions_open = show_suggestions && !suggestions.is_empty();
        if state.omnibox_suggestions_open {
            let max_w = (ui.ctx().screen_rect().width() - 16.0).max(200.0);
            let drop_w = omnibox_rect.width().clamp(240.0, max_w);
            let area_resp = egui::Area::new(egui::Id::new("oxide_omnibox_suggestions"))
                .order(egui::Order::Foreground)
                .fixed_pos(omnibox_rect.left_bottom() + Vec2::new(0.0, 4.0))
                .show(ui.ctx(), |ui| {
                    egui::Frame::popup(ui.style())
                        .fill(Color32::from_rgb(41, 43, 48))
                        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(60, 64, 72)))
                        .corner_radius(CornerRadius::same(12))
                        .inner_margin(Margin::same(6))
                        .show(ui, |ui| {
                            ui.set_width(drop_w);
                            ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                                for (idx, item) in suggestions.into_iter().enumerate() {
                                    let is_hi = selected_idx == Some(idx);
                                    let fill = if is_hi {
                                        Color32::from_rgb(56, 62, 74)
                                    } else {
                                        Color32::TRANSPARENT
                                    };
                                    let btn = ui.add(
                                        egui::Button::new(
                                            RichText::new(format!("{}  {}", item.kind, item.label))
                                                .size(12.5),
                                        )
                                        .min_size(Vec2::new(drop_w - 12.0, 26.0))
                                        .fill(fill)
                                        .truncate(),
                                    );
                                    if btn.clicked() {
                                        match item.target {
                                            SuggestionTarget::Tab(id) => switch_to_tab = Some(id),
                                            SuggestionTarget::Url(u) => navigate_to = Some(u),
                                        }
                                        state.omnibox_suggestions_open = false;
                                        ui.ctx().data_mut(|d| {
                                            d.insert_temp(dropdown_dismissed_id, true);
                                            d.remove::<usize>(dropdown_sel_id);
                                        });
                                    }
                                }
                            });
                        });
                });
            ui.ctx()
                .data_mut(|d| d.insert_temp(dropdown_rect_id, area_resp.response.rect));
        } else {
            ui.ctx().data_mut(|d| {
                d.remove::<Rect>(dropdown_rect_id);
                d.remove::<usize>(dropdown_sel_id);
            });
        }
    }

    if let Some(menu) = open_menu {
        state.pending_menu = Some(menu);
    }
    if do_toggle_bookmark {
        state.toggle_bookmark(&active_tab.url, &active_tab.title);
    }
    if do_reset_zoom {
        state.set_active_tab_zoom(1.0);
    }
    if let Some(id) = switch_to_tab {
        state.activate_tab(id);
    } else if let Some(url) = navigate_to {
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

enum SuggestionTarget {
    Tab(u64),
    Url(String),
}

struct Suggestion {
    kind: &'static str,
    label: String,
    target: SuggestionTarget,
}

fn collect_suggestions(state: &BrowserState, query: &str) -> Vec<Suggestion> {
    let q = query.to_ascii_lowercase();
    let mut out = Vec::new();
    let mut seen_urls = std::collections::HashSet::new();

    for tab in &state.tabs {
        if out.len() >= 7 {
            break;
        }
        if tab.title.to_ascii_lowercase().contains(&q)
            || tab.url.to_ascii_lowercase().contains(&q)
        {
            seen_urls.insert(tab.url.clone());
            out.push(Suggestion {
                kind: "Tab",
                label: format!("{} — {}", tab.title, tab.url),
                target: SuggestionTarget::Tab(tab.id),
            });
        }
    }

    for bm in &state.bookmarks {
        if out.len() >= 7 {
            break;
        }
        if !seen_urls.contains(&bm.url)
            && (bm.title.to_ascii_lowercase().contains(&q)
                || bm.url.to_ascii_lowercase().contains(&q))
        {
            seen_urls.insert(bm.url.clone());
            out.push(Suggestion {
                kind: "★",
                label: format!("{} — {}", bm.title, bm.url),
                target: SuggestionTarget::Url(bm.url.clone()),
            });
        }
    }

    if let Some(tab) = state.active_tab() {
        for url in tab.history.iter().rev() {
            if out.len() >= 7 {
                break;
            }
            if !seen_urls.contains(url) && url.to_ascii_lowercase().contains(&q) {
                seen_urls.insert(url.clone());
                out.push(Suggestion {
                    kind: "History",
                    label: url.clone(),
                    target: SuggestionTarget::Url(url.clone()),
                });
            }
        }
    }

    out.push(Suggestion {
        kind: "Search",
        label: format!("Search Google for \"{query}\""),
        target: SuggestionTarget::Url(query.to_string()),
    });
    out
}
