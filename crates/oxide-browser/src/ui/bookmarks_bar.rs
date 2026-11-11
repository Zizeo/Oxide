//! Bookmarks bar: one row of site chips under the omnibox.
//!
//! # Why a strip of its own
//!
//! Chrome gives bookmarks their own row so the omnibox never competes with
//! them for width. This bar is drawn from
//! [`BrowserState::bookmarks`](crate::browser::BrowserState), which is also
//! what serialises to disk, so the strip is always in sync with the model:
//! no separate view list to migrate when the bookmark set changes.
//!
//! Two details are worth knowing before editing this file:
//!
//! * The row can not mutate the model while it draws — the chip loop holds
//!   `&state.bookmarks` — so clicks are recorded as a pending URL or a
//!   [`TabAction`] and applied once the row is finished.
//! * The page is a native `wry::WebView` that paints *over* egui, so menus
//!   opened here are not drawn in this window at all: the chip records a
//!   [`TabAction::OpenMenu`] and a borderless popup window renders the menu
//!   above the page (see [`crate::menu_window`]).

use crate::browser::{site_key, BrowserState};
use crate::ui::favicon_texture;
use crate::ui::menu::{MenuKind, MenuPicks};
use crate::ui::tab_actions::{apply_all, TabAction};
use egui::{Color32, CornerRadius, Pos2, Rect, RichText, Sense, Ui, Vec2};

/// Bar background: darker than the nav bar above it `(45, 47, 54)` and
/// lighter than the page below, so the strip reads as chrome.
const BAR_FILL: Color32 = Color32::from_rgb(26, 27, 31);
const CHIP_FILL: Color32 = Color32::from_rgb(32, 34, 38);
const CHIP_HOVER_FILL: Color32 = Color32::from_rgb(45, 47, 54);
const CHIP_TEXT: Color32 = Color32::from_rgb(180, 185, 195);
const MUTED_TEXT: Color32 = Color32::from_rgb(120, 125, 135);
const ON_COLOR: Color32 = Color32::from_rgb(74, 222, 128);
const OFF_COLOR: Color32 = Color32::from_rgb(251, 191, 36);

/// Chip height. A 20 pt row keeps the bar reading as chrome instead of as
/// content, which matters because it never goes away.
const CHIP_HEIGHT: f32 = 20.0;

/// Room reserved for the chip row, including the scroll bar that shows up
/// once the bookmarks no longer fit. 24 pt leaves 4 pt for that bar so a
/// long list does not clip the chip bottoms.
const ROW_HEIGHT: f32 = 24.0;

/// Size of a favicon slot. Square, so a chip with an icon and one without
/// still share a text baseline.
const ICON_BOX: f32 = 12.0;

/// Longest bookmark title a chip shows before it is elided. Wide enough
/// for a page title, narrow enough that a handful of chips still fit.
const MAX_LABEL_CHARS: usize = 28;

/// Longest entry title inside a folder dropdown; menus are not width-limited
/// the way the bar is, but one pathological title should not fill the screen.
const MAX_MENU_CHARS: usize = 48;

/// The bar a fresh profile starts with.
///
/// An empty strip makes the feature look broken, so these are exactly the
/// shortcuts the nav bar used to hardcode: nothing disappears for a user who
/// never saved a bookmark.
const STARTERS: [(&str, &str); 7] = [
    ("⚡ Welcome", "oxide://welcome"),
    ("🔍 Google", "https://www.google.com"),
    ("▶ YouTube", "https://www.youtube.com"),
    ("⚙ Rust Lang", "https://www.rust-lang.org"),
    ("📰 Hacker News", "https://news.ycombinator.com"),
    ("🐙 GitHub", "https://github.com"),
    ("📚 Wikipedia", "https://en.wikipedia.org"),
];

/// Draws the bookmarks strip: root bookmarks, one chip per folder, and the
/// two security shields pinned to the right edge.
pub fn render(ui: &mut Ui, state: &mut BrowserState) {
    // Mutations are recorded, never applied, while `&state.bookmarks` is
    // borrowed by the chip loop: `navigate_active_tab` and `apply_all` both
    // need the model mutably and would otherwise collide with that borrow.
    let mut navigate_to: Option<String> = None;
    let mut actions: Vec<TabAction> = Vec::new();

    egui::Frame::NONE
        .fill(BAR_FILL)
        .inner_margin(egui::Margin::symmetric(8, 3))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(6.0, 0.0);

            // Right-to-left: the shields are laid out first, so they stay
            // pinned to the right edge no matter how much the bookmark list
            // grows, and the scrolling row takes whatever is left.
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                security_toggles(ui, state);

                egui::ScrollArea::horizontal()
                    .max_height(ROW_HEIGHT)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(4.0, 0.0);
                        ui.horizontal(|ui| {
                            if state.bookmarks.is_empty() {
                                starter_chips(ui, &mut navigate_to);
                            } else {
                                bookmark_chips(ui, state, &mut navigate_to, &mut actions);
                            }
                        });
                    });
            });
        });

    if let Some(url) = navigate_to {
        state.navigate_active_tab(url);
    }
    apply_all(state, actions);
}

/// The starter shortcuts, shown only while no bookmark exists.
fn starter_chips(ui: &mut Ui, navigate_to: &mut Option<String>) {
    ui.label(RichText::new("Bookmarks:").size(11.0).color(MUTED_TEXT));
    for (label, url) in STARTERS {
        let response = chip(ui, None, label).on_hover_text(url);
        if response.clicked() {
            *navigate_to = Some(url.to_owned());
        }
    }
}

/// Draws one chip per root bookmark and one per folder, in bar order.
fn bookmark_chips(
    ui: &mut Ui,
    state: &BrowserState,
    navigate_to: &mut Option<String>,
    actions: &mut Vec<TabAction>,
) {
    ui.label(RichText::new("Bookmarks:").size(11.0).color(MUTED_TEXT));

    // Folder counts are gathered in first-seen order: "Bookmark all tabs"
    // appends a whole folder at once, and one chip per tab would push the
    // rest of the bar out of view. One chip per folder keeps the bar stable
    // however many tabs went into it.
    let mut folders: Vec<(&str, usize)> = Vec::new();
    for bookmark in &state.bookmarks {
        let Some(folder) = bookmark.folder.as_deref() else {
            continue;
        };
        match folders.iter_mut().find(|(name, _)| *name == folder) {
            Some((_, count)) => *count += 1,
            None => folders.push((folder, 1)),
        }
    }

    for bookmark in &state.bookmarks {
        // Foldered entries are reachable through their folder chip, and an
        // entry that cannot be opened has no business being a chip.
        if bookmark.folder.is_some() || !navigable(&bookmark.url) {
            continue;
        }

        let label = truncate(&bookmark.title, MAX_LABEL_CHARS);
        let response = chip(ui, site_icon(ui, state, &bookmark.url), &label)
            .on_hover_text(bookmark.url.as_str());
        if response.clicked() {
            *navigate_to = Some(bookmark.url.clone());
        }
        // Right-click is the only editing verb the bar has, so a context
        // menu with a single item is the whole story rather than a stub of
        // a bookmark manager. The menu is drawn by the popup window, not
        // here: the page is a native child window painted above egui.
        if response.secondary_clicked() {
            actions.push(TabAction::OpenMenu {
                kind: MenuKind::Bookmark {
                    url: bookmark.url.clone(),
                },
                pos: response
                    .interact_pointer_pos()
                    .unwrap_or_else(|| response.rect.left_bottom()),
            });
        }
    }

    for (folder, count) in folders {
        folder_chip(ui, folder, count, actions);
    }
}

/// One chip per folder, opening a dropdown of its entries.
fn folder_chip(
    ui: &mut Ui,
    folder: &str,
    count: usize,
    actions: &mut Vec<TabAction>,
) {
    let label = format!("📁 {folder} ({count})");
    let response = chip(ui, None, &label);
    if response.clicked() {
        actions.push(TabAction::OpenMenu {
            kind: MenuKind::BookmarkFolder {
                folder: folder.to_owned(),
            },
            pos: response.rect.left_bottom(),
        });
    }
}

/// Contents of a bookmark chip's context menu.
pub fn build_bookmark_menu(ui: &mut Ui, state: &BrowserState, url: &str) -> MenuPicks {
    let mut picks = MenuPicks::default();
    let Some(bookmark) = state.bookmarks.iter().find(|b| b.url == url) else {
        return picks;
    };
    ui.label(RichText::new(truncate(&bookmark.title, MAX_MENU_CHARS)).strong());
    ui.label(RichText::new(url).size(11.0).color(MUTED_TEXT));
    ui.separator();
    if ui.button("Remove bookmark").clicked() {
        picks.push(TabAction::RemoveBookmark(url.to_owned()));
    }
    picks
}

/// Contents of a folder chip's dropdown.
pub fn build_folder_menu(ui: &mut Ui, state: &BrowserState, folder: &str) -> MenuPicks {
    let mut picks = MenuPicks::default();
    ui.label(RichText::new(format!("📁 {folder}")).strong());
    ui.separator();
    for bookmark in state
        .bookmarks
        .iter()
        .filter(|bookmark| bookmark.folder.as_deref() == Some(folder))
    {
        if !navigable(&bookmark.url) {
            continue;
        }
        let title = truncate(&bookmark.title, MAX_MENU_CHARS);
        if ui.button(RichText::new(title).size(11.5)).clicked() {
            picks.push(TabAction::OpenUrl(bookmark.url.clone()));
        }
    }
    ui.separator();
    // Dropping the whole folder is the only way to undo a "Bookmark all
    // tabs" burst without clicking through every entry.
    if ui
        .button(RichText::new("Remove folder").size(11.5))
        .clicked()
    {
        picks.push(TabAction::RemoveBookmarkFolder(folder.to_owned()));
    }
    picks
}

/// The two security shields, pinned to the right end of the bar.
///
/// They live here rather than in the nav bar because this row always has
/// spare room on the right; the omnibox needs every pixel it can get.
fn security_toggles(ui: &mut Ui, state: &mut BrowserState) {
    let (https_only, block_private) = {
        let policy = state.security_policy.read().unwrap();
        (policy.https_only, policy.block_private_networks)
    };

    let lan_btn = ui.add(
        egui::Button::new(
            RichText::new(if block_private {
                "🛡 SSRF/LAN Guard: ON"
            } else {
                "⚠ SSRF/LAN Guard: OFF"
            })
            .size(10.5)
            .color(if block_private { ON_COLOR } else { OFF_COLOR }),
        )
        .fill(Color32::from_rgb(28, 32, 38))
        .corner_radius(CornerRadius::same(4)),
    );
    if lan_btn.clicked() {
        set_block_private(state, !block_private);
    }

    let https_btn = ui.add(
        egui::Button::new(
            RichText::new(if https_only {
                "🔒 HTTPS-Only: ON"
            } else {
                "🔓 HTTPS-Only: OFF"
            })
            .size(10.5)
            .color(if https_only { ON_COLOR } else { OFF_COLOR }),
        )
        .fill(Color32::from_rgb(28, 32, 38))
        .corner_radius(CornerRadius::same(4)),
    );
    if https_btn.clicked() {
        set_https_only(state, !https_only);
    }
}

/// Flips the HTTPS-Only shield.
///
/// The policy lives behind a `RwLock`, so the setter only needs a shared
/// borrow: the bar owns `&mut BrowserState` but does not have to hand it out
/// for an operation that is a single atomic flag.
fn set_https_only(state: &BrowserState, on: bool) {
    if let Ok(mut policy) = state.security_policy.write() {
        policy.https_only = on;
    }
}

/// Flips the SSRF/LAN shield.
fn set_block_private(state: &BrowserState, on: bool) {
    if let Ok(mut policy) = state.security_policy.write() {
        policy.block_private_networks = on;
    }
}

/// Paints one chip and returns its click response.
///
/// A plain [`egui::Button`] cannot put a favicon to the left of its label, so
/// the chip is measured and painted by hand: the label galley decides the
/// width, which keeps every chip hugging its text the way Chrome's do.
fn chip(ui: &mut Ui, icon: Option<egui::TextureId>, label: &str) -> egui::Response {
    let galley = ui.painter().layout_no_wrap(
        label.to_owned(),
        egui::FontId::proportional(11.0),
        CHIP_TEXT,
    );
    let icon_width = if icon.is_some() { ICON_BOX + 4.0 } else { 0.0 };
    let size = Vec2::new(galley.size().x + icon_width + 12.0, CHIP_HEIGHT);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());

    let fill = if response.hovered() {
        CHIP_HOVER_FILL
    } else {
        CHIP_FILL
    };
    ui.painter().rect_filled(rect, CornerRadius::same(4), fill);

    let mut text_left = rect.left() + 6.0;
    if let Some(texture) = icon {
        let icon_rect = Rect::from_min_size(
            Pos2::new(text_left, rect.center().y - ICON_BOX * 0.5),
            Vec2::splat(ICON_BOX),
        );
        ui.painter().image(
            texture,
            icon_rect,
            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
            Color32::WHITE,
        );
        text_left += ICON_BOX + 4.0;
    }
    ui.painter().galley(
        Pos2::new(text_left, rect.center().y - galley.size().y * 0.5),
        galley,
        CHIP_TEXT,
    );

    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// The site favicon as a texture, once one has been fetched.
///
/// The texture is keyed by site, so every chip on one host shares a single
/// GPU upload with the tab strip.
fn site_icon(ui: &Ui, state: &BrowserState, url: &str) -> Option<egui::TextureId> {
    let image = state.favicon_for(url)?;
    let site = site_key(url)?;
    Some(favicon_texture(ui.ctx(), &site, image).id())
}

/// Only real pages may be opened from the bar.
///
/// Bookmarks can be created from any URL string, and `navigate_active_tab`
/// treats anything that is not a URL as a web search — which is not what a
/// chip showing a site's own title promises, so those entries are skipped.
fn navigable(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://") || url.starts_with("oxide://")
}

/// Elides a title to `max_chars` characters.
///
/// A single very long title must not push the rest of the bar into a scroll,
/// so it loses its tail rather than its head: the start of a page title is
/// what identifies it.
fn truncate(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_owned();
    }
    let mut truncated: String = text.chars().take(max_chars.saturating_sub(1)).collect();
    truncated.push('…');
    truncated
}
