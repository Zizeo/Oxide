//! Oxide Browser - Main Executable
//!
//! A modular, Chromium-inspired browser built in safe, idiomatic Rust.

mod app;
mod browser;
mod favicon;
mod menu_window;
mod session;
mod snapshot;
mod tab;
mod ui;
mod wgpu_context;

use app::BrowserApp;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};
use winit::event_loop::{ControlFlow, EventLoop};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize structured logging
    tracing_subscriber::registry()
        .with(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,oxide_browser=debug,oxide_net=debug")),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    tracing::info!("Starting Oxide Browser v0.1.0 (Milestone 1)...");

    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = BrowserApp::new()?;
    event_loop.run_app(&mut app)?;

    Ok(())
}
