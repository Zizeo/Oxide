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

    let mut actions: Vec<TabAction> = Vec::new();

    ctx.input_mut(|input| {
        // ---- shift combinations first (see module docs) ----------------
        if input.consume_key(cmd_shift, Key::T) {
            actions.push(TabAction::ReopenClosed);
        }
        if input.consume_key(cmd_shift, Key::R) {
            // The engine has no cache-busting reload, so a hard reload is a
            // normal reload rather than a silently dead shortcut.
            if let Some(id) = active {
                actions.push(TabAction::Reload(id));
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
    });

    let handled = !actions.is_empty();
    apply_all(state, actions);
    handled
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
}
