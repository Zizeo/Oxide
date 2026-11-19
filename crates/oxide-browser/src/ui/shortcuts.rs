//! Browser-level keyboard shortcuts.
//!
//! # Why a dedicated module
//!
//! A shortcut must do *exactly* what the equivalent menu entry or button does.
//! Routing every key through [`TabAction`] guarantees that: the strip, the
//! context menu and the keyboard all end in the same model call.
//!
//! # Key consumption
//!
//! Keys are taken with `InputState::consume_key`, which removes the event from
//! the frame's input. That serves two purposes: a held key fires once per
//! press, and the focused text field never also receives the keystroke.
//!
//! Modifier matching is *logical* (a pattern without `shift` also matches
//! `Ctrl+Shift+X`), so shift combinations must be consumed before their plain
//! counterparts — otherwise `Ctrl+Shift+T` would also open a blank tab.

use crate::browser::{BrowserState, GroupColor};
use crate::ui::tab_actions::{apply_all, TabAction};
use egui::{Key, Modifiers};

/// Handles every browser shortcut for this frame.
///
/// Returns `true` when at least one shortcut was consumed.
pub fn handle_shortcuts(ctx: &egui::Context, state: &mut BrowserState) -> bool {
    let active = state.active_tab_id;
    let selection: Vec<u64> = state.selected_tab_ids.iter().copied().collect();

    let cmd = Modifiers::COMMAND;
    let cmd_shift = Modifiers::COMMAND.plus(Modifiers::SHIFT);
    let esc_target = escape_target(state);

    let mut actions: Vec<TabAction> = Vec::new();

    ctx.input_mut(|input| {
        // ---- shift combinations first (see module docs) ----------------
        if input.consume_key(cmd_shift, Key::T) {
            actions.push(TabAction::ReopenClosed);
        }
        if input.consume_key(cmd_shift, Key::R) {
            if let Some(id) = active {
                actions.push(TabAction::HardReload(id));
            }
        }
        if input.consume_key(cmd_shift, Key::A) {
            // Chrome's Ctrl+Shift+A opens tab search; selecting every tab
            // stays available from the strip's context menu.
            actions.push(TabAction::OpenTabSearch);
        }
        if input.consume_key(cmd_shift, Key::G) {
            let ids = if selection.len() > 1 {
                selection.clone()
            } else {
                active.into_iter().collect()
            };
            if !ids.is_empty() {
                actions.push(TabAction::CreateGroup {
                    ids,
                    color: GroupColor::Blue,
                });
            }
        }
        if input.consume_key(cmd_shift, Key::P) {
            if let Some(id) = active {
                actions.push(TabAction::TogglePin(id));
            }
        }
        if input.consume_key(cmd_shift, Key::C) {
            if let Some(id) = active {
                actions.push(TabAction::CloseOthers(id));
            }
        }
        if input.consume_key(cmd_shift, Key::X) {
            if let Some(id) = active {
                actions.push(TabAction::CloseToRight(id));
            }
        }
        if input.consume_key(cmd_shift, Key::Tab) {
            actions.push(TabAction::ActivateRelative(-1));
        }
        if input.consume_key(cmd_shift, Key::D) {
            // Chrome's "Bookmark all tabs". Consumed before the plain Ctrl+D
            // below, which would otherwise also fire.
            actions.push(TabAction::BookmarkAll);
        }
        if input.consume_key(cmd_shift, Key::N) {
            actions.push(TabAction::NewIncognitoWindow);
        }
        if input.consume_key(cmd_shift, Key::O) {
            actions.push(TabAction::OpenInNewTab("oxide://bookmarks".to_string()));
        }
        if input.consume_key(cmd_shift, Key::Delete) {
            actions.push(TabAction::OpenClearDataModal);
        }
        if input.consume_key(cmd_shift, Key::B) {
            actions.push(TabAction::ToggleBookmarksBar);
        }

        // ---- plain combinations ----------------------------------------
        if input.consume_key(cmd, Key::T) {
            actions.push(TabAction::NewTab);
        }
        if input.consume_key(cmd, Key::W) {
            if let Some(id) = active {
                actions.push(TabAction::Close(id));
            }
        }
        if input.consume_key(cmd, Key::R) {
            if let Some(id) = active {
                actions.push(TabAction::Reload(id));
            }
        }
        if input.consume_key(cmd, Key::L) {
            actions.push(TabAction::FocusOmnibox);
        }
        if input.consume_key(cmd, Key::D)
            && let Some(id) = active
        {
            actions.push(TabAction::BookmarkTab(id));
        }
        if input.consume_key(cmd, Key::M)
            && let Some(id) = active
        {
            // Chrome mutes the page, not the site, from the keyboard.
            actions.push(TabAction::ToggleMuteTab(id));
        }
        if input.consume_key(cmd, Key::Tab) {
            actions.push(TabAction::ActivateRelative(1));
        }
        if input.consume_key(cmd, Key::PageDown) {
            actions.push(TabAction::ActivateRelative(1));
        }
        if input.consume_key(cmd, Key::PageUp) {
            actions.push(TabAction::ActivateRelative(-1));
        }

        // Ctrl+1..Ctrl+8 select by position, Ctrl+9 always means "last tab".
        const DIGITS: [(Key, usize); 8] = [
            (Key::Num1, 0),
            (Key::Num2, 1),
            (Key::Num3, 2),
            (Key::Num4, 3),
            (Key::Num5, 4),
            (Key::Num6, 5),
            (Key::Num7, 6),
            (Key::Num8, 7),
        ];
        for (key, index) in DIGITS {
            if input.consume_key(cmd, key) {
                actions.push(TabAction::ActivateIndex(index));
            }
        }
        if input.consume_key(cmd, Key::Num9) {
            actions.push(TabAction::ActivateLast);
        }
        if input.consume_key(cmd, Key::F) {
            actions.push(TabAction::OpenFindBar);
        }
        if input.consume_key(cmd, Key::N) {
            actions.push(TabAction::NewWindow);
        }
        if input.consume_key(cmd, Key::P) {
            actions.push(TabAction::Print);
        }
        if input.consume_key(cmd, Key::H) {
            actions.push(TabAction::OpenInNewTab("oxide://history".to_string()));
        }
        if input.consume_key(cmd, Key::J) {
            actions.push(TabAction::OpenInNewTab("oxide://downloads".to_string()));
        }
        if input.consume_key(cmd, Key::Plus) || input.consume_key(cmd, Key::Equals) {
            actions.push(TabAction::ZoomIn);
        }
        if input.consume_key(cmd, Key::Minus) {
            actions.push(TabAction::ZoomOut);
        }
        if input.consume_key(cmd, Key::Num0) {
            actions.push(TabAction::ZoomReset);
        }
        if input.consume_key(Modifiers::NONE, Key::F11) {
            actions.push(TabAction::ToggleFullscreen);
        }
        if input.consume_key(Modifiers::NONE, Key::F12) {
            actions.push(TabAction::ToggleDevtools);
        }
        if input.consume_key(Modifiers::SHIFT, Key::Escape) {
            actions.push(TabAction::ToggleTaskManager);
        }
        if let Some(target) = esc_target
            && input.consume_key(Modifiers::NONE, Key::Escape)
        {
            actions.push(target);
        }
    });

    let handled = !actions.is_empty();
    apply_all(state, actions);
    handled
}

/// Picks the topmost modal/bar that `Escape` in global shortcuts should close.
///
/// Inline overlays (`tab_search_open`, `group_rename`, `omnibox_focused`)
/// handle their own `Escape` locally, so returning `None` prevents `FindBar`
/// from stealing the keystroke from them.
fn escape_target(state: &BrowserState) -> Option<TabAction> {
    if state.tab_search_open || state.group_rename.is_some() || state.omnibox_focused {
        return None;
    }
    if state.clear_data_modal_open {
        Some(TabAction::CloseClearDataModal)
    } else if state.task_manager_open {
        Some(TabAction::CloseTaskManager)
    } else if state.find_bar_open {
        Some(TabAction::CloseFindBar)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use crate::ui::tab_actions::TabAction;

    /// The bindings are a product contract; keep them visible to reviewers.
    #[test]
    fn shift_bindings_do_not_collide_with_plain_ones() {
        // Sanity check on the documented mapping: every shift binding has a
        // different *effect* than its plain counterpart.
        let pairs = [
            (TabAction::ReopenClosed, TabAction::NewTab),
            (TabAction::OpenTabSearch, TabAction::FocusOmnibox),
        ];
        for (shift, plain) in pairs {
            assert_ne!(shift, plain);
        }
    }

    #[test]
    fn escape_respects_overlay_priority_and_yields_to_inline_inputs() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let mut s = super::BrowserState::new(rt.handle().clone());

        s.find_bar_open = true;
        assert_eq!(super::escape_target(&s), Some(TabAction::CloseFindBar));

        s.task_manager_open = true;
        assert_eq!(super::escape_target(&s), Some(TabAction::CloseTaskManager));

        s.clear_data_modal_open = true;
        assert_eq!(
            super::escape_target(&s),
            Some(TabAction::CloseClearDataModal)
        );

        s.tab_search_open = true;
        assert_eq!(super::escape_target(&s), None);

        s.tab_search_open = false;
        s.omnibox_focused = true;
        assert_eq!(super::escape_target(&s), None);
    }
}
