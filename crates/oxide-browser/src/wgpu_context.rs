use std::sync::Arc;
use egui_wgpu::{Renderer, ScreenDescriptor};
use winit::window::Window;

pub struct WgpuContext {
    pub window: Arc<Window>,
    pub surface: wgpu::Surface<'static>,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub config: wgpu::SurfaceConfiguration,
    pub renderer: Renderer,
    /// Kept so a second window (the menu popup) can create a surface that is
    /// compatible with the same device instead of spinning up its own GPU
    /// stack.
    pub instance: wgpu::Instance,
    pub adapter: wgpu::Adapter,
}

impl WgpuContext {
    pub async fn new(window: Arc<Window>) -> Self {
        let size = window.inner_size();
        let width = size.width.max(1);
        let height = size.height.max(1);

        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        // Arc<Window> provides 'static lifetime for the surface
        let surface = instance
            .create_surface(window.clone())
            .expect("Failed to create WGPU surface");

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .expect("Failed to find a suitable WGPU adapter");

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("oxide_gpu_device"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    memory_hints: Default::default(),
                },
                None,
            )
            .await
            .expect("Failed to create WGPU device");

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(surface_caps.formats[0]);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width,
            height,
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };

        surface.configure(&device, &config);

        let renderer = Renderer::new(&device, surface_format, None, 1, false);

        Self {
            window,
            surface,
            device,
            queue,
            config,
            renderer,
            instance,
            adapter,
        }
    }

    /// Builds a renderer for a second window on the same GPU device.
    ///
    /// Used by the menu popup: it needs its own surface and egui renderer, but
    /// a second adapter/device would cost a full GPU stack for a menu.
    pub fn new_secondary(window: Arc<Window>, primary: &WgpuContext) -> Self {
        let size = window.inner_size();
        let surface = primary
            .instance
            .create_surface(window.clone())
            .expect("Failed to create WGPU surface for the menu window");

        let surface_caps = surface.get_capabilities(&primary.adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(surface_caps.formats[0]);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&primary.device, &config);

        let renderer = Renderer::new(&primary.device, surface_format, None, 1, false);

        Self {
            window,
            surface,
            device: primary.device.clone(),
            queue: primary.queue.clone(),
            config,
            renderer,
            instance: primary.instance.clone(),
            adapter: primary.adapter.clone(),
        }
    }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(&self.device, &self.config);
        }
    }

    pub fn render(
        &mut self,
        egui_ctx: &egui::Context,
        shapes: Vec<egui::epaint::ClippedShape>,
        textures_delta: &egui::TexturesDelta,
        pixels_per_point: f32,
    ) -> Result<(), wgpu::SurfaceError> {
        let output = self.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let paint_jobs = egui_ctx.tessellate(shapes, pixels_per_point);

        let screen_descriptor = ScreenDescriptor {
            size_in_pixels: [self.config.width, self.config.height],
            pixels_per_point,
        };

        for (id, delta) in &textures_delta.set {
            self.renderer.update_texture(&self.device, &self.queue, *id, delta);
        }

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("oxide_render_encoder"),
            });

        self.renderer.update_buffers(
            &self.device,
            &self.queue,
            &mut encoder,
            &paint_jobs,
            &screen_descriptor,
        );

        {
            let mut rpass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("oxide_egui_render_pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: 0.06,
                                g: 0.07,
                                b: 0.09,
                                a: 1.0,
                            }),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                })
                .forget_lifetime();

            self.renderer
                .render(&mut rpass, &paint_jobs, &screen_descriptor);
        }

        for id in &textures_delta.free {
            self.renderer.free_texture(id);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(())
    }
}
