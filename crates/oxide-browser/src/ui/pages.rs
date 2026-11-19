//! Native egui internal management pages (`oxide://history`,
//! `oxide://downloads`, `oxide://bookmarks`, `oxide://settings`) and the
//! Clear Browsing Data modal.

use crate::browser::{
    unix_now, Bookmark, BrowserState, ClearRange, DownloadStatus, SearchEngine, StartupMode,
};
use egui::{Color32, CornerRadius, Margin, RichText, ScrollArea, Stroke, Ui};
use std::sync::atomic::Ordering;

pub fn render_internal_page(ui: &mut Ui, state: &mut BrowserState, url: &str) {
    let rect = ui.available_rect_before_wrap();
    ui.painter()
        .rect_filled(rect, 0.0, Color32::from_rgb(17, 18, 22));

    ScrollArea::vertical().show(ui, |ui| {
        ui.add_space(24.0);
        let pad = ((ui.available_width() - 860.0).max(48.0) * 0.5).floor() as i8;
        egui::Frame::NONE
            .inner_margin(Margin::symmetric(pad, 0))
            .show(ui, |ui| match url {
                "oxide://history" => render_history_page(ui, state),
                "oxide://downloads" => render_downloads_page(ui, state),
                "oxide://bookmarks" => render_bookmarks_page(ui, state),
                "oxide://settings" => render_settings_page(ui, state),
                _ => {}
            });
        ui.add_space(24.0);
    });
}

fn render_history_page(ui: &mut Ui, state: &mut BrowserState) {
    ui.horizontal_wrapped(|ui| {
        ui.heading(RichText::new("History").color(Color32::WHITE).size(22.0));
        ui.add_space(12.0);
        ui.add(
            egui::TextEdit::singleline(&mut state.history_search)
                .hint_text("Search history…")
                .desired_width(240.0),
        );
        if ui.button("Clear browsing data…").clicked() {
            state.clear_data_modal_open = true;
        }
    });
    ui.add_space(12.0);
    ui.separator();
    ui.add_space(8.0);

    if state.is_incognito {
        ui.label(
            RichText::new("Incognito mode: browsing history is not recorded.")
                .color(Color32::from_rgb(160, 165, 180)),
        );
        return;
    }

    let query = state.history_search.trim().to_ascii_lowercase();
    let now = unix_now();
    let day_secs = 86_400;
    let mut open_url: Option<String> = None;
    let mut remove_idx: Option<usize> = None;
    let mut last_group: Option<&'static str> = None;

    for (idx, entry) in state.history.iter().enumerate().rev() {
        if !query.is_empty()
            && !entry.title.to_ascii_lowercase().contains(&query)
            && !entry.url.to_ascii_lowercase().contains(&query)
        {
            continue;
        }
        let age = now.saturating_sub(entry.visited_at);
        let group = if age < day_secs {
            "Today"
        } else if age < 2 * day_secs {
            "Yesterday"
        } else {
            "Older"
        };
        if last_group != Some(group) {
            ui.add_space(8.0);
            ui.label(
                RichText::new(group)
                    .size(14.0)
                    .strong()
                    .color(Color32::from_rgb(120, 175, 255)),
            );
            last_group = Some(group);
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.small_button("✕").on_hover_text("Remove").clicked() {
                remove_idx = Some(idx);
            }
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                if ui.add(egui::Link::new(&entry.title)).clicked() {
                    open_url = Some(entry.url.clone());
                }
                ui.add(
                    egui::Label::new(RichText::new(&entry.url).size(11.5).color(Color32::GRAY))
                        .truncate(),
                );
            });
        });
    }

    if state.history.is_empty() {
        egui::Frame::NONE
            .fill(Color32::from_rgb(26, 28, 35))
            .stroke(Stroke::new(1.0_f32, Color32::from_rgb(46, 51, 64)))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(Margin::same(18))
            .show(ui, |ui| {
                ui.label(
                    RichText::new("Your flight log is empty")
                        .size(14.5)
                        .strong()
                        .color(Color32::WHITE),
                );
                ui.add_space(4.0);
                ui.label(
                    RichText::new("Pages you visit in regular windows appear here grouped by day. Incognito windows leave no trace.")
                        .size(12.5)
                        .color(Color32::from_rgb(156, 163, 175)),
                );
                ui.add_space(10.0);
                if ui.button("Open Welcome Page").clicked() {
                    open_url = Some("oxide://welcome".to_string());
                }
            });
    }

    if let Some(idx) = remove_idx {
        state.history.remove(idx);
    }
    if let Some(url) = open_url {
        state.navigate_active_tab(url);
    }
}

fn render_downloads_page(ui: &mut Ui, state: &mut BrowserState) {
    ui.horizontal(|ui| {
        ui.heading(RichText::new("Downloads").color(Color32::WHITE).size(22.0));
        ui.add_space(16.0);
        ui.label(
            RichText::new(format!("Folder: {}", state.settings.download_dir.display()))
                .size(12.0)
                .color(Color32::GRAY),
        );
    });
    ui.add_space(12.0);
    ui.separator();
    ui.add_space(8.0);

    if state.downloads.is_empty() {
        let dl_input_id = ui.make_persistent_id("oxide_manual_download_url");
        let mut draft = ui
            .ctx()
            .data(|d| d.get_temp::<String>(dl_input_id))
            .unwrap_or_default();
        let mut start_url: Option<String> = None;

        egui::Frame::NONE
            .fill(Color32::from_rgb(26, 28, 35))
            .stroke(Stroke::new(1.0_f32, Color32::from_rgb(46, 51, 64)))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(Margin::same(18))
            .show(ui, |ui| {
                ui.label(
                    RichText::new("No active or recent downloads")
                        .size(14.5)
                        .strong()
                        .color(Color32::WHITE),
                );
                ui.add_space(4.0);
                ui.label(
                    RichText::new("Files downloaded from the web or started directly below stream into your Downloads folder with pause and cancel controls.")
                        .size(12.5)
                        .color(Color32::from_rgb(156, 163, 175)),
                );
                ui.add_space(10.0);
                ui.horizontal_wrapped(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut draft)
                            .hint_text("https://example.com/file.zip")
                            .desired_width(280.0),
                    );
                    if ui.button("Download URL").clicked() && !draft.trim().is_empty() {
                        start_url = Some(draft.trim().to_string());
                        draft.clear();
                    }
                });
            });

        ui.ctx().data_mut(|d| d.insert_temp(dl_input_id, draft));
        if let Some(u) = start_url {
            state.start_download(&u);
        }
        return;
    }

    let mut clear_done = false;
    for dl in state.downloads.iter_mut().rev() {
        egui::Frame::NONE
            .fill(Color32::from_rgb(28, 31, 40))
            .stroke(Stroke::new(1.0_f32, Color32::from_rgb(48, 52, 66)))
            .corner_radius(CornerRadius::same(6))
            .inner_margin(Margin::same(10))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&dl.filename).strong().color(Color32::WHITE))
                        .on_hover_text(&dl.url);
                    ui.add(
                        egui::Label::new(
                            RichText::new(dl.save_path.to_string_lossy().as_ref())
                                .size(11.0)
                                .color(Color32::GRAY),
                        )
                        .truncate(),
                    );
                });
                let progress_text = match &dl.status {
                    DownloadStatus::InProgress => match dl.total_bytes {
                        Some(tot) if tot > 0 => format!(
                            "Downloading: {} / {} bytes ({}%)",
                            dl.bytes_received,
                            tot,
                            (dl.bytes_received * 100) / tot
                        ),
                        _ => format!("Downloading: {} bytes", dl.bytes_received),
                    },
                    DownloadStatus::Paused => format!("Paused ({} bytes)", dl.bytes_received),
                    DownloadStatus::Completed => format!("Completed ({} bytes)", dl.bytes_received),
                    DownloadStatus::Cancelled => "Cancelled".to_string(),
                    DownloadStatus::Failed(err) => format!("Failed: {err}"),
                };
                ui.horizontal(|ui| {
                    ui.label(RichText::new(progress_text).size(12.0).color(Color32::LIGHT_GRAY));
                    match dl.status {
                        DownloadStatus::InProgress => {
                            if ui.button("Pause").clicked() {
                                dl.paused.store(true, Ordering::Relaxed);
                                dl.status = DownloadStatus::Paused;
                            }
                            if ui.button("Cancel").clicked() {
                                dl.cancelled.store(true, Ordering::Relaxed);
                                dl.status = DownloadStatus::Cancelled;
                            }
                        }
                        DownloadStatus::Paused => {
                            if ui.button("Resume").clicked() {
                                dl.paused.store(false, Ordering::Relaxed);
                                dl.status = DownloadStatus::InProgress;
                            }
                            if ui.button("Cancel").clicked() {
                                dl.cancelled.store(true, Ordering::Relaxed);
                                dl.status = DownloadStatus::Cancelled;
                            }
                        }
                        _ => {}
                    }
                });
            });
        ui.add_space(6.0);
    }

    if ui.button("Clear completed").clicked() {
        clear_done = true;
    }
    if clear_done {
        state
            .downloads
            .retain(|d| matches!(d.status, DownloadStatus::InProgress | DownloadStatus::Paused));
    }
}

fn render_bookmarks_page(ui: &mut Ui, state: &mut BrowserState) {
    let status_id = ui.make_persistent_id("oxide_bookmarks_io_status");
    let mut io_status = ui
        .ctx()
        .data(|d| d.get_temp::<String>(status_id))
        .unwrap_or_default();

    ui.horizontal_wrapped(|ui| {
        ui.heading(
            RichText::new("Bookmarks & Reading List")
                .color(Color32::WHITE)
                .size(22.0),
        );
        ui.add_space(12.0);
        ui.add(
            egui::TextEdit::singleline(&mut state.bookmarks_search)
                .hint_text("Search bookmarks…")
                .desired_width(220.0),
        );
        if ui.button("Export HTML").clicked() {
            let html = export_bookmarks_html(&state.bookmarks);
            let path = state.settings.download_dir.join("oxide-bookmarks.html");
            let _ = std::fs::create_dir_all(&state.settings.download_dir);
            io_status = match std::fs::write(&path, html) {
                Ok(()) => format!("Exported {} bookmarks to {}", state.bookmarks.len(), path.display()),
                Err(e) => format!("Export failed: {e}"),
            };
        }
        if ui.button("Import HTML").clicked() {
            let path = state.settings.download_dir.join("oxide-bookmarks.html");
            io_status = match std::fs::read_to_string(&path) {
                Ok(raw) => {
                    let mut added = 0usize;
                    for bm in import_bookmarks_html(&raw) {
                        if !state.bookmarks.iter().any(|b| b.url == bm.url) {
                            state.bookmarks.push(bm);
                            added += 1;
                        }
                    }
                    format!("Imported {added} bookmark(s) from {}", path.display())
                }
                Err(e) => format!("Import failed ({}): {e}", path.display()),
            };
        }
    });
    if !io_status.is_empty() {
        ui.add_space(4.0);
        ui.label(
            RichText::new(&io_status)
                .size(11.5)
                .color(Color32::from_rgb(96, 165, 250)),
        );
    }
    ui.ctx().data_mut(|d| d.insert_temp(status_id, io_status));
    ui.add_space(12.0);
    ui.separator();
    ui.add_space(8.0);

    let query = state.bookmarks_search.trim().to_ascii_lowercase();
    let mut remove_bm: Option<usize> = None;
    let mut open_url: Option<String> = None;

    ui.label(RichText::new("Bookmarks").size(15.0).strong().color(Color32::WHITE));
    ui.add_space(4.0);
    for idx in 0..state.bookmarks.len() {
        let matches = query.is_empty()
            || state.bookmarks[idx].title.to_ascii_lowercase().contains(&query)
            || state.bookmarks[idx].url.to_ascii_lowercase().contains(&query);
        if !matches {
            continue;
        }
        if state.editing_bookmark == Some(idx) {
            ui.horizontal(|ui| {
                ui.text_edit_singleline(&mut state.bookmarks[idx].title);
                ui.text_edit_singleline(&mut state.bookmarks[idx].url);
                if ui.button("Done").clicked() {
                    state.editing_bookmark = None;
                }
            });
        } else {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("✕").clicked() {
                    remove_bm = Some(idx);
                }
                if ui.small_button("Edit").clicked() {
                    state.editing_bookmark = Some(idx);
                }
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    if ui.link(&state.bookmarks[idx].title).clicked() {
                        open_url = Some(state.bookmarks[idx].url.clone());
                    }
                    ui.add(
                        egui::Label::new(
                            RichText::new(&state.bookmarks[idx].url)
                                .size(11.5)
                                .color(Color32::GRAY),
                        )
                        .truncate(),
                    );
                });
            });
        }
    }
    if let Some(idx) = remove_bm {
        state.bookmarks.remove(idx);
        state.editing_bookmark = None;
    }

    ui.add_space(16.0);
    ui.separator();
    ui.add_space(8.0);
    ui.label(RichText::new("Reading List").size(15.0).strong().color(Color32::WHITE));
    ui.add_space(4.0);

    let mut remove_rl: Option<usize> = None;
    for (idx, item) in state.reading_list.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            ui.checkbox(&mut item.read, "");
            if ui.link(&item.title).clicked() {
                open_url = Some(item.url.clone());
            }
            ui.label(RichText::new(&item.url).size(11.5).color(Color32::GRAY));
            if ui.small_button("✕").clicked() {
                remove_rl = Some(idx);
            }
        });
    }
    if state.reading_list.is_empty() {
        egui::Frame::NONE
            .fill(Color32::from_rgb(26, 28, 35))
            .stroke(Stroke::new(1.0_f32, Color32::from_rgb(46, 51, 64)))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(Margin::same(16))
            .show(ui, |ui| {
                ui.label(
                    RichText::new("Nothing queued for later")
                        .size(14.0)
                        .strong()
                        .color(Color32::WHITE),
                );
                ui.add_space(4.0);
                ui.label(
                    RichText::new("Save articles from the main menu or with Ctrl+Shift+O to keep them out of your tab strip until you're ready.")
                        .size(12.0)
                        .color(Color32::from_rgb(156, 163, 175)),
                );
            });
    }
    if let Some(idx) = remove_rl {
        state.reading_list.remove(idx);
    }
    if let Some(url) = open_url {
        state.navigate_active_tab(url);
    }
}

fn render_settings_page(ui: &mut Ui, state: &mut BrowserState) {
    ui.heading(RichText::new("Settings").color(Color32::WHITE).size(22.0));
    ui.add_space(12.0);
    ui.separator();
    ui.add_space(10.0);

    // Search Engine
    ui.label(RichText::new("Search engine").size(15.0).strong().color(Color32::WHITE));
    ui.horizontal_wrapped(|ui| {
        ui.selectable_value(&mut state.settings.search_engine, SearchEngine::Google, "Google");
        ui.selectable_value(&mut state.settings.search_engine, SearchEngine::DuckDuckGo, "DuckDuckGo");
        ui.selectable_value(&mut state.settings.search_engine, SearchEngine::Bing, "Bing");
        ui.selectable_value(&mut state.settings.search_engine, SearchEngine::Custom, "Custom");
    });
    if state.settings.search_engine == SearchEngine::Custom {
        ui.text_edit_singleline(&mut state.settings.custom_search_url);
    }
    ui.add_space(12.0);

    // On Startup
    ui.label(RichText::new("On startup").size(15.0).strong().color(Color32::WHITE));
    ui.horizontal_wrapped(|ui| {
        ui.selectable_value(
            &mut state.settings.startup_mode,
            StartupMode::Welcome,
            "Open the New Tab page",
        );
        ui.selectable_value(
            &mut state.settings.startup_mode,
            StartupMode::ContinueSession,
            "Continue where you left off",
        );
        ui.selectable_value(
            &mut state.settings.startup_mode,
            StartupMode::CustomUrls,
            "Open a specific page",
        );
    });
    if state.settings.startup_mode == StartupMode::CustomUrls {
        let mut joined = state.settings.startup_urls.join(", ");
        if ui
            .add(egui::TextEdit::singleline(&mut joined).hint_text("Comma-separated URLs…"))
            .changed()
        {
            state.settings.startup_urls = joined
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
        }
    }
    ui.add_space(12.0);

    // Appearance
    ui.label(RichText::new("Appearance").size(15.0).strong().color(Color32::WHITE));
    ui.checkbox(&mut state.settings.show_bookmarks_bar, "Show bookmarks bar");
    ui.horizontal(|ui| {
        ui.label("Default page zoom:");
        ui.add(egui::Slider::new(&mut state.settings.default_zoom, 0.5..=2.0).step_by(0.1));
    });
    ui.add_space(12.0);

    // Privacy & Security
    ui.label(
        RichText::new("Privacy and security")
            .size(15.0)
            .strong()
            .color(Color32::WHITE),
    );
    if ui
        .checkbox(&mut state.settings.https_only, "Always use secure connections (HTTPS-Only)")
        .changed()
    {
        state.apply_settings_to_policy();
    }
    if ui
        .checkbox(
            &mut state.settings.block_private_networks,
            "Block private and local network requests (SSRF Guard)",
        )
        .changed()
    {
        state.apply_settings_to_policy();
    }
    if ui.button("Clear browsing data…").clicked() {
        state.clear_data_modal_open = true;
    }
    ui.add_space(12.0);

    // Downloads
    ui.label(RichText::new("Downloads").size(15.0).strong().color(Color32::WHITE));
    let mut dir_str = state.settings.download_dir.to_string_lossy().into_owned();
    if ui.text_edit_singleline(&mut dir_str).changed() {
        state.settings.download_dir = std::path::PathBuf::from(dir_str);
    }
    ui.add_space(12.0);

    // Performance
    ui.label(RichText::new("Performance").size(15.0).strong().color(Color32::WHITE));
    ui.checkbox(
        &mut state.settings.memory_saver,
        "Memory Saver (automatically unload inactive background tabs)",
    );
}

pub fn render_task_manager(ctx: &egui::Context, state: &mut BrowserState) {
    if !state.task_manager_open {
        return;
    }
    let mut open = true;
    let mut unload_id: Option<u64> = None;

    egui::Window::new("Task Manager")
        .open(&mut open)
        .collapsible(false)
        .default_width(560.0)
        .show(ctx, |ui| {
            ui.label(
                RichText::new("Active Tabs & Renderer Processes")
                    .strong()
                    .color(Color32::WHITE),
            );
            ui.add_space(6.0);
            for tab in &state.tabs {
                let is_live = state.webviews.contains_key(&tab.id);
                let dom_nodes = tab.document.as_ref().map_or(0, |d| d.len());
                let hist_len = tab.history.len();
                let status = if Some(tab.id) == state.active_tab_id {
                    "Active"
                } else if tab.discarded || !is_live {
                    "Discarded"
                } else {
                    "Background"
                };

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if is_live
                        && Some(tab.id) != state.active_tab_id
                        && ui.small_button("End Process").clicked()
                    {
                        unload_id = Some(tab.id);
                    }
                    ui.label(
                        RichText::new(format!("{dom_nodes} DOM nodes · {hist_len} history"))
                            .size(11.0)
                            .color(Color32::LIGHT_GRAY),
                    );
                    ui.label(RichText::new(format!("[{status}]")).size(11.0).color(Color32::GRAY));
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        ui.add(
                            egui::Label::new(RichText::new(&tab.title).strong()).truncate(),
                        );
                    });
                });
            }
        });

    if !open {
        state.task_manager_open = false;
    }
    if let Some(id) = unload_id {
        if let Some(wv) = state.webviews.remove(&id) {
            let _ = wv.set_visible(false);
        }
        state.webview_loaded_url.remove(&id);
        if let Some(tab) = state.tab_mut(id) {
            tab.discarded = true;
            tab.audio_playing = false;
        }
    }
}

pub fn render_clear_data_modal(ctx: &egui::Context, state: &mut BrowserState) {
    if !state.clear_data_modal_open {
        return;
    }
    let mut open = true;
    let mut confirm = false;
    egui::Window::new("Clear browsing data")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            ui.label("Time range:");
            ui.horizontal(|ui| {
                ui.selectable_value(&mut state.clear_data_range, ClearRange::LastHour, "Last hour");
                ui.selectable_value(
                    &mut state.clear_data_range,
                    ClearRange::Last24Hours,
                    "Last 24 hours",
                );
                ui.selectable_value(&mut state.clear_data_range, ClearRange::AllTime, "All time");
            });
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui.button("Clear data").clicked() {
                    confirm = true;
                }
                if ui.button("Cancel").clicked() {
                    state.clear_data_modal_open = false;
                }
            });
        });
    if !open {
        state.clear_data_modal_open = false;
    }
    if confirm {
        let range = state.clear_data_range;
        state.clear_browsing_data(range);
    }
}

pub fn export_bookmarks_html(bookmarks: &[Bookmark]) -> String {
    let mut out = String::from("<!DOCTYPE NETSCAPE-Bookmark-file-1>\n<TITLE>Bookmarks</TITLE>\n<H1>Bookmarks</H1>\n<DL><p>\n");
    for bm in bookmarks {
        out.push_str(&format!(
            "    <DT><A HREF=\"{}\">{}</A>\n",
            bm.url.replace('"', "&quot;"),
            bm.title.replace('<', "&lt;").replace('>', "&gt;")
        ));
    }
    out.push_str("</DL><p>\n");
    out
}

pub fn import_bookmarks_html(html: &str) -> Vec<Bookmark> {
    let mut out = Vec::new();
    for line in html.lines() {
        let lower = line.to_ascii_lowercase();
        let Some(href_pos) = lower.find("href=\"") else {
            continue;
        };
        let rest = &line[href_pos + 6..];
        let Some(end_quote) = rest.find('"') else {
            continue;
        };
        let url = rest[..end_quote].to_string();
        let title = rest[end_quote..]
            .find('>')
            .and_then(|gt| {
                let after = &rest[end_quote + gt + 1..];
                after.find('<').map(|lt| after[..lt].trim().to_string())
            })
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| url.clone());
        out.push(Bookmark {
            title,
            url,
            folder: None,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bookmarks_html_round_trips() {
        let input = vec![Bookmark {
            title: "Example".to_string(),
            url: "https://example.com/".to_string(),
            folder: None,
        }];
        let html = export_bookmarks_html(&input);
        let parsed = import_bookmarks_html(&html);
        assert_eq!(parsed, input);
    }
}
