# Oxide Browser: Wry Integration & Security Hardening Design

**Date:** 2026-11-10
**Scope:** Focused project upgrade (`oxide-browser` & `oxide-net`)

## 1. Intent & Why
Upgrade Oxide from a prototype layout viewer into a fully functioning, secure web browser capable of running Google Search, rendering complex modern websites (YouTube, Google, GitHub, Hacker News), and enforcing strict security boundaries—while keeping the windowing, UI chrome, tab management, and security policy enforcement in safe Rust.

## 2. Goals, Non-Goals, and Anti-Goals
- **Goals:**
  - Embed per-tab `wry::WebView` child instances inside our `winit` + `wgpu` + `egui` window.
  - Route search queries entered in the Omnibox directly to Google Search (`https://www.google.com/search?q=...`).
  - Synchronize `wry::WebView` viewport bounds dynamically with `egui`'s central panel so the top toolbar and bottom DevTools panel never overlap the web content.
  - Synchronize real-time WebView events (URL changes, document title changes, loading state) back to the Rust tab bar and omnibox.
  - Enforce strict Rust-side navigation security (scheme allowlisting, SSRF/private-IP blocking, popup interception, and HTTPS-Only mode).
- **Non-Goals:**
  - Chrome extension store compatibility or custom media DRM plugins.
- **Anti-Goals:**
  - Replacing the Rust `egui` UI chrome with HTML/JS UI.
  - Allowing unvalidated navigation to `file://`, `javascript:`, or local-network SSRF targets.

## 3. Constraints
- **Platform:** Windows (`win32`) with WebView2 via `wry`.
- **Tech Stack:** `winit` 0.30, `wgpu` 24, `egui` 0.31, `wry` (using `build_as_child(&window)`), `tokio`, `url`.

## 4. Approaches Considered
1. **Approach 1 (Selected): Rust `egui`/`wgpu` Chrome + Multi-Tab Child `wry::WebView`s**
   - Each tab owns a child `wry::WebView` positioned in the viewport rect below the `egui` toolbar. Switching tabs toggles WebView visibility (`set_visible`), preserving page state, forms, and media playback.
2. **Approach 2: Dual-Engine Mode (`wry` + Custom Rust Engine Toggle)**
   - Keeps both `wry` and the prototype Vello/Taffy renderer behind a mode toggle. Rejected for unnecessary complexity when `wry` handles all web rendering.
3. **Approach 3: Single Shared `wry::WebView` Swapped Across Tabs**
   - Uses one WebView for the whole window and navigates it when switching tabs. Rejected because switching tabs loses page state, scroll position, and form inputs.

## 5. Architecture & Security Design

### A. Hybrid Window & Viewport Synchronization (`crates/oxide-browser`)
- `winit` owns the main OS window; `wgpu` + `egui` render the top Tab Bar, Navigation Bar, and optional bottom DevTools drawer.
- Each frame, `egui::CentralPanel` records the exact viewport rectangle (`egui::Rect`). When the viewport size or position changes (e.g. window resize or DevTools drawer toggle), Rust updates the active `wry::WebView` via `webview.set_bounds(wry::Rect { position, size })`.
- Each `Tab` manages its own `wry::WebView` instance. Only the active tab's WebView has `set_visible(true)`; background tabs have `set_visible(false)`.

### B. Omnibox, Google Search & Custom Protocol (`crates/oxide-net` & `crates/oxide-browser`)
- **Omnibox Classifier (`resolve_omnibox_input`)**:
  - Explicit URLs (`https://...`, `http://...`, `oxide://...`) are parsed directly.
  - Domain-like strings (`example.com`, `rust-lang.org`, `github.com/rust-lang`) are prefixed with `https://`.
  - Search queries (inputs containing spaces or single words without a TLD like `rust programming` or `weather`) are URL-encoded and routed to `https://www.google.com/search?q=<query>`.
- **Internal Pages (`oxide://welcome`)**:
  - Handled via `wry`'s custom protocol handler (`with_custom_protocol("oxide", ...)`) or HTML injection, serving the Oxide Welcome dashboard with quick-search and architecture stats.

### C. Security Hardening (`crates/oxide-net::security`)
- **Threat Model & Trust Boundary**: Every URL entered by the user, clicked on a page, or triggered via JS redirect/popup crosses an untrusted boundary before navigation.
- **Navigation Policy Guard (`SecurityPolicy::validate_navigation`)**:
  - **Scheme Allowlist**: Permits `https`, `http` (blocked when HTTPS-Only mode is enabled), and `oxide`. Explicitly blocks `file:`, `javascript:`, `vbscript:`, `data:` (top-level), and unknown schemes.
  - **SSRF & Local Network Protection**: Parses hostnames/IPs and blocks loopback (`127.0.0.0/8`, `::1`, `localhost`), link-local (`169.254.0.0/16`), and private RFC1918 ranges (`10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`) by default (with an explicit opt-in toggle in the toolbar for local dev servers).
- **Popup & New-Window Interception (`with_new_window_req_handler`)**:
  - Intercepts `target="_blank"` and `window.open()` requests, validates the target URL through `SecurityPolicy`, and opens approved URLs in a new Oxide tab instead of spawning unmanaged OS popup windows.
- **Event Callbacks to Rust**:
  - `with_navigation_handler`: Enforces `SecurityPolicy`, logs allowed/blocked requests to the Rust DevTools Console, and updates the tab's URL and security badge (`🔒 HTTPS`, `⚠ HTTP`, `🛡 Blocked`).
  - `with_document_title_changed_handler`: Updates the Rust tab's title in real time.
