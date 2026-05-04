/// wgpu renderer + winit window + frame loop.
///
/// Strategie: jeden render pass per frame. Display list (painting) preveden
/// na vertex bufer (rectangly = 2 trojuhelniky). Text zatim ne (potrebuje glyph atlas).
///
/// Module je gated za feature `gui` aby `cargo build` v knihovne (testy)
/// nemusel slingovat winit/wgpu pri kazde kompilaci.

use super::paint::DisplayCommand;

#[cfg(feature = "gui")]
mod gpu_impl {
    use super::*;
    // ... budouci wgpu kod ...
}

/// Public API renderer - vytvori window, spustí event loop, vykresli display list.
/// Tento entry point bude volan z main.rs pri rezimu "browser run".
///
/// Aktualne stub - skutecny wgpu kod si vyzada zaslouzene mnozstvi setup.
pub fn run_browser(html: &str, css: &str) {
    use super::{html_parser, css_parser, cascade, layout, paint};

    let document = html_parser::parse_html(html, "about:blank");
    let stylesheets = vec![css_parser::parse_stylesheet(css)];
    let style_map = cascade::cascade(&document.root, &stylesheets);

    let viewport_w = 1024.0;
    let viewport_h = 768.0;
    let layout_root = layout::layout_tree(&document.root, &style_map, viewport_w, viewport_h);
    let display_list = paint::build_display_list(&layout_root);

    // Bez wgpu init: jen vypisem kolik commandu mame
    println!("Document title: {}", document.title);
    println!("Display list: {} commands", display_list.len());
    for (i, cmd) in display_list.iter().enumerate().take(10) {
        println!("  [{i}] {cmd:?}");
    }
    if display_list.len() > 10 {
        println!("  ... +{} more", display_list.len() - 10);
    }

    println!();
    println!("Pro real rendering spusti binar s argumentem 'window' (vyzaduje winit + wgpu setup).");
}

/// Public API - spusteni real GUI okna pres winit + wgpu.
/// Tato funkce je sync (block_on pro async wgpu init).
pub fn run_window_with_html(html: String, css: String) -> Result<(), String> {
    use winit::application::ApplicationHandler;
    use winit::event::WindowEvent;
    use winit::event_loop::{ActiveEventLoop, EventLoop};
    use winit::window::{Window, WindowId};

    struct App {
        html: String,
        css: String,
        window: Option<std::sync::Arc<Window>>,
        renderer: Option<Renderer>,
    }

    struct Renderer {
        surface: wgpu::Surface<'static>,
        device: wgpu::Device,
        queue: wgpu::Queue,
        config: wgpu::SurfaceConfiguration,
    }

    impl ApplicationHandler for App {
        fn resumed(&mut self, event_loop: &ActiveEventLoop) {
            let attrs = Window::default_attributes()
                .with_title("Rust Web Engine")
                .with_inner_size(winit::dpi::LogicalSize::new(1024.0, 768.0));
            let window = std::sync::Arc::new(event_loop.create_window(attrs).unwrap());
            self.window = Some(window.clone());

            // wgpu init
            let instance = wgpu::Instance::default();
            let surface = instance.create_surface(window.clone()).unwrap();
            let adapter = pollster::block_on(instance.request_adapter(
                &wgpu::RequestAdapterOptions {
                    compatible_surface: Some(&surface),
                    ..Default::default()
                }
            )).unwrap();
            let (device, queue) = pollster::block_on(adapter.request_device(
                &wgpu::DeviceDescriptor::default(),
                None,
            )).unwrap();
            let size = window.inner_size();
            let surface_caps = surface.get_capabilities(&adapter);
            let config = wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format: surface_caps.formats[0],
                width: size.width,
                height: size.height,
                present_mode: wgpu::PresentMode::Fifo,
                alpha_mode: surface_caps.alpha_modes[0],
                view_formats: vec![],
                desired_maximum_frame_latency: 2,
            };
            surface.configure(&device, &config);

            self.renderer = Some(Renderer { surface, device, queue, config });

            // Trigger inital paint
            window.request_redraw();
        }

        fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
            match event {
                WindowEvent::CloseRequested => event_loop.exit(),
                WindowEvent::Resized(size) => {
                    if let Some(r) = &mut self.renderer {
                        r.config.width = size.width.max(1);
                        r.config.height = size.height.max(1);
                        r.surface.configure(&r.device, &r.config);
                    }
                }
                WindowEvent::RedrawRequested => {
                    self.render();
                    if let Some(w) = &self.window { w.request_redraw(); }
                }
                _ => {}
            }
        }
    }

    impl App {
        fn render(&self) {
            let r = match &self.renderer { Some(r) => r, None => return };
            let frame = match r.surface.get_current_texture() {
                Ok(f) => f,
                Err(_) => return,
            };
            let view = frame.texture.create_view(&Default::default());
            let mut encoder = r.device.create_command_encoder(&Default::default());
            {
                let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("clear"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.95, g: 0.95, b: 0.97, a: 1.0 }),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                // TODO: render display list (rectangly + text) pres wgpu pipeline
            }
            r.queue.submit(std::iter::once(encoder.finish()));
            frame.present();
        }
    }

    let event_loop = EventLoop::new().map_err(|e| e.to_string())?;
    let mut app = App { html, css, window: None, renderer: None };
    event_loop.run_app(&mut app).map_err(|e| e.to_string())?;
    Ok(())
}
