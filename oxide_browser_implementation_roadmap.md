# Oxide Browser: Multi-Phase Implementation Roadmap

This roadmap structures the evolution of `oxide-browser` from its current prototype architecture (`oxide-browser`, `oxide-dom`, `oxide-style`, `oxide-layout`, `oxide-paint`, `oxide-net`, and `oxide-js`) into a full-featured, standards-compliant web browser comparable to Google Chrome.

---

## Phase 1: Core Browser Shell & Navigation UX (`oxide-browser`)
**Goal:** Complete the everyday browser user interface, window menus, and essential page interactions so the browser feels cohesive and responsive during daily testing.

### 1.1 Three-Dot Main Application Menu (`ui/menu.rs`, `menu_window.rs`, `ui/nav_bar.rs`)
* Add a kebab/hamburger menu button to the far right of `nav_bar.rs`.
* Wire it to spawn the popup menu window (`menu_window.rs`) with standard top-level actions:
  * New Tab (`Ctrl+T`), New Window (`Ctrl+N`), New Incognito Window (`Ctrl+Shift+N`)
  * History, Downloads, Bookmarks submenus
  * Inline Zoom controls (`-`, `100%`, `+`, Fullscreen)
  * Find and Edit actions (Cut, Copy, Paste, Print, Save Page As)
  * More Tools (DevTools, Task Manager, Clear Browsing Data)
  * Settings & Exit

### 1.2 Find in Page (`Ctrl+F`)
* Create a floating search bar widget (`ui/find_bar.rs`) anchored to the top-right of the active viewport.
* Implement text matching and highlight overlays across `oxide-dom` text nodes (and WebView script injection if hybrid mode is active), displaying match counts (`1 of N`) with Next/Previous (`Enter` / `Shift+Enter`) navigation.

### 1.3 Page Zoom & Fullscreen Controls (`ui/shortcuts.rs`, `app.rs`, `tab.rs`)
* Bind `Ctrl++`, `Ctrl+-`, and `Ctrl+0` to per-tab zoom state (scaling `oxide-layout` viewport metrics and font sizes).
* Display a zoom badge inside the Omnibox whenever a tab's zoom factor differs from `100%`.
* Bind `F11` to toggle OS borderless fullscreen mode on the active `winit` window.

### 1.4 Omnibox Autocomplete & Site Identity Pill (`ui/nav_bar.rs`)
* Add a floating suggestions dropdown beneath the URL bar that queries open tabs, bookmarks, browsing history, and search engine suggestions in real time.
* Make the left security indicator (`🔒 HTTPS` / `ⓘ Info`) clickable to display connection security details, certificate info, and per-site permission toggles.

### 1.5 Viewport Context Menus (`ui/viewport.rs`)
* Detect right-click hit targets inside the page viewport to open context-sensitive menus for:
  * **Page:** Back, Forward, Reload, Save As, Print, View Page Source, Inspect
  * **Hyperlinks:** Open in New Tab, Open in New Window, Copy Link Address
  * **Images:** Open Image in New Tab, Save Image As, Copy Image, Copy Image Address
  * **Selected Text / Inputs:** Cut, Copy, Paste, Select All, Search for Selection

---

## Phase 2: Persistence, History, Downloads & Settings (`oxide-browser`, `oxide-net`)
**Goal:** Expand `session.rs` into a robust profile storage engine (e.g., backed by SQLite via `rusqlite` or structured JSON stores) and build the internal `oxide://` management pages.

### 2.1 Browsing History (`oxide://history` & `Ctrl+H`)
* Record visited URLs, page titles, favicons, and timestamps upon navigation completion.
* Build an internal `oxide://history` page supporting full-text search, date grouping, individual item deletion, and a **Delete Browsing Data** (`Ctrl+Shift+Del`) modal with time-range filters.

### 2.2 Download Manager (`oxide://downloads` & `Ctrl+J`)
* Detect non-renderable binary MIME types and `Content-Disposition: attachment` headers in `oxide-net`.
* Stream downloads asynchronously to disk with byte-progress tracking, pause/resume/cancel controls, a toolbar download progress bubble, and a full `oxide://downloads` management view.

### 2.3 Full Bookmark Manager & Reading List (`oxide://bookmarks` & `Ctrl+Shift+O`)
* Extend `bookmarks_bar.rs` to support nested folders, drag-and-drop reordering, inline editing (title and URL), and HTML bookmark file import/export.
* Add a "Reading List" queue for saving articles to read later.

### 2.4 Persistent Settings (`oxide://settings`)
* Create a centralized configuration store and UI covering:
  * **Search Engine:** Default search provider selector (Google, DuckDuckGo, Bing, custom URL templates).
  * **On Startup:** Open New Tab, Continue where you left off (`session.rs`), or open specific URLs.
  * **Appearance:** Light/Dark/System themes, Bookmarks Bar visibility, default font sizes, and default zoom level.
  * **Privacy & Security:** Cookie policies, HTTPS-Only mode, DoH (DNS-over-HTTPS), and Clear Data triggers.
  * **Downloads:** Default download directory and "Ask where to save each file" toggle.

### 2.5 Incognito / Private Browsing (`Ctrl+Shift+N`)
* Introduce an `is_incognito` profile mode in `BrowserWindow` that isolates network caches, cookies, and storage in memory and purges all state when the last Incognito window closes.

---

## Phase 3: Networking, Cookies & Security Hardening (`oxide-net`)
**Goal:** Evolve `oxide-net` from a basic document fetcher into a standards-compliant browser networking and security layer.

### 3.1 RFC-Compliant HTTP Disk & Memory Cache (`oxide-net/src/lib.rs`)
* Replace simple in-memory caching with an HTTP cache honoring `Cache-Control` (`max-age`, `no-store`, `no-cache`), `Expires`, `ETag` (`If-None-Match`), and `Last-Modified` (`If-Modified-Since`).
* Differentiate normal Reload (`Ctrl+R`) from cache-bypassing Hard Reload (`Ctrl+Shift+R`).

### 3.2 Cookie Jar & Site Data Management
* Implement an origin-keyed Cookie Jar parsing `Set-Cookie` headers and attaching `Cookie` headers on outgoing requests.
* Enforce `Secure`, `HttpOnly`, `Domain`, `Path`, `Max-Age`/`Expires`, and `SameSite=Strict|Lax|None` rules.

### 3.3 Full HTTP Methods & Subresource Pipeline
* Support `POST`, `PUT`, `DELETE`, `PATCH`, `HEAD`, and `OPTIONS` requests with `application/x-www-form-urlencoded`, `multipart/form-data`, and JSON bodies.
* Build a concurrent subresource loader for external stylesheets (`<link rel="stylesheet">`), scripts (`<script src="...">`), images (`<img>`), and web fonts (`@font-face`).

### 3.4 Web Security Policies (`oxide-net/src/security.rs`)
* Enforce the **Same-Origin Policy (SOP)** and **CORS** preflight (`OPTIONS`) validation.
* Parse and enforce **Content Security Policy (CSP)** headers (`default-src`, `script-src`, `style-src`, `img-src`).
* Expose TLS certificate chains and cipher metadata to the UI security pill and DevTools Security panel.

---

## Phase 4: Custom Rendering Engine — CSS, Layout & GPU Paint (`oxide-style`, `oxide-layout`, `oxide-paint`)
**Goal:** Advance the custom Rust rendering pipeline so it can accurately style, lay out, and paint complex modern websites.

### 4.1 Advanced CSS Selectors, Cascade & Variables (`oxide-style`)
* **Complex Selectors:** Support descendant (` `), child (`>`), adjacent sibling (`+`), and general sibling (`~`) combinators, plus `:hover`, `:active`, `:focus`, `:nth-child()`, `:not()`, `:is()`, and `::before`/`::after` pseudo-elements.
* **Cascade & Specificity:** Sort declarations by origin, `!important`, specificity `(id, class, type)`, and source order.
* **Modern CSS Values:** Implement CSS Custom Properties (`--var` and `var()`), `calc()`, `min()`, `max()`, `clamp()`, and `@media` queries (viewport width/height, `prefers-color-scheme`).

### 4.2 Complete Layout Formatting Contexts (`oxide-layout`)
* **Flexbox & Grid:** Complete Taffy integration for full CSS Flexbox and CSS Grid properties (`grid-template-columns`, `grid-template-rows`, `gap`, `align-items`, `justify-content`).
* **Positioning & Z-Index:** Implement `position: relative`, `absolute`, `fixed`, and `sticky`, along with `z-index` stacking contexts.
* **Scroll Containers & Overflow:** Support `overflow: auto | scroll | hidden`, per-container scroll offsets, and scrollbar hit-testing.
* **Rich Typography:** Add complex text shaping, line wrapping, `line-height`, `text-align`, `text-overflow: ellipsis`, and `@font-face` custom font loading.

### 4.3 Images, Visual Effects & Layer Compositing (`oxide-paint`)
* **Image Decoding:** Decode PNG, JPEG, WebP, GIF, and SVG assets and upload them to `wgpu`/Vello texture atlases.
* **Visual Styling:** Paint `box-shadow`, `linear-gradient()`, `radial-gradient()`, per-corner `border-radius` clipping, `opacity`, and 2D CSS `transform` (`translate`, `scale`, `rotate`).
* **Dirty-Rect & Layer Invalidation:** Avoid full-scene re-rasterization on every frame by tracking damaged layout nodes and compositing cached GPU layers during scrolling.

### 4.4 Interactive HTML Form Controls (`oxide-dom`, `oxide-paint`)
* Implement native state, hit-testing, focus traversal (`Tab` / `Shift+Tab`), text cursor editing, and rendering for `<input>` (text, password, checkbox, radio, submit), `<textarea>`, `<select>`, and `<button>`.

---

## Phase 5: DOM Web APIs & JavaScript Engine (`oxide-dom`, `oxide-js`)
**Goal:** Expand the Boa JavaScript runtime integration (`oxide-js/src/runtime.rs`) so dynamic, interactive web apps can run natively against `oxide-dom`.

### 5.1 Standards-Compliant Event Dispatch (`EventTarget`)
* Implement the 3-phase DOM event flow (**Capture -> Target -> Bubble**) with `stopPropagation()` and `preventDefault()`.
* Dispatch native UI events from `oxide-browser` into `oxide-js`: `click`, `dblclick`, `mousedown`, `mouseup`, `mousemove`, `wheel`, `keydown`, `keyup`, `input`, `change`, `submit`, `focus`, `blur`, and `DOMContentLoaded`.

### 5.2 Complete DOM Tree & Style Mutation Bindings
* Expose `querySelector`, `querySelectorAll`, `getElementsByClassName`, `getElementsByTagName`.
* Bind tree mutation methods: `createElement`, `createTextNode`, `appendChild`, `insertBefore`, `removeChild`, `replaceChild`, `cloneNode`.
* Bind attribute and style APIs: `getAttribute`, `setAttribute`, `removeAttribute`, `dataset`, `classList` (`add`, `remove`, `toggle`, `contains`), `innerHTML`, `textContent`, `element.style.*`, and `getComputedStyle()`.
* Trigger incremental style recalculation and relayout automatically when JS mutates the DOM arena.

### 5.3 Asynchronous Event Loop & Timers
* Integrate a JavaScript task and microtask queue (`Promise` resolution) into the main browser event loop.
* Implement `setTimeout`, `clearTimeout`, `setInterval`, `clearInterval`, and `requestAnimationFrame`.

### 5.4 Essential Browser & Storage Web APIs
* **Networking:** `fetch()` (`Request`, `Response`, `Headers`), `XMLHttpRequest`, and `WebSocket`.
* **Storage:** `localStorage`, `sessionStorage`, and `document.cookie`.
* **Window & Navigation:** `window.location`, `window.history` (`pushState`, `replaceState`, `popstate`), `navigator.userAgent`, `URL`, and `URLSearchParams`.
* **Canvas 2D:** Basic `<canvas>` `getContext('2d')` bindings mapped to `oxide-paint` vector/bitmap draw calls.

---

## Phase 6: DevTools Pro, Multi-Process Sandbox & Ecosystem (`oxide-browser`)
**Goal:** Bring developer tooling, stability, and extensibility up to production desktop browser standards.

### 6.1 Full Interactive DevTools (`ui/devtools.rs`)
* **Elements Panel:** Live double-click editing of DOM attributes/text, live CSS property editing/toggling, interactive Box Model diagram, and `:hover`/`:focus` state forcing.
* **Console Panel:** Interactive JS evaluation REPL with expandable object trees, clickable stack traces, and log-level filters.
* **Sources Panel:** JS file navigator, syntax highlighting, line breakpoints, step-over/step-into/step-out execution controls, and scope variable inspection.
* **Network Panel:** Request waterfall timeline, HTTP request/response header and body viewers, cache/throttling toggles, and timing breakdown.
* **Performance & Application Panels:** Frame-time flamecharts (Style vs. Layout vs. Paint vs. JS) and an Application inspector for Cookies, `localStorage`, and Cache entries.

### 6.2 Browser Task Manager & Performance Controls (`Shift+Esc`)
* Build a real-time Task Manager window displaying per-tab memory footprint, CPU usage, network throughput, and an "End Process / Unload Tab" action.
* Add **Memory Saver** (automatic background tab freezing/discarding after inactivity).

### 6.3 Multi-Process Site Isolation & Extensions
* **Process Separation:** Move each tab's `oxide-dom` + `oxide-style` + `oxide-layout` + `oxide-js` pipeline into an isolated child renderer process communicating with the main `oxide-browser` UI process over IPC, ensuring a crashed or malicious page never brings down the browser UI.
* **WebExtensions Support:** Introduce a `manifest.json` loader and sandboxed extension API (`browser.tabs`, `browser.runtime`, content scripts, and toolbar popup badges).