use std::collections::{BTreeSet, HashMap};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, RwLock};
use oxide_net::security::{
    resolve_omnibox_input, welcome_page_html, NavigationDecision, SecurityPolicy,
};
use oxide_net::{HttpResponse, NetError, NetworkEngine};
use winit::window::Window;
use wry::dpi::{LogicalPosition, LogicalSize, Position, Size};
use wry::{Rect as WryRect, WebView, WebViewBuilder};
use crate::tab::Tab;
use crate::ui::menu::MenuKind;

/// Raw `HWND` of the main window, used to locate the page's child window.
#[cfg(windows)]
fn window_hwnd(window: &Window) -> Option<isize> {
    use wry::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    match window.window_handle().ok()?.as_raw() {
        RawWindowHandle::Win32(h) => Some(h.hwnd.get()),
        _ => None,
    }
}

pub struct TabFetchResult {
    pub tab_id: u64,
    pub request_id: u64,
    pub result: Result<HttpResponse, NetError>,
}

#[derive(Debug, Clone)]
pub enum WebViewEvent {
    TitleChanged {
        tab_id: u64,
        title: String,
    },
    UrlChanged {
        tab_id: u64,
        url: String,
    },
    UpgradeRedirect {
        tab_id: u64,
        https_url: String,
    },
    NewTabRequested {
        url: String,
    },
    SecurityAlert {
        tab_id: u64,
        url: String,
        reason: String,
    },
    /// Reported by the injected media probe: the page started or stopped
    /// producing audio.
    AudioState {
        tab_id: u64,
        playing: bool,
    },
}

#[derive(Debug, Clone)]
pub struct ConsoleEntry {
    pub level: ConsoleLevel,
    pub message: String,
    pub timestamp: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsoleLevel {
    Info,
    Warn,
    Error,
    Net,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DevToolsTab {
    Elements,
    Network,
    Source,
    Console,
}

/// Chrome-style tab group colours.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupColor {
    Grey,
    Blue,
    Red,
    Yellow,
    Green,
    Pink,
    Purple,
    Cyan,
}

impl GroupColor {
    pub const ALL: [GroupColor; 8] = [
        GroupColor::Grey,
        GroupColor::Blue,
        GroupColor::Red,
        GroupColor::Yellow,
        GroupColor::Green,
        GroupColor::Pink,
        GroupColor::Purple,
        GroupColor::Cyan,
    ];

    /// Accent colour used by the tab strip.
    pub fn rgb(self) -> (u8, u8, u8) {
        match self {
            GroupColor::Grey => (154, 160, 166),
            GroupColor::Blue => (60, 145, 240),
            GroupColor::Red => (233, 89, 89),
            GroupColor::Yellow => (240, 190, 70),
            GroupColor::Green => (76, 190, 120),
            GroupColor::Pink => (230, 120, 200),
            GroupColor::Purple => (150, 120, 235),
            GroupColor::Cyan => (70, 200, 215),
        }
    }

    pub fn next(self) -> GroupColor {
        let index = GroupColor::ALL
            .iter()
            .position(|c| *c == self)
            .unwrap_or(0);
        GroupColor::ALL[(index + 1) % GroupColor::ALL.len()]
    }

    /// Human-readable name, used by the group menus.
    pub fn label(self) -> &'static str {
        match self {
            GroupColor::Grey => "Grey",
            GroupColor::Blue => "Blue",
            GroupColor::Red => "Red",
            GroupColor::Yellow => "Yellow",
            GroupColor::Green => "Green",
            GroupColor::Pink => "Pink",
            GroupColor::Purple => "Purple",
            GroupColor::Cyan => "Cyan",
        }
    }

    /// Stable palette index, used by the session file so a saved group keeps
    /// its colour even if the palette gains entries later.
    pub fn index(self) -> u8 {
        GroupColor::ALL
            .iter()
            .position(|c| *c == self)
            .unwrap_or(0) as u8
    }

    /// Inverse of [`GroupColor::index`]. Unknown indices fall back to grey
    /// instead of failing: a session written by a newer build must never stop
    /// the browser from starting.
    pub fn from_index(index: u8) -> GroupColor {
        GroupColor::ALL
            .get(index as usize)
            .copied()
            .unwrap_or(GroupColor::Grey)
    }
}

/// A named, coloured, collapsible group of tabs.
#[derive(Debug, Clone)]
pub struct TabGroup {
    pub id: u64,
    pub name: String,
    pub color: GroupColor,
    pub collapsed: bool,
}

/// Entry of the "recently closed" stack (kept in memory only).
#[derive(Debug, Clone)]
pub struct ClosedTab {
    pub url: String,
    pub title: String,
    pub group: Option<String>,
}

/// Inline group-rename popup state.
#[derive(Debug, Clone)]
pub struct GroupRename {
    pub group_id: u64,
    pub name: String,
}

/// Maximum number of live child WebViews kept for background tabs.
///
/// Every tab that has ever been displayed keeps a full native `WebView`
/// alive, which is the dominant memory cost of the browser. Past this budget
/// the least recently used *hidden* page is discarded and reloaded when its
/// tab is selected again — the trade-off Chrome's Memory Saver makes, with a
/// smaller budget here because the Rust-side document tree is kept as well.
const LIVE_WEBVIEW_LIMIT: usize = 4;

/// Upper bound on a fetched favicon, in bytes.
///
/// A real icon is a few kilobytes. The limit exists so a site that answers
/// `/favicon.ico` with a video stream cannot make the browser buffer it.
const FAVICON_MAX_BYTES: usize = 256 * 1024;

/// One entry on the bookmarks bar.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Bookmark {
    pub title: String,
    pub url: String,
    /// `Some(folder)` when "Bookmark all tabs" created the entry: the bar
    /// draws one chip per folder instead of one chip per URL.
    #[serde(default)]
    pub folder: Option<String>,
}

/// A tab group the user saved, so it can be reopened later.
///
/// Chrome stores saved groups in the bookmarks hierarchy; Oxide keeps them in
/// their own list, which is the same idea without needing a bookmark tree.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SavedGroup {
    pub name: String,
    /// Palette index, stable across releases — see [`GroupColor::index`].
    pub color: u8,
    pub urls: Vec<String>,
}

/// What the browser knows about one site's icon.
#[derive(Clone)]
pub enum FaviconState {
    /// A fetch is in flight.
    Pending,
    /// Decoded and ready to be turned into a texture.
    Ready(std::sync::Arc<egui::ColorImage>),
    /// Nothing usable was served; the strip keeps the scheme glyph.
    Failed,
}

/// A finished favicon fetch.
pub struct FaviconFetchResult {
    pub host: String,
    pub image: Option<egui::ColorImage>,
}

/// Javascript injected into every page before it loads.
///
/// # Why a page script
///
/// `wry` gives no audio callback: a `WebView` says nothing about whether the
/// page is playing sound. The only portable signal is the DOM, so the browser
/// installs a probe that watches the media elements and reports transitions
/// back over the IPC channel.
///
/// `__MUTED__` is replaced with the initial mute state, so a muted site is
/// silent from its very first frame instead of starting to play and then
/// being muted by a follow-up script.
const AUDIO_PROBE_JS: &str = r#"
(function () {
    if (window.__oxideAudioProbe) { return; }
    window.__oxideAudioProbe = true;

    var muted = __MUTED__;
    var last = null;

    function media() {
        return document.querySelectorAll('audio,video');
    }

    window.__oxideSetMuted = function (value) {
        muted = !!value;
        var list = media();
        for (var i = 0; i < list.length; i++) {
            try { list[i].muted = muted; } catch (e) { /* detached element */ }
        }
    };

    function isPlaying() {
        var list = media();
        for (var i = 0; i < list.length; i++) {
            var element = list[i];
            if (!element.paused && !element.ended
                && element.currentTime > 0 && element.readyState >= 2) {
                return true;
            }
        }
        return false;
    }

    function report() {
        // A page can rewrite `muted` whenever it likes, so a muted site
        // re-asserts the flag on every tick instead of trusting one write.
        if (muted) { window.__oxideSetMuted(true); }
        var now = isPlaying();
        if (now === last) { return; }
        last = now;
        try {
            window.ipc.postMessage('oxide-audio:' + (now ? '1' : '0'));
        } catch (e) { /* no IPC channel: nothing to report to */ }
    }

    setInterval(report, 1000);
    document.addEventListener('play', report, true);
    document.addEventListener('pause', report, true);
    document.addEventListener('ended', report, true);
    document.addEventListener('volumechange', report, true);
    document.addEventListener('DOMContentLoaded', report, true);
})();
"#;

/// The site key used for muting and favicon caching.
///
/// A leading `www.` is dropped so `www.example.com` and `example.com` share
/// one entry: users think of them as the same site, and Chrome's site mute
/// works that way too.
pub fn site_key(url: &str) -> Option<String> {
    let parsed = url::Url::parse(url).ok()?;
    // Only real network sites have a host worth keying on. Note that
    // `oxide://welcome` parses with `welcome` as its *host*, which is a page
    // name rather than a site — muting or fetching a favicon for it would be
    // meaningless.
    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return None;
    }
    let host = parsed.host_str()?.to_ascii_lowercase();
    Some(host.strip_prefix("www.").unwrap_or(&host).to_string())
}

/// Builds the probe with the tab's current mute state baked in.
fn audio_probe_script(muted: bool) -> String {
    AUDIO_PROBE_JS.replace("__MUTED__", if muted { "true" } else { "false" })
}

/// A context menu the UI asked for, waiting to be shown.
///
/// `pos` is in the main window's logical client coordinates, which is what
/// egui reports; the app converts it to a screen position when it places the
/// popup window.
#[derive(Debug, Clone, PartialEq)]
pub struct PendingMenu {
    pub kind: MenuKind,
    pub pos: egui::Pos2,
}

/// A tab drag released outside the strip, waiting for the app to open a new
/// window for it.
///
/// The strip cannot create a window itself (that needs the event loop), so it
/// records the request exactly like [`PendingMenu`]. `pos` is the release
/// position in logical client coordinates, used to place the new window under
/// the cursor.
#[derive(Debug, Clone, PartialEq)]
pub struct PendingDetach {
    pub ids: Vec<u64>,
    pub pos: egui::Pos2,
}

pub struct BrowserState {
    pub tabs: Vec<Tab>,
    /// Stable id of the tab whose page is displayed. Never an index: the strip
    /// is reordered by drag and drop.
    pub active_tab_id: Option<u64>,
    pub next_tab_id: u64,
    pub groups: Vec<TabGroup>,
    pub next_group_id: u64,
    /// Tabs targeted by bulk actions (Ctrl/Shift click). Independent from the
    /// active tab, exactly like Chrome.
    pub selected_tab_ids: BTreeSet<u64>,
    /// Anchor used by Shift+click range selection.
    pub selection_anchor: Option<u64>,
    /// Most recently closed last; bounded stack used by "Reopen closed tab".
    pub closed_tabs: Vec<ClosedTab>,
    /// Monotonic counter ordering tabs by last activation.
    activation_seq: u64,
    /// Active inline group-rename popup, if any.
    pub group_rename: Option<GroupRename>,
    /// Set by Ctrl+L; the nav bar focuses the omnibox on the next frame.
    pub focus_omnibox_request: bool,
    /// A right-click asked for a context menu that has not been shown yet.
    ///
    /// The menu is not drawn by this window: the page is a native child
    /// `wry::WebView` that the OS paints above egui, so a menu drawn here
    /// would be buried under it. The app takes this request and opens a
    /// borderless popup window above the page instead (see
    /// [`crate::menu_window`]), which keeps the page visible and live.
    pub pending_menu: Option<PendingMenu>,
    /// A drag that ended outside the strip, waiting to be moved into a new
    /// window. Taken by the app after the UI pass.
    pub pending_detach: Option<PendingDetach>,
    /// `true` while the tab-search overlay (Ctrl+Shift+A) is open.
    ///
    /// The overlay is drawn by egui, so like a context menu it would sit
    /// behind the native child `WebView`; the page is hidden while it is up.
    pub tab_search_open: bool,
    /// Filter text of the tab-search overlay.
    pub tab_search_query: String,
    /// Highlighted row of [`BrowserState::tab_search_matches`]. The overlay
    /// clamps it, so keyboard navigation can never select a missing row.
    pub tab_search_selected: usize,
    /// Entries on the bookmarks bar, in bar order.
    pub bookmarks: Vec<Bookmark>,
    /// Groups the user saved, newest last.
    pub saved_groups: Vec<SavedGroup>,
    /// Sites whose audio is muted. Holds [`site_key`] values, so the mute
    /// survives navigation inside the site and is shared by every tab.
    pub muted_sites: BTreeSet<String>,
    /// Per-site icon state, keyed by [`site_key`].
    pub favicons: HashMap<String, FaviconState>,
    favicon_tx: Sender<FaviconFetchResult>,
    favicon_rx: Receiver<FaviconFetchResult>,
    /// Set when background work (WebView callbacks, favicon fetches) changed
    /// something the chrome displays. The event loop turns it into a redraw
    /// request: without it an icon or a speaker could arrive and stay
    /// invisible until the next mouse move.
    dirty: bool,
    pub net_engine: Arc<NetworkEngine>,
    pub tokio_handle: tokio::runtime::Handle,
    pub result_tx: Sender<TabFetchResult>,
    pub result_rx: Receiver<TabFetchResult>,
    pub wv_event_tx: Sender<WebViewEvent>,
    pub wv_event_rx: Receiver<WebViewEvent>,
    pub webviews: HashMap<u64, WebView>,
    pub webview_loaded_url: HashMap<u64, String>,
    pub allow_internal_html: HashMap<u64, Arc<AtomicBool>>,
    pub security_policy: Arc<RwLock<SecurityPolicy>>,
    pub viewport_rect: egui::Rect,
    /// Frozen copy of the page painted while a menu or overlay hides the
    /// native WebView, so the page stays visible under the menu instead of
    /// going blank. Captured once per menu open; see [`crate::snapshot`].
    pub page_snapshot: Option<crate::snapshot::PageSnapshot>,
    pub show_devtools: bool,
    pub devtools_tab: DevToolsTab,
    pub devtools_height: f32,
    pub console_logs: Vec<ConsoleEntry>,
}


impl BrowserState {
    pub fn new(tokio_handle: tokio::runtime::Handle) -> Self {
        let mut browser = Self::empty(tokio_handle);
        browser.log(
            ConsoleLevel::Info,
            "Oxide Browser initialized (Wry WebView + Rust Security Guard active).",
        );
        browser.create_tab("oxide://welcome");
        browser
    }

    /// A window with no tabs at all.
    ///
    /// Only a window created by tearing a tab off the strip starts this way;
    /// it is filled by [`Self::replace_strip_with`] on the same frame.
    pub fn empty(tokio_handle: tokio::runtime::Handle) -> Self {
        let (tx, rx) = channel();
        let (wv_tx, wv_rx) = channel();
        let (fav_tx, fav_rx) = channel();
        let net_engine = Arc::new(NetworkEngine::new());
        let security_policy = Arc::new(RwLock::new(SecurityPolicy::default()));

        Self {
            tabs: Vec::new(),
            active_tab_id: None,
            next_tab_id: 1,
            groups: Vec::new(),
            next_group_id: 1,
            selected_tab_ids: BTreeSet::new(),
            selection_anchor: None,
            closed_tabs: Vec::new(),
            activation_seq: 0,
            group_rename: None,
            focus_omnibox_request: false,
            pending_menu: None,
            pending_detach: None,
            tab_search_open: false,
            tab_search_query: String::new(),
            tab_search_selected: 0,
            bookmarks: Vec::new(),
            saved_groups: Vec::new(),
            muted_sites: BTreeSet::new(),
            favicons: HashMap::new(),
            favicon_tx: fav_tx,
            favicon_rx: fav_rx,
            dirty: false,
            net_engine,
            tokio_handle,
            result_tx: tx,
            result_rx: rx,
            wv_event_tx: wv_tx,
            wv_event_rx: wv_rx,
            webviews: HashMap::new(),
            webview_loaded_url: HashMap::new(),
            allow_internal_html: HashMap::new(),
            security_policy,
            viewport_rect: egui::Rect::from_min_size(
                egui::pos2(0.0, 92.0),
                egui::vec2(1280.0, 728.0),
            ),
            page_snapshot: None,
            show_devtools: false,
            devtools_tab: DevToolsTab::Elements,
            devtools_height: 280.0,
            console_logs: Vec::new(),
        }
    }

    pub fn log(&mut self, level: ConsoleLevel, message: impl Into<String>) {
        let now = chrono_like_now();
        self.console_logs.push(ConsoleEntry {
            level,
            message: message.into(),
            timestamp: now,
        });

        if self.console_logs.len() > 1000 {
            self.console_logs.drain(0..200);
        }
    }

    // ---- lookup ---------------------------------------------------------

    /// Strip index of the active tab. Everything else works with stable ids.
    pub fn active_tab_index(&self) -> Option<usize> {
        self.active_tab_id.and_then(|id| self.tab_index(id))
    }

    pub fn tab_index(&self, id: u64) -> Option<usize> {
        self.tabs.iter().position(|t| t.id == id)
    }

    pub fn tab(&self, id: u64) -> Option<&Tab> {
        self.tabs.iter().find(|t| t.id == id)
    }

    pub fn tab_mut(&mut self, id: u64) -> Option<&mut Tab> {
        self.tabs.iter_mut().find(|t| t.id == id)
    }

    pub fn active_tab(&self) -> Option<&Tab> {
        let id = self.active_tab_id?;
        self.tab(id)
    }

    pub fn active_tab_mut(&mut self) -> Option<&mut Tab> {
        let id = self.active_tab_id?;
        self.tab_mut(id)
    }

    pub fn group(&self, id: u64) -> Option<&TabGroup> {
        self.groups.iter().find(|g| g.id == id)
    }

    pub fn is_selected(&self, id: u64) -> bool {
        self.selected_tab_ids.contains(&id)
    }

    // ---- activation and selection ---------------------------------------

    /// Makes `id` the displayed tab. A plain activation resets the
    /// multi-selection to just that tab, exactly like Chrome.
    pub fn activate_tab(&mut self, id: u64) {
        if self.tab(id).is_none() {
            return;
        }
        // Choosing a tab supersedes the tab-search overlay: the page must
        // come back even if the user picked a tab with the mouse.
        self.tab_search_open = false;
        self.activation_seq += 1;
        let seq = self.activation_seq;
        if let Some(tab) = self.tab_mut(id) {
            tab.last_activated = seq;
        }
        self.active_tab_id = Some(id);
        self.selection_anchor = Some(id);
        self.selected_tab_ids.clear();
        self.selected_tab_ids.insert(id);
    }

    /// Ctrl+click: add or remove a tab from the selection without changing the
    /// displayed page.
    pub fn toggle_tab_selection(&mut self, id: u64) {
        if self.tab(id).is_none() {
            return;
        }
        if !self.selected_tab_ids.remove(&id) {
            self.selected_tab_ids.insert(id);
        }
        self.selection_anchor = Some(id);
        // The active tab is always part of the selection.
        if self.selected_tab_ids.is_empty()
            && let Some(active) = self.active_tab_id
        {
            self.selected_tab_ids.insert(active);
        }
    }

    /// Shift+click: select the contiguous range from the anchor to `id`.
    pub fn extend_selection_to(&mut self, id: u64) {
        let target = self.tab_index(id);
        let anchor = self.selection_anchor.and_then(|a| self.tab_index(a));
        let (Some(to), Some(from)) = (target, anchor) else {
            self.activate_tab(id);
            return;
        };
        let (lo, hi) = (from.min(to), from.max(to));
        let ids: Vec<u64> = self.tabs[lo..=hi].iter().map(|t| t.id).collect();
        self.selected_tab_ids.clear();
        self.selected_tab_ids.extend(ids);
    }

    pub fn select_all_tabs(&mut self) {
        self.selected_tab_ids = self.tabs.iter().map(|t| t.id).collect();
    }

    pub fn clear_selection(&mut self) {
        self.selected_tab_ids.clear();
        if let Some(active) = self.active_tab_id {
            self.selected_tab_ids.insert(active);
        }
    }

    /// Ids a bulk action started from `id` should target: the whole selection
    /// when `id` is selected, otherwise just `id`.
    pub fn bulk_targets(&self, id: u64) -> Vec<u64> {
        if self.selected_tab_ids.contains(&id) && self.selected_tab_ids.len() > 1 {
            self.tabs
                .iter()
                .filter(|t| self.selected_tab_ids.contains(&t.id))
                .map(|t| t.id)
                .collect()
        } else {
            vec![id]
        }
    }

    // ---- tab search ------------------------------------------------------

    /// Opens the tab-search overlay with a fresh query and highlight.
    pub fn open_tab_search(&mut self) {
        self.tab_search_open = true;
        self.tab_search_query.clear();
        self.tab_search_selected = 0;
    }

    /// Closes the overlay and forgets the query, so the next open starts from
    /// the full tab list rather than a stale filter.
    pub fn close_tab_search(&mut self) {
        self.tab_search_open = false;
        self.tab_search_query.clear();
        self.tab_search_selected = 0;
    }

    /// Tabs matching [`BrowserState::tab_search_query`], in strip order.
    ///
    /// An empty query matches every tab, which is what makes the overlay work
    /// as a plain switcher in addition to a search box.
    pub fn tab_search_matches(&self) -> Vec<u64> {
        let needle = self.tab_search_query.trim().to_lowercase();
        self.tabs
            .iter()
            .filter(|tab| {
                needle.is_empty()
                    || tab.title.to_lowercase().contains(&needle)
                    || tab.url.to_lowercase().contains(&needle)
            })
            .map(|tab| tab.id)
            .collect()
    }

    // ---- creation -------------------------------------------------------

    /// Empties the strip so a saved session can be rebuilt from scratch.
    ///
    /// Only used at startup, before any page has been shown. The native
    /// WebViews are dropped explicitly (and not just removed from the map) so
    /// no OS-level child window can outlive its tab.
    pub fn clear_for_restore(&mut self) {
        for wv in self.webviews.drain().map(|(_, wv)| wv) {
            let _ = wv.set_visible(false);
        }
        self.webview_loaded_url.clear();
        self.allow_internal_html.clear();
        self.tabs.clear();
        self.groups.clear();
        self.selected_tab_ids.clear();
        self.selection_anchor = None;
        self.active_tab_id = None;
        self.closed_tabs.clear();
        self.group_rename = None;
        self.activation_seq = 0;
    }

    // ---- audio ----------------------------------------------------------

    /// `true` when this tab's audio is silenced, by its own flag or by a
    /// site-wide mute.
    pub fn is_tab_muted(&self, tab: &Tab) -> bool {
        tab.muted
            || site_key(&tab.url).is_some_and(|key| self.muted_sites.contains(&key))
    }

    /// Pushes a tab's mute state into its live page.
    ///
    /// Only pages that exist can be told anything, so a discarded or
    /// never-shown tab simply waits: the state is baked into the probe when
    /// its `WebView` is created.
    fn sync_mute(&self, id: u64) {
        let Some(tab) = self.tab(id) else {
            return;
        };
        let muted = self.is_tab_muted(tab);
        if let Some(wv) = self.webviews.get(&id) {
            let _ = wv.evaluate_script(&format!(
                "window.__oxideSetMuted && window.__oxideSetMuted({muted})"
            ));
        }
    }

    /// Mutes or unmutes one tab, leaving the rest of its site alone.
    pub fn toggle_mute_tab(&mut self, id: u64) {
        let Some(tab) = self.tab_mut(id) else {
            return;
        };
        tab.muted = !tab.muted;
        let muted = tab.muted;
        self.sync_mute(id);
        self.log(
            ConsoleLevel::Info,
            if muted {
                "Muted tab"
            } else {
                "Unmuted tab"
            },
        );
    }

    /// Mutes or unmutes every tab showing the clicked tab's site.
    ///
    /// This is Chrome's "Mute site": the setting outlives the tab, so it is
    /// kept per [`site_key`] and reapplied to tabs that open later.
    pub fn toggle_mute_site(&mut self, id: u64) {
        let Some(key) = self.tab(id).and_then(|tab| site_key(&tab.url)) else {
            return;
        };
        let muted = if self.muted_sites.remove(&key) {
            false
        } else {
            self.muted_sites.insert(key.clone());
            true
        };

        let affected: Vec<u64> = self
            .tabs
            .iter()
            .filter(|tab| site_key(&tab.url).as_deref() == Some(key.as_str()))
            .map(|tab| tab.id)
            .collect();
        for tab_id in affected {
            self.sync_mute(tab_id);
        }
        self.log(
            ConsoleLevel::Info,
            format!(
                "{} audio for {key}",
                if muted { "Muted" } else { "Unmuted" }
            ),
        );
    }

    /// Mutes or unmutes every tab, the strip's bulk action.
    pub fn set_all_muted(&mut self, muted: bool) {
        for tab in &mut self.tabs {
            tab.muted = muted;
        }
        let ids: Vec<u64> = self.tabs.iter().map(|tab| tab.id).collect();
        for id in ids {
            self.sync_mute(id);
        }
    }

    /// `true` when at least one tab is producing audio.
    ///
    /// Unused by the chrome today — the strip paints a per-tab glyph — but it
    /// is the natural hook for a window-level "this browser is making noise"
    /// affordance, so it is kept next to the mute helpers it complements.
    #[allow(dead_code)]
    pub fn any_audio_playing(&self) -> bool {
        self.tabs.iter().any(|tab| tab.audio_playing)
    }

    // ---- bookmarks ------------------------------------------------------

    pub fn is_bookmarked(&self, url: &str) -> bool {
        self.bookmarks.iter().any(|bookmark| bookmark.url == url)
    }

    /// Adds a bookmark unless the URL is already on the bar.
    pub fn add_bookmark(&mut self, url: &str, title: &str) {
        if url.trim().is_empty() || self.is_bookmarked(url) {
            return;
        }
        let title = if title.trim().is_empty() {
            url.to_string()
        } else {
            title.trim().to_string()
        };
        self.bookmarks.push(Bookmark {
            title,
            url: url.to_string(),
            folder: None,
        });
        self.log(ConsoleLevel::Info, format!("Bookmarked {url}"));
    }

    pub fn remove_bookmark(&mut self, url: &str) {
        self.bookmarks.retain(|bookmark| bookmark.url != url);
    }

    /// Adds the bookmark, or removes it when the URL is already saved.
    pub fn toggle_bookmark(&mut self, url: &str, title: &str) {
        if self.is_bookmarked(url) {
            self.remove_bookmark(url);
            self.log(ConsoleLevel::Info, format!("Removed bookmark {url}"));
        } else {
            self.add_bookmark(url, title);
        }
    }

    /// Ctrl+Shift+D: puts every open tab on the bar as one folder.
    ///
    /// Chrome creates a bookmarks-bar folder here, so the entries go into a
    /// named folder instead of flooding the bar with one chip per tab.
    pub fn bookmark_all_tabs(&mut self) {
        let Some(active) = self.active_tab() else {
            return;
        };
        let folder = active
            .group
            .and_then(|gid| self.group(gid))
            .map(|group| group.name.clone())
            .unwrap_or_else(|| "All tabs".to_string());

        let entries: Vec<(String, String)> = self
            .tabs
            .iter()
            .map(|tab| (tab.title.clone(), tab.url.clone()))
            .collect();
        let mut added = 0;
        for (title, url) in entries {
            if url.trim().is_empty()
                || self
                    .bookmarks
                    .iter()
                    .any(|b| b.url == url && b.folder.as_deref() == Some(folder.as_str()))
            {
                continue;
            }
            self.bookmarks.push(Bookmark {
                title: if title.trim().is_empty() {
                    url.clone()
                } else {
                    title
                },
                url,
                folder: Some(folder.clone()),
            });
            added += 1;
        }
        self.log(
            ConsoleLevel::Info,
            format!("Bookmarked {added} tabs into “{folder}”"),
        );
    }

    /// Removes every bookmark in a folder, which is how the bar's folder
    /// menu closes one.
    pub fn remove_bookmark_folder(&mut self, folder: &str) {
        self.bookmarks
            .retain(|bookmark| bookmark.folder.as_deref() != Some(folder));
    }

    // ---- saved groups ---------------------------------------------------

    /// Saves the group's current membership so it can be reopened later.
    ///
    /// Returns `false` when the group is gone or holds no tabs.
    pub fn save_group(&mut self, group_id: u64) -> bool {
        let Some(group) = self.group(group_id).cloned() else {
            return false;
        };
        let urls: Vec<String> = self
            .tabs
            .iter()
            .filter(|tab| tab.group == Some(group_id))
            .map(|tab| tab.url.clone())
            .collect();
        if urls.is_empty() {
            return false;
        }
        self.saved_groups.push(SavedGroup {
            name: group.name.clone(),
            color: group.color.index(),
            urls: urls.clone(),
        });
        self.log(
            ConsoleLevel::Info,
            format!("Saved group “{}” ({} tabs)", group.name, urls.len()),
        );
        true
    }

    /// Reopens a saved group as a new live group at the end of the strip.
    pub fn reopen_saved_group(&mut self, index: usize) {
        let Some(saved) = self.saved_groups.get(index).cloned() else {
            return;
        };
        self.restore_urls_in_group(&saved.urls, &saved.name, GroupColor::from_index(saved.color));
    }

    /// Forgets a saved group without touching the live strip.
    pub fn delete_saved_group(&mut self, index: usize) {
        if index < self.saved_groups.len() {
            self.saved_groups.remove(index);
        }
    }

    /// Opens `urls` as one collapsible group named `name`.
    fn restore_urls_in_group(&mut self, urls: &[String], name: &str, color: GroupColor) {
        let mut ids = Vec::new();
        for url in urls {
            self.create_tab(url);
            if let Some(id) = self.active_tab_id {
                ids.push(id);
            }
        }
        if ids.is_empty() {
            return;
        }
        if let Some(group_id) = self.create_group(&ids, color) {
            self.rename_group(group_id, name.to_string());
        }
    }

    // ---- recently closed ------------------------------------------------

    /// The recently-closed stack, newest first, for the menu.
    pub fn recent_closed(&self, limit: usize) -> Vec<(usize, String)> {
        self.closed_tabs
            .iter()
            .rev()
            .take(limit)
            .enumerate()
            .map(|(index, closed)| {
                let label = if closed.title.trim().is_empty() {
                    closed.url.clone()
                } else {
                    closed.title.clone()
                };
                (index, label)
            })
            .collect()
    }

    /// Reopens the `index`-th most recently closed tab.
    pub fn reopen_closed_at(&mut self, index: usize) {
        let Some(position) = self.closed_tabs.len().checked_sub(index + 1) else {
            return;
        };
        let closed = self.closed_tabs.remove(position);
        self.open_closed(closed);
    }

    // ---- favicons -------------------------------------------------------

    /// Starts a fetch for every visible tab whose site has no icon yet.
    ///
    /// Cheap enough to call every frame: it is a hash lookup per tab unless a
    /// new site appears.
    pub fn refresh_favicons(&mut self) {
        let mut wanted: Vec<String> = Vec::new();
        for tab in &self.tabs {
            let Some(key) = site_key(&tab.url) else {
                continue;
            };
            if self.favicons.contains_key(&key) {
                continue;
            }
            self.favicons.insert(key.clone(), FaviconState::Pending);
            wanted.push(key);
        }

        for host in wanted {
            let net_engine = self.net_engine.clone();
            let tx = self.favicon_tx.clone();
            self.tokio_handle.spawn(async move {
                // `/favicon.ico` is what browsers fall back to and what the
                // overwhelming majority of sites serve.
                let url = format!("https://{host}/favicon.ico");
                let image = match net_engine.fetch_bytes(&url, FAVICON_MAX_BYTES).await {
                    Ok(bytes) => crate::favicon::decode(&bytes),
                    Err(_) => None,
                };
                let _ = tx.send(FaviconFetchResult { host, image });
            });
        }
    }

    /// Applies finished favicon fetches to the cache.
    pub fn poll_favicons(&mut self) {
        while let Ok(result) = self.favicon_rx.try_recv() {
            let state = match result.image {
                Some(image) => FaviconState::Ready(std::sync::Arc::new(image)),
                None => FaviconState::Failed,
            };
            self.favicons.insert(result.host, state);
            // An icon that arrives between redraws must not wait for the next
            // user event to be painted.
            self.dirty = true;
        }
    }

    /// Consumes the "something changed off-screen" flag.
    ///
    /// Returns `true` when the caller must ask the window for another frame.
    pub fn take_dirty(&mut self) -> bool {
        std::mem::take(&mut self.dirty)
    }

    /// The decoded icon for a URL's site, once one has arrived.
    pub fn favicon_for(&self, url: &str) -> Option<&std::sync::Arc<egui::ColorImage>> {
        let key = site_key(url)?;
        match self.favicons.get(&key) {
            Some(FaviconState::Ready(image)) => Some(image),
            _ => None,
        }
    }

    pub fn create_tab(&mut self, raw_url: &str) {        let resolved = resolve_omnibox_input(raw_url);
        let tab_id = self.next_tab_id;
        self.next_tab_id += 1;

        let mut tab = Tab::new(tab_id, &resolved);
        tab.loading = true;
        self.tabs.push(tab);
        self.activate_tab(tab_id);

        self.log(
            ConsoleLevel::Info,
            format!("Created Tab #{} ({})", tab_id, resolved),
        );
        self.apply_navigation_for_tab(tab_id, resolved);
    }

    /// Opens a copy of `id` right after it (Chrome's "Duplicate tab").
    pub fn duplicate_tab(&mut self, id: u64) {
        let Some(url) = self.tab(id).map(|t| t.url.clone()) else {
            return;
        };
        let Some(source_index) = self.tab_index(id) else {
            return;
        };
        self.create_tab(&url);
        if let Some(new_id) = self.active_tab_id {
            self.move_tabs(&[new_id], source_index + 1);
        }
    }

    // ---- closing --------------------------------------------------------

    pub fn close_tab(&mut self, id: u64) {
        let Some(index) = self.tab_index(id) else {
            return;
        };

        // Existing behaviour: the last remaining tab is recycled into the
        // welcome page instead of leaving an empty window.
        if self.tabs.len() <= 1 {
            if let Some(tab) = self.tabs.get_mut(0) {
                tab.navigate_to("oxide://welcome".to_string());
                let tab_id = tab.id;
                self.apply_navigation_for_tab(tab_id, "oxide://welcome".to_string());
            }
            return;
        }

        // Chrome activates the neighbour on the right, falling back to the left.
        let next_active = self
            .tabs
            .iter()
            .skip(index + 1)
            .map(|t| t.id)
            .next()
            .or_else(|| self.tabs[..index].last().map(|t| t.id));
        let was_active = self.active_tab_id == Some(id);

        let removed = self.tabs.remove(index);
        self.record_closed(&removed);
        if let Some(wv) = self.webviews.remove(&removed.id) {
            let _ = wv.set_visible(false);
        }
        self.webview_loaded_url.remove(&removed.id);
        self.allow_internal_html.remove(&removed.id);
        self.selected_tab_ids.remove(&removed.id);

        if let Some(group_id) = removed.group {
            self.drop_empty_group(group_id);
        }
        if was_active && let Some(next) = next_active {
            self.activate_tab(next);
        }
    }

    // ---- window detach ---------------------------------------------------

    /// Copies `ids` out of the strip for a brand-new window.
    ///
    /// Returns the tabs in strip order and removes them from this window. The
    /// native `WebView`s are dropped rather than moved: the new window owns a
    /// separate GPU/event pipeline, so the page reloads there from its URL.
    ///
    /// Unlike [`Self::close_tab`] the last tab is *not* recycled into the
    /// welcome page - an emptied window is closed by the caller instead.
    pub fn take_tabs_for_window(&mut self, ids: &[u64]) -> Vec<Tab> {
        let mut taken = Vec::new();
        let mut kept = Vec::with_capacity(self.tabs.len());
        for tab in std::mem::take(&mut self.tabs) {
            if ids.contains(&tab.id) {
                if let Some(wv) = self.webviews.remove(&tab.id) {
                    let _ = wv.set_visible(false);
                }
                self.webview_loaded_url.remove(&tab.id);
                self.allow_internal_html.remove(&tab.id);
                self.selected_tab_ids.remove(&tab.id);
                taken.push(tab);
            } else {
                kept.push(tab);
            }
        }
        self.tabs = kept;
        self.normalize_order();

        // The source window must keep showing a page. Chrome activates a
        // neighbour of the detached tab; the right one is next to it because
        // the strip order was preserved, so the first survivor is as good.
        if let Some(active) = self.active_tab_id
            && !self.tabs.iter().any(|tab| tab.id == active)
        {
            self.active_tab_id = None;
            if let Some(next) = self.tabs.first().map(|tab| tab.id) {
                self.activate_tab(next);
            }
        }

        taken
    }

    /// Fills a freshly created window's strip with detached tabs.
    ///
    /// The tabs keep their URL, title and back/forward history; ids are
    /// renumbered because ids only have to be unique inside one window.
    /// `groups` are the source window's groups that still have members here.
    pub fn replace_strip_with(&mut self, mut tabs: Vec<Tab>, groups: Vec<TabGroup>) {
        for tab in std::mem::take(&mut self.tabs) {
            if let Some(wv) = self.webviews.remove(&tab.id) {
                let _ = wv.set_visible(false);
            }
            self.webview_loaded_url.remove(&tab.id);
            self.allow_internal_html.remove(&tab.id);
        }
        self.selected_tab_ids.clear();
        self.group_rename = None;

        self.groups.clear();
        self.next_group_id = 1;
        let mut remap: Vec<(u64, u64)> = Vec::new();
        for mut group in groups {
            let old_id = group.id;
            group.id = self.next_group_id;
            self.next_group_id += 1;
            remap.push((old_id, group.id));
            self.groups.push(group);
        }

        for tab in &mut tabs {
            tab.id = self.next_tab_id;
            self.next_tab_id += 1;
            tab.discarded = false;
            // Group ids are window-local, so follow the remap.
            tab.group = tab.group.and_then(|old| {
                remap
                    .iter()
                    .find(|(from, _)| *from == old)
                    .map(|(_, to)| *to)
            });
        }
        self.tabs = tabs;
        self.normalize_order();

        self.active_tab_id = None;
        if let Some(first) = self.tabs.first().map(|tab| tab.id) {
            self.activate_tab(first);
        }
        self.dirty = true;
    }

    /// The groups the given tabs belong to, in strip order.
    pub fn groups_of(&self, ids: &[u64]) -> Vec<TabGroup> {
        let mut groups: Vec<TabGroup> = Vec::new();
        for id in ids {
            if let Some(group_id) = self.tab(*id).and_then(|tab| tab.group)
                && !groups.iter().any(|group| group.id == group_id)
                && let Some(group) = self.group(group_id)
            {
                groups.push(group.clone());
            }
        }
        groups
    }

    /// Re-attaches tabs whose detach could not be completed (no window could
    /// be created), so a failed tear-off never loses them.
    pub fn restore_tabs(&mut self, mut tabs: Vec<Tab>, groups: Vec<TabGroup>) {
        for group in groups {
            if self.group(group.id).is_none() {
                self.groups.push(group);
            }
        }
        self.tabs.append(&mut tabs);
        self.normalize_order();
        if let Some(last) = self.tabs.last().map(|tab| tab.id) {
            self.activate_tab(last);
        }
        self.dirty = true;
    }

    /// Bulk close of the current selection. Pinned tabs are protected.
    pub fn close_selected_tabs(&mut self) {
        let ids: Vec<u64> = self
            .tabs
            .iter()
            .filter(|t| self.selected_tab_ids.contains(&t.id) && !t.pinned)
            .map(|t| t.id)
            .collect();
        for id in ids {
            self.close_tab(id);
        }
    }

    pub fn close_other_tabs(&mut self, keep: u64) {
        let ids: Vec<u64> = self
            .tabs
            .iter()
            .filter(|t| t.id != keep && !t.pinned)
            .map(|t| t.id)
            .collect();
        for id in ids {
            self.close_tab(id);
        }
    }

    pub fn close_tabs_to_right(&mut self, id: u64) {
        let Some(index) = self.tab_index(id) else {
            return;
        };
        let ids: Vec<u64> = self.tabs[index + 1..]
            .iter()
            .filter(|t| !t.pinned)
            .map(|t| t.id)
            .collect();
        for tab_id in ids {
            self.close_tab(tab_id);
        }
    }

    /// Closes every tab left of `id`. Pinned tabs are protected, like every
    /// other bulk close.
    pub fn close_tabs_to_left(&mut self, id: u64) {
        let Some(index) = self.tab_index(id) else {
            return;
        };
        let ids: Vec<u64> = self.tabs[..index]
            .iter()
            .filter(|t| !t.pinned)
            .map(|t| t.id)
            .collect();
        for tab_id in ids {
            self.close_tab(tab_id);
        }
    }

    /// Restores the most recently closed tab, re-attaching its group when the
    /// group still exists.
    pub fn reopen_closed_tab(&mut self) {
        let Some(closed) = self.closed_tabs.pop() else {
            self.log(ConsoleLevel::Warn, "No recently closed tab to reopen.");
            return;
        };
        self.open_closed(closed);
    }

    /// Reopens one entry of the recently-closed stack.
    fn open_closed(&mut self, closed: ClosedTab) {
        self.create_tab(&closed.url);
        if let (Some(group_name), Some(tab_id)) = (closed.group, self.active_tab_id)
            && let Some(group_id) = self
                .groups
                .iter()
                .find(|g| g.name == group_name)
                .map(|g| g.id)
        {
            self.add_to_group(&[tab_id], group_id);
        }
        self.log(
            ConsoleLevel::Info,
            format!("Reopened closed tab: {}", closed.url),
        );
    }

    fn record_closed(&mut self, tab: &Tab) {
        let group = tab
            .group
            .and_then(|gid| self.groups.iter().find(|g| g.id == gid))
            .map(|g| g.name.clone());
        self.closed_tabs.push(ClosedTab {
            url: tab.url.clone(),
            title: tab.title.clone(),
            group,
        });
        if self.closed_tabs.len() > 25 {
            self.closed_tabs.remove(0);
        }
    }

    // ---- pinning --------------------------------------------------------

    pub fn toggle_pin(&mut self, id: u64) {
        let Some(pinned) = self.tab(id).map(|t| t.pinned) else {
            return;
        };
        self.set_pinned(id, !pinned);
    }

    pub fn set_pinned(&mut self, id: u64, pinned: bool) {
        if let Some(tab) = self.tab_mut(id) {
            tab.pinned = pinned;
            if pinned {
                // Pinned tabs are always ungrouped, like Chrome.
                tab.group = None;
            }
        }
        self.normalize_order();
    }

    // ---- ordering and drag & drop ---------------------------------------

    /// Moves `ids` (kept in strip order) so that they sit before strip index
    /// `to_index`. Pinned tabs stay in the pinned prefix and normal tabs stay
    /// after it.
    pub fn move_tabs(&mut self, ids: &[u64], to_index: usize) {
        let mut moved: Vec<u64> = self
            .tabs
            .iter()
            .filter(|t| ids.contains(&t.id))
            .map(|t| t.id)
            .collect();
        if moved.is_empty() {
            return;
        }

        let mut order: Vec<u64> = self.tabs.iter().map(|t| t.id).collect();
        let mut insert_at = to_index.min(order.len());
        insert_at -= order[..insert_at]
            .iter()
            .filter(|id| moved.contains(id))
            .count();
        order.retain(|id| !moved.contains(id));

        let moving_pinned = self.tab(moved[0]).is_some_and(|t| t.pinned);
        let pinned_count = order
            .iter()
            .filter(|id| self.tab(**id).is_some_and(|t| t.pinned))
            .count();
        let insert_at = if moving_pinned {
            insert_at.min(pinned_count)
        } else {
            insert_at.max(pinned_count)
        };

        for (offset, id) in moved.drain(..).enumerate() {
            order.insert(insert_at + offset, id);
        }
        self.apply_order(&order);
        self.normalize_order();
    }

    /// Replaces the strip order without ever losing a tab.
    fn apply_order(&mut self, order: &[u64]) {
        let mut by_id: std::collections::BTreeMap<u64, Tab> =
            self.tabs.drain(..).map(|t| (t.id, t)).collect();
        self.tabs = order.iter().filter_map(|id| by_id.remove(id)).collect();
        self.tabs.extend(by_id.into_values());
    }

    /// Restores the two structural invariants of the strip: pinned tabs form a
    /// prefix, and members of a group are contiguous.
    fn normalize_order(&mut self) {
        let mut order: Vec<u64> = self.tabs.iter().filter(|t| t.pinned).map(|t| t.id).collect();
        order.extend(self.tabs.iter().filter(|t| !t.pinned).map(|t| t.id));

        let mut result: Vec<u64> = Vec::with_capacity(order.len());
        let mut emitted: Vec<u64> = Vec::new();
        for id in &order {
            let Some(tab) = self.tab(*id) else {
                continue;
            };
            match tab.group {
                Some(group_id) if !emitted.contains(&group_id) => {
                    emitted.push(group_id);
                    let members: Vec<u64> = self
                        .tabs
                        .iter()
                        .filter(|t| t.group == Some(group_id) && !t.pinned)
                        .map(|t| t.id)
                        .collect();
                    result.extend(members);
                }
                Some(_) => {}
                None => result.push(*id),
            }
        }
        // Defensive: never drop a tab because of a stale id.
        for tab in &self.tabs {
            if !result.contains(&tab.id) {
                result.push(tab.id);
            }
        }
        self.apply_order(&result);
    }

    // ---- tab groups -----------------------------------------------------

    pub fn create_group(&mut self, ids: &[u64], color: GroupColor) -> Option<u64> {
        let members: Vec<u64> = self
            .tabs
            .iter()
            .filter(|t| ids.contains(&t.id) && !t.pinned)
            .map(|t| t.id)
            .collect();
        if members.is_empty() {
            return None;
        }

        let group_id = self.next_group_id;
        self.next_group_id += 1;
        let name = format!("Group {}", self.groups.len() + 1);
        self.groups.push(TabGroup {
            id: group_id,
            name,
            color,
            collapsed: false,
        });
        for id in members {
            if let Some(tab) = self.tab_mut(id) {
                tab.group = Some(group_id);
            }
        }
        self.normalize_order();
        self.log(
            ConsoleLevel::Info,
            format!("Created tab group #{group_id}"),
        );
        Some(group_id)
    }

    pub fn add_to_group(&mut self, ids: &[u64], group_id: u64) {
        if self.group(group_id).is_none() {
            return;
        }
        for id in ids {
            if let Some(tab) = self.tab_mut(*id)
                && !tab.pinned
            {
                tab.group = Some(group_id);
            }
        }
        self.normalize_order();
    }

    pub fn remove_from_group(&mut self, ids: &[u64]) {
        let mut touched = Vec::new();
        for id in ids {
            if let Some(tab) = self.tab_mut(*id)
                && let Some(group_id) = tab.group.take()
            {
                touched.push(group_id);
            }
        }
        for group_id in touched {
            self.drop_empty_group(group_id);
        }
        self.normalize_order();
    }

    pub fn group_tabs(&self, group_id: u64) -> Vec<u64> {
        self.tabs
            .iter()
            .filter(|t| t.group == Some(group_id))
            .map(|t| t.id)
            .collect()
    }

    pub fn toggle_group_collapsed(&mut self, group_id: u64) {
        if let Some(group) = self.groups.iter_mut().find(|g| g.id == group_id) {
            group.collapsed = !group.collapsed;
        }
    }

    pub fn rename_group(&mut self, group_id: u64, name: String) {
        let name = name.trim().to_string();
        if name.is_empty() {
            return;
        }
        if let Some(group) = self.groups.iter_mut().find(|g| g.id == group_id) {
            group.name = name;
        }
    }

    pub fn set_group_color(&mut self, group_id: u64, color: GroupColor) {
        if let Some(group) = self.groups.iter_mut().find(|g| g.id == group_id) {
            group.color = color;
        }
    }

    pub fn close_group(&mut self, group_id: u64) {
        let ids = self.group_tabs(group_id);
        for id in ids {
            self.close_tab(id);
        }
        self.drop_empty_group(group_id);
    }

    pub fn ungroup(&mut self, group_id: u64) {
        let ids = self.group_tabs(group_id);
        for id in ids {
            if let Some(tab) = self.tab_mut(id) {
                tab.group = None;
            }
        }
        self.groups.retain(|g| g.id != group_id);
        if self.group_rename.as_ref().is_some_and(|r| r.group_id == group_id) {
            self.group_rename = None;
        }
        self.normalize_order();
    }

    fn drop_empty_group(&mut self, group_id: u64) {
        if !self.tabs.iter().any(|t| t.group == Some(group_id)) {
            self.groups.retain(|g| g.id != group_id);
            if self.group_rename.as_ref().is_some_and(|r| r.group_id == group_id) {
                self.group_rename = None;
            }
        }
    }

    // ---- tab navigation -------------------------------------------------

    /// Ctrl+Tab / Ctrl+Shift+Tab: cycles through the strip with wraparound.
    pub fn activate_relative(&mut self, delta: isize) {
        if self.tabs.is_empty() {
            return;
        }
        let len = self.tabs.len() as isize;
        let current = self.active_tab_index().unwrap_or(0) as isize;
        let next = (current + delta).rem_euclid(len) as usize;
        let id = self.tabs[next].id;
        self.activate_tab(id);
    }

    pub fn activate_tab_at(&mut self, index: usize) {
        if let Some(tab) = self.tabs.get(index) {
            let id = tab.id;
            self.activate_tab(id);
        }
    }

    /// Reloads a tab, activating it first so the WebView sync picks it up.
    pub fn reload_tab(&mut self, id: u64) {
        if self.active_tab_id != Some(id) {
            self.activate_tab(id);
        }
        self.reload_active_tab();
    }

    pub fn navigate_active_tab(&mut self, raw_input: String) {
        if let Some(tab) = self.active_tab_mut() {
            tab.navigate_to(raw_input);
            let tab_id = tab.id;
            let resolved_url = tab.url.clone();
            self.apply_navigation_for_tab(tab_id, resolved_url);
        }
    }

    fn apply_navigation_for_tab(&mut self, tab_id: u64, target_url: String) {
        let decision = {
            let pol = self.security_policy.read().unwrap();
            pol.validate_navigation(&target_url)
        };

        match decision {
            NavigationDecision::Block(reason) => {
                if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) {
                    tab.set_error(reason.clone());
                }
                if let Some(wv) = self.webviews.get(&tab_id) {
                    let _ = wv.set_visible(false);
                }
                self.log(
                    ConsoleLevel::Error,
                    format!("🛡 Security Blocked [{}]: {}", target_url, reason),
                );
            }
            NavigationDecision::UpgradeToHttps(https_url) => {
                self.log(
                    ConsoleLevel::Warn,
                    format!("🛡 HTTPS-Only: Upgraded {} -> {}", target_url, https_url),
                );
                if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) {
                    tab.url = https_url.clone();
                    tab.omnibox_input = https_url.clone();
                }
                self.load_into_webview_and_inspector(tab_id, https_url);
            }
            NavigationDecision::Allow(allowed_url) => {
                self.log(ConsoleLevel::Net, format!("Navigating to: {}", allowed_url));
                self.load_into_webview_and_inspector(tab_id, allowed_url);
            }
        }
    }

    fn load_into_webview_and_inspector(&mut self, tab_id: u64, url: String) {
        let req_id = if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) {
            tab.error = None;
            tab.request_id
        } else {
            return;
        };

        if let Some(wv) = self.webviews.get(&tab_id) {
            if url == "oxide://welcome" {
                if let Some(flag) = self.allow_internal_html.get(&tab_id) {
                    flag.store(true, Ordering::SeqCst);
                }
                let _ = wv.load_html(welcome_page_html());
            } else {
                let _ = wv.load_url(&url);
            }
            self.webview_loaded_url.insert(tab_id, url.clone());
        }

        if self.show_devtools {
            self.spawn_fetch(tab_id, req_id, url);
        }
    }

    pub fn go_back(&mut self) {
        if let Some(tab) = self.active_tab_mut() {
            let tab_id = tab.id;
            if let Some(url) = tab.go_back() {
                self.log(ConsoleLevel::Net, format!("Navigating Back to: {}", url));
                self.apply_navigation_for_tab(tab_id, url);
            }
        }
    }

    pub fn go_forward(&mut self) {
        if let Some(tab) = self.active_tab_mut() {
            let tab_id = tab.id;
            if let Some(url) = tab.go_forward() {
                self.log(ConsoleLevel::Net, format!("Navigating Forward to: {}", url));
                self.apply_navigation_for_tab(tab_id, url);
            }
        }
    }

    pub fn reload_active_tab(&mut self) {
        if let Some(tab) = self.active_tab_mut() {
            let tab_id = tab.id;
            let url = tab.reload();
            self.log(ConsoleLevel::Net, format!("Reloading: {}", url));
            self.apply_navigation_for_tab(tab_id, url);
        }
    }

    pub fn stop_active_tab(&mut self) {
        let stopped_url = if let Some(tab) = self.active_tab_mut() {
            tab.stop();
            Some(tab.url.clone())
        } else {
            None
        };
        if let Some(url) = stopped_url {
            self.log(ConsoleLevel::Warn, format!("Stopped loading: {}", url));
        }
    }

    /// Creates missing child WebViews and synchronizes bounds & visibility with the egui viewport.
    pub fn ensure_and_sync_webviews(&mut self, window: &Window) {
        let x = self.viewport_rect.min.x.max(0.0) as f64;
        let y = self.viewport_rect.min.y.max(0.0) as f64;
        let w = self.viewport_rect.width().max(1.0) as f64;
        let h = self.viewport_rect.height().max(1.0) as f64;

        let bounds = WryRect {
            position: Position::Logical(LogicalPosition::new(x, y)),
            size: Size::Logical(LogicalSize::new(w, h)),
        };

        let active_info = self
            .active_tab()
            .map(|t| (t.id, t.url.clone(), t.error.is_some()));

        if let Some((active_id, active_url, has_error)) = active_info.clone()
            && !has_error
            && !self.webviews.contains_key(&active_id)
        {
            let tx_nav = self.wv_event_tx.clone();
            let tx_title = self.wv_event_tx.clone();
            let tx_popup = self.wv_event_tx.clone();
            let policy_nav = self.security_policy.clone();
            let policy_popup = self.security_policy.clone();
            let allow_html_flag = Arc::new(AtomicBool::new(active_url == "oxide://welcome"));
            let allow_html_nav = allow_html_flag.clone();
            let tx_ipc = self.wv_event_tx.clone();
            // A muted site must be silent from its first frame, so the state
            // is baked into the probe instead of being applied afterwards.
            let probe_script = audio_probe_script(
                self.tab(active_id)
                    .is_some_and(|tab| self.is_tab_muted(tab)),
            );

            let mut builder = WebViewBuilder::new()
                .with_bounds(bounds)
                .with_initialization_script_for_main_only(&probe_script, true)
                .with_user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Oxide/0.2.0 Chrome/131.0.0.0 Safari/537.36")
                .with_navigation_handler(move |nav_url: String| -> bool {
                    if nav_url == "about:blank" {
                        return true;
                    }
                    let decision = {
                        let pol = policy_nav.read().unwrap();
                        pol.validate_webview_navigation(&nav_url, &allow_html_nav)
                    };
                    match decision {
                        NavigationDecision::Allow(allowed) => {
                            let _ = tx_nav.send(WebViewEvent::UrlChanged {
                                tab_id: active_id,
                                url: allowed,
                            });
                            true
                        }
                        NavigationDecision::UpgradeToHttps(https_url) => {
                            let _ = tx_nav.send(WebViewEvent::UpgradeRedirect {
                                tab_id: active_id,
                                https_url,
                            });
                            false
                        }
                        NavigationDecision::Block(reason) => {
                            let _ = tx_nav.send(WebViewEvent::SecurityAlert {
                                tab_id: active_id,
                                url: nav_url,
                                reason,
                            });
                            false
                        }
                    }
                })
                .with_document_title_changed_handler(move |new_title: String| {
                    if !new_title.trim().is_empty() {
                        let _ = tx_title.send(WebViewEvent::TitleChanged {
                            tab_id: active_id,
                            title: new_title,
                        });
                    }
                })
                .with_new_window_req_handler(move |popup_url: String| -> bool {
                    let decision = {
                        let pol = policy_popup.read().unwrap();
                        pol.validate_navigation(&popup_url)
                    };
                    match decision {
                        NavigationDecision::Allow(allowed)
                        | NavigationDecision::UpgradeToHttps(allowed) => {
                            let _ = tx_popup.send(WebViewEvent::NewTabRequested { url: allowed });
                        }
                        NavigationDecision::Block(reason) => {
                            let _ = tx_popup.send(WebViewEvent::SecurityAlert {
                                tab_id: active_id,
                                url: popup_url,
                                reason,
                            });
                        }
                    }
                    false
                })
                .with_ipc_handler(move |request| {
                    // The media probe posts `oxide-audio:1` / `oxide-audio:0`
                    // when playback starts and stops.
                    let body = request.body();
                    if let Some(flag) = body.strip_prefix("oxide-audio:") {
                        let _ = tx_ipc.send(WebViewEvent::AudioState {
                            tab_id: active_id,
                            playing: flag == "1",
                        });
                    }
                });

            builder = if active_url == "oxide://welcome" {
                builder.with_html(welcome_page_html())
            } else {
                builder.with_url(&active_url)
            };

            match builder.build_as_child(window) {
                Ok(wv) => {
                    self.webview_loaded_url.insert(active_id, active_url);
                    self.allow_internal_html.insert(active_id, allow_html_flag);
                    self.webviews.insert(active_id, wv);
                    // The page is being (re)loaded, including when a
                    // discarded tab is selected again: show the spinner and
                    // stop advertising the tab as sleeping.
                    if let Some(tab) = self.tab_mut(active_id) {
                        tab.loading = true;
                        tab.discarded = false;
                    }
                }
                Err(e) => {
                    self.log(
                        ConsoleLevel::Error,
                        format!("Failed to initialize Wry WebView: {e}"),
                    );
                }
            }
        }

        // Sync visibility and bounds across all tab WebViews
        let active_id_opt = active_info.as_ref().map(|(id, _, _)| *id);
        let active_has_error = active_info.as_ref().is_some_and(|(_, _, err)| *err);
        // Drop the least recently used hidden pages beyond the budget.
        self.enforce_webview_budget(active_id_opt);
        // The tab-search overlay is drawn by egui, so it would be buried
        // under the native page. Rather than blank the page, freeze it into
        // a texture that egui paints in its place (see `crate::snapshot`),
        // then hide the WebView. Context menus do *not* come through here:
        // they live in their own popup window above the page, so the page
        // stays visible and live while a menu is open.
        let overlay_open = self.tab_search_open;
        let page_is_covered = active_has_error || overlay_open;

        // Capture the page on the first covered frame, while the WebView is
        // still visible. The copy is painted from the *next* frame, so the
        // real page is left up for this one to avoid a one-frame blank.
        let mut hide_page = page_is_covered;
        if overlay_open && self.page_snapshot.is_none() {
            #[allow(unused_mut)]
            let mut captured = false;
            #[cfg(windows)]
            if let Some(hwnd) = window_hwnd(window)
                && let Some(image) = crate::snapshot::capture_page(hwnd)
            {
                self.page_snapshot = Some(crate::snapshot::PageSnapshot {
                    image,
                    texture: None,
                });
                captured = true;
            }
            if captured {
                hide_page = false;
            }
        }
        if !overlay_open {
            self.page_snapshot = None;
        }

        // `set_visible` alone has been unreliable for a child `WebView2` on
        // Windows, so the covered page is also parked outside the window. The
        // size is left untouched: resizing to zero would reflow the document
        // and make the page flash back at a different layout when the menu
        // closes.
        let parked = WryRect {
            position: Position::Logical(LogicalPosition::new(-20_000.0, -20_000.0)),
            size: Size::Logical(LogicalSize::new(w, h)),
        };

        for (&tab_id, wv) in &self.webviews {
            if Some(tab_id) == active_id_opt && !hide_page {
                let _ = wv.set_bounds(bounds);
                let _ = wv.set_visible(true);
            } else if Some(tab_id) == active_id_opt {
                let _ = wv.set_bounds(parked);
                let _ = wv.set_visible(false);
            } else {
                let _ = wv.set_visible(false);
            }
        }
    }

    /// Drops the WebViews of the least recently used *hidden* tabs once the
    /// live-page budget is exceeded, marking those tabs discarded.
    ///
    /// Discarding is safe because `ensure_and_sync_webviews` recreates any
    /// missing page the next time its tab becomes active.
    fn enforce_webview_budget(&mut self, active_id: Option<u64>) {
        while self.webviews.len() > LIVE_WEBVIEW_LIMIT {
            let victim = self
                .tabs
                .iter()
                .filter(|tab| Some(tab.id) != active_id && self.webviews.contains_key(&tab.id))
                .min_by_key(|tab| tab.last_activated)
                .map(|tab| tab.id);
            let Some(victim) = victim else {
                break;
            };
            if let Some(wv) = self.webviews.remove(&victim) {
                let _ = wv.set_visible(false);
            }
            self.webview_loaded_url.remove(&victim);
            if let Some(tab) = self.tab_mut(victim) {
                tab.discarded = true;
                // A page that no longer exists cannot be producing audio.
                tab.audio_playing = false;
            }
        }
    }

    /// Processes events emitted by Wry WebView callbacks.
    pub fn poll_webview_events(&mut self) {
        let mut pending_events = Vec::new();
        while let Ok(ev) = self.wv_event_rx.try_recv() {
            pending_events.push(ev);
        }
        if !pending_events.is_empty() {
            // The strip and the page both mirror this state, and the window
            // only redraws on request.
            self.dirty = true;
        }

        for ev in pending_events {
            match ev {
                WebViewEvent::TitleChanged { tab_id, title } => {
                    if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) {
                        tab.title = title;
                        tab.loading = false;
                    }
                }
                WebViewEvent::UrlChanged { tab_id, url } => {
                    let should_fetch_inspector =
                        self.webview_loaded_url.get(&tab_id) != Some(&url);
                    self.webview_loaded_url.insert(tab_id, url.clone());

                    let req_id_opt =
                        if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) {
                            tab.sync_webview_url(url.clone());
                            Some(tab.request_id)
                        } else {
                            None
                        };

                    if self.show_devtools && should_fetch_inspector && let Some(req_id) = req_id_opt {
                        self.spawn_fetch(tab_id, req_id, url);
                    }
                }
                WebViewEvent::UpgradeRedirect { tab_id, https_url } => {
                    self.log(
                        ConsoleLevel::Warn,
                        format!("🛡 HTTPS-Only: Auto-upgraded navigation to {https_url}"),
                    );
                    if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) {
                        tab.sync_webview_url(https_url.clone());
                    }
                    self.load_into_webview_and_inspector(tab_id, https_url);
                }
                WebViewEvent::NewTabRequested { url } => {
                    self.log(
                        ConsoleLevel::Info,
                        format!("Popup / new window captured into new tab: {url}"),
                    );
                    self.create_tab(&url);
                }
                WebViewEvent::SecurityAlert {
                    tab_id,
                    url,
                    reason,
                } => {
                    self.log(
                        ConsoleLevel::Error,
                        format!("🛡 Security Blocked [{url}]: {reason}"),
                    );
                    if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) {
                        tab.set_error(reason);
                    }
                }
                WebViewEvent::AudioState { tab_id, playing } => {
                    if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) {
                        tab.audio_playing = playing;
                    }
                }
            }
        }
    }

    fn spawn_fetch(&self, tab_id: u64, request_id: u64, url: String) {
        let net_engine = self.net_engine.clone();
        let tx = self.result_tx.clone();

        self.tokio_handle.spawn(async move {
            let result = net_engine.fetch(&url).await;
            let _ = tx.send(TabFetchResult {
                tab_id,
                request_id,
                result,
            });
        });
    }

    /// Process any completed background network requests (for DevTools DOM/Network inspector)
    pub fn poll_network_events(&mut self) {
        let mut logs = Vec::new();

        while let Ok(msg) = self.result_rx.try_recv() {
            if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == msg.tab_id)
                && tab.request_id == msg.request_id
            {
                match msg.result {
                    Ok(resp) => {
                        logs.push((
                            ConsoleLevel::Net,
                            format!(
                                "Inspector Loaded [{}] {} ({} bytes in {}ms)",
                                resp.status,
                                resp.url,
                                resp.body.len(),
                                resp.duration_ms
                            ),
                        ));
                        tab.set_response(resp);
                    }
                    Err(err) => {
                        let err_str = err.to_string();
                        logs.push((
                            ConsoleLevel::Warn,
                            format!("Inspector fetch note for {}: {}", tab.url, err_str),
                        ));
                        tab.loading = false;
                    }
                }
            }
        }

        for (level, msg) in logs {
            self.log(level, msg);
        }
    }

    /// Triggers an on-demand inspector fetch for the active tab when DevTools is opened.
    pub fn fetch_active_tab_for_inspector(&self) {
        if let Some(tab) = self.active_tab()
            && !tab.url.is_empty()
        {
            self.spawn_fetch(tab.id, tab.request_id, tab.url.clone());
        }
    }
}

fn chrono_like_now() -> String {
    use std::time::SystemTime;
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();
    let hours = (secs / 3600) % 24;
    let mins = (secs / 60) % 60;
    let s = secs % 60;
    let millis = now.subsec_millis();
    format!("{:02}:{:02}:{:02}.{:03}", hours, mins, s, millis)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn handle() -> tokio::runtime::Handle {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        // Leak: the handle has to outlive the state for the test's duration.
        Box::leak(Box::new(rt)).handle().clone()
    }

    fn state() -> BrowserState {
        BrowserState::new(handle())
    }

    #[test]
    fn site_key_drops_the_www_prefix_and_the_path() {
        assert_eq!(
            site_key("https://www.Example.com/a/b?c=1").as_deref(),
            Some("example.com")
        );
        assert_eq!(site_key("http://example.com").as_deref(), Some("example.com"));
        // Internal schemes have no host, so they can never be muted per site.
        assert_eq!(site_key("oxide://welcome"), None);
    }

    #[test]
    fn muting_a_site_mutes_every_tab_on_it_and_survives_a_new_tab() {
        let mut s = state();
        s.create_tab("https://example.com/a");
        s.create_tab("https://example.com/b");
        s.create_tab("https://other.example/c");

        // `new()` already created the start page, so the created tabs are
        // found by URL instead of by index.
        let find = |s: &BrowserState, needle: &str| {
            s.tabs
                .iter()
                .find(|t| t.url.contains(needle))
                .map(|t| t.id)
                .expect("tab exists")
        };
        let first = find(&s, "example.com/a");
        let second = find(&s, "example.com/b");
        let other = find(&s, "other.example/c");

        s.toggle_mute_site(first);

        assert!(s.is_tab_muted(s.tab(first).unwrap()));
        assert!(
            s.is_tab_muted(s.tab(second).unwrap()),
            "same site, muted too"
        );
        assert!(
            !s.is_tab_muted(s.tab(other).unwrap()),
            "a different site is untouched"
        );

        // The mute is per site, so a tab opened later starts muted.
        s.create_tab("https://example.com/later");
        assert!(s.is_tab_muted(s.tabs.last().unwrap()));

        // Toggling again clears it everywhere.
        s.toggle_mute_site(first);
        assert!(!s.is_tab_muted(s.tab(first).unwrap()));
    }

    #[test]
    fn muting_one_tab_leaves_its_siblings_audible() {
        let mut s = state();
        s.create_tab("https://example.com/a");
        let target = s.tabs[0].id;
        s.toggle_mute_tab(target);
        assert!(s.is_tab_muted(&s.tabs[0]));
        assert!(!s.is_tab_muted(&s.tabs[1]));
    }

    #[test]
    fn the_audio_flag_is_not_part_of_the_mute_state() {
        let mut s = state();
        s.tabs[0].audio_playing = true;
        assert!(s.any_audio_playing());
        assert!(!s.is_tab_muted(&s.tabs[0]), "playing is not muted");
    }

    #[test]
    fn bookmarking_is_a_toggle_and_a_duplicate_url_is_ignored() {
        let mut s = state();
        s.add_bookmark("https://example.com/x", "X");
        s.add_bookmark("https://example.com/x", "X again");
        assert_eq!(s.bookmarks.len(), 1, "the second add is a no-op");

        s.toggle_bookmark("https://example.com/x", "X");
        assert!(!s.is_bookmarked("https://example.com/x"));
        s.toggle_bookmark("https://example.com/x", "X");
        assert!(s.is_bookmarked("https://example.com/x"));
    }

    #[test]
    fn bookmark_all_collects_one_folder_not_a_bar_full_of_chips() {
        let mut s = state();
        s.create_tab("https://example.com/a");
        s.create_tab("https://example.com/b");

        s.bookmark_all_tabs();
        let folders: Vec<&str> = s
            .bookmarks
            .iter()
            .filter_map(|b| b.folder.as_deref())
            .collect();
        assert_eq!(folders.len(), s.tabs.len());
        assert!(folders.iter().all(|f| *f == "All tabs"));

        // Running it twice must not duplicate the entries.
        let before = s.bookmarks.len();
        s.bookmark_all_tabs();
        assert_eq!(s.bookmarks.len(), before);

        s.remove_bookmark_folder("All tabs");
        assert!(s.bookmarks.is_empty());
    }

    #[test]
    fn saving_a_group_records_its_urls_and_membership_order() {
        let mut s = state();
        s.create_tab("https://example.com/a");
        s.create_tab("https://example.com/b");
        let ids: Vec<u64> = s.tabs.iter().map(|t| t.id).collect();
        let group = s
            .create_group(&ids[1..], GroupColor::Green)
            .expect("group is created");
        s.rename_group(group, "Work".to_string());

        assert!(s.save_group(group));
        assert_eq!(s.saved_groups.len(), 1);
        assert_eq!(s.saved_groups[0].name, "Work");
        assert_eq!(s.saved_groups[0].urls.len(), 2);

        s.delete_saved_group(0);
        assert!(s.saved_groups.is_empty());
    }

    #[test]
    fn reopening_a_saved_group_restores_its_tabs_as_a_group() {
        let mut s = state();
        s.create_tab("https://example.com/a");
        s.create_tab("https://example.com/b");
        let ids: Vec<u64> = s.tabs.iter().map(|t| t.id).collect();
        let group = s
            .create_group(&ids[1..], GroupColor::Purple)
            .expect("group is created");
        s.save_group(group);
        let original_group_ids = s.group_tabs(group);
        s.close_group(group);

        s.reopen_saved_group(0);
        let restored = s.groups.last().expect("the group is back");
        assert_eq!(restored.color, GroupColor::Purple);
        assert_eq!(s.group_tabs(restored.id).len(), original_group_ids.len());
    }

    #[test]
    fn recently_closed_is_listed_newest_first_and_reopens_by_index() {
        let mut s = state();
        s.create_tab("https://example.com/one");
        s.create_tab("https://example.com/two");
        let one = s.tabs[1].id;
        let two = s.tabs[2].id;
        s.close_tab(one);
        s.close_tab(two);

        let listed = s.recent_closed(8);
        assert_eq!(listed[0].0, 0);
        assert_eq!(listed.len(), 2);

        // Index 0 is the newest close, which is the *last* entry pushed.
        s.reopen_closed_at(0);
        assert!(s.tabs.iter().any(|t| t.url.contains("two")));
    }

    #[test]
    fn a_failed_favicon_fetch_is_remembered_so_it_is_not_retried() {
        let mut s = state();
        s.favicons
            .insert("example.com".to_string(), FaviconState::Failed);
        assert!(s.favicon_for("https://example.com").is_none());
        assert!(
            s.favicons.contains_key("example.com"),
            "the failure stays cached, so refresh_favicons skips the site"
        );
    }

    #[test]
    fn tearing_off_every_tab_leaves_the_source_window_empty() {
        let mut s = state();
        s.create_tab("https://example.com/a");
        let ids: Vec<u64> = s.tabs.iter().map(|t| t.id).collect();

        let taken = s.take_tabs_for_window(&ids);

        assert_eq!(taken.len(), ids.len());
        assert!(
            s.tabs.is_empty(),
            "the last tab must not be recycled into the welcome page: the app closes the window"
        );
    }

    #[test]
    fn a_detached_tab_keeps_its_url_and_history_in_the_new_window() {
        let mut source = state();
        source.create_tab("https://example.com/a");
        let id = source.tabs.last().unwrap().id;
        let groups = source.groups_of(&[id]);

        let taken = source.take_tabs_for_window(&[id]);
        assert!(source.tab(id).is_none());

        let mut target = BrowserState::empty(handle());
        target.replace_strip_with(taken, groups);

        assert_eq!(target.tabs.len(), 1);
        assert_eq!(target.tabs[0].url, "https://example.com/a");
        assert_eq!(target.tabs[0].history.last().unwrap(), "https://example.com/a");
        assert_eq!(target.active_tab_id, Some(target.tabs[0].id));
    }

    #[test]
    fn a_detached_group_follows_its_tabs_into_the_new_window() {
        let mut source = state();
        let first = source.tabs[0].id;
        source.create_tab("https://example.com/a");
        let second = source.tabs[1].id;
        let group = source
            .create_group(&[first, second], GroupColor::Blue)
            .unwrap();
        assert!(source.group(group).is_some());

        let groups = source.groups_of(&[first]);
        let taken = source.take_tabs_for_window(&[first]);

        let mut target = BrowserState::empty(handle());
        target.replace_strip_with(taken, groups);

        let carried_id = target.tabs[0].group.expect("the group came along");
        let carried = target.group(carried_id).expect("the group itself came along");
        assert_eq!(carried.color, GroupColor::Blue);
    }
}
