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

    if crate::browser::is_internal_ui_page(&active_tab.url) {
        crate::ui::pages::render_internal_page(ui, state, &active_tab.url);
        return;
    }

    // Security / Navigation Error Screen (WebView is automatically hidden while error is active)
    if let Some(err) = &active_tab.error {
        let mut go_safe = false;
        let mut go_back = false;
        let mut retry = false;

        ui.centered_and_justified(|ui| {
            let card_w = (ui.available_width() - 48.0).clamp(280.0, 540.0);
            egui::Frame::NONE
                .fill(Color32::from_rgb(26, 28, 35))
                .stroke(Stroke::new(1.0_f32, Color32::from_rgb(46, 51, 64)))
                .corner_radius(CornerRadius::same(10))
                .inner_margin(Margin::same(24))
                .show(ui, |ui| {
                    ui.set_width(card_w);
                    ui.horizontal(|ui| {
                        egui::Frame::NONE
                            .fill(Color32::from_rgb(60, 24, 28))
                            .corner_radius(CornerRadius::same(10))
                            .inner_margin(Margin::symmetric(8, 3))
                            .show(ui, |ui| {
                                ui.label(
                                    RichText::new("Security Guard")
                                        .size(11.0)
                                        .strong()
                                        .color(Color32::from_rgb(248, 113, 113)),
                                );
                            });
                    });
                    ui.add_space(10.0);
                    ui.label(
                        RichText::new("Navigation blocked before page scripts ran")
                            .size(19.0)
                            .color(Color32::WHITE)
                            .strong(),
                    );
                    ui.add_space(6.0);
                    ui.add(
                        egui::Label::new(
                            RichText::new(&active_tab.url)
                                .monospace()
                                .size(12.0)
                                .color(Color32::from_rgb(156, 163, 175)),
                        )
                        .truncate(),
                    );
                    ui.add_space(12.0);
                    egui::Frame::NONE
                        .fill(Color32::from_rgb(20, 22, 27))
                        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(127, 29, 29)))
                        .corner_radius(CornerRadius::same(6))
                        .inner_margin(Margin::symmetric(12, 10))
                        .show(ui, |ui| {
                            ui.label(
                                RichText::new(err)
                                    .monospace()
                                    .size(12.0)
                                    .color(Color32::from_rgb(252, 165, 165)),
                            );
                        });
                    ui.add_space(16.0);
                    ui.horizontal_wrapped(|ui| {
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new("Return to Welcome")
                                        .size(12.5)
                                        .color(Color32::WHITE),
                                )
                                .fill(Color32::from_rgb(59, 130, 246))
                                .corner_radius(CornerRadius::same(6)),
                            )
                            .clicked()
                        {
                            go_safe = true;
                        }
                        if active_tab.can_go_back()
                            && ui.button("Go Back").clicked()
                        {
                            go_back = true;
                        }
                        if ui.button("Try Again").clicked() {
                            retry = true;
                        }
                    });
                });
        });

        if go_safe {
            state.navigate_active_tab("oxide://welcome".to_string());
        } else if go_back {
            state.go_back();
        } else if retry {
            state.reload_active_tab();
        }
    }
}
