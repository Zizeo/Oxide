//! Session persistence — "continue where you left off".
//!
//! # What is stored
//!
//! Only the *tab model* is serialised: URL, pinned flag, group membership and
//! the group table (name, colour, collapsed state). Page state — DOM, scroll
//! position, JS heap — is deliberately not serialised: Chrome reloads
//! restored tabs too, and "the tab is back, the page reloads" is the honest
//! contract. Restoring the model instead of native `WebView`s is also what
//! keeps the file small, readable and safe to write during shutdown.
//!
//! The file is written on a clean shutdown only, so a crash leaves the
//! previous session intact instead of a half-written one.

use crate::browser::{Bookmark, BrowserState, ConsoleLevel, GroupColor, SavedGroup};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// URL of the start page. A session that only holds this page is not worth
/// restoring — the browser opens it anyway.
const START_PAGE: &str = "oxide://welcome";

/// Upper bound on restored tabs, so a corrupt or absurd file cannot stall
/// startup.
const MAX_RESTORED_TABS: usize = 64;

/// One tab as it appears in the session file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionTab {
    pub url: String,
    #[serde(default)]
    pub pinned: bool,
    /// Index into [`Session::groups`], `None` for an ungrouped tab.
    #[serde(default)]
    pub group: Option<usize>,
}

/// One group as it appears in the session file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionGroup {
    pub name: String,
    /// Palette index (see [`GroupColor::index`]), stable across releases.
    #[serde(default)]
    pub color: u8,
    #[serde(default)]
    pub collapsed: bool,
}

/// A complete snapshot of the tab strip.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Session {
    pub tabs: Vec<SessionTab>,
    #[serde(default)]
    pub groups: Vec<SessionGroup>,
    /// Strip index of the tab that was displayed.
    #[serde(default)]
    pub active: Option<usize>,
    /// Bookmarks bar contents. Independent of the tab strip.
    #[serde(default)]
    pub bookmarks: Vec<Bookmark>,
    /// Groups the user saved for later.
    #[serde(default)]
    pub saved_groups: Vec<SavedGroup>,
}

impl Session {
    /// Snapshots the live model. Read-only: saving must never disturb the
    /// browser it is saving.
    pub fn capture(state: &BrowserState) -> Self {
        Self {
            tabs: state
                .tabs
                .iter()
                .map(|tab| SessionTab {
                    url: tab.url.clone(),
                    pinned: tab.pinned,
                    group: tab
                        .group
                        .and_then(|gid| state.groups.iter().position(|group| group.id == gid)),
                })
                .collect(),
            groups: state
                .groups
                .iter()
                .map(|group| SessionGroup {
                    name: group.name.clone(),
                    color: group.color.index(),
                    collapsed: group.collapsed,
                })
                .collect(),
            active: state.active_tab_index(),
            bookmarks: state.bookmarks.clone(),
            saved_groups: state.saved_groups.clone(),
        }
    }

    /// Rebuilds the strip from the snapshot.
    ///
    /// Groups are recreated lazily, by the first member that references them:
    /// the model cannot hold an empty group, so restoring an empty one would
    /// be dropped immediately anyway.
    pub fn apply(&self, state: &mut BrowserState) {
        state.clear_for_restore();
        // Bookmarks and saved groups describe the browser, not the window:
        // they are rebuilt from the snapshot like the tabs are.
        state.bookmarks = self.bookmarks.clone();
        state.saved_groups = self.saved_groups.clone();

        let mut group_ids: Vec<Option<u64>> = vec![None; self.groups.len()];

        for saved in self.tabs.iter().take(MAX_RESTORED_TABS) {
            if saved.url.trim().is_empty() {
                continue;
            }
            state.create_tab(&saved.url);
            let Some(tab_id) = state.active_tab_id else {
                continue;
            };
            if let Some(tab) = state.tab_mut(tab_id) {
                // Nothing is loaded yet: without this the strip would show a
                // spinner that can never stop on background tabs.
                tab.loading = false;
                tab.discarded = true;
            }

            if saved.pinned {
                state.set_pinned(tab_id, true);
                continue;
            }
            let Some(group_index) = saved.group else {
                continue;
            };
            let Some(saved_group) = self.groups.get(group_index) else {
                continue;
            };

            let group_id = match group_ids[group_index] {
                Some(id) => Some(id),
                None => {
                    let color = GroupColor::from_index(saved_group.color);
                    let created = state.create_group(&[tab_id], color);
                    if let Some(id) = created {
                        state.rename_group(id, saved_group.name.clone());
                        if saved_group.collapsed {
                            state.toggle_group_collapsed(id);
                        }
                    }
                    created
                }
            };
            if let Some(group_id) = group_id {
                group_ids[group_index] = Some(group_id);
                state.add_to_group(&[tab_id], group_id);
            }
        }

        if let Some(active) = self.active {
            state.activate_tab_at(active);
        }
    }

    /// `true` when the snapshot contains a page the user would miss.
    fn is_worth_restoring(&self) -> bool {
        self.tabs.iter().take(MAX_RESTORED_TABS).any(|tab| {
            let url = tab.url.trim();
            !url.is_empty() && url != START_PAGE
        })
    }
}

/// Where the session file lives, following the usual per-OS config location.
fn session_path() -> Option<PathBuf> {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from))
        .or_else(|| {
            std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config"))
        })?;
    Some(base.join("Oxide").join("session.json"))
}

/// Reads a session file, or `None` when it is missing or unreadable.
fn read_session(path: &std::path::Path) -> Option<Session> {
    let raw = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&raw).ok()
}

/// Writes `session` to `path` atomically (write, then rename).
fn write_session(path: &std::path::Path, session: &Session) -> bool {
    let Ok(json) = serde_json::to_string_pretty(session) else {
        return false;
    };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    // Write-then-rename: a crash or a full disk mid-write must not leave a
    // truncated file that the next start would reject.
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json).is_ok() && std::fs::rename(&tmp, path).is_ok()
}

/// Loads the previous session into `state`, replacing the start page.
///
/// Returns `false` (leaving `state` untouched) when there is no file, when the
/// file is unreadable or stale, or when it holds nothing worth restoring.
pub fn restore(state: &mut BrowserState) -> bool {
    let Some(path) = session_path() else {
        return false;
    };
    let Some(session) = read_session(&path) else {
        if path.exists() {
            state.log(ConsoleLevel::Warn, "Ignoring unreadable session file");
        }
        return false;
    };
    if !session.is_worth_restoring() {
        // Bookmarks and saved groups are not tied to the tab strip, so they
        // still load: a user who closed everything last time expects the bar
        // to be there.
        state.bookmarks = session.bookmarks;
        state.saved_groups = session.saved_groups;
        return false;
    }

    session.apply(state);
    if state.tabs.is_empty() {
        // Nothing usable survived the file: do not leave an empty window.
        state.create_tab(START_PAGE);
        return false;
    }
    state.log(
        ConsoleLevel::Info,
        format!("Restored {} tabs from the previous session", state.tabs.len()),
    );
    true
}

/// Writes the current strip to disk. Failures are silent by design: a browser
/// must never refuse to close because its session file is not writable.
pub fn save(state: &BrowserState) {
    let Some(path) = session_path() else {
        return;
    };
    if state.tabs.is_empty() {
        return;
    }
    let _ = write_session(&path, &Session::capture(state));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> BrowserState {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        // Leak: the handle has to outlive the state for the test's duration.
        let handle = Box::leak(Box::new(rt)).handle().clone();
        BrowserState::new(handle)
    }

    fn urls(state: &BrowserState) -> Vec<String> {
        state.tabs.iter().map(|tab| tab.url.clone()).collect()
    }

    #[test]
    fn round_trip_keeps_order_pins_and_groups() {
        let mut original = state();
        original.create_tab("https://example.com/a");
        original.create_tab("https://example.com/b");
        let ids: Vec<u64> = original.tabs.iter().map(|tab| tab.id).collect();

        original.set_pinned(ids[0], true);
        let group = original
            .create_group(&[ids[1], ids[2]], GroupColor::Purple)
            .expect("group is created");
        original.rename_group(group, "Docs".to_string());
        original.toggle_group_collapsed(group);
        original.activate_tab(ids[2]);

        let snapshot = Session::capture(&original);

        let mut restored = state();
        snapshot.apply(&mut restored);

        assert_eq!(urls(&restored), urls(&original));
        assert!(restored.tabs[0].pinned, "the pinned tab stays pinned");
        assert_eq!(restored.tabs[1].group, restored.tabs[2].group);
        let restored_group = &restored.groups[0];
        assert_eq!(restored_group.name, "Docs");
        assert_eq!(restored_group.color, GroupColor::Purple);
        assert!(restored_group.collapsed);
        assert_eq!(
            restored.active_tab_index(),
            original.active_tab_index(),
            "the displayed tab is remembered"
        );
    }

    #[test]
    fn a_session_holding_only_the_start_page_is_not_restored() {
        let original = state();
        let snapshot = Session::capture(&original);
        assert!(!snapshot.is_worth_restoring());
    }

    #[test]
    fn blank_urls_are_skipped() {
        let snapshot = Session {
            tabs: vec![
                SessionTab {
                    url: "   ".to_string(),
                    pinned: false,
                    group: None,
                },
                SessionTab {
                    url: "https://example.com".to_string(),
                    pinned: false,
                    group: None,
                },
            ],
            groups: Vec::new(),
            active: None,
            ..Default::default()
        };
        let mut restored = state();
        snapshot.apply(&mut restored);
        assert_eq!(restored.tabs.len(), 1);
        assert_eq!(restored.tabs[0].url, "https://example.com/");
    }

    #[test]
    fn unknown_group_colour_falls_back_to_grey() {
        assert_eq!(GroupColor::from_index(255), GroupColor::Grey);
        assert_eq!(GroupColor::from_index(GroupColor::Cyan.index()), GroupColor::Cyan);
    }

    #[test]
    fn session_file_round_trips_through_disk() {
        let mut original = state();
        original.create_tab("https://example.com/disk");
        original.set_pinned(original.tabs[0].id, true);

        let dir = std::env::temp_dir().join(format!("oxide-session-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("session.json");

        assert!(write_session(&path, &Session::capture(&original)));
        let loaded = read_session(&path).expect("session reads back");
        assert!(loaded.is_worth_restoring());

        let mut restored = state();
        loaded.apply(&mut restored);
        assert_eq!(urls(&restored), urls(&original));
        assert!(restored.tabs[0].pinned);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn bookmarks_and_saved_groups_survive_a_round_trip() {
        let mut original = state();
        original.create_tab("https://example.com/a");
        original.create_tab("https://example.com/b");
        original.add_bookmark("https://example.com/docs", "Docs");
        original.bookmark_all_tabs();

        let group = original
            .create_group(&[original.tabs[1].id], GroupColor::Cyan)
            .expect("group is created");
        original.rename_group(group, "Reading".to_string());
        assert!(original.save_group(group), "the group holds a tab");

        let snapshot = Session::capture(&original);
        let json = serde_json::to_string(&snapshot).unwrap();
        let loaded: Session = serde_json::from_str(&json).unwrap();

        let mut restored = state();
        loaded.apply(&mut restored);
        assert!(restored.is_bookmarked("https://example.com/docs"));
        // "Bookmark all tabs" is one folder, not one entry per tab on the bar.
        assert!(
            restored
                .bookmarks
                .iter()
                .any(|b| b.folder.as_deref() == Some("All tabs"))
        );
        assert_eq!(restored.saved_groups.len(), 1);
        assert_eq!(restored.saved_groups[0].name, "Reading");
        assert_eq!(restored.saved_groups[0].color, GroupColor::Cyan.index());
    }
}
