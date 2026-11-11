//! Application lifecycle, Winit event loop, and WGPU/Egui renderer.

use crate::browser::{BrowserState, PendingDetach, PendingMenu};
use crate::menu_window::{MenuEvent, MenuWindow};
use crate::ui::render_browser_ui;
use crate::wgpu_context::WgpuContext;
use egui_winit::State;
use std::sync::Arc;
use tracing::{error, info};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalPosition, LogicalSize, PhysicalSize};
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowAttributes, WindowId};

/// One browser window and everything it owns.
///
/// Windows are fully independent: their own strip, page, GPU surface, egui
/// context and context-menu popup. Dragging a tab out of one creates another
/// (see [`BrowserApp::detach_tabs`]) rather than sharing state, which keeps
/// every window's WebViews parented to exactly one OS window.
struct BrowserWindow {
    wgpu: WgpuContext,
    egui_ctx: egui::Context,
    egui_state: State,
    state: BrowserState,
    /// The context-menu popup: a borderless window stacked above the page, so
    /// the menu and the live page are visible at the same time. See
    /// [`crate::menu_window`].
    menu: Option<MenuWindow>,
}

impl BrowserWindow {
    fn new(window: Arc<Window>, tokio_handle: tokio::runtime::Handle, start_page: bool) -> Self {
        let wgpu = pollster::block_on(WgpuContext::new(window.clone()));
        let egui_ctx = egui::Context::default();
        let egui_state = State::new(
            egui_ctx.clone(),
            egui::ViewportId::ROOT,
            &window,
            Some(window.scale_factor() as f32),
            None,
            None,
        );

        let state = if start_page {
            BrowserState::new(tokio_handle)
        } else {
            // A window created by a tear-off is filled with the dragged tabs
            // on the same frame; a start page would flash and then vanish.
            BrowserState::empty(tokio_handle)
        };

        Self {
            wgpu,
            egui_ctx,
            egui_state,
            state,
            menu: None,
        }
    }
}

/// The attributes of a fresh browser window.
fn window_attributes() -> WindowAttributes {
    Window::default_attributes()
        .with_title("Oxide Browser")
        .with_inner_size(LogicalSize::new(1280.0, 820.0))
        .with_min_inner_size(LogicalSize::new(640.0, 480.0))
}

/// Whether an event on the main window means "the user left the menu".
///
/// The popup usually catches this itself by losing focus, but a window that
/// never received focus would otherwise leave a menu stuck on screen.
fn dismisses_menu(event: &WindowEvent) -> bool {
    match event {
        WindowEvent::Focused(true) => true,
        WindowEvent::MouseInput {
            state: ElementState::Pressed,
            ..
        } => true,
        WindowEvent::KeyboardInput { event, .. } => event.state == ElementState::Pressed,
        _ => false,
    }
}

/// The main Oxide Browser application runner.
pub struct BrowserApp {
    #[allow(dead_code)]
    tokio_runtime: tokio::runtime::Runtime,
    windows: Vec<BrowserWindow>,
}

impl BrowserApp {
    /// Initializes runtime and egui context.
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let tokio_runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()?;

        Ok(Self {
            tokio_runtime,
            windows: Vec::new(),
        })
    }

    /// Creates an OS window with its own state and returns its index.
    fn create_window(
        &mut self,
        event_loop: &ActiveEventLoop,
        attributes: WindowAttributes,
        start_page: bool,
    ) -> Option<usize> {
        let window = match event_loop.create_window(attributes) {
            Ok(window) => Arc::new(window),
            Err(e) => {
                error!("Failed to create window: {:?}", e);
                return None;
            }
        };

        let handle = self.tokio_runtime.handle().clone();
        self.windows
            .push(BrowserWindow::new(window, handle, start_page));
        Some(self.windows.len() - 1)
    }

    fn window_index(&self, window_id: WindowId) -> Option<usize> {
        self.windows
            .iter()
            .position(|w| w.wgpu.window.id() == window_id)
    }

    /// The window whose context-menu popup is `window_id`, if any.
    fn menu_owner(&self, window_id: WindowId) -> Option<usize> {
        self.windows
            .iter()
            .position(|w| w.menu.as_ref().is_some_and(|m| m.window_id() == window_id))
    }

    /// Drops the popup menu (which closes its window).
    ///
    /// `refocus` hands focus back to the browser window; it is only set for
    /// in-menu dismissals, never when the user moved to another window.
    fn close_menu(&mut self, index: usize, refocus: bool) {
        if self.windows[index].menu.take().is_some() && refocus {
            self.windows[index].wgpu.window.focus_window();
        }
    }

    /// Closes one window. The last one saves the session and quits.
    fn close_window(&mut self, event_loop: &ActiveEventLoop, index: usize) {
        if self.windows.len() > 1 {
            self.windows.remove(index);
            return;
        }

        // Only the last window's strip is remembered: the session file holds
        // one strip, and a crash keeps the previous file instead of a
        // truncated one.
        if let Some(window) = self.windows.first() {
            crate::session::save(&window.state);
        }
        info!("Close requested, exiting Oxide");
        event_loop.exit();
    }

    /// Routes an event for the popup window owned by `index`.
    fn menu_event(&mut self, index: usize, event: WindowEvent) {
        let mut close = None;
        {
            let window = &mut self.windows[index];
            if let Some(menu) = window.menu.as_mut() {
                match event {
                    WindowEvent::RedrawRequested => {
                        let picks = menu.frame(&window.state);
                        crate::ui::tab_actions::apply_all(&mut window.state, picks.actions);
                        if picks.activated {
                            close = Some(true);
                        }
                    }
                    other => {
                        if let MenuEvent::Close { refocus } = menu.handle_event(&other) {
                            close = Some(refocus);
                        }
                    }
                }
            }
        }
        if let Some(refocus) = close {
            self.close_menu(index, refocus);
        }
    }

    /// Paints and processes one frame of the window at `index`.
    fn redraw(&mut self, event_loop: &ActiveEventLoop, index: usize) {
        let detach = {
            let BrowserWindow {
                wgpu,
                egui_ctx,
                egui_state,
                state,
                menu,
            } = &mut self.windows[index];

            // Poll any WebView and background network responses
            state.poll_webview_events();
            state.poll_network_events();
            state.poll_favicons();
            // New sites are picked up here, so an icon that arrives after
            // first paint is still cached for the next frame.
            state.refresh_favicons();

            let raw_input = egui_state.take_egui_input(&wgpu.window);
            let full_output = egui_ctx.run(raw_input, |ctx| {
                render_browser_ui(ctx, state);
            });

            egui_state.handle_platform_output(&wgpu.window, full_output.platform_output);

            // A right-click recorded during the UI pass opens the menu in its
            // own window; the page is left alone, so it stays live.
            if let Some(pending) = state.pending_menu.take() {
                open_menu(menu, state, wgpu, event_loop, pending);
            }

            // A drag released outside the strip asks for a window, which only
            // the event loop can create; it is handled below.
            state.ensure_and_sync_webviews(&wgpu.window);

            if let Err(e) = wgpu.render(
                egui_ctx,
                full_output.shapes,
                &full_output.textures_delta,
                full_output.pixels_per_point,
            ) {
                match e {
                    wgpu::SurfaceError::Outdated | wgpu::SurfaceError::Lost => {
                        wgpu.surface.configure(&wgpu.device, &wgpu.config);
                        wgpu.window.request_redraw();
                    }
                    wgpu::SurfaceError::OutOfMemory => {
                        error!("Surface out of memory!");
                        event_loop.exit();
                    }
                    other => {
                        error!("WGPU surface render error: {:?}", other);
                    }
                }
            }

            // If active tab is still loading, request another frame to animate
            // the spinner.
            if state.active_tab().is_some_and(|t| t.loading) {
                wgpu.window.request_redraw();
            }

            state.pending_detach.take()
        };

        if let Some(pending) = detach {
            self.detach_tabs(event_loop, index, pending);
        }
    }

    /// Opens a new window for tabs dragged out of the window at `source`.
    ///
    /// The tabs are copied across (URL, title, back/forward history, group) and
    /// removed from the source strip. The page itself reloads in the new
    /// window: a `wry` WebView is a native child window of exactly one parent,
    /// so carrying the live page over would mean reparenting it by hand.
    fn detach_tabs(&mut self, event_loop: &ActiveEventLoop, source: usize, pending: PendingDetach) {
        let (tabs, groups, attributes) = {
            let window = &mut self.windows[source];

            // Groups are read before the tabs leave, then remapped into the
            // new window.
            let groups = window.state.groups_of(&pending.ids);
            let tabs = window.state.take_tabs_for_window(&pending.ids);
            if tabs.is_empty() {
                return;
            }

            // Same size as the source, opened just under the cursor so the
            // window appears where the tab was dropped.
            let scale = window.wgpu.window.scale_factor();
            let origin = window.wgpu.window.inner_position().unwrap_or_default();
            let x = origin.x as f64 / scale + pending.pos.x as f64;
            let y = origin.y as f64 / scale + pending.pos.y as f64;
            let size = window.wgpu.window.inner_size();

            let attributes = Window::default_attributes()
                .with_title("Oxide Browser")
                .with_inner_size(PhysicalSize::new(size.width, size.height))
                .with_min_inner_size(LogicalSize::new(640.0, 480.0))
                .with_position(LogicalPosition::new(
                    (x - 120.0).max(0.0),
                    (y - 16.0).max(0.0),
                ));
            (tabs, groups, attributes)
        };

        let Some(target) = self.create_window(event_loop, attributes, false) else {
            // No window could be created: put the tabs back instead of losing
            // them.
            let window = &mut self.windows[source];
            window.state.restore_tabs(tabs, groups);
            return;
        };

        {
            let window = &mut self.windows[target];
            window.state.replace_strip_with(tabs, groups);
            window.wgpu.window.request_redraw();
        }

        // A window emptied by the drag has nothing left to show, exactly like
        // Chrome closing the source window after a tear-off.
        if self.windows[source].state.tabs.is_empty() {
            self.windows.remove(source);
        }
    }
}

/// Places the popup for a pending request and paints its first frame.
///
/// The request carries a position in the main window's logical client
/// coordinates; the popup needs screen coordinates, so the window's client
/// origin is added.
///
/// A free function rather than a method: it runs in the middle of the main
/// window's redraw, where `state` and `wgpu_ctx` are already borrowed.
fn open_menu(
    slot: &mut Option<MenuWindow>,
    state: &mut BrowserState,
    primary: &WgpuContext,
    event_loop: &ActiveEventLoop,
    pending: PendingMenu,
) {
    let scale = primary.window.scale_factor() as f32;
    let origin = primary.window.inner_position().unwrap_or_default();
    let anchor = egui::pos2(
        origin.x as f32 / scale + pending.pos.x,
        origin.y as f32 / scale + pending.pos.y,
    );

    // Only one menu at a time; dropping the old one closes its window.
    *slot = None;

    tracing::debug!("opening {:?} menu at {anchor:?}", pending.kind);

    let Some(mut menu) = MenuWindow::open(event_loop, primary, pending.kind, anchor) else {
        error!("Failed to open the context-menu window");
        return;
    };

    // Paint immediately: the window is created hidden and only becomes visible
    // once it knows how big the menu is.
    let picks = menu.frame(state);
    crate::ui::tab_actions::apply_all(state, picks.actions);
    if !picks.activated {
        *slot = Some(menu);
    }
}

impl ApplicationHandler for BrowserApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if !self.windows.is_empty() {
            return;
        }

        info!("Initializing Oxide Window and WGPU Graphics Context");

        let Some(index) = self.create_window(event_loop, window_attributes(), true) else {
            return;
        };

        // "Continue where you left off": a saved strip replaces the start page.
        crate::session::restore(&mut self.windows[index].state);

        info!("Oxide Browser shell initialized successfully");
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        // The context menu lives in its own window, above the page.
        if let Some(index) = self.menu_owner(window_id) {
            self.menu_event(index, event);
            return;
        }

        let Some(index) = self.window_index(window_id) else {
            return;
        };

        // Anything the user does in a browser window dismisses its menu.
        if self.windows[index].menu.is_some() && dismisses_menu(&event) {
            self.close_menu(index, false);
        }

        let repaint = {
            let window = &mut self.windows[index];
            window
                .egui_state
                .on_window_event(&window.wgpu.window, &event)
                .repaint
        };
        if repaint {
            self.windows[index].wgpu.window.request_redraw();
        }

        match event {
            WindowEvent::CloseRequested => self.close_window(event_loop, index),
            WindowEvent::Resized(new_size) => {
                let window = &mut self.windows[index];
                window.wgpu.resize(new_size);
                if let Some(tab) = window.state.active_tab_mut() {
                    tab.recompute_layout(new_size.width as f32, new_size.height as f32);
                }
                window.wgpu.window.request_redraw();
            }
            WindowEvent::RedrawRequested => self.redraw(event_loop, index),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        for window in &mut self.windows {
            let wgpu_ctx = &window.wgpu;
            let state = &mut window.state;

            state.poll_webview_events();
            state.poll_network_events();
            state.poll_favicons();
            if state.take_dirty() {
                wgpu_ctx.window.request_redraw();
            }
            if state.active_tab().is_some_and(|t| t.loading) {
                wgpu_ctx.window.request_redraw();
            }
            // A menu or tear-off request is honoured right away, even if the
            // UI pass that produced it did not ask for another frame.
            if state.pending_menu.is_some() || state.pending_detach.is_some() {
                wgpu_ctx.window.request_redraw();
            }
        }
    }
}
