pub mod bookmarks_bar;
pub mod devtools;
pub mod find_bar;
pub mod menu;
pub mod nav_bar;
pub mod pages;
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
    style.visuals.window_fill = egui::Color32::from_rgb(41, 43, 48);
    style.visuals.panel_fill = egui::Color32::from_rgb(31, 32, 36);
    style.visuals.extreme_bg_color = egui::Color32::from_rgb(24, 25, 28);
    style.visuals.faint_bg_color = egui::Color32::from_rgb(36, 38, 43);
    style.visuals.window_corner_radius = egui::CornerRadius::same(10);
    style.visuals.menu_corner_radius = egui::CornerRadius::same(10);
    style.visuals.window_stroke =
        egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(60, 64, 72));

    let r6 = egui::CornerRadius::same(6);
    style.visuals.widgets.noninteractive.corner_radius = r6;
    style.visuals.widgets.noninteractive.bg_stroke =
        egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(54, 57, 65));
    style.visuals.widgets.inactive.corner_radius = r6;
    style.visuals.widgets.inactive.weak_bg_fill = egui::Color32::from_rgb(52, 54, 60);
    style.visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(52, 54, 60);
    style.visuals.widgets.inactive.fg_stroke =
        egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(232, 234, 237));
    style.visuals.widgets.hovered.corner_radius = r6;
    style.visuals.widgets.hovered.weak_bg_fill = egui::Color32::from_rgb(64, 67, 74);
    style.visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(64, 67, 74);
    style.visuals.widgets.hovered.bg_stroke =
        egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(84, 88, 98));
    style.visuals.widgets.active.corner_radius = r6;
    style.visuals.widgets.active.bg_fill = egui::Color32::from_rgb(72, 76, 84);

    style.visuals.selection.bg_fill = egui::Color32::from_rgba_unmultiplied(102, 157, 246, 85);
    style.visuals.selection.stroke =
        egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(102, 157, 246));

    style.spacing.button_padding = egui::vec2(8.0, 4.0);
    style.spacing.item_spacing = egui::vec2(6.0, 5.0);
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
        .frame(egui::Frame::NONE.fill(egui::Color32::from_rgb(31, 32, 36)))
        .show(ctx, |ui| {
            ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);

            // 1. Tab Bar
            ui.add_space(4.0);
            tab_bar::render_tab_bar(ui, state);

            // 2. Navigation Bar (seamless merge with active tab)
            nav_bar::render_nav_bar(ui, state);

            // 3. Bookmarks bar (toggleable via Ctrl+Shift+B or Settings)
            if state.settings.show_bookmarks_bar {
                bookmarks_bar::render(ui, state);
            }

            // 4. Find in Page bar (shown on Ctrl+F)
            find_bar::render(ui, state);

            // Subtle 1px bottom hairline separating chrome from page viewport
            let bar_rect = ui.min_rect();
            ui.painter().line_segment(
                [
                    egui::pos2(bar_rect.left(), bar_rect.bottom()),
                    egui::pos2(bar_rect.right(), bar_rect.bottom()),
                ],
                egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(24, 25, 28)),
            );
        });

    // DevTools bottom panel if open
    if state.show_devtools {
        let max_h = (ctx.screen_rect().height() - 220.0).max(120.0);
        let resp = egui::TopBottomPanel::bottom("browser_devtools")
            .resizable(true)
            .min_height(120.0)
            .max_height(max_h)
            .default_height(state.devtools_height.clamp(120.0, max_h))
            .frame(egui::Frame::side_top_panel(&ctx.style()).fill(egui::Color32::from_rgb(24, 24, 27)))
            .show(ctx, |ui| {
                devtools::render_devtools(ui, state);
            });
        state.devtools_height = resp.response.rect.height().clamp(120.0, max_h);
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
    pages::render_clear_data_modal(ctx, state);
    pages::render_task_manager(ctx, state);
}
