//! Every tab-strip mutation the UI can request, as one enum.
//!
//! # Why an action enum
//!
//! The strip has to iterate `state.tabs` immutably while it draws, so it can
//! not mutate the model in the same breath. Instead each widget records a
//! [`TabAction`] and the actions are applied once, after the layout pass, by
//! [`apply`]. This also keeps the *decision* (which action a click or a
//! shortcut means) separate from the *effect* (model mutation), which is what
//! makes bulk operations and pin/group invariants easy to reason about.
//!
//! All mutation rules live in [`BrowserState`](crate::browser::BrowserState);
//! this module only translates an intent into the smallest sequence of model
//! calls.

use crate::browser::{
    BrowserState, ConsoleLevel, DevToolsTab, GroupColor, GroupRename, PendingDetach,
};

/// A user intent on the tab strip.
#[derive(Debug, Clone, PartialEq)]
pub enum TabAction {
    // ---- activation and selection ----
    /// Plain click / programmatic switch: display this tab and make it the
    /// only selected tab.
    Activate(u64),
    /// Ctrl+click: add/remove from the multi-selection, active tab unchanged.
    ToggleSelection(u64),
    /// Shift+click: select the range from the anchor to this tab.
    ExtendSelection(u64),
    SelectAll,
    ClearSelection,
    /// Ctrl+Shift+A: show the tab-search overlay.
    OpenTabSearch,
    /// Right-click: ask the app to open a context menu in its own window.
    ///
    /// The menu cannot be drawn by the window that produced it — the page is
    /// a native child window painted above egui — so the request is recorded
    /// and replayed by [`crate::menu_window`].
    OpenMenu {
        kind: crate::ui::menu::MenuKind,
        pos: egui::Pos2,
    },
    /// Open a URL in the active tab (bookmarks-bar entries).
    OpenUrl(String),

    // ---- audio, bookmarks, saved groups ----
    /// Mute or unmute one tab, leaving the rest of its site alone.
    ToggleMuteTab(u64),
    /// Mute or unmute every tab showing the clicked tab's site.
    ToggleMuteSite(u64),
    /// Mute or unmute the whole strip.
    MuteAll(bool),
    /// Bookmark the tab, or drop its bookmark when it already has one.
    BookmarkTab(u64),
    RemoveBookmark(String),
    /// Removes every bookmark that belongs to one folder chip.
    RemoveBookmarkFolder(String),
    /// Ctrl+Shift+D: bookmark every open tab into one folder.
    BookmarkAll,
    /// Copies a live group into the saved-group list.
    SaveGroup(u64),
    ReopenSavedGroup(usize),
    DeleteSavedGroup(usize),
    /// Reopen the `n`-th most recently closed tab (`0` is the newest).
    ReopenClosedAt(usize),

    // ---- lifecycle ----
    NewTab,
    /// Open a welcome tab directly inside an existing group.
    NewTabInGroup(u64),
    Duplicate(u64),
    Close(u64),
    CloseSelected,
    CloseOthers(u64),
    CloseToRight(u64),
    CloseToLeft(u64),
    ReopenClosed,

    // ---- pinning ----
    TogglePin(u64),

    // ---- ordering ----
    /// Drag & drop: move `ids` (in strip order) before strip index `to_index`.
    MoveTabs { ids: Vec<u64>, to_index: usize },
    /// Drag & drop released outside the strip: hand `ids` to a new window.
    /// The app creates it after the UI pass and places it under `pos`.
    DetachTabs { ids: Vec<u64>, pos: egui::Pos2 },

    // ---- groups ----
    CreateGroup { ids: Vec<u64>, color: GroupColor },
    AddToGroup { ids: Vec<u64>, group: u64 },
    RemoveFromGroup { ids: Vec<u64> },
    SelectGroup(u64),
    ToggleGroupCollapsed(u64),
    SetGroupColor { group: u64, color: GroupColor },
    /// Opens the inline rename editor (does not change the name).
    BeginRenameGroup(u64),
    /// Applies whatever the inline editor currently holds.
    CommitGroupRename,
    /// Closes the inline editor without applying it.
    CancelGroupRename,
    Ungroup(u64),
    CloseGroup(u64),

    // ---- navigation ----
    Reload(u64),
    ToggleDevtools,
    ActivateRelative(isize),
    ActivateIndex(usize),
    /// Ctrl+9 in Chrome — the last tab in the strip.
    ActivateLast,
    /// Ctrl+L: focus and select the contents of the omnibox.
    FocusOmnibox,
}

/// Applies a single action. Never panics on stale ids: the tab may have been
/// closed between the frame that produced the action and this call.
pub fn apply(state: &mut BrowserState, action: TabAction) {
    match action {
        TabAction::Activate(id) => state.activate_tab(id),
        TabAction::ToggleSelection(id) => state.toggle_tab_selection(id),
        TabAction::ExtendSelection(id) => state.extend_selection_to(id),
        TabAction::SelectAll => state.select_all_tabs(),
        TabAction::ClearSelection => state.clear_selection(),
        TabAction::OpenTabSearch => state.open_tab_search(),
    TabAction::OpenMenu { kind, pos } => {
        state.pending_menu = Some(crate::browser::PendingMenu { kind, pos });
    }
    TabAction::OpenUrl(url) => state.navigate_active_tab(url),

        TabAction::NewTab => state.create_tab("oxide://welcome"),
        TabAction::NewTabInGroup(group) => {
            state.create_tab("oxide://welcome");
            if let Some(id) = state.active_tab_id {
                state.add_to_group(&[id], group);
            }
        }
        TabAction::Duplicate(id) => state.duplicate_tab(id),
        TabAction::Close(id) => state.close_tab(id),
        TabAction::CloseSelected => state.close_selected_tabs(),
        TabAction::CloseOthers(id) => state.close_other_tabs(id),
        TabAction::CloseToRight(id) => state.close_tabs_to_right(id),
        TabAction::CloseToLeft(id) => state.close_tabs_to_left(id),
        TabAction::ReopenClosed => state.reopen_closed_tab(),

        TabAction::ToggleMuteTab(id) => state.toggle_mute_tab(id),
        TabAction::ToggleMuteSite(id) => state.toggle_mute_site(id),
        TabAction::MuteAll(muted) => state.set_all_muted(muted),
        TabAction::BookmarkTab(id) => {
            if let Some(tab) = state.tab(id) {
                let (url, title) = (tab.url.clone(), tab.title.clone());
                state.toggle_bookmark(&url, &title);
            }
        }
        TabAction::RemoveBookmark(url) => state.remove_bookmark(&url),
        TabAction::RemoveBookmarkFolder(folder) => state.remove_bookmark_folder(&folder),
        TabAction::BookmarkAll => state.bookmark_all_tabs(),
        TabAction::SaveGroup(group) => {
            state.save_group(group);
        }
        TabAction::ReopenSavedGroup(index) => state.reopen_saved_group(index),
        TabAction::DeleteSavedGroup(index) => state.delete_saved_group(index),
        TabAction::ReopenClosedAt(index) => state.reopen_closed_at(index),

        TabAction::TogglePin(id) => state.toggle_pin(id),

        TabAction::MoveTabs { ids, to_index } => state.move_tabs(&ids, to_index),
        TabAction::DetachTabs { ids, pos } => {
            state.pending_detach = Some(PendingDetach { ids, pos });
        }

        TabAction::CreateGroup { ids, color } => {
            state.create_group(&ids, color);
        }
        TabAction::AddToGroup { ids, group } => state.add_to_group(&ids, group),
        TabAction::RemoveFromGroup { ids } => state.remove_from_group(&ids),
        TabAction::SelectGroup(group) => {
            let ids = state.group_tabs(group);
            state.selected_tab_ids = ids.iter().copied().collect();
            if let Some(first) = ids.first() {
                state.activate_tab(*first);
            }
        }
        TabAction::ToggleGroupCollapsed(group) => {
            state.toggle_group_collapsed(group);
        }
        TabAction::SetGroupColor { group, color } => {
            state.set_group_color(group, color);
        }
        TabAction::BeginRenameGroup(group) => {
            if let Some(g) = state.group(group) {
                state.group_rename = Some(GroupRename {
                    group_id: group,
                    name: g.name.clone(),
                });
            }
        }
        TabAction::CommitGroupRename => {
            if let Some(rename) = state.group_rename.take() {
                state.rename_group(rename.group_id, rename.name);
            }
        }
        TabAction::CancelGroupRename => state.group_rename = None,
        TabAction::Ungroup(group) => state.ungroup(group),
        TabAction::CloseGroup(group) => state.close_group(group),

        TabAction::Reload(id) => state.reload_tab(id),
        TabAction::ToggleDevtools => {
            state.show_devtools = !state.show_devtools;
            if state.show_devtools {
                state.devtools_tab = DevToolsTab::Elements;
                state.fetch_active_tab_for_inspector();
            }
            state.log(
                ConsoleLevel::Info,
                if state.show_devtools {
                    "DevTools opened"
                } else {
                    "DevTools closed"
                },
            );
        }
        TabAction::ActivateRelative(delta) => state.activate_relative(delta),
        TabAction::ActivateIndex(index) => state.activate_tab_at(index),
        TabAction::ActivateLast => {
            if let Some(id) = state.tabs.last().map(|t| t.id) {
                state.activate_tab(id);
            }
        }
        TabAction::FocusOmnibox => state.focus_omnibox_request = true,
    }
}

/// Applies a batch, preserving the order the widgets produced.
pub fn apply_all(state: &mut BrowserState, actions: impl IntoIterator<Item = TabAction>) {
    for action in actions {
        apply(state, action);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The model needs a tokio handle; tests only exercise pure state logic.
    fn state() -> BrowserState {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        // Leak: the handle must outlive the state for the duration of a test.
        let handle = Box::leak(Box::new(rt)).handle().clone();
        let mut state = BrowserState::new(handle);
        // `new` already opens the welcome tab, so this yields three tabs.
        state.create_tab("https://example.com/a");
        state.create_tab("https://example.com/b");
        state
    }

    fn ids(state: &BrowserState) -> Vec<u64> {
        state.tabs.iter().map(|t| t.id).collect()
    }

    fn urls(state: &BrowserState) -> Vec<String> {
        state.tabs.iter().map(|t| t.url.clone()).collect()
    }

    #[test]
    fn detaching_tabs_records_a_new_window_request() {
        let mut s = state();
        let order = ids(&s);
        apply(
            &mut s,
            TabAction::DetachTabs {
                ids: vec![order[1]],
                pos: egui::pos2(12.0, 34.0),
            },
        );
        let pending = s.pending_detach.clone().expect("a detach request");
        assert_eq!(pending.ids, vec![order[1]]);
        assert_eq!(pending.pos, egui::pos2(12.0, 34.0));
        // The strip is untouched until the app actually moves the tab.
        assert_eq!(ids(&s), order);
    }

    #[test]
    fn pinned_tabs_stay_in_the_prefix() {
        let mut s = state();
        let order = ids(&s);
        apply(&mut s, TabAction::TogglePin(order[2]));
        assert_eq!(ids(&s)[0], order[2], "pinned tab moves to the front");

        // Moving a normal tab to index 0 must not displace a pinned tab.
        let normal = ids(&s)[2];
        apply(&mut s, TabAction::MoveTabs { ids: vec![normal], to_index: 0 });
        assert_eq!(ids(&s)[0], order[2]);
    }

    #[test]
    fn grouping_makes_members_contiguous() {
        let mut s = state();
        let order = ids(&s);
        // Group two non-adjacent tabs.
        apply(
            &mut s,
            TabAction::CreateGroup {
                ids: vec![order[0], order[2]],
                color: GroupColor::Blue,
            },
        );
        let after = ids(&s);
        assert_eq!(after[0], order[0]);
        assert_eq!(after[1], order[2], "group members become adjacent");
    }

    #[test]
    fn closing_selected_tabs_keeps_one_tab_alive() {
        let mut s = state();
        apply(&mut s, TabAction::SelectAll);
        apply(&mut s, TabAction::CloseSelected);
        // The last tab is recycled to the welcome page, never removed.
        assert_eq!(s.tabs.len(), 1);
        assert_eq!(s.tabs[0].url, "oxide://welcome");
    }

    #[test]
    fn closing_selected_tabs_skips_pinned_tabs() {
        let mut s = state();
        let order = ids(&s);
        apply(&mut s, TabAction::TogglePin(order[0]));
        apply(&mut s, TabAction::SelectAll);
        apply(&mut s, TabAction::CloseSelected);
        assert_eq!(s.tabs.len(), 1);
        assert!(s.tabs[0].pinned);
        assert_eq!(s.tabs[0].id, order[0]);
    }

    #[test]
    fn rename_is_committed_only_from_the_editor() {
        let mut s = state();
        let order = ids(&s);
        let group = s.create_group(&order[..2], GroupColor::Green).unwrap();

        apply(&mut s, TabAction::BeginRenameGroup(group));
        assert!(s.group_rename.is_some());

        apply(&mut s, TabAction::CancelGroupRename);
        assert!(s.group_rename.is_none());
        assert_eq!(s.group(group).unwrap().name, "Group 1");

        apply(&mut s, TabAction::BeginRenameGroup(group));
        s.group_rename.as_mut().unwrap().name = "Docs".to_string();
        apply(&mut s, TabAction::CommitGroupRename);
        assert_eq!(s.group(group).unwrap().name, "Docs");
    }

    #[test]
    fn stale_ids_are_ignored() {
        let mut s = state();
        let before = urls(&s);
        apply(&mut s, TabAction::Activate(9_999));
        apply(&mut s, TabAction::Close(9_999));
        apply(&mut s, TabAction::TogglePin(9_999));
        assert_eq!(urls(&s), before);
    }

    #[test]
    fn tab_search_matches_titles_urls_and_ignores_case() {
        let mut s = state();
        s.tabs[1].title = "Rust Documentation".to_string();

        s.tab_search_query = "rust doc".to_string();
        assert_eq!(s.tab_search_matches(), vec![s.tabs[1].id]);

        s.tab_search_query = "EXAMPLE.COM/B".to_string();
        assert_eq!(s.tab_search_matches(), vec![s.tabs[2].id]);

        // An empty query is the plain switcher: every tab is listed.
        s.tab_search_query.clear();
        assert_eq!(s.tab_search_matches().len(), s.tabs.len());
    }

    #[test]
    fn open_tab_search_resets_the_query_and_highlight() {
        let mut s = state();
        s.tab_search_query = "stale".to_string();
        s.tab_search_selected = 7;
        apply(&mut s, TabAction::OpenTabSearch);
        assert!(s.tab_search_open);
        assert!(s.tab_search_query.is_empty());
        assert_eq!(s.tab_search_selected, 0);
    }

    #[test]
    fn closing_tabs_to_the_left_keeps_the_clicked_tab() {
        let mut s = state();
        let order = ids(&s);
        apply(&mut s, TabAction::CloseToLeft(order[2]));
        assert_eq!(ids(&s), vec![order[2]]);
    }

    #[test]
    fn closing_tabs_to_the_left_protects_pinned_tabs() {
        let mut s = state();
        let order = ids(&s);
        apply(&mut s, TabAction::TogglePin(order[0]));
        apply(&mut s, TabAction::CloseToLeft(order[2]));
        assert_eq!(ids(&s), vec![order[0], order[2]]);
    }
}
