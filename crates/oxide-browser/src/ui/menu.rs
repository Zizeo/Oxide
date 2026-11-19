//! Which context menu to show, and what a menu pass produced.
//!
//! # Why menus live in their own window
//!
//! The page is a native child `wry::WebView`, and the OS always paints child
//! windows above their parent's client area. An egui menu drawn into the main
//! window is therefore *behind* the page: it is only visible while the page is
//! hidden, which blanks the viewport. Neither is acceptable, so menus are
//! rendered into a borderless popup window of their own (see
//! [`crate::menu_window`]), which the OS stacks above the page. The page keeps
//! rendering live underneath.
//!
//! That split is why a menu is described by a small [`MenuKind`] value instead
//! of a closure: the strip records the request, and the popup window rebuilds
//! the contents from the model on every frame.

use crate::browser::{site_key, BrowserState, DevToolsTab};
use crate::ui::tab_actions::TabAction;
use egui::{Button, Color32, RichText};

/// The menu a right-click asked for.
///
/// Deliberately data, not a closure: the request outlives the frame that
/// produced it and is replayed by a different window.
#[derive(Debug, Clone, PartialEq)]
pub enum MenuKind {
    /// A tab's menu, or the strip-wide menu when `tab_id` is `None`.
    Tab { tab_id: Option<u64> },
    /// A group chip's menu.
    Group { group_id: u64 },
    /// A bookmarks-bar chip's menu.
    Bookmark { url: String },
    /// A bookmarks-bar folder's dropdown.
    BookmarkFolder { folder: String },
    /// Three-dot main application menu.
    App,
    /// Site identity / security pill popup.
    SiteInfo,
    /// Viewport right-click context menu.
    Viewport {
        link_url: Option<String>,
        img_url: Option<String>,
        selection: Option<String>,
        is_editable: bool,
    },
}

impl MenuKind {
    /// Whether the thing this menu describes still exists.
    ///
    /// A menu can outlive its target — the tab may be closed from a shortcut
    /// while the popup is up — and a menu for a vanished tab would silently
    /// turn into the strip menu, which is worse than closing.
    pub fn is_valid(&self, state: &BrowserState) -> bool {
        match self {
            MenuKind::Tab { tab_id } => tab_id.is_none_or(|id| state.tab(id).is_some()),
            MenuKind::Group { group_id } => state.group(*group_id).is_some(),
            MenuKind::Bookmark { url } => state.bookmarks.iter().any(|b| &b.url == url),
            MenuKind::BookmarkFolder { folder } => state
                .bookmarks
                .iter()
                .any(|b| b.folder.as_deref() == Some(folder.as_str())),
            MenuKind::App | MenuKind::SiteInfo | MenuKind::Viewport { .. } => true,
        }
    }
}

/// What one menu pass produced.
///
/// `activated` is separate from `actions` because not every entry mutates the
/// model: "Copy URL" only touches the clipboard, yet it must still close the
/// menu the way any other pick does.
#[derive(Debug, Default)]
pub struct MenuPicks {
    pub actions: Vec<TabAction>,
    pub activated: bool,
}

impl MenuPicks {
    /// Records a picked entry: it will be applied, and the menu will close.
    pub fn push(&mut self, action: TabAction) {
        self.actions.push(action);
        self.activated = true;
    }
}

/// Builds the contents of `kind` into `ui`.
pub fn build(ui: &mut egui::Ui, state: &BrowserState, kind: &MenuKind) -> MenuPicks {
    match kind {
        MenuKind::Tab { tab_id } => crate::ui::tab_menu::build_tab_menu(ui, state, *tab_id),
        MenuKind::Group { group_id } => crate::ui::tab_menu::build_group_menu(ui, state, *group_id),
        MenuKind::Bookmark { url } => crate::ui::bookmarks_bar::build_bookmark_menu(ui, state, url),
        MenuKind::BookmarkFolder { folder } => {
            crate::ui::bookmarks_bar::build_folder_menu(ui, state, folder)
        }
        MenuKind::App => build_app_menu(ui, state),
        MenuKind::SiteInfo => build_site_info_menu(ui, state),
        MenuKind::Viewport {
            link_url,
            img_url,
            selection,
            is_editable,
        } => build_viewport_menu(
            ui,
            state,
            link_url.as_deref(),
            img_url.as_deref(),
            selection.as_deref(),
            *is_editable,
        ),
    }
}

fn build_app_menu(ui: &mut egui::Ui, state: &BrowserState) -> MenuPicks {
    let mut picked = MenuPicks::default();

    if ui.button("New tab\tCtrl+T").clicked() {
        picked.push(TabAction::NewTab);
    }
    if ui.button("New window\tCtrl+N").clicked() {
        picked.push(TabAction::NewWindow);
    }
    if ui.button("New Incognito window\tCtrl+Shift+N").clicked() {
        picked.push(TabAction::NewIncognitoWindow);
    }
    ui.separator();

    // Inline Zoom + Fullscreen row
    let zoom_pct = state.active_tab().map_or(100, |t| t.zoom_percent());
    ui.horizontal(|ui| {
        ui.label(RichText::new("Zoom").color(Color32::from_rgb(180, 185, 195)));
        if ui.button(" − ").clicked() {
            picked.push(TabAction::ZoomOut);
        }
        if ui.button(format!("{zoom_pct}%")).clicked() {
            picked.push(TabAction::ZoomReset);
        }
        if ui.button(" + ").clicked() {
            picked.push(TabAction::ZoomIn);
        }
        if ui.button(" ⛶ ").on_hover_text("Fullscreen (F11)").clicked() {
            picked.push(TabAction::ToggleFullscreen);
        }
    });
    ui.separator();

    if ui.button("Find in page…\tCtrl+F").clicked() {
        picked.push(TabAction::OpenFindBar);
    }
    if ui.button("Print…\tCtrl+P").clicked() {
        picked.push(TabAction::Print);
    }
    ui.separator();

    ui.menu_button("Bookmarks", |ui| {
        if ui.button("Show bookmarks bar\tCtrl+Shift+B").clicked() {
            picked.push(TabAction::ToggleBookmarksBar);
        }
        if ui.button("Bookmark manager\tCtrl+Shift+O").clicked() {
            picked.push(TabAction::OpenInNewTab("oxide://bookmarks".to_string()));
        }
        if let Some(tab) = state.active_tab()
            && ui.button("Add tab to Reading List").clicked()
        {
            picked.push(TabAction::AddToReadingList(tab.id));
        }
        ui.separator();
        if state.bookmarks.is_empty() {
            ui.add_enabled(false, Button::new("No bookmarks"));
        } else {
            egui::ScrollArea::vertical()
                .id_salt("app_menu_bookmarks_scroll")
                .max_height(320.0)
                .show(ui, |ui| {
                    ui.set_max_width(320.0);
                    for bm in &state.bookmarks {
                        let raw = if bm.title.trim().is_empty() {
                            bm.url.as_str()
                        } else {
                            bm.title.as_str()
                        };
                        if ui.button(short_menu_text(raw, 48)).clicked() {
                            picked.push(TabAction::OpenUrl(bm.url.clone()));
                        }
                    }
                });
        }
    });
    ui.menu_button("History", |ui| {
        if ui.button("History\tCtrl+H").clicked() {
            picked.push(TabAction::OpenInNewTab("oxide://history".to_string()));
        }
        ui.separator();
        if let Some(tab) = state.active_tab() {
            ui.set_max_width(340.0);
            for url in tab.history.iter().rev().take(12) {
                if ui.button(short_menu_text(url, 52)).clicked() {
                    picked.push(TabAction::OpenUrl(url.clone()));
                }
            }
        } else {
            ui.add_enabled(false, Button::new("No history"));
        }
    });
    if ui.button("Downloads\tCtrl+J").clicked() {
        picked.push(TabAction::OpenInNewTab("oxide://downloads".to_string()));
    }
    if ui.button("Clear browsing data…\tCtrl+Shift+Del").clicked() {
        picked.push(TabAction::OpenClearDataModal);
    }
    ui.separator();

    if ui.button("Developer tools\tF12").clicked() {
        picked.push(TabAction::ToggleDevtools);
    }
    if ui.button("Task manager\tShift+Esc").clicked() {
        picked.push(TabAction::ToggleTaskManager);
    }
    if ui.button("Settings").clicked() {
        picked.push(TabAction::OpenInNewTab("oxide://settings".to_string()));
    }
    ui.separator();

    if ui.button("Exit").clicked() {
        picked.push(TabAction::ExitApp);
    }

    picked
}

fn build_site_info_menu(ui: &mut egui::Ui, state: &BrowserState) -> MenuPicks {
    let mut picked = MenuPicks::default();
    let Some(tab) = state.active_tab() else {
        return picked;
    };

    let (title, color) = if tab.error.is_some() {
        ("Blocked by Security Guard", Color32::from_rgb(248, 113, 113))
    } else if tab.url.starts_with("https://") {
        ("Connection is secure (HTTPS)", Color32::from_rgb(74, 222, 128))
    } else if tab.url.starts_with("oxide://") {
        ("Oxide internal page", Color32::from_rgb(96, 165, 250))
    } else {
        ("Connection is not secure (HTTP)", Color32::from_rgb(251, 191, 36))
    };

    ui.set_max_width(340.0);
    ui.label(RichText::new(title).color(color).strong());
    ui.add(
        egui::Label::new(
            RichText::new(short_menu_text(&tab.url, 56))
                .size(11.0)
                .color(Color32::GRAY),
        )
        .truncate(),
    );
    if let Some(resp) = &tab.response {
        let sec = resp.security_metadata();
        ui.label(RichText::new(sec.summary).size(11.0).color(Color32::LIGHT_GRAY));
    }
    ui.separator();

    if let Some(site) = site_key(&tab.url) {
        let muted = state.muted_sites.contains(&site);
        let label = if muted {
            format!("Unmute {site}")
        } else {
            format!("Mute {site}")
        };
        if ui.button(label).clicked() {
            picked.push(TabAction::ToggleMuteSite(tab.id));
        }
    }

    if ui.button("Copy URL").clicked() {
        ui.ctx().copy_text(tab.url.clone());
        picked.activated = true;
    }

    picked
}

fn short_menu_text(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max_chars.saturating_sub(1)).collect();
    out.push('…');
    out
}

fn build_viewport_menu(
    ui: &mut egui::Ui,
    state: &BrowserState,
    link_url: Option<&str>,
    img_url: Option<&str>,
    selection: Option<&str>,
    is_editable: bool,
) -> MenuPicks {
    let mut picked = MenuPicks::default();

    if let Some(url) = link_url {
        if ui.button("Open link in new tab").clicked() {
            picked.push(TabAction::OpenInNewTab(url.to_string()));
        }
        if ui.button("Open link in new window").clicked() {
            picked.push(TabAction::OpenInNewWindow(url.to_string()));
        }
        if ui.button("Copy link address").clicked() {
            ui.ctx().copy_text(url.to_string());
            picked.activated = true;
        }
        ui.separator();
    }

    if let Some(url) = img_url {
        if ui.button("Open image in new tab").clicked() {
            picked.push(TabAction::OpenInNewTab(url.to_string()));
        }
        if ui.button("Copy image address").clicked() {
            ui.ctx().copy_text(url.to_string());
            picked.activated = true;
        }
        ui.separator();
    }

    if let Some(sel) = selection.filter(|s| !s.is_empty()) {
        if ui.button("Copy").clicked() {
            ui.ctx().copy_text(sel.to_string());
            picked.activated = true;
        }
        let preview: String = sel.chars().take(24).collect();
        let label = if sel.chars().count() > 24 {
            format!("Search Google for \"{preview}…\"")
        } else {
            format!("Search Google for \"{preview}\"")
        };
        if ui.button(label).clicked() {
            let search_url = oxide_net::security::resolve_omnibox_input(sel);
            picked.push(TabAction::OpenInNewTab(search_url));
        }
        ui.separator();
    } else if is_editable {
        ui.label(RichText::new("Text input").size(11.0).color(Color32::GRAY));
        ui.separator();
    }

    let Some(tab) = state.active_tab() else {
        return picked;
    };
    if ui.add_enabled(tab.can_go_back(), Button::new("Back")).clicked() {
        picked.push(TabAction::GoBack);
    }
    if ui.add_enabled(tab.can_go_forward(), Button::new("Forward")).clicked() {
        picked.push(TabAction::GoForward);
    }
    if ui.button("Reload\tCtrl+R").clicked() {
        picked.push(TabAction::Reload(tab.id));
    }
    ui.separator();

    if ui.button("Print…\tCtrl+P").clicked() {
        picked.push(TabAction::Print);
    }
    if ui.button("View page source").clicked() {
        picked.push(TabAction::OpenDevtools(DevToolsTab::Source));
    }
    if ui.button("Inspect").clicked() {
        picked.push(TabAction::OpenDevtools(DevToolsTab::Elements));
    }

    picked
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> BrowserState {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let handle = Box::leak(Box::new(rt)).handle().clone();
        BrowserState::new(handle)
    }

    #[test]
    fn a_menu_for_a_closed_tab_is_invalid() {
        let mut s = state();
        // The last tab is recycled into the welcome page instead of being
        // removed, so a second tab is needed for a real close.
        s.create_tab("https://example.com/");
        let id = s.tabs[0].id;
        let kind = MenuKind::Tab { tab_id: Some(id) };
        assert!(kind.is_valid(&s));
        s.close_tab(id);
        assert!(!kind.is_valid(&s), "a closed tab must not keep its menu");
    }

    #[test]
    fn the_strip_menu_is_always_valid() {
        let s = state();
        assert!(MenuKind::Tab { tab_id: None }.is_valid(&s));
    }

    #[test]
    fn a_bookmark_menu_dies_with_its_bookmark() {
        let mut s = state();
        s.toggle_bookmark("https://example.com/", "Example");
        let kind = MenuKind::Bookmark {
            url: "https://example.com/".to_string(),
        };
        assert!(kind.is_valid(&s));
        s.remove_bookmark("https://example.com/");
        assert!(!kind.is_valid(&s));
    }
}
