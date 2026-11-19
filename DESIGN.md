---
name: Oxide Browser
description: Precision dark-slate desktop browser shell built on egui, wgpu, and Wry.
colors:
  primary: "#3b82f6"
  primary-bright: "#60a5fa"
  chrome-canvas: "#111216"
  chrome-strip: "#1a1b1e"
  chrome-elevated: "#20232a"
  surface-popover: "#1a1c23"
  surface-well: "#14161b"
  control-rest: "#22252e"
  control-hover: "#2e323e"
  control-active: "#383e4c"
  hairline-border: "#2e3340"
  hairline-hover: "#484f62"
  text-primary: "#d4d8e2"
  text-secondary: "#9ca3af"
  text-muted: "#585e6c"
  status-secure: "#4ade80"
  status-warning: "#fbbf24"
  status-danger: "#f87171"
  incognito-surface: "#42275a"
  incognito-text: "#d8b4fe"
typography:
  headline:
    fontFamily: "Segoe UI, system-ui, sans-serif"
    fontSize: "22px"
    fontWeight: 600
    lineHeight: 1.25
    letterSpacing: "0em"
  title:
    fontFamily: "Segoe UI, system-ui, sans-serif"
    fontSize: "15px"
    fontWeight: 600
    lineHeight: 1.3
    letterSpacing: "0em"
  body:
    fontFamily: "Segoe UI, system-ui, sans-serif"
    fontSize: "12.5px"
    fontWeight: 400
    lineHeight: 1.4
    letterSpacing: "0em"
  label:
    fontFamily: "Segoe UI, system-ui, sans-serif"
    fontSize: "11px"
    fontWeight: 600
    lineHeight: 1.2
    letterSpacing: "0em"
  mono:
    fontFamily: "Consolas, ui-monospace, monospace"
    fontSize: "12.5px"
    fontWeight: 400
    lineHeight: 1.35
    letterSpacing: "0em"
rounded:
  xs: "4px"
  sm: "6px"
  md: "8px"
  lg: "10px"
  pill: "16px"
spacing:
  xs: "4px"
  sm: "6px"
  md: "10px"
  lg: "24px"
components:
  button-nav-icon:
    backgroundColor: "transparent"
    textColor: "{colors.text-primary}"
    rounded: "{rounded.pill}"
    height: "28px"
    width: "28px"
  button-chrome-pill:
    backgroundColor: "{colors.control-rest}"
    textColor: "{colors.text-secondary}"
    rounded: "{rounded.pill}"
    padding: "4px 10px"
    height: "28px"
  button-chrome-pill-hover:
    backgroundColor: "{colors.control-hover}"
    textColor: "{colors.text-primary}"
  input-omnibox:
    backgroundColor: "{colors.surface-well}"
    textColor: "{colors.text-primary}"
    typography: "{typography.mono}"
    rounded: "{rounded.pill}"
    padding: "2px 6px"
  chip-bookmark:
    backgroundColor: "{colors.control-rest}"
    textColor: "{colors.text-secondary}"
    rounded: "{rounded.sm}"
    height: "20px"
  popover-surface:
    backgroundColor: "{colors.surface-popover}"
    textColor: "{colors.text-primary}"
    rounded: "{rounded.md}"
    padding: "6px"
---

# Design System: Oxide Browser

## Overview

**Creative North Star: "The Precision Cockpit"**

Oxide Browser is an operational desktop shell whose chrome behaves like a low-glare cockpit instrument panel: dense, quiet, and visually subordinate to the rendered web page. Every surface steps cleanly in dark slate luminance rather than competing with page content through heavy drop shadows or saturated decoration.

Controls are crisp and restrained—compact pills, circular 28px icon targets, monochrome vector/geometric glyphs, and 1px hairline borders—while saturated color is held in reserve strictly for active focus rings, security indicators, and Private/Incognito session identity.

**Key Characteristics:**
- Low-glare dark-slate surface stepping (`chrome-canvas` -> `chrome-strip` -> `chrome-elevated`)
- Zero-clipping `right_to_left` layout discipline across every toolbar and management row
- Crisp 1px structural hairlines instead of heavy ambient shadows
- Accent blue reserved exclusively for active focus, selection, and DevTools state

## Colors

The palette pairs cool obsidian neutrals with a single Cobalt Focus accent and strict semantic status colors for connection security and Private browsing.

### Primary
- **Cobalt Focus** (`{colors.primary}`): Focus ring on the Omnibox pill, active selection highlights, and toggled DevTools controls.
- **Sky Signal** (`{colors.primary-bright}`): Active DevTools label, zoom indicator badge, and internal `oxide://` identity pill.

### Neutral
- **Obsidian Canvas** (`{colors.chrome-canvas}`): Base panel fill (`panel_fill`) behind internal `oxide://` pages and viewport chrome.
- **Charcoal Strip** (`{colors.chrome-strip}`): Inactive tab strip and bookmarks bar backdrop (`BAR_FILL`).
- **Slate Elevated** (`{colors.chrome-elevated}`): Navigation bar surface (`BAR_BG`) and active tab fill.
- **Midnight Well** (`{colors.surface-well}`): Recessed Omnibox input interior (`PILL_BG`).
- **Graphite Popover** (`{colors.surface-popover}`): Floating menus (`MenuWindow`), Task Manager window, and Omnibox suggestions container (`window_fill`).
- **Slate Control** (`{colors.control-rest}` / `{colors.control-hover}` / `{colors.control-active}`): Standard rest, hover, and pressed button surfaces.
- **Steel Hairline** (`{colors.hairline-border}` / `{colors.hairline-hover}`): 1px structural borders on popovers, cards, and hovered widgets.
- **Cloud Ink / Ash Ink / Slate Muted** (`{colors.text-primary}` / `{colors.text-secondary}` / `{colors.text-muted}`): Primary text, secondary labels, and disabled glyphs.

### Semantic & Session States
- **Emerald Secure** (`{colors.status-secure}`): Valid `HTTPS` identity badge.
- **Amber Caution** (`{colors.status-warning}`): Plain `HTTP` connection badge.
- **Crimson Blocked** (`{colors.status-danger}`): Security Guard blocked navigation badge and stop-loading glyph.
- **Violet Veil** (`{colors.incognito-surface}` / `{colors.incognito-text}`): Incognito window pill indicator.

### Named Rules
**The Reserved Accent Rule.** Cobalt Focus and Sky Signal appear only on active focus rings, selected states, and internal status badges—never as decorative fills on resting chrome.

## Typography

**Display / UI Font:** Platform sans (`Segoe UI, system-ui, sans-serif`)
**Monospace Font:** Platform monospace (`Consolas, ui-monospace, monospace`)

**Character:** Legible, compact operational typography where proportional UI sans handles navigation, menus, and settings while monospace is reserved strictly for URLs, DevTools measurements, and raw source views.

### Hierarchy
- **Headline** (`22px`, semibold `600`, `1.25` line-height): Internal `oxide://` page headings (`History`, `Downloads`, `Bookmarks & Reading List`, `Settings`).
- **Title** (`15px`, semibold `600`, `1.3` line-height): Settings section headers and Bookmarks/Reading List subheaders.
- **Body** (`12.5px`, regular `400`, `1.4` line-height): Tab titles, autocomplete suggestions, DevTools tabs, and list rows.
- **Label** (`11px`, semibold `600`, `1.2` line-height): Security identity badges, zoom percentage badge, Incognito pill, and status tags.
- **Mono** (`12.5px`, regular `400`, `1.35` line-height): Omnibox URL input and DevTools performance/console telemetry.

### Named Rules
**The Telemetry-Only Monospace Rule.** Monospace is used exclusively for URLs, code, and timing readouts—never as decorative styling on UI headings or buttons.

## Layout

Oxide uses a stacked top-chrome + central-viewport desktop shell (`TopBottomPanel` + `CentralPanel`) with optional bottom DevTools drawer:

- **Tab Strip** (`34px` tab height): Elastic normal tabs (`64px`–`240px`) switching smoothly to horizontal scroll below `64px`; fixed `44px` pinned tabs and `96px` group chips.
- **Navigation Bar**: Left cluster (Back, Forward, Reload/Stop, Home) and right-to-left pinned right cluster (`⋮`, Incognito, Downloads, DevTools) flanking a fluid center Omnibox pill that occupies `ui.available_width()`.
- **Bookmarks Bar** (`24px` row height): Right-to-left pinned security toggles with a horizontally scrolling chip strip (`20px` chips).
- **Internal Pages (`oxide://`)**: Centered reading/management column capped at `860px` max width (`>= 24px` outer padding), `horizontal_wrapped` filter/radio toolbars, and `right_to_left` row action buttons so long URLs truncate before any button can clip.
- **Responsive Constraint:** Minimum window width is `640px` (`640x480`); every horizontal strip must remain usable without clipping at `640px`.

### Named Rules
**The Right-First Chrome Rule.** In any horizontal bar or list row mixing variable-width text with fixed action buttons, lay out right-side actions `right_to_left` first so buttons never clip off-screen.

## Elevation & Depth

Depth is conveyed through **tonal stepping and 1px hairline borders** rather than drop shadows. Chrome surfaces sit flat at rest (`#111216` -> `#1a1b1e` -> `#20232a`), while floating surfaces—`MenuWindow` popups, the Omnibox autocomplete dropdown, Find bar, and Task Manager—step to `#1a1c23` and assert a crisp `1px` `#2e3340` border (`window_stroke`).

### Named Rules
**The Tonal Separation Rule.** Separate adjacent chrome bands and floating popovers by surface luminance step plus a `1px` hairline stroke—never with heavy blur or offset shadows.

## Shapes

Oxide pairs soft architectural rectangles (`6px` widget radius, `8px` window/popover radius, `10px` dropdown radius) with full pills (`14px`–`16px`) for primary navigation controls:

- **Navigation Icon Targets:** Circular `28x28px` (`14px` radius) transparent at rest, subtle slate fill on hover.
- **Omnibox & Status Pills:** `16px` outer pill housing `10px` inner identity/zoom pills.
- **Bookmarks, DevTools Tabs & Standard Buttons:** `4px`–`6px` rounded rectangles.
- **Menus, Find Bar & Windows:** `8px`–`10px` rounded containers with `1px` hairline strokes.

## Components

### Navigation Icon Buttons
- **Shape:** Circular (`14px` radius, `28x28px`)
- **Rest:** Transparent background, crisp geometric monochrome glyph (`◀`, `▶`, `⟳`, `⌂`, `⋮`)
- **Hover / Disabled:** `control-hover` fill; `text-muted` glyph when disabled

### Omnibox Pill
- **Shape:** Full pill (`16px` radius, `26px` min height)
- **Container:** `surface-well` background, `1px` `hairline-border` stroke shifting to `primary` (`#3b82f6`) on focus
- **Anatomy:** Left clickable Site Identity Pill (`HTTPS` / `HTTP` / `Oxide` / `Blocked`), fluid borderless monospace `TextEdit`, right-aligned Zoom badge (when != `100%`) and `→` submit glyph

### Chrome Pill Buttons (DevTools / Downloads / Site Badge)
- **Shape:** Pill (`14px` radius, `28px` height)
- **Rest:** `control-rest` fill, `text-secondary` label
- **Active (DevTools Open):** Deep cobalt tint (`#1a2840`) with `1px` `primary` border and `primary-bright` text

### Popovers & Floating Bars (Menus, Autocomplete, Find Bar)
- **Shape:** `8px`–`10px` radius with `1px` `hairline-border` stroke on `surface-popover`
- **Overflow:** Autocomplete dropdown capped to `260px` height with vertical `ScrollArea` and truncated row labels

### Management List Rows (`oxide://history`, `oxide://bookmarks`, Task Manager)
- **Layout:** `right_to_left` action buttons (`✕`, `Edit`, `End Process`) anchored first, paired with truncated `Label` / `Link` URLs

## Do's and Don'ts

### Do:
- **Do** lay out right-edge chrome controls and row action buttons `right_to_left` before fluid text inputs or URL labels.
- **Do** keep navigation chrome monochrome with crisp geometric glyphs (`◀`, `▶`, `⟳`, `⌂`, `⋮`, `▲`, `▼`, `✕`), reserving color for focus rings and security states.
- **Do** use `horizontal_wrapped` or `ScrollArea::horizontal()` on multi-item toolbars (DevTools tabs, Settings radio groups) so narrow `640px` windows stay clip-free.

### Don't:
- **Don't** subtract hardcoded pixel widths (`ui.available_width() - 220.0`) to size the Omnibox or Find bar.
- **Don't** render emoji icons inside operational chrome buttons or DevTools tabs.
- **Don't** render untrimmed long URLs in horizontal rows without `.truncate()`.
