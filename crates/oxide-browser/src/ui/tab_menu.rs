//! Tab-strip context menus.
//!
//! # Design
//!
//! These functions only *build menu contents* and return the actions the user
//! picked. They never mutate [`BrowserState`]: the strip applies the returned
//! actions after its layout pass, which is what keeps the borrow checker (and
//! the model invariants) happy. Building menus is therefore also pure enough
//! to reason about: for a given state the set of visible entries is a pure
//! function of the clicked tab/group.
//!
//! Menus follow Chrome's rules: actions that do not apply are hidden or
//! disabled, bulk actions target the whole selection when the clicked tab is
//! part of it, and destructive entries sit in their own section at the bottom.

use crate::browser::{site_key, BrowserState, GroupColor};
use crate::ui::menu::MenuPicks;
use crate::ui::tab_actions::TabAction;
use egui::{Button, Color32, RichText};

/// Pushes `action` when a plain menu entry is clicked.
fn item(ui: &mut egui::Ui, label: &str, action: TabAction, picked: &mut MenuPicks) {
    if ui.button(label).clicked() {
        picked.push(action);
    }
}

/// Same, but the entry can be disabled (`Add → Button::new` keeps the label
/// legible instead of dropping the item, which communicates availability).
fn item_enabled(
    ui: &mut egui::Ui,
    enabled: bool,
    label: &str,
    action: TabAction,
    picked: &mut MenuPicks,
) {
    if ui.add_enabled(enabled, Button::new(label)).clicked() {
        picked.push(action);
    }
}

/// `https://host/path` → `host`, `oxide://welcome` → `oxide://welcome`.
fn host_of(url: &str) -> String {
    let without_scheme = url.split_once("://").map_or(url, |(_, rest)| rest);
    if url.starts_with("oxide://") {
        return url.to_string();
    }
    without_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or(without_scheme)
        .to_string()
}

/// Shortens a title for the menu header without cutting a grapheme in half.
fn short_title(title: &str) -> String {
    const MAX: usize = 48;
    if title.chars().count() <= MAX {
        return title.to_string();
    }
    let mut out: String = title.chars().take(MAX).collect();
    out.push('…');
    out
}

/// Colour for a freshly created group: the palette is cycled so two groups
/// created in a row never share a colour.
fn new_group_color(state: &BrowserState) -> GroupColor {
    state
        .groups
        .last()
        .map_or(GroupColor::Blue, |group| group.color.next())
}

/// How many recently-closed entries a menu offers.
///
/// The stack itself is unbounded, but a context menu that has to scroll is
/// slower to use than "the last few things", which is all a user recognises.
const RECENT_LIMIT: usize = 8;

/// "Recently closed" submenu, shared by the tab and strip menus.
///
/// Omitted entirely when the stack is empty: unlike a disabled row, a submenu
/// with nothing inside cannot even explain itself.
fn recently_closed_menu(ui: &mut egui::Ui, state: &BrowserState, picked: &mut MenuPicks) {
    if state.closed_tabs.is_empty() {
        return;
    }
    ui.menu_button("Recently closed", |ui| {
        for (index, label) in state.recent_closed(RECENT_LIMIT) {
            item(
                ui,
                &short_title(&label),
                TabAction::ReopenClosedAt(index),
                picked,
            );
        }
    });
}

/// "Saved groups" submenu, shared by the tab and strip menus.
///
/// The model appends saved groups, so walking backwards puts the most recent
/// save at the top, where a user looks for it.
fn saved_groups_menu(ui: &mut egui::Ui, state: &BrowserState, picked: &mut MenuPicks) {
    if state.saved_groups.is_empty() {
        return;
    }
    ui.menu_button("Saved groups", |ui| {
        for (index, group) in state.saved_groups.iter().enumerate().rev() {
            let label = format!("{} ({} tabs)", short_title(&group.name), group.urls.len());
            ui.menu_button(label, |ui| {
                item(ui, "Open group", TabAction::ReopenSavedGroup(index), picked);
                item(ui, "Delete", TabAction::DeleteSavedGroup(index), picked);
            });
        }
    });
}

/// Builds the context menu for a tab, or for the empty part of the strip when
/// `tab_id` is `None`.
pub fn build_tab_menu(
    ui: &mut egui::Ui,
    state: &BrowserState,
    tab_id: Option<u64>,
) -> MenuPicks {
    let mut picked = MenuPicks::default();
    let tab = tab_id.and_then(|id| state.tab(id));

    // Bulk targets: the whole selection when the clicked tab belongs to it,
    // otherwise just the clicked tab (Chrome's "preserve selection" rule).
    let targets: Vec<u64> = tab_id.map(|id| state.bulk_targets(id)).unwrap_or_default();

    // ---- header -------------------------------------------------------
    match tab {
        Some(tab) => {
            ui.label(RichText::new(short_title(&tab.title)).strong());
            ui.label(
                RichText::new(host_of(&tab.url))
                    .size(11.0)
                    .color(Color32::from_rgb(140, 145, 155)),
            );
        }
        None => {
            ui.label(RichText::new("Tab strip").strong());
        }
    }
    ui.separator();

    // ---- page ---------------------------------------------------------
    item(ui, "New tab", TabAction::NewTab, &mut picked);
    if tab.is_none() {
        // The strip-wide menu is also the discoverability entry point for
        // tab search.
        item(ui, "Search tabs…", TabAction::OpenTabSearch, &mut picked);
        item(ui, "Bookmark all tabs", TabAction::BookmarkAll, &mut picked);
        // The label names what the click does, so a fully muted strip still
        // offers the way back out.
        let all_muted =
            !state.tabs.is_empty() && state.tabs.iter().all(|tab| state.is_tab_muted(tab));
        item(
            ui,
            if all_muted {
                "Unmute all tabs"
            } else {
                "Mute all tabs"
            },
            TabAction::MuteAll(!all_muted),
            &mut picked,
        );
    }
    if let Some(tab) = tab {
        item(ui, "Reload", TabAction::Reload(tab.id), &mut picked);
        item(ui, "Duplicate tab", TabAction::Duplicate(tab.id), &mut picked);
        // Copying is a side effect on the clipboard, not a model change, so
        // it needs no `TabAction`.
        if ui.button("Copy URL").clicked() {
            ui.ctx().copy_text(tab.url.clone());
            picked.activated = true;
        }
        item(
            ui,
            if tab.pinned { "Unpin tab" } else { "Pin tab" },
            TabAction::TogglePin(tab.id),
            &mut picked,
        );
        // Muting is only worth offering while there is something to hear or
        // an existing mute to undo; otherwise the entry is pure noise.
        let tab_muted = state.is_tab_muted(tab);
        if tab.audio_playing || tab_muted {
            item(
                ui,
                if tab_muted { "Unmute tab" } else { "Mute tab" },
                TabAction::ToggleMuteTab(tab.id),
                &mut picked,
            );
        }
        // Site scope needs a host to key on, so internal pages get no entry.
        if let Some(key) = site_key(&tab.url) {
            item(
                ui,
                if state.muted_sites.contains(&key) {
                    "Unmute site"
                } else {
                    "Mute site"
                },
                TabAction::ToggleMuteSite(tab.id),
                &mut picked,
            );
        }
        item(
            ui,
            if state.is_bookmarked(&tab.url) {
                "Remove bookmark"
            } else {
                "Bookmark this tab"
            },
            TabAction::BookmarkTab(tab.id),
            &mut picked,
        );
    }
    ui.separator();

    // ---- organisation -------------------------------------------------
    if let Some(tab) = tab {
        ui.menu_button("Add to tab group", |ui| {
            item(
                ui,
                "New group",
                TabAction::CreateGroup {
                    ids: targets.clone(),
                    color: new_group_color(state),
                },
                &mut picked,
            );
            if !state.groups.is_empty() {
                ui.separator();
            }
            for group in &state.groups {
                let (r, g, b) = group.color.rgb();
                if ui
                    .button(RichText::new(&group.name).color(Color32::from_rgb(r, g, b)))
                    .clicked()
                {
                    picked.push(TabAction::AddToGroup {
                        ids: targets.clone(),
                        group: group.id,
                    });
                }
            }
            if tab.group.is_some() {
                ui.separator();
                item(
                    ui,
                    "Remove from group",
                    TabAction::RemoveFromGroup {
                        ids: targets.clone(),
                    },
                    &mut picked,
                );
            }
        });

        // Saving snapshots the group's current members, so it belongs with
        // the other organisation entries rather than near the closing ones.
        if let Some(group) = tab.group {
            item(ui, "Save group", TabAction::SaveGroup(group), &mut picked);
        }
    }

    // Both histories belong to the whole strip, so the same submenus appear
    // whether a tab or the bare strip was clicked.
    saved_groups_menu(ui, state, &mut picked);
    recently_closed_menu(ui, state, &mut picked);

    // ---- closing ------------------------------------------------------
    ui.separator();
    item(ui, "Select all tabs", TabAction::SelectAll, &mut picked);
    if state.selected_tab_ids.len() > 1 {
        item(
            ui,
            "Close selected tabs",
            TabAction::CloseSelected,
            &mut picked,
        );
    }
    if let Some(tab) = tab {
        item_enabled(
            ui,
            state.tab_index(tab.id).is_some_and(|i| i > 0),
            "Close tabs to the left",
            TabAction::CloseToLeft(tab.id),
            &mut picked,
        );
        item_enabled(
            ui,
            state.tab_index(tab.id).is_some_and(|i| i + 1 < state.tabs.len()),
            "Close tabs to the right",
            TabAction::CloseToRight(tab.id),
            &mut picked,
        );
        item_enabled(
            ui,
            state.tabs.len() > 1,
            "Close other tabs",
            TabAction::CloseOthers(tab.id),
            &mut picked,
        );
    }

    // ---- destructive --------------------------------------------------
    ui.separator();
    if let Some(tab) = tab {
        item(ui, "Close tab", TabAction::Close(tab.id), &mut picked);
    }
    // The entry says *what* comes back, so reopening is predictable instead
    // of a surprise. Only the in-memory stack is offered; nothing survives a
    // restart.
    let reopen = ui.add_enabled(
        !state.closed_tabs.is_empty(),
        Button::new("Reopen closed tab"),
    );
    let reopen = match state.closed_tabs.last() {
        Some(tab) => reopen.on_hover_text(format!("Reopen {}", short_title(&tab.title))),
        None => reopen.on_disabled_hover_text("Nothing to reopen"),
    };
    if reopen.clicked() {
        picked.push(TabAction::ReopenClosed);
    }
    item(
        ui,
        "Inspect (DevTools)",
        TabAction::ToggleDevtools,
        &mut picked,
    );

    picked
}

/// Builds the context menu shown when a group chip is right-clicked.
pub fn build_group_menu(
    ui: &mut egui::Ui,
    state: &BrowserState,
    group_id: u64,
) -> MenuPicks {
    let mut picked = MenuPicks::default();
    let Some(group) = state.group(group_id) else {
        return picked;
    };

    let (r, g, b) = group.color.rgb();
    let accent = Color32::from_rgb(r, g, b);
    let members = state.group_tabs(group_id).len();

    // ---- header -------------------------------------------------------
    ui.label(RichText::new(&group.name).strong().color(accent));
    ui.label(
        RichText::new(match members {
            1 => "1 tab".to_string(),
            n => format!("{n} tabs"),
        })
        .size(11.0)
        .color(Color32::from_rgb(140, 145, 155)),
    );
    ui.separator();

    // ---- group actions ------------------------------------------------
    item(
        ui,
        "New tab in group",
        TabAction::NewTabInGroup(group_id),
        &mut picked,
    );
    item(
        ui,
        "Rename group",
        TabAction::BeginRenameGroup(group_id),
        &mut picked,
    );

    ui.menu_button("Change color", |ui| {
        for color in GroupColor::ALL {
            let (r, g, b) = color.rgb();
            let label = if color == group.color {
                format!("{} (current)", color.label())
            } else {
                color.label().to_string()
            };
            if ui
                .button(RichText::new(label).color(Color32::from_rgb(r, g, b)))
                .clicked()
            {
                picked.push(TabAction::SetGroupColor {
                    group: group_id,
                    color,
                });
            }
        }
    });

    item(
        ui,
        if group.collapsed {
            "Expand group"
        } else {
            "Collapse group"
        },
        TabAction::ToggleGroupCollapsed(group_id),
        &mut picked,
    );
    item(
        ui,
        "Select tabs in group",
        TabAction::SelectGroup(group_id),
        &mut picked,
    );
    // The chip already names the group, so saving needs no dialog: the
    // snapshot is taken from the members as they are right now.
    item(
        ui,
        "Save group",
        TabAction::SaveGroup(group_id),
        &mut picked,
    );

    // ---- destructive --------------------------------------------------
    ui.separator();
    item(ui, "Ungroup", TabAction::Ungroup(group_id), &mut picked);
    item(
        ui,
        "Close group",
        TabAction::CloseGroup(group_id),
        &mut picked,
    );

    picked
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_is_extracted_without_scheme_or_path() {
        assert_eq!(host_of("https://example.com/a/b?q=1"), "example.com");
        assert_eq!(host_of("http://localhost:8080/"), "localhost:8080");
        assert_eq!(host_of("oxide://welcome"), "oxide://welcome");
    }

    #[test]
    fn long_titles_are_ellipsized() {
        let title = "a".repeat(100);
        let short = short_title(&title);
        assert_eq!(short.chars().count(), 49);
        assert!(short.ends_with('…'));
        assert_eq!(short_title("short"), "short");
    }
}
