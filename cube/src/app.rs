use std::sync::Arc;

use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowId};

use crate::platform::Runtime;

pub struct App {
  // The name of the application. This is used for the window title and other purposes.
  name: String,

  // Abstraction over the platform-specific runtime (desktop or web)
  // This is what holds the reference to the WGPU Renderer
  runtime: Runtime,

  // The window that the application is rendering to. This is first created when the application is resumed.
  window: Option<Arc<Window>>,
}

impl App {
  pub fn new(name: &str, runtime: Runtime) -> Self {
    Self {
      name: name.to_string(),
      runtime,
      window: None,
    }
  }
}

impl ApplicationHandler for App {
  fn resumed(&mut self, event_loop: &ActiveEventLoop) {
    if self.window.is_some() {
      return;
    }

    let attributes = self.runtime.window_attributes(&self.name);
    let window = match event_loop.create_window(attributes) {
      Ok(window) => Arc::new(window),
      Err(error) => {
        self.runtime.report_error(&format!("Window creation failed: {error}"));
        return;
      }
    };

    if let Err(error) = self.runtime.start_renderer(Arc::clone(&window)) {
      self.runtime.report_error(&format!("Renderer initialisation failed: {error:#}"));
    }

    self.window = Some(window);
  }

  fn window_event(&mut self, event_loop: &ActiveEventLoop, _window_id: WindowId, event: WindowEvent) {
    match event {
      WindowEvent::Resized(size) => {
        self.runtime.with_renderer_mut(|renderer| renderer.resize(size));
      }

      WindowEvent::CloseRequested => event_loop.exit(),

      WindowEvent::RedrawRequested => {
        let render_result = self.runtime.with_renderer_mut(|renderer| {
          renderer.update();
          renderer.render()
        });

        if let Some(Err(error)) = render_result {
          self.runtime.report_error(&format!("Rendering failed: {error:#}"));
        }

        if self.runtime.is_ready()
          && let Some(window) = self.window.as_ref()
        {
          window.request_redraw();
        }
      }

      WindowEvent::KeyboardInput { event, .. } => {
        if event.logical_key == winit::keyboard::Key::Named(winit::keyboard::NamedKey::Escape) {
          event_loop.exit();
        }
      }

      _ => {}
    }
  }
}
