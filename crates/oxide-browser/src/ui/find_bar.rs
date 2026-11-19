//! Floating Find in Page bar (`Ctrl+F`).

use crate::browser::BrowserState;
use egui::{Color32, CornerRadius, Key, Margin, RichText, Stroke, Ui, Vec2};

pub fn render(ui: &mut Ui, state: &mut BrowserState) {
    if !state.find_bar_open {
        return;
    }

    let focus = std::mem::take(&mut state.find_bar_focus_request);
    let mut run_dir: Option<i32> = None;
    let mut close = false;

    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        let bar_w = (ui.available_width() - 24.0).clamp(200.0, 380.0);
        egui::Frame::NONE
            .fill(Color32::from_rgb(30, 33, 41))
            .stroke(Stroke::new(1.0_f32, Color32::from_rgb(52, 57, 70)))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(Margin::symmetric(10, 5))
            .show(ui, |ui| {
                ui.set_width(bar_w);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(6.0, 0.0);

                    if ui.button("✕").on_hover_text("Close (Esc)").clicked() {
                        close = true;
                    }
                    if ui.button("▼").on_hover_text("Next (Enter)").clicked() {
                        run_dir = Some(1);
                    }
                    if ui.button("▲").on_hover_text("Previous (Shift+Enter)").clicked() {
                        run_dir = Some(-1);
                    }

                    if !state.find_query.is_empty() {
                        ui.label(
                            RichText::new(format!(
                                "{} / {}",
                                state.find_match_index, state.find_match_count
                            ))
                            .size(11.5)
                            .color(Color32::from_rgb(156, 163, 175)),
                        );
                    }

                    let input_w = ui.available_width().max(80.0);
                    let res = ui.add(
                        egui::TextEdit::singleline(&mut state.find_query)
                            .hint_text("Find in page…")
                            .desired_width(input_w),
                    );
                    if focus {
                        res.request_focus();
                    }
                    if res.changed() {
                        run_dir = Some(1);
                    }
                    if (res.has_focus() || res.lost_focus())
                        && ui.input(|i| i.key_pressed(Key::Enter))
                    {
                        let shift = ui.input(|i| i.modifiers.shift);
                        run_dir = Some(if shift { -1 } else { 1 });
                        res.request_focus();
                    }
                });
            });
    });

    if close {
        state.clear_find();
    } else if let Some(dir) = run_dir {
        state.run_find(dir);
    }
}
