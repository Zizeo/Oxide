//! Chrome-style tab search / switcher overlay (`Ctrl+Shift+A`).
//!
//! # Why an overlay and not a popup list
//!
//! The page is a native child `WebView`, painted above the egui surface. Any
//! egui widget that leaves the chrome would be buried under it, so the overlay
//! hides the page while it is open — see `BrowserState::tab_search_open` and
//! `ensure_and_sync_webviews`. A tab search that covers the page is also what
//! Chrome does, so the trade-off costs nothing here.
//!
//! # State
//!
//! The query, the highlight index and the open flag all live in
//! [`BrowserState`] rather than in egui temp storage: the shortcut handler has
//! to open the overlay from outside the UI pass, and the model is the natural
//! place both sides can reach.

use crate::browser::BrowserState;
use crate::ui::tab_actions::{apply_all, TabAction};
use egui::{Color32, Context, CornerRadius, Id, Key, Margin, RichText, Stroke, Vec2};

/// Muted text colour used for secondary information.
const DIM: Color32 = Color32::from_rgb(140, 145, 155);

/// Height of the chrome above the page: the overlay opens just below it.
const OVERLAY_TOP: f32 = 104.0;

/// Shortens a title so a single row cannot stretch the overlay.
fn short_title(title: &str) -> String {
    const MAX: usize = 48;
    if title.chars().count() <= MAX {
        return title.to_string();
    }
    let mut out: String = title.chars().take(MAX).collect();
    out.push('…');
    out
}

/// Temp-storage key remembering that the search box was focused for this
/// opening, so focus is requested once and not fought every frame.
fn focus_id() -> Id {
    Id::new("oxide_tab_search_focus")
}

/// Draws the overlay when it is open, and applies whatever the user picked.
pub fn render(ctx: &Context, state: &mut BrowserState) {
    if !state.tab_search_open {
        ctx.data_mut(|data| data.remove::<bool>(focus_id()));
        return;
    }

    let mut picked: Option<u64> = None;
    let mut close = false;

    let screen = ctx.screen_rect();
    let width = 460.0_f32.min((screen.width() - 32.0).max(240.0));
    let pos = egui::pos2(
        (screen.center().x - width * 0.5).max(screen.left() + 8.0),
        screen.top() + OVERLAY_TOP,
    );

    egui::Area::new(Id::new("oxide_tab_search"))
        .order(egui::Order::Foreground)
        .fixed_pos(pos)
        .movable(false)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style())
                .fill(Color32::from_rgb(28, 30, 35))
                .stroke(Stroke::new(1.0_f32, Color32::from_rgb(70, 74, 84)))
                .corner_radius(CornerRadius::same(10))
                .inner_margin(Margin::same(10))
                .show(ui, |ui| {
                    ui.set_width(width - 20.0);
                    ui.spacing_mut().item_spacing = Vec2::new(6.0, 4.0);

                    // Read the navigation keys *before* the text field is
                    // built: a single-line `TextEdit` consumes Enter and may
                    // consume Escape by surrendering focus, which would leave
                    // the overlay deaf to them.
                    let pressed_down = ui.input(|i| i.key_pressed(Key::ArrowDown));
                    let pressed_up = ui.input(|i| i.key_pressed(Key::ArrowUp));
                    let pressed_enter = ui.input(|i| i.key_pressed(Key::Enter));
                    let pressed_escape = ui.input(|i| i.key_pressed(Key::Escape));

                    ui.label(
                        RichText::new("Search tabs")
                            .strong()
                            .size(11.5)
                            .color(DIM),
                    );

                    let search = ui.add(
                        egui::TextEdit::singleline(&mut state.tab_search_query)
                            .hint_text("Search tabs by title or URL…")
                            .desired_width(f32::INFINITY)
                            .margin(Margin::symmetric(8, 6)),
                    );
                    // Focus exactly once per opening.
                    if ctx.data(|data| data.get_temp::<bool>(focus_id())).is_none() {
                        search.request_focus();
                        ctx.data_mut(|data| data.insert_temp(focus_id(), true));
                    }
                    if search.changed() {
                        state.tab_search_selected = 0;
                    }

                    let matches = state.tab_search_matches();
                    let mut activate_highlighted = false;

                    if !matches.is_empty() {
                        if pressed_down {
                            state.tab_search_selected =
                                (state.tab_search_selected + 1) % matches.len();
                        }
                        if pressed_up {
                            state.tab_search_selected =
                                (state.tab_search_selected + matches.len() - 1) % matches.len();
                        }
                        if pressed_enter {
                            activate_highlighted = true;
                        }
                    }
                    if pressed_escape {
                        close = true;
                    }

                    // A shorter list (query narrowed) must never keep a
                    // highlight past its end.
                    if state.tab_search_selected >= matches.len() {
                        state.tab_search_selected = matches.len().saturating_sub(1);
                    }

                    ui.separator();

                    if matches.is_empty() {
                        ui.label(
                            RichText::new("No matching tabs")
                                .size(12.0)
                                .color(DIM),
                        );
                    } else {
                        egui::ScrollArea::vertical()
                            .max_height(320.0)
                            .show(ui, |ui| {
                                for (index, id) in matches.iter().enumerate() {
                                    let Some(tab) = state.tab(*id) else {
                                        continue;
                                    };
                                    let mut label = String::new();
                                    if tab.pinned {
                                        label.push_str("📌 ");
                                    }
                                    if tab.loading {
                                        label.push_str("⟳ ");
                                    }
                                    if tab.discarded {
                                        label.push_str("◌ ");
                                    }
                                    if tab.muted {
                                        label.push_str("🔇 ");
                                    } else if tab.audio_playing {
                                        label.push_str("🔊 ");
                                    }
                                    label.push_str(&short_title(&tab.title));
                                    if let Some(group) =
                                        tab.group.and_then(|gid| state.group(gid))
                                    {
                                        label.push_str("  ·  ");
                                        label.push_str(&group.name);
                                    }

                                    let highlighted = index == state.tab_search_selected;
                                    let response = ui.selectable_label(
                                        highlighted,
                                        RichText::new(label).size(13.0),
                                    );
                                    let response =
                                        response.on_hover_text(format!("{}\n{}", tab.title, tab.url));
                                    if highlighted {
                                        response.scroll_to_me(None);
                                    }
                                    if response.clicked() {
                                        picked = Some(*id);
                                    }
                                }
                            });
                    }

                    if activate_highlighted
                        && let Some(id) = matches.get(state.tab_search_selected)
                    {
                        picked = Some(*id);
                    }

                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!(
                                "{} of {} tabs",
                                matches.len(),
                                state.tabs.len()
                            ))
                            .size(10.5)
                            .color(DIM),
                        );
                        ui.with_layout(
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                ui.label(
                                    RichText::new("↑↓ navigate · Enter switch · Esc close")
                                        .size(10.5)
                                        .color(DIM),
                                );
                            },
                        );
                    });
                });
        });

    if close {
        state.close_tab_search();
    }
    if let Some(id) = picked {
        // Activating a tab replaces the selection and the displayed page; the
        // overlay must be gone before the next WebView sync.
        state.close_tab_search();
        apply_all(state, [TabAction::Activate(id)]);
    }
}
