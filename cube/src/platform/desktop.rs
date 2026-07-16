use std::sync::Arc;

use winit::dpi::LogicalSize;
use winit::event_loop::EventLoop;
use winit::window::{Window, WindowAttributes};

use crate::app::App;
use crate::renderer::Renderer;

pub use std::time::Instant;

pub struct Runtime {
  renderer: Option<Renderer>,
}

impl Runtime {
  pub fn new() -> Self {
    Self { renderer: None }
  }

  pub fn window_attributes(&self, name: &str) -> WindowAttributes {
    Window::default_attributes().with_title(name).with_inner_size(LogicalSize::new(1024, 768))
  }

  // On desktop we synchronously create the renderer, since we can block using pollster
  pub fn start_renderer(&mut self, window: Arc<Window>) -> anyhow::Result<()> {
    let renderer = pollster::block_on(Renderer::new(Arc::clone(&window), window.inner_size()))?;

    self.renderer = Some(renderer);
    window.request_redraw();

    Ok(())
  }

  pub fn with_renderer_mut<T>(&mut self, callback: impl FnOnce(&mut Renderer) -> T) -> Option<T> {
    self.renderer.as_mut().map(callback)
  }

  pub fn is_ready(&self) -> bool {
    self.renderer.is_some()
  }

  pub fn report_error(&self, message: &str) {
    log::error!("{message}");
  }
}

pub fn launch() -> anyhow::Result<()> {
  let _ = env_logger::try_init();
  let event_loop = EventLoop::new()?;
  let mut app = App::new("wgpu cube", Runtime::new());

  event_loop.run_app(&mut app)?;

  Ok(())
}
