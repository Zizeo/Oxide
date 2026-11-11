//! A borderless popup window that draws context menus above the page.
//!
//! # Why a second window
//!
//! The page is a native child `wry::WebView`. The OS composites child windows
//! above their parent's client area, so anything egui paints into the main
//! window is *behind* the page: a menu drawn there is only visible while the
//! page is hidden, which blanks the viewport. Neither is acceptable.
//!
//! A top-level window, on the other hand, is stacked above the page by the
//! window manager. So the menu gets its own borderless, always-on-top window,
//! sized to the menu, and the page keeps rendering live underneath it.
//!
//! # Geometry
//!
//! The window must be exactly as large as the menu, because a swapchain
//! surface cannot be transparent: any extra area would be an opaque box over
//! the page. Submenus make the menu grow, so the window is re-fitted every
//! frame from the rect egui actually painted:
//!
//! * `anchor` is the screen position of the menu's top-left corner, fixed for
//!   the menu's lifetime (clamped into the monitor's work area on the first
//!   frame, once the menu's size is known).
//! * `area_offset` is where the menu's content starts inside the window. It is
//!   negative when a submenu opened to the left of the anchor.
//! * The window is placed at `anchor + content.min` and sized to `content`,
//!   where `content` is the painted rect expressed relative to the anchor.
//!
//! Because `content` is measured relative to the anchor, the base menu never
//! moves on screen when a submenu grows the window, and the fit is stable
//! across frames.

use std::sync::Arc;

use egui::{Pos2, Rect, Vec2};
use winit::dpi::{LogicalPosition, LogicalSize};
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId, WindowLevel};

use crate::browser::BrowserState;
use crate::ui::menu::{self, MenuKind, MenuPicks};
use crate::wgpu_context::WgpuContext;

/// Smallest sensible menu width, so a menu with short labels still reads as a
/// menu instead of a tooltip.
const MIN_MENU_WIDTH: f32 = 220.0;

/// Id of the base menu area; submenus and tooltips are separate areas.
const MENU_AREA_ID: &str = "oxide_menu";

/// What a window event means for the menu's lifetime.
pub enum MenuEvent {
    /// Keep the menu open.
    Continue,
    /// Close the menu.
    ///
    /// `refocus` is `true` for in-menu dismissals (an item was picked, or
    /// Escape was pressed) and `false` when the user moved to another window:
    /// refocusing then would yank focus back from the app they just clicked.
    Close { refocus: bool },
}

pub struct MenuWindow {
    window: Arc<Window>,
    wgpu: WgpuContext,
    egui_ctx: egui::Context,
    egui_state: egui_winit::State,
    kind: MenuKind,
    /// Screen position (logical points) of the menu's top-left corner.
    anchor: Pos2,
    /// Where the menu's content starts inside the window, in points.
    area_offset: Vec2,
    /// The monitor's usable area, in logical points, fixed for the menu's life.
    work_area: Rect,
    /// Last physical `(x, y, width, height)` applied to the OS window.
    applied_px: Option<(i32, i32, u32, u32)>,
    /// `false` until the first pass has measured the menu and pinned `anchor`.
    measured: bool,
    shown: bool,
    /// Windows sends a focus-lost event while the window is being created;
    /// only a focus loss *after* a focus gain means "the user clicked away".
    focused_once: bool,
}

impl MenuWindow {
    /// Creates the popup for `kind`, with its top-left corner at `anchor`
    /// (logical screen points).
    pub fn open(
        event_loop: &ActiveEventLoop,
        primary: &WgpuContext,
        kind: MenuKind,
        anchor: Pos2,
    ) -> Option<Self> {
        let attributes = Window::default_attributes()
            .with_title("Oxide Menu")
            .with_decorations(false)
            .with_resizable(false)
            .with_visible(false)
            .with_window_level(WindowLevel::AlwaysOnTop)
            .with_inner_size(LogicalSize::new(1.0, 1.0))
            .with_position(LogicalPosition::new(anchor.x as f64, anchor.y as f64));

        let window = match event_loop.create_window(attributes) {
        Ok(window) => window,
        Err(e) => {
            tracing::error!("failed to create the menu window: {e}");
            return None;
        }
    };
        #[cfg(windows)]
        hide_from_taskbar(&window);
        let window = Arc::new(window);

        let work_area = work_area_of(&window);
        let wgpu = WgpuContext::new_secondary(window.clone(), primary);
        let egui_ctx = egui::Context::default();
        crate::ui::apply_style(&egui_ctx);
        let egui_state = egui_winit::State::new(
            egui_ctx.clone(),
            egui::ViewportId::ROOT,
            &window,
            Some(window.scale_factor() as f32),
            None,
            None,
        );

        Some(Self {
            window,
            wgpu,
            egui_ctx,
            egui_state,
            kind,
            anchor,
            area_offset: Vec2::ZERO,
            work_area,
            applied_px: None,
            measured: false,
            shown: false,
            focused_once: false,
        })
    }

    pub fn window_id(&self) -> WindowId {
        self.window.id()
    }

    /// Runs one egui pass for the menu.
    fn run_pass(&mut self, state: &BrowserState) -> (egui::FullOutput, MenuPicks) {
        let mut raw = self.egui_state.take_egui_input(&self.window);
        // The menu is positioned in screen coordinates, so the context's
        // screen rect is the monitor's work area seen from this window.
        raw.screen_rect = Some(self.screen_rect());

        let offset = self.area_offset;
        let kind = self.kind.clone();
        let mut picks = MenuPicks::default();
        let output = self.egui_ctx.run(raw, |ctx| {
            egui::Area::new(egui::Id::new(MENU_AREA_ID))
                .order(egui::Order::Foreground)
                .fixed_pos(offset.to_pos2())
                // The window is fitted to the content instead, so egui must
                // not move the menu on its own: that would fight the fit.
                .constrain(false)
                .show(ctx, |ui| {
                    egui::Frame::popup(&ctx.style())
                        .shadow(egui::Shadow::NONE)
                        .corner_radius(0.0)
                        .show(ui, |ui| {
                            ui.set_min_width(MIN_MENU_WIDTH);
                            picks = menu::build(ui, state, &kind);
                        });
                });
        });
        self.egui_state
            .handle_platform_output(&self.window, output.platform_output.clone());
        (output, picks)
    }

    /// Runs one frame: builds the menu, re-fits the window, and paints.
    ///
    /// Returns what the user picked. `activated` is also set when the menu's
    /// target disappeared, which closes the popup.
    pub fn frame(&mut self, state: &BrowserState) -> MenuPicks {
        if !self.kind.is_valid(state) {
            return MenuPicks {
                actions: Vec::new(),
                activated: true,
            };
        }

        // `egui::Area` marks its very first pass as an invisible sizing pass
        // (`output.shapes` is empty) so it can measure the content first.
        // Run that sizing pass up front and clamp `anchor` before the paint
        // pass, so the first rendered frame already has the menu at its final
        // position and size. Its `textures_delta` holds the initial font
        // atlas (`Managed(0)`), so it must be merged into the paint pass.
        let mut textures_delta = egui::TexturesDelta::default();
        if !self.measured {
            let (sizing, _) = self.run_pass(state);
            textures_delta.append(sizing.textures_delta);
            let used = painted_rect(&self.egui_ctx);
            if used.width() >= 1.0 && used.height() >= 1.0 {
                let (offset, content) = layout(used, self.area_offset);
                self.area_offset = offset;
                self.anchor.x = self.anchor.x.clamp(
                    self.work_area.left(),
                    (self.work_area.right() - content.width()).max(self.work_area.left()),
                );
                self.anchor.y = self.anchor.y.clamp(
                    self.work_area.top(),
                    (self.work_area.bottom() - content.height()).max(self.work_area.top()),
                );
                self.measured = true;
            }
        }

        let (output, picks) = self.run_pass(state);
        textures_delta.append(output.textures_delta);

        let used = painted_rect(&self.egui_ctx);
        if used.width() >= 1.0 && used.height() >= 1.0 {
            let (offset, content) = layout(used, self.area_offset);
            self.area_offset = offset;
            let target = Rect::from_min_size(self.anchor + content.min.to_vec2(), content.size());
            self.fit(target);
        }

        // `visible_layer_ids` still reports the previous frame's areas, so a
        // submenu that just closed keeps the window large for a frame or two.
        // Keep repainting until only the base menu is left, otherwise a dark
        // rectangle is left over the page.
        if let Some(base) = self
            .egui_ctx
            .memory(|m| m.area_rect(egui::Id::new(MENU_AREA_ID)))
            && used != base
        {
            self.window.request_redraw();
        }

        if let Err(e) = self.wgpu.render(
            &self.egui_ctx,
            output.shapes,
            &textures_delta,
            output.pixels_per_point,
        ) {
            match e {
                wgpu::SurfaceError::Outdated | wgpu::SurfaceError::Lost => {
                    self.wgpu.surface.configure(&self.wgpu.device, &self.wgpu.config);
                    self.window.request_redraw();
                }
                other => tracing::error!("menu surface error: {other:?}"),
            }
        }

        // Paint before showing so the first visible frame is the menu, not an
        // empty surface.
        self.show();

        picks
    }

    /// Feeds a window event to egui. Returns what the event means for the
    /// menu's lifetime.
    pub fn handle_event(&mut self, event: &WindowEvent) -> MenuEvent {
        let response = self.egui_state.on_window_event(&self.window, event);
        if response.repaint {
            self.window.request_redraw();
        }

        match event {
            WindowEvent::CloseRequested => MenuEvent::Close { refocus: false },
            WindowEvent::Resized(size) => {
                if self.applied_px.is_none() {
                    self.wgpu.resize(*size);
                }
                self.window.request_redraw();
                MenuEvent::Continue
            }
            WindowEvent::Focused(true) => {
                self.focused_once = true;
                MenuEvent::Continue
            }
            // Clicking anywhere else gives focus back to that window, which is
            // exactly "the user dismissed the menu". Do not steal focus back:
            // they are now working in another window.
            WindowEvent::Focused(false) => {
                if self.focused_once {
                    MenuEvent::Close { refocus: false }
                } else {
                    MenuEvent::Continue
                }
            }
            WindowEvent::KeyboardInput { event, .. }
                if event.state == ElementState::Pressed
                    && event.logical_key == Key::Named(NamedKey::Escape) =>
            {
                MenuEvent::Close { refocus: true }
            }
            _ => MenuEvent::Continue,
        }
    }

    /// The monitor's work area, expressed in this window's coordinates.
    fn screen_rect(&self) -> Rect {
        let origin = self
            .applied_px
            .map(|(x, y, _, _)| {
                let scale = self.window.scale_factor() as f32;
                Pos2::new(x as f32 / scale, y as f32 / scale)
            })
            .unwrap_or(self.anchor);
        self.work_area.translate(-origin.to_vec2())
    }

    /// Moves and resizes the window to `target` (logical screen points).
    ///
    /// Does not show the window: the caller paints first, so the first
    /// visible frame is the menu rather than an empty surface.
    fn fit(&mut self, target: Rect) {
        let scale = self.window.scale_factor() as f32;
        let x = (target.min.x * scale).round() as i32;
        let y = (target.min.y * scale).round() as i32;
        let width = (target.width().max(1.0) * scale).round().max(1.0) as u32;
        let height = (target.height().max(1.0) * scale).round().max(1.0) as u32;
        let bounds = (x, y, width, height);

        if self.applied_px != Some(bounds) {
            #[cfg(windows)]
            win32_set_bounds(&self.window, x, y, width, height);
            #[cfg(not(windows))]
            {
                self.window.set_outer_position(LogicalPosition::new(
                    target.min.x as f64,
                    target.min.y as f64,
                ));
                let _ = self.window.request_inner_size(LogicalSize::new(
                    target.width().max(1.0) as f64,
                    target.height().max(1.0) as f64,
                ));
            }
            self.applied_px = Some(bounds);
        }

        let want_size = winit::dpi::PhysicalSize::new(width, height);
        if self.wgpu.config.width != want_size.width || self.wgpu.config.height != want_size.height
        {
            self.wgpu.resize(want_size);
        }
    }

    /// Shows the window once it has been painted at the right size.
    fn show(&mut self) {
        if !self.shown {
            self.window.set_visible(true);
            // `set_visible` in winit re-applies its stored `GWL_EXSTYLE`, so
            // `WS_EX_TOOLWINDOW` must be set again after showing.
            #[cfg(windows)]
            hide_from_taskbar(&self.window);
            tracing::debug!(
                "menu window shown at {:?} size {:?}",
                self.window.outer_position(),
                self.window.inner_size()
            );
            self.shown = true;
        }
    }
}

/// Sets the popup's exact physical screen position and client size in one call.
///
/// `winit`'s `request_inner_size` on Windows runs `AdjustWindowRectEx` with
/// `WS_BORDER | WS_EX_WINDOWEDGE` still set even when `decorations` is `false`
/// (adding ~6–8px of phantom border), uses `SWP_ASYNCWINDOWPOS`, and always
/// returns `None`. Setting the bounds directly avoids the border inflation and
/// sizes the window synchronously before the first present.
#[cfg(windows)]
fn win32_set_bounds(window: &Window, x: i32, y: i32, width: u32, height: u32) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        SetWindowPos, SWP_NOACTIVATE, SWP_NOZORDER,
    };
    use wry::raw_window_handle::{HasWindowHandle, RawWindowHandle};

    let Ok(handle) = window.window_handle() else {
        return;
    };
    let RawWindowHandle::Win32(handle) = handle.as_raw() else {
        return;
    };
    let hwnd = HWND(handle.hwnd.get() as *mut core::ffi::c_void);
    unsafe {
        let _ = SetWindowPos(
            hwnd,
            None,
            x,
            y,
            width as i32,
            height as i32,
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
}

/// The union of every visible area's rect, in window coordinates.
///
/// `Context::used_rect` is not usable here: it only unions `Order::Middle`
/// areas and skips sublayers, so it misses both the menu (drawn in the
/// foreground) and its submenus. The popup has to be fitted to everything it
/// paints, so the rects are unioned directly.
fn painted_rect(ctx: &egui::Context) -> Rect {
    let ids: Vec<egui::Id> = ctx.memory(|m| {
        m.areas()
            .visible_layer_ids()
            .into_iter()
            // `Context::begin_pass` registers `LayerId::background()` covering
            // the whole `screen_rect` (the monitor's work area). Including it
            // would size the popup window to the entire screen.
            .filter(|layer| layer.order != egui::Order::Background)
            .map(|layer| layer.id)
            .collect()
    });
    ctx.memory(|m| {
        let mut used = Rect::NOTHING;
        for id in ids {
            if let Some(rect) = m.area_rect(id) {
                used = used.union(rect);
            }
        }
        used
    })
}

/// Splits a painted rect into the menu's offset inside the window and the
/// window rect relative to the anchor.
///
/// `used` is in window coordinates and `area_offset` is where the menu was
/// placed, so subtracting the offset expresses the painted rect relative to
/// the anchor — which is what keeps the base menu still while submenus grow
/// the window.
fn layout(used: Rect, area_offset: Vec2) -> (Vec2, Rect) {
    let content = used.translate(-area_offset);
    (-content.min.to_vec2(), content)
}

/// The monitor's usable area (excluding the taskbar), in logical points.
fn work_area_of(window: &Window) -> Rect {
    #[cfg(windows)]
    if let Some(rect) = win32_work_area(window) {
        return rect;
    }

    let scale = window.scale_factor() as f32;
    if let Some(monitor) = window.current_monitor() {
        let position = monitor.position();
        let size = monitor.size();
        return Rect::from_min_size(
            Pos2::new(position.x as f32 / scale, position.y as f32 / scale),
            Vec2::new(size.width as f32 / scale, size.height as f32 / scale),
        );
    }
    Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 800.0))
}

#[cfg(windows)]
fn win32_work_area(window: &Window) -> Option<Rect> {
    use windows::Win32::Foundation::{HWND, RECT};
    use windows::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    };
    use wry::raw_window_handle::{HasWindowHandle, RawWindowHandle};

    let handle = window.window_handle().ok()?;
    let RawWindowHandle::Win32(handle) = handle.as_raw() else {
        return None;
    };
    let hwnd = HWND(handle.hwnd.get() as *mut core::ffi::c_void);
    let monitor = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        rcMonitor: RECT::default(),
        rcWork: RECT::default(),
        dwFlags: 0,
    };
    if !unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
        return None;
    }
    let scale = window.scale_factor() as f32;
    let work = info.rcWork;
    Some(Rect::from_min_max(
        Pos2::new(work.left as f32 / scale, work.top as f32 / scale),
        Pos2::new(work.right as f32 / scale, work.bottom as f32 / scale),
    ))
}

/// Keeps the popup out of the taskbar and the alt-tab list.
#[cfg(windows)]
fn hide_from_taskbar(window: &Window) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, SetWindowLongPtrW, SetWindowPos, GWL_EXSTYLE, SWP_FRAMECHANGED,
        SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, WS_EX_APPWINDOW, WS_EX_TOOLWINDOW,
    };
    use wry::raw_window_handle::{HasWindowHandle, RawWindowHandle};

    let Ok(handle) = window.window_handle() else {
        return;
    };
    let RawWindowHandle::Win32(handle) = handle.as_raw() else {
        return;
    };
    let hwnd = HWND(handle.hwnd.get() as *mut core::ffi::c_void);
    unsafe {
        let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        // `WS_EX_APPWINDOW` forces a taskbar button even for a tool window,
        // so it has to be cleared, not just shadowed by `WS_EX_TOOLWINDOW`.
        let style = (style & !(WS_EX_APPWINDOW.0 as isize)) | WS_EX_TOOLWINDOW.0 as isize;
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, style);
        // Style changes only reach the shell after a frame change.
        let _ = SetWindowPos(
            hwnd,
            None,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_FRAMECHANGED,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn painted_rect_measures_foreground_areas() {
        // Regression: `Context::used_rect` skips `Order::Foreground` areas,
        // which left the popup window unfitted and therefore never shown.
        let ctx = egui::Context::default();
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0))),
            ..Default::default()
        };
        let _ = ctx.run(raw, |ctx| {
            egui::Area::new(egui::Id::new("test_menu"))
                .order(egui::Order::Foreground)
                .fixed_pos(Pos2::new(10.0, 20.0))
                .constrain(false)
                .show(ctx, |ui| {
                    ui.set_min_size(Vec2::new(200.0, 100.0));
                });
        });

        let used = painted_rect(&ctx);
        assert_eq!(
            used,
            Rect::from_min_size(Pos2::new(10.0, 20.0), Vec2::new(200.0, 100.0)),
            "painted_rect must return the foreground area's rect, not the full screen_rect"
        );
    }

    #[test]
    fn a_menu_without_submenus_fills_the_window() {
        let used = Rect::from_min_size(Pos2::new(0.0, 0.0), Vec2::new(200.0, 300.0));
        let (offset, content) = layout(used, Vec2::ZERO);
        assert_eq!(offset, Vec2::ZERO);
        assert_eq!(content, used);
    }

    #[test]
    fn a_submenu_to_the_left_grows_the_window_leftwards() {
        // The menu sits at the anchor; a submenu opened 60pt to its left.
        let used = Rect::from_min_max(Pos2::new(-60.0, 0.0), Pos2::new(200.0, 300.0));
        let (offset, content) = layout(used, Vec2::ZERO);
        assert_eq!(offset, Vec2::new(60.0, 0.0), "content shifts right in the window");
        assert_eq!(content.min, Pos2::new(-60.0, 0.0), "window starts left of the anchor");
        assert_eq!(content.size(), Vec2::new(260.0, 300.0));
    }

    #[test]
    fn the_fit_is_stable_across_frames() {
        // A pass in which a submenu grew to the left of the menu, painted in
        // a window that already holds the base menu 60pt in.
        let used = Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(260.0, 300.0));
        let (offset, content) = layout(used, Vec2::new(60.0, 0.0));
        assert_eq!(offset, Vec2::new(60.0, 0.0));
        assert_eq!(
            content,
            Rect::from_min_max(Pos2::new(-60.0, 0.0), Pos2::new(200.0, 300.0))
        );

        // Re-running the pass with the fitted offset paints the same rect, so
        // the window does not move again next frame.
        let (offset2, content2) = layout(used, offset);
        assert_eq!(offset2, offset, "the offset must not drift");
        assert_eq!(content2, content, "the window rect must not drift");
    }
}
