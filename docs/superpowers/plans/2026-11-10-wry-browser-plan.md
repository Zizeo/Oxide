# Wry WebView & Security Hardening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Upgrade Oxide into a fully functioning, secure web browser using per-tab `wry::WebView` child instances inside the Rust `winit` + `wgpu` + `egui` shell, with Google Search omnibox routing and strict navigation security policies.

**Architecture:** `oxide-net` provides omnibox URL/search resolution (`resolve_omnibox_input`) and a threat-modeled `SecurityPolicy` that validates every navigation against scheme allowlists, HTTPS-Only upgrades, and SSRF/private-IP blocks. `oxide-browser` renders the top tab/navigation chrome and bottom DevTools panel with `egui`/`wgpu`, while managing per-tab `wry::WebView` child views synced to the central viewport rectangle.

**Tech Stack:** Rust 2024, `wry`, `winit` 0.30, `wgpu` 24, `egui` 0.31, `url` 2, `percent-encoding` 2.

**Spec:** `docs/superpowers/specs/2026-11-10-wry-browser-design.md`

## Global Constraints

- Platform: Windows (`win32`) using `wry::WebViewBuilder::build_as_child(&window)`.
- All browser chrome (tab bar, omnibox, navigation buttons, security shield toggles, DevTools console) remains in Rust (`egui` + `wgpu`).
- Allowlisted schemes: `https`, `http` (upgraded/blocked when `https_only` is active), `oxide`, and `about:blank`.
- Forbidden schemes: `javascript`, `vbscript`, `file`, `data` (top-level navigation), and unknown schemes.
- Private/SSRF network ranges (`localhost`, `127.0.0.0/8`, `10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`, `169.254.0.0/16`, `::1`, `fc00::/7`, `fe80::/10`) blocked when `block_private_networks` is true.

## Review Focus

- Single-word search terms in the omnibox (e.g., `rust`) vs single-word local hosts (`localhost`): `resolve_omnibox_input` must route general words to Google Search while preserving explicit `localhost` or `localhost:3000` as host URLs so the security policy can evaluate them cleanly.
- IPv4 and IPv6 loopback/private addresses in URLs (e.g., `http://127.0.0.1:8080/admin`, `http://[::1]/`, `http://169.254.169.254/latest/meta-data`): `SecurityPolicy::validate_navigation` must parse IP literals and reject all loopback, link-local, and RFC1918 addresses when `block_private_networks` is enabled.
- Dangerous scheme injection with mixed case or whitespace (e.g., `  JaVaScRiPt:alert(1)`): `SecurityPolicy::validate_navigation` must trim and case-fold before checking schemes.
- Plaintext `http://` navigation when `https_only` is enabled: `SecurityPolicy::validate_navigation` must return `NavigationDecision::UpgradeToHttps("https://...")` so the browser automatically upgrades the request.
- Zero-sized viewport bounds when the window is minimized or DevTools fills the window: `sync_webview_bounds` must clamp width and height to `.max(1)` before calling `webview.set_bounds`.

---

### Task 1: Omnibox Google Search Resolver & Navigation Security Policy (`crates/oxide-net`)

**Files:**
- Modify: `Cargo.toml`
- Modify: `crates/oxide-net/Cargo.toml`
- Create: `crates/oxide-net/src/security.rs`
- Modify: `crates/oxide-net/src/lib.rs`

**Interfaces:**
- Produces:
  - `oxide_net::security::SecurityPolicy` (`pub https_only: bool`, `pub block_private_networks: bool`)
  - `oxide_net::security::NavigationDecision` (`Allow(String)`, `UpgradeToHttps(String)`, `Block(String)`)
  - `SecurityPolicy::validate_navigation(&self, raw_url: &str) -> NavigationDecision`
  - `oxide_net::security::resolve_omnibox_input(input: &str) -> String`
  - `oxide_net::security::welcome_page_html() -> &'static str`

- [ ] **Step 1: Write the failing unit tests in `crates/oxide-net/src/security.rs`**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_omnibox_google_search() {
        assert_eq!(
            resolve_omnibox_input("rust web browser"),
            "https://www.google.com/search?q=rust%20web%20browser"
        );
        assert_eq!(
            resolve_omnibox_input("weather"),
            "https://www.google.com/search?q=weather"
        );
    }

    #[test]
    fn test_resolve_omnibox_domains_and_schemes() {
        assert_eq!(
            resolve_omnibox_input("google.com"),
            "https://google.com/"
        );
        assert_eq!(
            resolve_omnibox_input("https://news.ycombinator.com"),
            "https://news.ycombinator.com/"
        );
        assert_eq!(
            resolve_omnibox_input("oxide://welcome"),
            "oxide://welcome"
        );
    }

    #[test]
    fn test_security_policy_blocks_dangerous_schemes() {
        let policy = SecurityPolicy::default();
        assert!(matches!(
            policy.validate_navigation("  JaVaScRiPt:alert(1)"),
            NavigationDecision::Block(_)
        ));
        assert!(matches!(
            policy.validate_navigation("file:///C:/Windows/System32/drivers/etc/hosts"),
            NavigationDecision::Block(_)
        ));
        assert!(matches!(
            policy.validate_navigation("data:text/html,<script>alert(1)</script>"),
            NavigationDecision::Block(_)
        ));
    }

    #[test]
    fn test_security_policy_blocks_ssrf_and_private_ips() {
        let policy = SecurityPolicy::default();
        for target in [
            "https://localhost:8080/admin",
            "https://127.0.0.1/secret",
            "https://169.254.169.254/latest/meta-data/",
            "https://192.168.1.1/",
            "https://10.0.0.1/",
            "https://172.16.0.5/",
            "https://[::1]:3000/",
        ] {
            assert!(
                matches!(policy.validate_navigation(target), NavigationDecision::Block(_)),
                "Expected SSRF block for {target}"
            );
        }
    }

    #[test]
    fn test_security_policy_https_only_upgrade() {
        let policy = SecurityPolicy {
            https_only: true,
            block_private_networks: true,
        };
        assert_eq!(
            policy.validate_navigation("http://example.com/path?a=1"),
            NavigationDecision::UpgradeToHttps("https://example.com/path?a=1".to_string())
        );
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p oxide-net`
Expected: FAIL (module `security` not yet created/wired)

- [ ] **Step 3: Implement `crates/oxide-net/src/security.rs` and export in `crates/oxide-net/src/lib.rs`**

Add `percent-encoding = "2"` to workspace `Cargo.toml` and `crates/oxide-net/Cargo.toml`. Implement `resolve_omnibox_input`, `SecurityPolicy`, `NavigationDecision`, `is_private_or_loopback_host(host: &str) -> bool` (checking `localhost`, `.local`, `std::net::IpAddr` loopback, private, link-local, unique-local), and `welcome_page_html()`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p oxide-net`
Expected: PASS (all tests in `oxide-net` pass)

---

### Task 2: Wry Multi-Tab WebView Engine & Security Toolbar (`crates/oxide-browser`)

**Files:**
- Modify: `Cargo.toml`
- Modify: `crates/oxide-browser/Cargo.toml`
- Modify: `crates/oxide-browser/src/tab.rs`
- Modify: `crates/oxide-browser/src/browser.rs`
- Modify: `crates/oxide-browser/src/ui/mod.rs`
- Modify: `crates/oxide-browser/src/ui/nav_bar.rs`
- Modify: `crates/oxide-browser/src/ui/viewport.rs`
- Modify: `crates/oxide-browser/src/ui/devtools.rs`
- Modify: `crates/oxide-browser/src/app.rs`

**Interfaces:**
- Consumes:
  - `oxide_net::security::{NavigationDecision, SecurityPolicy, resolve_omnibox_input, welcome_page_html}`
- Produces:
  - `browser::WebViewEvent` (`TitleChanged { tab_id: u64, title: String }`, `UrlChanged { tab_id: u64, url: String }`, `NewTabRequested { url: String }`, `SecurityAlert { tab_id: u64, message: String }`)
  - `BrowserState::ensure_tab_webview(&mut self, tab_id: u64, window: &winit::window::Window)`
  - `BrowserState::sync_webview_visibility_and_bounds(&mut self, scale_factor: f64)`

- [ ] **Step 1: Add `wry` dependency and write unit tests for tab & security state in `crates/oxide-browser/src/tab.rs`**

Add `wry = "0.50"` to workspace `Cargo.toml` and `crates/oxide-browser/Cargo.toml`. Add tests in `crates/oxide-browser/src/tab.rs` verifying that `Tab::navigate_to` normalizes search queries via `resolve_omnibox_input` and records history accurately.

- [ ] **Step 2: Run `cargo check -p oxide-browser --tests` to verify compilation of `wry` and test setup**

Run: `cargo check -p oxide-browser --tests`
Expected: Compiles `wry` and checks test target.

- [ ] **Step 3: Implement per-tab `wry::WebView` management, bounds synchronization, and security toolbar controls**

1. In `crates/oxide-browser/src/browser.rs`:
   - Store `webviews: HashMap<u64, wry::WebView>`, `security_policy: Arc<RwLock<SecurityPolicy>>`, `viewport_rect: egui::Rect`, and `wv_event_rx: Receiver<WebViewEvent>`.
   - Implement `ensure_active_webview(&mut self, window: &Window)` using `wry::WebViewBuilder::new()`:
     - Configure custom protocol `"oxide"` (or load `welcome_page_html()` when URL is `oxide://welcome`).
     - Attach `with_navigation_handler` that locks `security_policy`, runs `validate_navigation(&url)`, sends `WebViewEvent::UrlChanged` on allow, sends `WebViewEvent::SecurityAlert` + blocks (`false`) on block, or sends an upgrade event on `UpgradeToHttps`.
     - Attach `with_document_title_changed_handler` to send `WebViewEvent::TitleChanged`.
     - Attach `with_new_window_req_handler` to validate target URL and send `WebViewEvent::NewTabRequested { url }`, returning `false` so popups open as controlled tabs inside Oxide.
     - Build via `.build_as_child(window)`.
   - Implement `sync_webviews(&mut self, window: &Window)`:
     - Only the active tab's WebView has `set_visible(true)` (or `false` if an error overlay is shown); all other tabs have `set_visible(false)`.
     - Clamp viewport width and height to `.max(1.0)` and call `webview.set_bounds(wry::Rect { position: LogicalPosition::new(x, y).into(), size: LogicalSize::new(w, h).into() })`.
2. In `crates/oxide-browser/src/ui/nav_bar.rs` & `viewport.rs`:
   - Add quick-search support in Omnibox (placeholder `"Search Google or type a URL..."`), quick links (`Google`, `YouTube`, `Rust Lang`, `Hacker News`, `GitHub`, `Welcome`), and security policy toggles (`🛡 HTTPS-Only`, `🔒 Block LAN/SSRF`).
   - Record `state.viewport_rect = ui.available_rect_before_wrap()` in `viewport.rs`.
3. In `crates/oxide-browser/src/app.rs`:
   - Call `state.poll_webview_events()` and `state.ensure_and_sync_webviews(&wgpu_ctx.window)` each frame.

- [ ] **Step 4: Run full workspace tests and clippy verification**

Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS with 0 warnings and all unit tests green.
