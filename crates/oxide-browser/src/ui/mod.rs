pub mod bookmarks_bar;
pub mod devtools;
pub mod menu;
pub mod nav_bar;
pub mod shortcuts;
pub mod tab_actions;
pub mod tab_bar;
pub mod tab_menu;
pub mod tab_search;
pub mod viewport;

use egui::Context;
use crate::browser::BrowserState;

/// Applies the Chromium-inspired dark theme.
///
/// Shared with the menu popup window, which runs its own egui context and
/// would otherwise paint menus in egui's default light style.
pub fn apply_style(ctx: &Context) {
    let mut style = (*ctx.style()).clone();
    style.visuals.dark_mode = true;
    style.visuals.window_fill = egui::Color32::from_rgb(24, 24, 27); // Zinc 900
    style.visuals.panel_fill = egui::Color32::from_rgb(18, 18, 20);
    ctx.set_style(style);
}

/// Returns the texture for a site's decoded favicon, creating it once.
///
/// The handle lives in egui's temp storage instead of [`BrowserState`]: the
/// strip only holds an immutable borrow while it draws, and a texture is pure
/// view state. Keying by site means every tab on one site shares a single GPU
/// texture instead of one upload per tab.
pub fn favicon_texture(
    ctx: &egui::Context,
    site: &str,
    image: &egui::ColorImage,
) -> egui::TextureHandle {
    let id = egui::Id::new(("oxide_favicon_texture", site));
    if let Some(handle) = ctx.data(|data| data.get_temp::<egui::TextureHandle>(id)) {
        return handle;
    }
    let handle = ctx.load_texture(
        format!("oxide_favicon:{site}"),
        image.clone(),
        egui::TextureOptions::LINEAR,
    );
    ctx.data_mut(|data| data.insert_temp(id, handle.clone()));
    handle
}

pub fn render_browser_ui(ctx: &Context, state: &mut BrowserState) {
    // Shortcuts run before any panel is laid out, so a single Ctrl+T press
    // can only ever create one tab and the strip already reflects it.
    shortcuts::handle_shortcuts(ctx, state);

    apply_style(ctx);

    egui::TopBottomPanel::top("browser_top_bar")
        .frame(egui::Frame::NONE.fill(egui::Color32::from_rgb(30, 31, 35)))
        .show(ctx, |ui| {
            ui.spacing_mut().item_spacing = egui::vec2(6.0, 4.0);

            // 1. Tab Bar
            tab_bar::render_tab_bar(ui, state);

            // 2. Navigation Bar (Back, Forward, Reload, Omnibox, DevTools toggle)
            nav_bar::render_nav_bar(ui, state);

            // 3. Bookmarks bar: a row of site chips under the omnibox, exactly
            // where Chrome puts it, so `Ctrl+Shift+B`-style visibility
            // questions never arise (the bar is always shown).
            bookmarks_bar::render(ui, state);
        });

    // DevTools bottom panel if open
    if state.show_devtools {
        egui::TopBottomPanel::bottom("browser_devtools")
            .resizable(true)
            .min_height(120.0)
            .default_height(state.devtools_height)
            .frame(egui::Frame::side_top_panel(&ctx.style()).fill(egui::Color32::from_rgb(24, 24, 27)))
            .show(ctx, |ui| {
                devtools::render_devtools(ui, state);
            });
    }

    // Main Viewport Area
    egui::CentralPanel::default()
        .frame(egui::Frame::NONE.fill(egui::Color32::from_rgb(15, 17, 23)))
        .show(ctx, |ui| {
            viewport::render_viewport(ui, state);
        });

    // Tab search is an overlay over the page: drawn last, and the native
    // WebView hides itself while `tab_search_open` is set.
    tab_search::render(ctx, state);
}
