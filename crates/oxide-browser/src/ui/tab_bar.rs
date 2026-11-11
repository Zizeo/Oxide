//! Tab strip rendering with Chromium-style dynamic tab widths.
//!
//! # Layout model
//!
//! Every frame we measure how much width is actually available for the strip
//! (window width minus the "+" button and the inter-slot gaps) and recompute
//! the normal-tab width from scratch. Nothing is cached, so window resizing,
//! opening and closing tabs all "just work" — the layout is a pure function of
//! the current width and the slot count.
//!
//! Pinned tabs and group chips have fixed widths; only normal tabs are elastic.
//! For `n` normal tabs the *ideal* width is the leftover space divided by `n`.
//! That value is clamped into [`TAB_MIN_WIDTH`]..[`TAB_MAX_WIDTH`]:
//!
//! * few tabs → the ideal exceeds the maximum, so every tab stops growing at
//!   [`TAB_MAX_WIDTH`] and the strip hugs the last slot (a single tab does not
//!   stretch across the whole window, matching Chromium);
//! * many tabs → the ideal drops below the minimum, tabs stop shrinking at
//!   [`TAB_MIN_WIDTH`] and the strip becomes horizontally scrollable instead of
//!   squashing tabs into unusable slivers.
//!
//! `ideal < TAB_MIN_WIDTH` is therefore exactly the overflow condition, and it
//! is what selects between "fit" and "scroll" mode.
//!
//! # Mutations
//!
//! Everything drawn here is read-only. Clicks, drags and context menus push
//! [`TabAction`] values into one vector which is applied exactly once, after
//! the layout pass. That keeps the model borrow immutable while we iterate
//! `state.tabs` and makes the action ordering explicit (e.g. a right click on
//! an unselected tab queues `Activate` *before* the menu's own actions).

use crate::browser::{BrowserState, TabGroup, site_key};
use crate::tab::Tab;
use crate::ui::favicon_texture;
use crate::ui::tab_actions::{self, TabAction};
use egui::{
    Align, Align2, Color32, CornerRadius, FontId, Id, Key, Layout, Margin, PointerButton, Pos2,
    Rect, Response, RichText, Sense, Stroke, StrokeKind, TextStyle, Ui, Vec2, pos2, vec2,
};

/// Lower bound for a normal tab: icon + a few characters of title + close
/// button. Below this width a tab stops being useful, so the strip scrolls
/// instead.
pub const TAB_MIN_WIDTH: f32 = 64.0;

/// Upper bound for a normal tab. Browsers cap this so that a lone tab (or two)
/// does not look absurdly wide.
pub const TAB_MAX_WIDTH: f32 = 240.0;

/// Height of the tab itself.
pub const TAB_HEIGHT: f32 = 34.0;

/// Horizontal gap between neighbouring slots (and before the "+" button).
pub const TAB_GAP: f32 = 2.0;

/// Width reserved for the "new tab" button, which sits right after the slots.
pub const ADD_BUTTON_WIDTH: f32 = 32.0;

/// Width reserved for the tab-search button at the right edge of the row.
const SEARCH_BUTTON_WIDTH: f32 = 30.0;

/// Pinned tabs are icon-only, so their width is a fixed slot rather than an
/// elastic share of the strip.
pub const PINNED_WIDTH: f32 = 44.0;

/// Width of the coloured chip that introduces a tab group.
pub const GROUP_CHIP_WIDTH: f32 = 96.0;

/// Extra height handed to the scroll area when it has to paint a scrollbar.
const SCROLLBAR_ALLOWANCE: f32 = 8.0;

/// Padding inside a tab, on both the left and right edge.
const TAB_PADDING: f32 = 8.0;

/// Box reserved for the favicon / spinner / status glyph.
const ICON_BOX: f32 = 16.0;

/// Box reserved for the close button.
const CLOSE_BOX: f32 = 18.0;

/// Below this width the close button only shows up on the active or hovered
/// tab, which is how Chromium keeps narrow tabs readable.
const CLOSE_ALWAYS_WIDTH: f32 = 88.0;

/// Slot handed to the audio / mute glyph at the right end of the title area.
/// Narrow on purpose: it eats into an already elastic title, not into the
/// fixed icon and close slots.
const AUDIO_ICON_WIDTH: f32 = 13.0;

/// Multi-selection accent, also used for the drag insertion marker.
const ACCENT: Color32 = Color32::from_rgb(90, 150, 240);

/// Default (inactive) tab background.
const INACTIVE_BG: Color32 = Color32::from_rgb(26, 27, 30);

/// Slightly lifted background for the hovered tab.
const HOVER_BG: Color32 = Color32::from_rgb(34, 36, 41);

/// The displayed tab is brighter than its neighbours and keeps a white title.
const ACTIVE_BG: Color32 = Color32::from_rgb(45, 47, 54);

/// Selected-but-not-active tabs get their own fill plus the accent border.
const SELECTED_BG: Color32 = Color32::from_rgb(52, 56, 64);

/// How close the pointer has to be to a strip edge before it auto-scrolls.
const EDGE_ZONE: f32 = 24.0;

/// Points per frame the strip scrolls while a drag hovers an edge.
const AUTOSCROLL_SPEED: f32 = 10.0;

/// Result of the width computation for one frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TabStripMetrics {
    /// Width every normal tab gets this frame.
    pub tab_width: f32,
    /// Total width the strip needs, including fixed slots and all gaps.
    pub content_width: f32,
    /// `true` when the tabs no longer fit and the strip must scroll.
    pub overflowing: bool,
}

impl TabStripMetrics {
    /// Computes the normal-tab width for `normal_tab_count` elastic tabs in
    /// `available_width` px, with `fixed_width` px already consumed by pinned
    /// tabs and group chips.
    ///
    /// `slot_count` is the number of *drawn* slots (pinned + chips + visible
    /// normal tabs). It matters because gaps are only paid between slots: a
    /// collapsed group contributes one chip, not one chip per hidden member.
    ///
    /// Pure function: trivially testable and safe for every input, including
    /// `slot_count == 0` and negative/zero available width (minimised windows).
    pub fn for_strip(
        available_width: f32,
        normal_tab_count: usize,
        fixed_width: f32,
        slot_count: usize,
    ) -> Self {
        let fixed_width = fixed_width.max(0.0);
        let gaps = TAB_GAP * slot_count.saturating_sub(1) as f32;

        if normal_tab_count == 0 {
            // Nothing elastic to split: the strip overflows as soon as the
            // fixed slots alone do not fit.
            let content_width = fixed_width + gaps;
            return Self {
                tab_width: TAB_MAX_WIDTH,
                content_width,
                overflowing: content_width > available_width.max(0.0),
            };
        }

        let count = normal_tab_count as f32;

        // Width each normal tab would get if we split the leftover evenly.
        // `.max(0.0)` keeps degenerate (very small) windows well-defined.
        let ideal = ((available_width - fixed_width - gaps) / count).max(0.0);
        let tab_width = ideal.clamp(TAB_MIN_WIDTH, TAB_MAX_WIDTH);

        Self {
            tab_width,
            content_width: fixed_width + tab_width * count + gaps,
            // The tabs do not fit iff even the fairest split is below the
            // minimum — that is the single decision point for scroll mode.
            overflowing: ideal < TAB_MIN_WIDTH,
        }
    }
}

/// X offset (relative to the left edge of the strip) at which the "+" button
/// starts.
///
/// The strip is laid out at its natural content width, so this puts the button
/// immediately after the last slot; once the tabs overflow, the strip is capped
/// at `usable_width` and the button lands on the right edge, where it stays
/// reachable instead of scrolling away with the tabs.
pub fn add_button_offset(metrics: &TabStripMetrics, usable_width: f32) -> f32 {
    metrics.content_width.min(usable_width) + TAB_GAP
}

// ---------------------------------------------------------------------------
// Drag state
// ---------------------------------------------------------------------------

/// Payload kept in egui's temp storage for the duration of a drag. It lives in
/// temp storage (not in `BrowserState`) because it is pure UI state that must
/// vanish when the pointer is released or the window closes.
#[derive(Debug, Clone)]
struct DragState {
    /// Tabs being moved, in strip order. One element for an unselected tab,
    /// the whole selection when the dragged tab is part of it.
    ids: Vec<u64>,
}

fn tab_drag_id() -> Id {
    Id::new("oxide_tab_drag")
}

fn rename_focus_id() -> Id {
    Id::new("oxide_group_rename_focus")
}

/// Records a right-click as a request for a context menu.
///
/// The menu itself is *not* drawn here: the page is a native child window that
/// the OS paints above egui, so a menu drawn into this window would be buried
/// under it. The request is replayed by a borderless popup window that the OS
/// stacks above the page (see [`crate::menu_window`]), which is what lets the
/// page stay visible and live while the menu is open.
fn request_menu(
    response: &Response,
    kind: crate::ui::menu::MenuKind,
    actions: &mut Vec<TabAction>,
) {
    if !response.secondary_clicked() {
        return;
    }
    let pos = response
        .interact_pointer_pos()
        .unwrap_or_else(|| response.rect.left_bottom());
    actions.push(TabAction::OpenMenu { kind, pos });
}

// ---------------------------------------------------------------------------
// Render plan
// ---------------------------------------------------------------------------

/// One drawn slot of the strip, in draw order.
#[derive(Debug, Clone, Copy)]
enum StripSlot {
    Pinned { tab_index: usize },
    /// A group chip. `member_count` is the number of tabs in the group; when
    /// the group is collapsed those members are hidden but still counted in
    /// the drop boundary that follows the chip.
    Chip {
        group_id: u64,
        first_member: usize,
        member_count: usize,
        collapsed: bool,
    },
    Normal { tab_index: usize },
}

/// Where a slot starts in strip-tab-index space.
fn slot_before(slot: &StripSlot) -> usize {
    match slot {
        StripSlot::Pinned { tab_index } | StripSlot::Normal { tab_index } => *tab_index,
        StripSlot::Chip { first_member, .. } => *first_member,
    }
}

/// Maps every *boundary* between drawn slots (there are `plan.len() + 1`) to
/// the strip index a drop there should insert before.
///
/// A chip is not a real tab, so the boundary after it points at:
/// * the first member when expanded (the chip visually introduces it), or
/// * the tab after the last hidden member when collapsed.
fn plan_boundaries(plan: &[StripSlot], tab_count: usize) -> Vec<usize> {
    let mut boundaries = Vec::with_capacity(plan.len() + 1);
    boundaries.push(plan.first().map(slot_before).unwrap_or(tab_count));
    for slot in plan {
        let after = match slot {
            StripSlot::Pinned { tab_index } | StripSlot::Normal { tab_index } => tab_index + 1,
            StripSlot::Chip {
                first_member,
                member_count,
                collapsed,
                ..
            } => {
                if *collapsed {
                    first_member + member_count
                } else {
                    *first_member
                }
            }
        };
        boundaries.push(after);
    }
    boundaries
}

/// Turns the model into the list of slots to draw.
///
/// Pinned tabs are a prefix (`normalize_order` guarantees it), then each group
/// is introduced by its chip followed by its members — or only the chip when
/// collapsed.
fn build_plan(state: &BrowserState) -> Vec<StripSlot> {
    let mut plan = Vec::new();

    for (tab_index, tab) in state.tabs.iter().enumerate() {
        if tab.pinned {
            plan.push(StripSlot::Pinned { tab_index });
        }
    }

    let mut emitted: Vec<u64> = Vec::new();
    for (tab_index, tab) in state.tabs.iter().enumerate() {
        if tab.pinned {
            continue;
        }
        match tab.group {
            Some(group_id) if state.group(group_id).is_some() => {
                // `group()` was just checked, so this cannot fail; the model
                // clears stale group ids in `normalize_order`.
                let Some(group) = state.group(group_id) else {
                    continue;
                };
                // Chrome keeps a group expanded while it holds the displayed tab:
                // collapsing it must never hide the page the user is looking at.
                let active_inside = state
                    .active_tab_id
                    .and_then(|id| state.tab(id))
                    .is_some_and(|t| t.group == Some(group_id));
                let collapsed = group.collapsed && !active_inside;
                if !emitted.contains(&group_id) {
                    emitted.push(group_id);
                    let member_count = state
                        .tabs
                        .iter()
                        .filter(|t| t.group == Some(group_id) && !t.pinned)
                        .count();
                    plan.push(StripSlot::Chip {
                        group_id,
                        first_member: tab_index,
                        member_count,
                        collapsed,
                    });
                }
                if !collapsed {
                    plan.push(StripSlot::Normal { tab_index });
                }
            }
            _ => plan.push(StripSlot::Normal { tab_index }),
        }
    }

    plan
}

// ---------------------------------------------------------------------------
// Slot bookkeeping for drag & drop
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SlotKind {
    Pinned,
    Chip(u64),
    Normal,
}

/// Screen-space rect of a drawn slot, tagged with its plan position so a drop
/// boundary can be translated back into a strip index.
#[derive(Debug, Clone, Copy)]
struct SlotInfo {
    plan_index: usize,
    rect: Rect,
    kind: SlotKind,
}

/// What releasing the drag at the current pointer position would do.
#[derive(Debug, Clone, Copy)]
enum DropTarget {
    /// Insert before the strip index derived from this plan boundary.
    Insert {
        plan_boundary: usize,
        strip_index: usize,
    },
    /// Drop onto a group chip: the dragged tabs join the group instead of
    /// being reordered.
    Group(u64),
}

fn drop_target(
    slots: &[SlotInfo],
    boundaries: &[usize],
    pointer: Pos2,
    dragged: &[u64],
    state: &BrowserState,
) -> Option<DropTarget> {
    // Pinned tabs can never join a group, so a pinned drag always inserts.
    let can_group = dragged
        .iter()
        .all(|id| state.tab(*id).is_some_and(|tab| !tab.pinned));

    if can_group {
        for slot in slots {
            if let SlotKind::Chip(group_id) = slot.kind
                && slot.rect.contains(pointer)
            {
                return Some(DropTarget::Group(group_id));
            }
        }
    }

    if slots.is_empty() {
        return None;
    }

    // A boundary sits between two slot centres; picking the last slot whose
    // centre is left of the pointer is exact and also works for empty space
    // after the final slot.
    let mut plan_boundary = 0;
    for slot in slots {
        if pointer.x < slot.rect.center().x {
            break;
        }
        plan_boundary = slot.plan_index + 1;
    }

    let strip_index = *boundaries
        .get(plan_boundary)
        .unwrap_or(boundaries.last()?);
    Some(DropTarget::Insert {
        plan_boundary,
        strip_index,
    })
}

/// X coordinate of the insertion marker for a plan boundary.
fn marker_x(slots: &[SlotInfo], plan_boundary: usize) -> Option<f32> {
    if slots.is_empty() {
        return None;
    }
    if plan_boundary == 0 {
        Some(slots[0].rect.left() - TAB_GAP * 0.5)
    } else {
        slots
            .get(plan_boundary - 1)
            .map(|slot| slot.rect.right() + TAB_GAP * 0.5)
    }
}

/// True when inserting `ids` before `to_index` would not change the order, so
/// no `MoveTabs` should be emitted (a drop must never cause a pointless
/// reorder/renormalize pass).
fn is_noop_move(state: &BrowserState, ids: &[u64], to_index: usize) -> bool {
    let positions: Vec<usize> = ids
        .iter()
        .filter_map(|id| state.tab_index(*id))
        .collect();
    let Some(&first) = positions.first() else {
        return true;
    };
    let contiguous = positions
        .iter()
        .enumerate()
        .all(|(offset, position)| *position == first + offset);
    // Only a contiguous block can be a no-op: dropping it back on itself.
    contiguous && (to_index == first || to_index == first + positions.len())
}

/// Paints the 2px insertion marker, clamped to the visible strip so it never
/// escapes the scroll viewport.
fn paint_insertion_marker(ui: &Ui, slots: &[SlotInfo], plan_boundary: usize, strip: Rect) {
    let Some(x) = marker_x(slots, plan_boundary) else {
        return;
    };
    let x = x.clamp(strip.left() + 1.0, strip.right() - 1.0);
    let Some(first) = slots.first() else {
        return;
    };
    let marker = Rect::from_min_max(
        pos2(x - 1.0, first.rect.top()),
        pos2(x + 1.0, first.rect.bottom()),
    );
    ui.painter()
        .rect_filled(marker, CornerRadius::same(1), ACCENT);
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

/// Renders the whole tab strip: pinned tabs, group chips, normal tabs,
/// horizontal overflow scrolling and the "+" button.
pub fn render_tab_bar(ui: &mut Ui, state: &mut BrowserState) {
    let plan = build_plan(state);
    let normal_count = plan
        .iter()
        .filter(|slot| matches!(slot, StripSlot::Normal { .. }))
        .count();
    let fixed_width: f32 = plan
        .iter()
        .map(|slot| match slot {
            StripSlot::Pinned { .. } => PINNED_WIDTH,
            StripSlot::Chip { .. } => GROUP_CHIP_WIDTH,
            StripSlot::Normal { .. } => 0.0,
        })
        .sum();
    let boundaries = plan_boundaries(&plan, state.tabs.len());

    // The "+" and the tab-search buttons take their slots plus one gap each,
    // so the tabs may only use what is left of the window width. The search
    // button lives at the far right, so it has to be reserved here or the
    // tabs would grow underneath it in scroll mode.
    let usable_width = (ui.available_width()
        - ADD_BUTTON_WIDTH
        - TAB_GAP
        - SEARCH_BUTTON_WIDTH
        - TAB_GAP)
        .max(0.0);
    let metrics = TabStripMetrics::for_strip(usable_width, normal_count, fixed_width, plan.len());

    let mut actions: Vec<TabAction> = Vec::new();
    let mut slots: Vec<SlotInfo> = Vec::new();
    let mut strip_screen_rect = Rect::NOTHING;
    let mut rename_anchor: Option<Pos2> = None;

    let drag_id = tab_drag_id();
    let drag_active = ui.ctx().data(|data| data.get_temp::<DragState>(drag_id)).is_some();

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(TAB_GAP, 0.0);

        let strip_height =
            TAB_HEIGHT + if metrics.overflowing { SCROLLBAR_ALLOWANCE } else { 0.0 };
        let add_offset = add_button_offset(&metrics, usable_width);
        let strip_width = (add_offset - TAB_GAP).max(0.0);

        // Know where the strip will be *before* creating the scroll area:
        // `scroll_with_delta` has to be fed to the next scroll area, so both
        // the wheel translation and the drag auto-scroll must run first.
        let row_rect = ui.available_rect_before_wrap();
        let strip_rect = Rect::from_min_size(row_rect.min, vec2(strip_width, strip_height));

        if metrics.overflowing {
            let wheel_y = ui.input(|i| i.smooth_scroll_delta.y);
            if wheel_y != 0.0 && ui.ui_contains_pointer() {
                ui.scroll_with_delta(vec2(wheel_y, 0.0));
            }

            if drag_active
                && let Some(pointer) = ui.input(|i| i.pointer.hover_pos())
            {
                let near_row = pointer.y >= strip_rect.top() - TAB_HEIGHT
                    && pointer.y <= strip_rect.bottom() + TAB_HEIGHT;
                // Content moves left for a negative delta, i.e. the view
                // scrolls towards the end of the strip.
                if near_row && pointer.x < strip_rect.left() + EDGE_ZONE {
                    ui.scroll_with_delta(vec2(AUTOSCROLL_SPEED, 0.0));
                } else if near_row && pointer.x > strip_rect.right() - EDGE_ZONE {
                    ui.scroll_with_delta(vec2(-AUTOSCROLL_SPEED, 0.0));
                }
            }
        }

        // Allocate exactly as much room as the slots need (capped by the
        // usable width). That keeps the "+" button glued to the last slot
        // instead of floating at the far right of the window.
        let strip = ui.allocate_ui_with_layout(
            Vec2::new(strip_width, strip_height),
            Layout::left_to_right(Align::Center),
            |ui| {
                egui::ScrollArea::horizontal()
                    .id_salt("oxide_tab_strip")
                    // Tab drags are handled here; letting the scroll area treat a
                    // primary-button drag as scrolling would steal the reorder gesture.
                    .drag_to_scroll(false)
                    .auto_shrink([false, false])
                    .max_height(strip_height)
                    .scroll_bar_visibility(
                        egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded,
                    )
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(TAB_GAP, 0.0);
                        ui.horizontal(|ui| {
                            for (plan_index, slot) in plan.iter().enumerate() {
                                match slot {
                                    StripSlot::Pinned { tab_index } => {
                                        let Some(tab) = state.tabs.get(*tab_index) else {
                                            continue;
                                        };
                                        let rect = draw_tab(
                                            ui,
                                            state,
                                            tab,
                                            PINNED_WIDTH,
                                            true,
                                            &mut actions,
                                        );
                                        slots.push(SlotInfo {
                                            plan_index,
                                            rect,
                                            kind: SlotKind::Pinned,
                                        });
                                    }
                                    StripSlot::Normal { tab_index } => {
                                        let Some(tab) = state.tabs.get(*tab_index) else {
                                            continue;
                                        };
                                        let rect = draw_tab(
                                            ui,
                                            state,
                                            tab,
                                            metrics.tab_width,
                                            false,
                                            &mut actions,
                                        );
                                        slots.push(SlotInfo {
                                            plan_index,
                                            rect,
                                            kind: SlotKind::Normal,
                                        });
                                    }
                                    StripSlot::Chip {
                                        group_id,
                                        member_count,
                                        collapsed,
                                        ..
                                    } => {
                                        let Some(group) = state.group(*group_id) else {
                                            continue;
                                        };
                                        let rect = draw_group_chip(
                                            ui,
                                            group,
                                            *member_count,
                                            *collapsed,
                                            &mut actions,
                                        );
                                        // The inline rename editor opens just
                                        // below the chip it belongs to.
                                        rename_anchor =
                                            Some(pos2(rect.left(), rect.bottom() + 4.0));
                                        slots.push(SlotInfo {
                                            plan_index,
                                            rect,
                                            kind: SlotKind::Chip(*group_id),
                                        });
                                    }
                                }
                            }
                        });
                    });
            },
        );

        strip_screen_rect = strip.response.rect;

        // Position the button right after the last slot. Because the strip
        // is allocated at its natural width, this lands directly beside the
        // last slot; when the strip overflows, `strip_width == usable_width`
        // and the button ends up pinned to the right edge, staying reachable.
        let add_rect = Rect::from_min_size(
            pos2(
                strip.response.rect.left() + add_offset,
                strip.response.rect.top(),
            ),
            Vec2::new(ADD_BUTTON_WIDTH, TAB_HEIGHT),
        );

        let add_tab_btn = ui.put(
            add_rect,
            egui::Button::new(
                RichText::new("+")
                    .size(16.0)
                    .color(Color32::from_rgb(200, 205, 215)),
            )
            .fill(Color32::from_rgb(38, 40, 46))
            .corner_radius(CornerRadius::same(6)),
        );

        if add_tab_btn.clicked() {
            actions.push(TabAction::NewTab);
        }

        // Tab search sits at the far right of the row, like Chrome's
        // tab-search button, so it never scrolls out of reach with the tabs.
        let search_rect = Rect::from_min_size(
            pos2(row_rect.right() - SEARCH_BUTTON_WIDTH, row_rect.top()),
            Vec2::new(SEARCH_BUTTON_WIDTH, TAB_HEIGHT),
        );
        let search_btn = ui
            .put(
                search_rect,
                egui::Button::new(RichText::new("🔍").size(13.0))
                    .fill(Color32::from_rgb(38, 40, 46))
                    .corner_radius(CornerRadius::same(6)),
            )
            .on_hover_text("Search tabs (Ctrl+Shift+A)");
        if search_btn.clicked() {
            state.open_tab_search();
        }

        // The leftover part of the row (right of the "+") is the empty strip
        // area: right clicking it offers the strip-wide actions.
        // `ui.put` does not necessarily advance the layout cursor, so the
        // strip-wide interact must never overlap the "+" button: registered
        // later it would sit on top and swallow the click.
        let leftover = {
            let mut rect = ui.available_rect_before_wrap();
            if rect.intersects(add_rect) {
                rect.min.x = rect.min.x.max(add_rect.right() + TAB_GAP);
            }
            // The search button owns the right edge: the strip-wide interact
            // must not cover it, or the click would land on the empty area.
            if rect.intersects(search_rect) {
                rect.max.x = rect.max.x.min(search_rect.left() - TAB_GAP);
            }
            rect
        };
        if leftover.width() > 1.0 && leftover.height() > 1.0 {
            let empty = ui.interact(
                leftover,
                ui.id().with("oxide_tab_strip_empty"),
                Sense::click(),
            );
            request_menu(
                &empty,
                crate::ui::menu::MenuKind::Tab { tab_id: None },
                &mut actions,
            );
            // Chrome clears the selection when the strip background
            // is clicked, so a stray selection never blocks a drag.
            if empty.clicked() {
                actions.push(TabAction::ClearSelection);
            }
        }
    });

    // ---- drag & drop -----------------------------------------------------
    if let Some(drag) = ui.ctx().data(|data| data.get_temp::<DragState>(drag_id)) {
        // `interact_pos` rather than `hover_pos`: a drag keeps its last known
        // position even after the pointer leaves the egui area, which is what
        // a tear-off release needs.
        let pointer = ui.ctx().input(|i| i.pointer.interact_pos());
        let in_drop_band = pointer.is_some_and(|p| {
            p.x >= strip_screen_rect.left() - EDGE_ZONE
                && p.x <= strip_screen_rect.right() + EDGE_ZONE
                && p.y >= strip_screen_rect.top() - TAB_HEIGHT
                && p.y <= strip_screen_rect.bottom() + TAB_HEIGHT
        });

        let target = match pointer {
            Some(pointer) if in_drop_band => {
                drop_target(&slots, &boundaries, pointer, &drag.ids, state)
            }
            _ => None,
        };

        if let Some(DropTarget::Insert { plan_boundary, .. }) = target {
            paint_insertion_marker(ui, &slots, plan_boundary, strip_screen_rect);
        }

        // `any_released` rather than the widget response: the release can
        // happen anywhere, and the tab that started the drag may be scrolled
        // out of view by then.
        // Also clear when the button is no longer down: a release outside
        // the window never reaches egui, and a stuck payload would turn the
        // next click into a drop.
        if ui.ctx().input(|i| i.pointer.any_released() || !i.pointer.any_down()) {
            let released = ui.ctx().input(|i| i.pointer.any_released());
            match target {
                Some(DropTarget::Group(group)) => actions.push(TabAction::AddToGroup {
                    ids: drag.ids.clone(),
                    group,
                }),
                Some(DropTarget::Insert { strip_index, .. }) => {
                    if !is_noop_move(state, &drag.ids, strip_index) {
                        actions.push(TabAction::MoveTabs {
                            ids: drag.ids.clone(),
                            to_index: strip_index,
                        });
                    }
                }
                // Released away from the strip: tear the tabs off into their
                // own window, like Chrome. Only a real release counts - the
                // `!any_down` fallback also fires when the window loses the
                // button, and there is no position to place a window at then.
                None => {
                    if released
                        && !in_drop_band
                        && let Some(pos) = pointer
                    {
                        actions.push(TabAction::DetachTabs {
                            ids: drag.ids.clone(),
                            pos,
                        });
                    }
                }
            }
            ui.ctx().data_mut(|data| data.remove::<DragState>(drag_id));
        }
    }

    // ---- inline group rename --------------------------------------------
    if let Some(group_id) = state.group_rename.as_ref().map(|rename| rename.group_id) {
        if state.group(group_id).is_none() {
            // The group vanished while its editor was open (closed or
            // ungrouped). Drop the editor instead of leaving an orphan popup.
            actions.push(TabAction::CancelGroupRename);
        } else {
            let anchor = rename_anchor.unwrap_or_else(|| {
                // Chip scrolled out of view: fall back to the strip's left.
                pos2(strip_screen_rect.left(), strip_screen_rect.bottom() + 4.0)
            });
            let screen = ui.ctx().screen_rect();
            let pos = pos2(
                anchor.x.clamp(4.0, (screen.right() - 200.0).max(4.0)),
                anchor.y.clamp(0.0, (screen.bottom() - 40.0).max(0.0)),
            );

            let mut commit = false;
            let mut cancel = false;
            if let Some(rename) = state.group_rename.as_mut() {
                egui::Area::new(Id::new("oxide_group_rename"))
                    .order(egui::Order::Foreground)
                    .fixed_pos(pos)
                    .show(ui.ctx(), |ui| {
                        egui::Frame::popup(ui.style())
                            .fill(Color32::from_rgb(30, 32, 38))
                            .inner_margin(Margin::same(6))
                            .show(ui, |ui| {
                                let response = ui.add(
                                    egui::TextEdit::singleline(&mut rename.name)
                                        .desired_width(160.0)
                                        .hint_text("Group name")
                                        .font(TextStyle::Body),
                                );

                                // Focus exactly once, when the editor opens:
                                // re-requesting every frame would fight the
                                // click-outside focus loss.
                                let first_frame = ui
                                    .ctx()
                                    .data(|data| data.get_temp::<u64>(rename_focus_id()))
                                    != Some(rename.group_id);
                                if first_frame {
                                    response.request_focus();
                                    ui.ctx()
                                        .data_mut(|data| {
                                            data.insert_temp(rename_focus_id(), rename.group_id)
                                        });
                                }

                                if ui.input(|i| i.key_pressed(Key::Escape)) {
                                    cancel = true;
                                } else if ui.input(|i| i.key_pressed(Key::Enter))
                                    || response.lost_focus()
                                {
                                    commit = true;
                                }
                            });
                    });
            }

            if commit || cancel {
                ui.ctx()
                    .data_mut(|data| data.remove::<u64>(rename_focus_id()));
                actions.push(if cancel {
                    TabAction::CancelGroupRename
                } else {
                    TabAction::CommitGroupRename
                });
            }
        }
    } else {
        ui.ctx()
            .data_mut(|data| data.remove::<u64>(rename_focus_id()));
    }

    // Everything above only recorded intents: the model is mutated once.
    tab_actions::apply_all(state, actions);
}

// ---------------------------------------------------------------------------
// Tab painting
// ---------------------------------------------------------------------------

/// Draws one tab at `width` and records the interactions it received.
///
/// The tab body is painted directly (rather than using a `Frame`) because the
/// width is imposed by the strip, not by the content: painting also lets the
/// title ellipsize while the controls keep a fixed, clickable size.
///
/// Returns the tab rect so the caller can feed drag & drop geometry.
fn draw_tab(
    ui: &mut Ui,
    state: &BrowserState,
    tab: &Tab,
    width: f32,
    compact: bool,
    actions: &mut Vec<TabAction>,
) -> Rect {
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(width, TAB_HEIGHT),
        // A tab is both clickable and draggable: holding the primary button
        // and moving reorders it, and releasing outside the strip tears it
        // into a new window. `Sense::click()` alone would swallow the drag.
        Sense::click_and_drag(),
    );

    // Tabs scrolled out of the strip are clipped; skip their text layout.
    if !ui.is_rect_visible(rect) {
        return rect;
    }

    let is_active = state.active_tab_id == Some(tab.id);
    let is_selected = state.is_selected(tab.id);
    // Silence can come from the tab's own flag or from a site-wide mute held
    // by the model, so the glyph and the tooltip both ask the model instead of
    // reading the tab field.
    let muted = state.is_tab_muted(tab);

    let rounding = CornerRadius {
        nw: 8,
        ne: 8,
        sw: 0,
        se: 0,
    };

    // Active is the brightest fill; selected-but-not-active is distinguished
    // by both its fill and a *thicker* accent border, so the state is not
    // carried by colour alone.
    let (bg, stroke) = if is_active {
        (
            ACTIVE_BG,
            if is_selected {
                Stroke::new(2.0_f32, ACCENT)
            } else {
                Stroke::new(1.0_f32, Color32::from_rgb(60, 64, 72))
            },
        )
    } else if is_selected {
        (SELECTED_BG, Stroke::new(2.0_f32, ACCENT))
    } else if response.hovered() {
        (HOVER_BG, Stroke::NONE)
    } else {
        (INACTIVE_BG, Stroke::NONE)
    };

    let painter = ui.painter().clone();
    painter.rect_filled(rect, rounding, bg);
    if stroke != Stroke::NONE {
        painter.rect_stroke(rect, rounding, stroke, StrokeKind::Inside);
    }

    // Fixed layout slots: [pad][icon][gap][title ...][close][pad].
    // The title is the only elastic element; icon and close never move or
    // shrink, so they stay reachable even at TAB_MIN_WIDTH.
    let icon_center = if compact {
        rect.center()
    } else {
        pos2(rect.left() + TAB_PADDING + ICON_BOX * 0.5, rect.center().y)
    };
    let icon_rect = Rect::from_center_size(icon_center, Vec2::splat(ICON_BOX));

    if tab.loading {
        ui.put(icon_rect, egui::Spinner::new().size(12.0));
    } else if tab.error.is_some() {
        // A page that failed to load keeps a warning glyph instead of a
        // favicon, so the failure is visible without switching to the tab.
        ui.put(
            icon_rect,
            egui::Label::new(
                RichText::new("⚠")
                    .size(12.0)
                    .color(Color32::from_rgb(239, 68, 68)),
            ),
        );
    } else if let Some(image) = state.favicon_for(&tab.url) {
        // The decoded site icon replaces the glyph as soon as it exists. The
        // texture is keyed by site, so every tab on one host shares a single
        // upload; `site_key` cannot fail for a URL that produced a favicon,
        // but the fallback keeps the slot painted if it ever does.
        let site = site_key(&tab.url).unwrap_or_default();
        let texture = favicon_texture(ui.ctx(), &site, image);
        // One pixel of inset keeps the glyph's optical padding between the
        // image and the tab edge.
        ui.painter().image(
            texture.id(),
            icon_rect.shrink(1.0),
            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
            Color32::WHITE,
        );
    } else {
        // Until the icon arrives (or when the site has none) the letter
        // glyph still tells the tabs apart at a glance.
        let (glyph, color) = favicon_for(&tab.url);
        ui.put(
            icon_rect,
            egui::Label::new(RichText::new(glyph).size(12.0).color(color)),
        );
    }

    if !compact {
        let close_center = pos2(
            rect.right() - TAB_PADDING - CLOSE_BOX * 0.5,
            rect.center().y,
        );
        let close_rect = Rect::from_center_size(close_center, Vec2::splat(CLOSE_BOX));

        // Narrow tabs only reveal the close button on the tab the user is
        // actually looking at (active or hovered).
        let show_close = width >= CLOSE_ALWAYS_WIDTH || is_active || response.hovered();

        let title_left = icon_rect.right() + 4.0;
        let title_right = if show_close {
            close_rect.left() - 2.0
        } else {
            rect.right() - TAB_PADDING
        };
        // Muted wins over playing: it is the state the user chose, and the
        // one that explains why the tab is silent.
        let audio_glyph = if muted {
            Some(("🔇", Color32::from_rgb(240, 180, 80)))
        } else if tab.audio_playing {
            Some(("🔊", Color32::from_rgb(130, 190, 255)))
        } else {
            None
        };
        let title_right = if let Some((glyph, color)) = audio_glyph {
            // The title gives up exactly the room the glyph takes, so the
            // two can never collide however narrow the tab becomes. The
            // indicator is display-only: muting lives in the context menu,
            // and this region still belongs to the tab's click handler.
            let title_right = title_right - (AUDIO_ICON_WIDTH + 3.0);
            let audio_rect = Rect::from_min_max(
                pos2(title_right + 2.0, rect.top()),
                pos2(title_right + 2.0 + AUDIO_ICON_WIDTH, rect.bottom()),
            );
            ui.put(
                audio_rect,
                egui::Label::new(RichText::new(glyph).size(10.5).color(color)),
            );
            title_right
        } else {
            title_right
        };

        let title_rect = Rect::from_min_max(
            pos2(title_left, rect.top()),
            pos2(title_right.max(title_left), rect.bottom()),
        );

        let title_color = if is_active {
            Color32::WHITE
        } else if is_selected {
            Color32::from_rgb(220, 228, 240)
        } else if tab.discarded {
            // Sleeping tabs read as available but dormant — the same dimming
            // Chrome uses for a discarded page.
            Color32::from_rgb(118, 124, 136)
        } else {
            Color32::from_rgb(156, 163, 175)
        };
        // `truncate` ellipsizes to the slot width instead of hard-cutting
        // chars, which is what makes very narrow tabs still readable.
        ui.put(
            title_rect,
            egui::Label::new(RichText::new(&tab.title).size(12.5).color(title_color))
                .truncate()
                .selectable(false),
        );

        if show_close {
            let close_btn = ui.put(
                close_rect,
                // U+00D7 `×` is present in the bundled Ubuntu font. Fancy
                // marks such as U+2715 `✕` are *not* covered by any bundled
                // font and render as tofu.
                egui::Button::new(
                    RichText::new("×")
                        .size(15.0)
                        .color(Color32::from_rgb(140, 145, 155)),
                )
                .frame(false),
            );
            if close_btn.clicked() {
                actions.push(TabAction::Close(tab.id));
            }
        }
    }

    // The plain case points at the title without allocating; any appended
    // line (audio state, discard reason, load error) needs an owned copy.
    let response = if tab.error.is_none() && !tab.discarded && !tab.audio_playing && !muted {
        response.on_hover_text(tab.title.as_str())
    } else {
        let mut text = tab.title.clone();
        if tab.audio_playing {
            text.push_str("\n🔊 Playing audio");
        }
        if muted {
            text.push_str("\n🔇 Muted");
        }
        if tab.discarded {
            text.push_str("\n◌ Discarded to save memory — reloads when selected");
        }
        if let Some(error) = &tab.error {
            text.push('\n');
            text.push_str(error);
        }
        response.on_hover_text(text)
    };
    queue_tab_interaction(ui, &response, state, tab, is_selected, actions);
    rect
}

/// Records the click / right-click / drag intentions of a tab.
///
/// Order matters: a right click on an unselected tab queues its `Activate`
/// before the menu is built, so the actions the menu returns (applied later in
/// the same batch) already act on exactly one tab.
fn queue_tab_interaction(
    ui: &Ui,
    response: &Response,
    state: &BrowserState,
    tab: &Tab,
    is_selected: bool,
    actions: &mut Vec<TabAction>,
) {
    if response.clicked_by(PointerButton::Middle) {
        actions.push(TabAction::Close(tab.id));
    } else if response.clicked() {
        let modifiers = ui.input(|i| i.modifiers);
        actions.push(if modifiers.ctrl || modifiers.command {
            TabAction::ToggleSelection(tab.id)
        } else if modifiers.shift {
            TabAction::ExtendSelection(tab.id)
        } else {
            TabAction::Activate(tab.id)
        });
    }

    if response.secondary_clicked() && !is_selected {
        actions.push(TabAction::Activate(tab.id));
    }
    request_menu(
        response,
        crate::ui::menu::MenuKind::Tab {
            tab_id: Some(tab.id),
        },
        actions,
    );

    if response.drag_started() {
        // Dragging a selected tab moves the whole selection; dragging any
        // other tab moves just that tab.
        let ids = state.bulk_targets(tab.id);
        ui.ctx()
            .data_mut(|data| data.insert_temp(tab_drag_id(), DragState { ids }));
    }
}

/// Draws a group chip: colour fill, a 3px colour bar on the leading edge, the
/// truncated group name, its member count and the collapse glyph.
fn draw_group_chip(
    ui: &mut Ui,
    group: &TabGroup,
    member_count: usize,
    collapsed: bool,
    actions: &mut Vec<TabAction>,
) -> Rect {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(GROUP_CHIP_WIDTH, TAB_HEIGHT), Sense::click());

    if !ui.is_rect_visible(rect) {
        return rect;
    }

    let (r, g, b) = group.color.rgb();
    let base = Color32::from_rgb(r, g, b);
    let rounding = CornerRadius {
        nw: 8,
        ne: 8,
        sw: 0,
        se: 0,
    };

    let painter = ui.painter().clone();
    // Low-alpha fill keeps the group colour as a hint rather than a slab, and
    // hover lifts it slightly so the chip reads as interactive.
    let fill_alpha = if response.hovered() { 70 } else { 42 };
    painter.rect_filled(
        rect,
        rounding,
        Color32::from_rgba_unmultiplied(r, g, b, fill_alpha),
    );
    painter.rect_stroke(
        rect,
        rounding,
        Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(r, g, b, 130)),
        StrokeKind::Inside,
    );

    // The 3px bar is the strongest colour cue, so the group stays identifiable
    // even when the fill is subtle.
    let bar = Rect::from_min_max(
        rect.left_top(),
        pos2(rect.left() + 3.0, rect.bottom()),
    );
    painter.rect_filled(
        bar,
        CornerRadius {
            nw: 8,
            ne: 0,
            sw: 8,
            se: 0,
        },
        base,
    );

    // `⏷` expanded / `⏵` collapsed are both covered by the bundled font stack.
    let glyph = if collapsed { "⏵" } else { "⏷" };
    let glyph_rect = Rect::from_center_size(
        pos2(rect.right() - 15.0, rect.center().y),
        Vec2::splat(14.0),
    );
    painter.text(
        glyph_rect.center(),
        Align2::CENTER_CENTER,
        glyph,
        FontId::proportional(12.0),
        Color32::from_rgb(220, 224, 232),
    );

    let count = member_count.to_string();
    painter.text(
        pos2(glyph_rect.left() - 4.0, rect.center().y),
        Align2::RIGHT_CENTER,
        count.as_str(),
        FontId::proportional(10.5),
        Color32::from_rgb(200, 205, 215),
    );

    let name_left = rect.left() + 3.0 + 7.0;
    let name_right = (glyph_rect.left() - 20.0).max(name_left);
    ui.put(
        Rect::from_min_max(
            pos2(name_left, rect.top()),
            pos2(name_right, rect.bottom()),
        ),
        egui::Label::new(
            RichText::new(&group.name)
                .size(12.0)
                .color(Color32::from_rgb(225, 229, 236)),
        )
        .truncate()
        .selectable(false),
    );

    if response.double_clicked() {
        actions.push(TabAction::BeginRenameGroup(group.id));
    } else if response.clicked() {
        actions.push(TabAction::ToggleGroupCollapsed(group.id));
    }
    request_menu(
        &response,
        crate::ui::menu::MenuKind::Group {
            group_id: group.id,
        },
        actions,
    );
    let _ = response.on_hover_text(group.name.as_str());
    rect
}

/// Status glyph shown where a real browser paints the favicon.
fn favicon_for(url: &str) -> (&'static str, Color32) {
    if url.starts_with("oxide://") {
        ("⚡", Color32::from_rgb(56, 189, 248))
    } else if url.starts_with("https://") {
        ("🔒", Color32::from_rgb(74, 222, 128))
    } else {
        ("📄", Color32::from_rgb(156, 163, 175))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A lone tab is capped, not stretched across the window.
    #[test]
    fn single_tab_is_capped_at_max_width() {
        let m = TabStripMetrics::for_strip(2400.0, 1, 0.0, 1);
        assert_eq!(m.tab_width, TAB_MAX_WIDTH);
        assert!(!m.overflowing);
        assert_eq!(m.content_width, TAB_MAX_WIDTH); // no gap for one slot
    }

    /// Tabs shrink evenly (leftover / n) while above the minimum.
    #[test]
    fn tabs_shrink_evenly_until_min_width() {
        let available = 480.0 + 3.0 * TAB_GAP; // 4 tabs, 3 gaps
        let m = TabStripMetrics::for_strip(available, 4, 0.0, 4);
        assert!((m.tab_width - 120.0).abs() < 0.01);
        assert!(!m.overflowing);
    }

    /// Too many tabs: widths stop at the minimum and the strip overflows.
    #[test]
    fn too_many_tabs_stop_at_min_and_overflow() {
        let m = TabStripMetrics::for_strip(200.0, 10, 0.0, 10);
        assert_eq!(m.tab_width, TAB_MIN_WIDTH);
        assert!(m.overflowing);
        assert!(m.content_width > 200.0); // scrollbar needed
    }

    /// Exact fit sits right on the minimum without overflowing.
    #[test]
    fn exact_fit_does_not_overflow() {
        let n = 5usize;
        let available = TAB_MIN_WIDTH * n as f32 + TAB_GAP * (n as f32 - 1.0);
        let m = TabStripMetrics::for_strip(available, n, 0.0, n);
        assert!((m.tab_width - TAB_MIN_WIDTH).abs() < 0.01);
        assert!(!m.overflowing);
    }

    /// Empty strip must not divide by zero.
    #[test]
    fn zero_tabs_is_safe() {
        let m = TabStripMetrics::for_strip(1000.0, 0, 0.0, 0);
        assert_eq!(m.content_width, 0.0);
        assert!(!m.overflowing);
    }

    /// Minimised / degenerate window widths are safe.
    #[test]
    fn degenerate_strip_width_is_safe() {
        let m = TabStripMetrics::for_strip(-50.0, 3, 0.0, 3);
        assert_eq!(m.tab_width, TAB_MIN_WIDTH);
        assert!(m.overflowing);
    }

    /// Pinned tabs keep their fixed width and shrink what is left for the
    /// elastic tabs.
    #[test]
    fn pinned_tabs_are_fixed_and_reduce_normal_space() {
        let available = 480.0 + 4.0 * TAB_GAP; // 5 slots, 4 gaps
        let plain = TabStripMetrics::for_strip(available, 3, 0.0, 3);
        let pinned = TabStripMetrics::for_strip(available, 3, 2.0 * PINNED_WIDTH, 5);

        assert!(pinned.tab_width < plain.tab_width);
        assert!(!pinned.overflowing);
        assert_eq!(
            pinned.content_width,
            2.0 * PINNED_WIDTH + 3.0 * pinned.tab_width + 4.0 * TAB_GAP
        );
    }

    /// A collapsed group is a single chip slot: its hidden members must not
    /// steal width, nor pay gaps.
    #[test]
    fn collapsed_group_counts_as_one_slot() {
        let available = 500.0;
        let m = TabStripMetrics::for_strip(available, 2, GROUP_CHIP_WIDTH, 3);

        let expected = (available - GROUP_CHIP_WIDTH - 2.0 * TAB_GAP) / 2.0;
        assert!((m.tab_width - expected).abs() < 0.01);
        assert_eq!(
            m.content_width,
            GROUP_CHIP_WIDTH + 2.0 * m.tab_width + 2.0 * TAB_GAP
        );
        assert!(!m.overflowing);
    }

    /// A strip made only of fixed slots overflows once they no longer fit.
    #[test]
    fn fixed_slots_overflow_on_their_own() {
        let m = TabStripMetrics::for_strip(40.0, 0, 3.0 * PINNED_WIDTH, 3);
        assert!(m.overflowing);
        assert_eq!(m.content_width, 3.0 * PINNED_WIDTH + 2.0 * TAB_GAP);
    }

    /// The "+" button hugs the last tab when the tabs fit …
    #[test]
    fn add_button_sits_right_after_the_last_tab() {
        let available = 1600.0;
        let usable = available - ADD_BUTTON_WIDTH - TAB_GAP;
        let m = TabStripMetrics::for_strip(usable, 3, 0.0, 3); // 3 * 240 + 2 gaps
        let offset = add_button_offset(&m, usable);

        assert_eq!(offset, m.content_width + TAB_GAP);
        assert!(
            offset + ADD_BUTTON_WIDTH < available,
            "button must not be pinned to the far right"
        );
    }

    /// … and is pinned to the right edge once the tabs overflow.
    #[test]
    fn add_button_is_pinned_when_overflowing() {
        let available = 340.0;
        let usable = available - ADD_BUTTON_WIDTH - TAB_GAP;
        let m = TabStripMetrics::for_strip(usable, 12, 0.0, 12);
        assert!(m.overflowing);

        let offset = add_button_offset(&m, usable);
        assert_eq!(offset, usable + TAB_GAP);
        // Its right edge lands exactly on the right edge of the strip.
        assert_eq!(offset + ADD_BUTTON_WIDTH, available);
    }

    /// The model needs a tokio handle; these tests only exercise pure layout.
    fn strip_state() -> BrowserState {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        // Leak: the handle has to outlive the state for the test's duration.
        let handle = Box::leak(Box::new(rt)).handle().clone();
        let mut state = BrowserState::new(handle);
        // `new` already opens the welcome tab, so this yields three tabs.
        state.create_tab("https://example.com/a");
        state.create_tab("https://example.com/b");
        state
    }

    fn tab_ids(state: &BrowserState) -> Vec<u64> {
        state.tabs.iter().map(|t| t.id).collect()
    }

    /// A pinned tab is drawn first and as a `Pinned` slot, and every other tab
    /// is still drawn exactly once.
    #[test]
    fn plan_puts_pinned_tabs_first_without_losing_any() {
        let mut s = strip_state();
        let ids = tab_ids(&s);
        s.set_pinned(ids[1], true);

        let plan = build_plan(&s);
        assert_eq!(plan.len(), s.tabs.len(), "every tab is still drawn");
        assert!(matches!(plan[0], StripSlot::Pinned { tab_index: 0 }));
        assert_eq!(
            plan.iter()
                .filter(|slot| matches!(slot, StripSlot::Normal { .. }))
                .count(),
            s.tabs.len() - 1
        );
    }

    /// A group is introduced by one chip followed by its members.
    #[test]
    fn plan_draws_a_chip_then_its_members() {
        let mut s = strip_state();
        let ids = tab_ids(&s);
        s.create_group(&[ids[0], ids[1]], crate::browser::GroupColor::Blue);

        let plan = build_plan(&s);
        match plan.first() {
            Some(StripSlot::Chip {
                member_count,
                collapsed,
                first_member,
                ..
            }) => {
                assert_eq!(*member_count, 2);
                assert!(!collapsed);
                assert_eq!(*first_member, 0);
            }
            other => panic!("expected a chip first, got {other:?}"),
        }
        assert_eq!(plan.len(), s.tabs.len() + 1, "chip + every member");
    }

    /// Collapsing a group hides its members but keeps the chip, and the hidden
    /// members stop consuming width.
    #[test]
    fn collapsed_group_draws_only_its_chip() {
        let mut s = strip_state();
        let ids = tab_ids(&s);
        let group = s.create_group(&[ids[0], ids[1]], crate::browser::GroupColor::Blue).unwrap();
        s.toggle_group_collapsed(group);

        let plan = build_plan(&s);
        assert!(matches!(
            plan.first(),
            Some(StripSlot::Chip {
                collapsed: true,
                member_count: 2,
                ..
            })
        ));
        assert_eq!(
            plan.iter()
                .filter(|slot| matches!(slot, StripSlot::Normal { .. }))
                .count(),
            1,
            "only the ungrouped tab is still drawn"
        );
    }

    /// Collapsing must never hide the page the user is looking at: a group
    /// holding the active tab renders expanded.
    #[test]
    fn group_holding_the_active_tab_stays_expanded() {
        let mut s = strip_state();
        let ids = tab_ids(&s);
        let group = s.create_group(&[ids[0], ids[1]], crate::browser::GroupColor::Blue).unwrap();
        s.activate_tab(ids[0]);
        s.toggle_group_collapsed(group);

        assert!(s.group(group).unwrap().collapsed, "model keeps the flag");
        let plan = build_plan(&s);
        assert!(
            matches!(plan.first(), Some(StripSlot::Chip { collapsed: false, .. })),
            "the chip renders expanded while it holds the active tab"
        );
        assert_eq!(
            plan.iter()
                .filter(|slot| matches!(slot, StripSlot::Normal { .. }))
                .count(),
            s.tabs.len()
        );
    }

    /// A drop boundary after an expanded chip points at its first member; after
    /// a collapsed chip it points past the hidden members.
    #[test]
    fn chip_boundaries_account_for_hidden_members() {
        let expanded = vec![
            StripSlot::Chip {
                group_id: 1,
                first_member: 0,
                member_count: 2,
                collapsed: false,
            },
            StripSlot::Normal { tab_index: 0 },
            StripSlot::Normal { tab_index: 1 },
            StripSlot::Normal { tab_index: 2 },
        ];
        let boundaries = plan_boundaries(&expanded, 3);
        assert_eq!(boundaries.first(), Some(&0));
        assert_eq!(boundaries[1], 0, "drop after the chip lands on member 0");
        assert_eq!(boundaries[4], 3, "drop after the last slot is the end");

        let collapsed = vec![
            StripSlot::Chip {
                group_id: 1,
                first_member: 0,
                member_count: 2,
                collapsed: true,
            },
            StripSlot::Normal { tab_index: 2 },
        ];
        let boundaries = plan_boundaries(&collapsed, 3);
        assert_eq!(boundaries[1], 2, "hidden members are skipped");
        assert_eq!(boundaries[2], 3);
    }

    /// Dropping a tab back where it already is must not reorder anything.
    #[test]
    fn dropping_a_tab_on_itself_is_a_noop() {
        let s = strip_state();
        let ids = tab_ids(&s);
        assert!(is_noop_move(&s, &[ids[0]], 0));
        assert!(is_noop_move(&s, &[ids[0]], 1));
        assert!(!is_noop_move(&s, &[ids[0]], 2));
        assert!(is_noop_move(&s, &[ids[0], ids[1]], 0));
        assert!(is_noop_move(&s, &[ids[0], ids[1]], 2));
        assert!(!is_noop_move(&s, &[ids[0], ids[1]], 1));
    }

    /// The insertion marker sits between slots and is clamped to the viewport.
    #[test]
    fn insertion_marker_sits_between_slots() {
        let slots = vec![
            SlotInfo {
                plan_index: 0,
                rect: Rect::from_min_size(pos2(10.0, 0.0), vec2(100.0, 30.0)),
                kind: SlotKind::Normal,
            },
            SlotInfo {
                plan_index: 1,
                rect: Rect::from_min_size(pos2(112.0, 0.0), vec2(100.0, 30.0)),
                kind: SlotKind::Normal,
            },
        ];
        assert_eq!(marker_x(&slots, 0), Some(9.0));
        assert_eq!(marker_x(&slots, 1), Some(111.0));
        assert_eq!(marker_x(&slots, 2), Some(213.0));
        assert_eq!(marker_x(&[], 0), None);
    }

}
