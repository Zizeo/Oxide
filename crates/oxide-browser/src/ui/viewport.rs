//! Browser viewport synchronizing the Wry WebView bounds and rendering security/error overlays.

use crate::browser::BrowserState;
use egui::{Color32, CornerRadius, Margin, RichText, Stroke, Ui};

pub fn render_viewport(ui: &mut Ui, state: &mut BrowserState) {
    // Record the exact central viewport rectangle so the child Wry WebView
    // aligns below the egui top bar and above the DevTools drawer.
    state.viewport_rect = ui.available_rect_before_wrap();

    // While a menu or overlay is open the native WebView is hidden and this
    // frozen copy of the page is painted in its place, so the page stays
    // visible under the menu instead of going blank.
    if let Some(snap) = &mut state.page_snapshot {
        let rect = state.viewport_rect;
        if snap.texture.is_none() {
            snap.texture = Some(ui.ctx().load_texture(
                "oxide_page_snapshot",
                snap.image.clone(),
                egui::TextureOptions::LINEAR,
            ));
        }
        if let Some(tex) = &snap.texture {
            ui.painter().image(
                tex.id(),
                rect,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        }
    }

    let active_tab = match state.active_tab() {
        Some(t) => t.clone(),
        None => {
            ui.centered_and_justified(|ui| {
                ui.label(RichText::new("No open tabs").color(Color32::GRAY));
            });
            return;
        }
    };

    // Security / Navigation Error Screen (WebView is automatically hidden while error is active)
    if let Some(err) = &active_tab.error {
        let mut go_safe = false;
        let mut retry = false;

        ui.centered_and_justified(|ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(60.0);
                ui.label(
                    RichText::new("🛡 Oxide Security Guard — Navigation Blocked")
                        .size(22.0)
                        .color(Color32::from_rgb(239, 68, 68))
                        .strong(),
                );
                ui.add_space(12.0);
                ui.label(
                    RichText::new(format!("Target URL: {}", active_tab.url))
                        .size(14.0)
                        .color(Color32::from_rgb(209, 213, 219)),
                );
                ui.add_space(10.0);
                egui::Frame::NONE
                    .fill(Color32::from_rgb(30, 20, 20))
                    .stroke(Stroke::new(1.0_f32, Color32::from_rgb(185, 28, 28)))
                    .corner_radius(CornerRadius::same(6))
                    .inner_margin(Margin::symmetric(16, 12))
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new(err)
                                .monospace()
                                .size(13.0)
                                .color(Color32::from_rgb(252, 165, 165)),
                        );
                    });
                ui.add_space(18.0);
                ui.horizontal(|ui| {
                    if ui
                        .add(
                            egui::Button::new(
                                RichText::new("◀ Back to Safety (Welcome)")
                                    .size(13.5)
                                    .color(Color32::WHITE),
                            )
                            .fill(Color32::from_rgb(37, 99, 235))
                            .corner_radius(CornerRadius::same(6)),
                        )
                        .clicked()
                    {
                        go_safe = true;
                    }

                    if ui
                        .add(
                            egui::Button::new(
                                RichText::new("⟳ Try Again")
                                    .size(13.5)
                                    .color(Color32::from_rgb(209, 213, 219)),
                            )
                            .fill(Color32::from_rgb(45, 48, 56))
                            .corner_radius(CornerRadius::same(6)),
                        )
                        .clicked()
                    {
                        retry = true;
                    }
                });
            });
        });

        if go_safe {
            state.navigate_active_tab("oxide://welcome".to_string());
        } else if retry {
            state.reload_active_tab();
        }
    }
}
