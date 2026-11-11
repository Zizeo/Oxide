use oxide_dom::{Document, NodeId};
use oxide_js::JsRuntime;
use oxide_layout::LayoutTree;
use oxide_net::HttpResponse;
use oxide_style::StyleTree;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct Tab {
    pub id: u64,
    pub title: String,
    pub url: String,
    pub omnibox_input: String,
    pub history: Vec<String>,
    pub history_index: usize,
    pub loading: bool,
    pub response: Option<HttpResponse>,
    pub document: Option<Document>,
    pub style_tree: Option<StyleTree>,
    pub layout_tree: Option<LayoutTree>,
    pub selected_dom_node: Option<NodeId>,
    pub dom_search_query: String,
    pub js_runtime: Option<Rc<RefCell<JsRuntime>>>,
    pub js_repl_input: String,
    pub error: Option<String>,
    pub request_id: u64,
    /// Pinned tabs are compact, drawn before all normal tabs, and cannot be
    /// closed by bulk actions.
    pub pinned: bool,
    /// Id of the [`crate::browser::TabGroup`] this tab belongs to, if any.
    pub group: Option<u64>,
    /// `true` when the page was discarded to free memory (Memory Saver).
    ///
    /// Discarding drops the native `WebView` but keeps the tab, its title and
    /// its URL in the strip. Selecting the tab recreates the page and reloads
    /// the URL, exactly like Chrome's discarded tabs.
    pub discarded: bool,
    /// Monotonic counter of the last activation, used to sort "recently used"
    /// views. 0 means the tab has never been activated.
    pub last_activated: u64,
    /// `true` while the page is producing audio.
    ///
    /// Reported by the injected media probe (see `browser::AUDIO_PROBE_JS`);
    /// the strip paints a speaker glyph while it is set.
    pub audio_playing: bool,
    /// `true` when this tab's audio is silenced.
    ///
    /// Independent of [`BrowserState::muted_hosts`]: muting a site sets the
    /// flag on every tab showing that site, and a single tab can be muted on
    /// its own. Effective muting is `muted || muted_hosts.contains(host)`.
    pub muted: bool,
}

impl Tab {
    pub fn new(id: u64, initial_url: &str) -> Self {
        let url = initial_url.to_string();
        Self {
            id,
            title: "New Tab".to_string(),
            url: url.clone(),
            omnibox_input: url.clone(),
            history: vec![url],
            history_index: 0,
            loading: false,
            response: None,
            document: None,
            style_tree: None,
            layout_tree: None,
            selected_dom_node: None,
            dom_search_query: String::new(),
            js_runtime: None,
            js_repl_input: String::new(),
            error: None,
            request_id: 0,
            pinned: false,
            group: None,
            discarded: false,
            last_activated: 0,
            audio_playing: false,
            muted: false,
        }
    }

    pub fn can_go_back(&self) -> bool {
        self.history_index > 0
    }

    pub fn can_go_forward(&self) -> bool {
        self.history_index + 1 < self.history.len()
    }

    pub fn navigate_to(&mut self, raw_input: String) {
        let url = oxide_net::security::resolve_omnibox_input(&raw_input);
        if self.loading && self.url == url {
            return;
        }

        // Truncate forward history if navigating to a new page
        if self.history_index + 1 < self.history.len() {
            self.history.truncate(self.history_index + 1);
        }

        self.history.push(url.clone());
        self.history_index = self.history.len() - 1;
        self.url = url.clone();
        self.omnibox_input = url;
        self.loading = true;
        self.error = None;
        self.request_id += 1;
    }

    /// Synchronizes the tab's URL and history when an in-page navigation occurs inside the WebView.
    pub fn sync_webview_url(&mut self, live_url: String) {
        self.loading = false;
        self.error = None;
        if self.url == live_url {
            return;
        }
        self.url = live_url.clone();
        self.omnibox_input = live_url.clone();
        if self.history.get(self.history_index) != Some(&live_url) {
            if self.history_index + 1 < self.history.len() {
                self.history.truncate(self.history_index + 1);
            }
            self.history.push(live_url);
            self.history_index = self.history.len() - 1;
        }
    }

    pub fn go_back(&mut self) -> Option<String> {
        if self.can_go_back() {
            self.history_index -= 1;
            let url = self.history[self.history_index].clone();
            self.url = url.clone();
            self.omnibox_input = url.clone();
            self.loading = true;
            self.error = None;
            self.request_id += 1;
            Some(url)
        } else {
            None
        }
    }

    pub fn go_forward(&mut self) -> Option<String> {
        if self.can_go_forward() {
            self.history_index += 1;
            let url = self.history[self.history_index].clone();
            self.url = url.clone();
            self.omnibox_input = url.clone();
            self.loading = true;
            self.error = None;
            self.request_id += 1;
            Some(url)
        } else {
            None
        }
    }

    pub fn reload(&mut self) -> String {
        self.loading = true;
        self.error = None;
        self.request_id += 1;
        self.url.clone()
    }

    pub fn stop(&mut self) {
        self.loading = false;
        self.request_id += 1;
    }

    pub fn set_response(&mut self, response: HttpResponse) {
        self.loading = false;
        self.error = None;

        // Infer title from headers or response URL
        if let Some(t) = response.headers.get("title") {
            self.title = t.clone();
        } else if let Some(t) = extract_html_title(&response.body) {
            self.title = t;
        } else if response.url == "about:blank" {
            self.title = "Blank Page".to_string();
        } else if let Ok(parsed) = url::Url::parse(&response.url) {
            if let Some(host) = parsed.host_str() {
                self.title = host.to_string();
            } else {
                self.title = response.url.clone();
            }
        } else {
            self.title = response.url.clone();
        }

        // Parse HTML into Document, compute styles and initial layout
        if response.content_type.contains("html")
            || response.url.starts_with("data:text/html")
            || response.url.starts_with("oxide://")
            || response.body.contains("<html")
            || response.body.contains("<!DOCTYPE")
        {
            let doc = Document::parse(&response.body);
            let is_lightweight_doc = doc.len() <= 1500 && response.body.len() <= 131_072;

            let shared_doc = Arc::new(Mutex::new(doc));
            let js_logs = Arc::new(Mutex::new(Vec::new()));
            let js_mutated = Arc::new(AtomicBool::new(false));

            let mut js = JsRuntime::new(shared_doc.clone(), js_logs, js_mutated);
            if is_lightweight_doc {
                let _ = js.execute_document_scripts();
            }

            let final_doc = shared_doc.lock().unwrap().clone();

            if let Some(t) = final_doc.title()
                && !t.is_empty()
            {
                self.title = t;
            }
            if is_lightweight_doc {
                let styles = oxide_style::resolve_styles(&final_doc);
                let layout = oxide_layout::compute_layout(&final_doc, &styles, 1000.0, 700.0);
                self.style_tree = Some(styles);
                self.layout_tree = Some(layout);
            } else {
                self.style_tree = None;
                self.layout_tree = None;
            }
            self.document = Some(final_doc);
            self.js_runtime = Some(Rc::new(RefCell::new(js)));
        } else {
            self.document = None;
            self.style_tree = None;
            self.layout_tree = None;
            self.js_runtime = None;
        }

        self.selected_dom_node = None;
        self.url = response.url.clone();
        self.omnibox_input = response.url.clone();
        self.response = Some(response);
    }

    pub fn recompute_layout(&mut self, width: f32, height: f32) {
        if let (Some(doc), Some(styles)) = (&self.document, &self.style_tree) {
            self.layout_tree = Some(oxide_layout::compute_layout(doc, styles, width, height));
        }
    }

    pub fn set_error(&mut self, err_msg: String) {
        self.loading = false;
        self.error = Some(err_msg.clone());
        self.title = "Error".to_string();
    }
}

/// Helper to quickly extract `<title>...</title>` for tab header
fn extract_html_title(html: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let start_tag = "<title>";
    let end_tag = "</title>";

    let start_idx = lower.find(start_tag)? + start_tag.len();
    let end_idx = lower[start_idx..].find(end_tag)? + start_idx;

    let title = html[start_idx..end_idx].trim();
    if title.is_empty() {
        None
    } else {
        Some(title.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tab_history_and_navigation() {
        let mut tab = Tab::new(1, "oxide://welcome");
        assert_eq!(tab.url, "oxide://welcome");
        assert!(!tab.can_go_back());
        assert!(!tab.can_go_forward());

        tab.navigate_to("https://example.com".to_string());
        assert_eq!(tab.url, "https://example.com/");
        assert!(tab.can_go_back());
        assert!(!tab.can_go_forward());

        tab.navigate_to("https://rust-lang.org".to_string());
        assert_eq!(tab.url, "https://rust-lang.org/");
        assert!(tab.can_go_back());

        let prev = tab.go_back();
        assert_eq!(prev, Some("https://example.com/".to_string()));
        assert_eq!(tab.url, "https://example.com/");
        assert!(tab.can_go_forward());

        // Navigating to a new page truncates forward history
        tab.navigate_to("https://news.ycombinator.com".to_string());
        assert!(!tab.can_go_forward());
        assert_eq!(tab.history.len(), 3);
    }

    #[test]
    fn test_extract_html_title() {
        let html = "<html><head><title>My Awesome Page</title></head><body>Hello</body></html>";
        assert_eq!(extract_html_title(html), Some("My Awesome Page".to_string()));

        let html_no_title = "<html><body>Hello</body></html>";
        assert_eq!(extract_html_title(html_no_title), None);
    }

    #[test]
    fn test_tab_set_response_parses_dom() {
        use std::collections::HashMap;

        let mut tab = Tab::new(1, "https://example.com");
        let resp = HttpResponse {
            url: "https://example.com".to_string(),
            status: 200,
            headers: HashMap::new(),
            body: "<!DOCTYPE html><html><head><title>Example</title></head><body><h1 id='title'>Test</h1></body></html>".to_string(),
            content_type: "text/html; charset=utf-8".to_string(),
            duration_ms: 15,
        };

        tab.set_response(resp);
        assert_eq!(tab.title, "Example");
        assert!(tab.document.is_some());
        assert!(tab.style_tree.is_some());
        assert!(tab.layout_tree.is_some());

        let doc = tab.document.as_ref().unwrap();
        assert_eq!(doc.find_elements_by_tag("h1").len(), 1);
        let title_elem = doc.get_element_by_id("title");
        assert!(title_elem.is_some());

        let layout = tab.layout_tree.as_ref().unwrap();
        assert!(layout.root.is_some());
        assert!(tab.js_runtime.is_some());
    }

    #[test]
    fn test_tab_executes_script_tag_and_mutates_dom() {
        use std::collections::HashMap;

        let mut tab = Tab::new(2, "https://example.com");
        let html = r#"<!DOCTYPE html>
        <html>
        <head><title>Before JS</title></head>
        <body>
            <h1 id="hdr">Initial</h1>
            <script>
                document.title = "After JS";
                document.getElementById("hdr").textContent = "Mutated By Script Tag";
            </script>
        </body>
        </html>"#;

        let resp = HttpResponse {
            url: "https://example.com".to_string(),
            status: 200,
            headers: HashMap::new(),
            body: html.to_string(),
            content_type: "text/html".to_string(),
            duration_ms: 5,
        };

        tab.set_response(resp);
        assert_eq!(tab.title, "After JS");

        let doc = tab.document.as_ref().unwrap();
        let hdr_id = doc.get_element_by_id("hdr").unwrap();
        assert_eq!(doc.text_content(hdr_id), "Mutated By Script Tag");
    }

    #[test]
    fn test_tab_navigate_resolves_google_search_and_domains() {
        let mut tab = Tab::new(3, "oxide://welcome");
        tab.navigate_to("rust language".to_string());
        assert_eq!(tab.url, "https://www.google.com/search?q=rust%20language");
        assert_eq!(tab.omnibox_input, "https://www.google.com/search?q=rust%20language");

        tab.navigate_to("youtube.com".to_string());
        assert_eq!(tab.url, "https://youtube.com/");
    }
}

