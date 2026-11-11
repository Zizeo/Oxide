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

use crate::browser::BrowserState;
use crate::ui::tab_actions::TabAction;

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
    }
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
