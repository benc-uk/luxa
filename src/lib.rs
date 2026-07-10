use std::sync::Arc;

use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowId};

mod camera;
mod models;
mod renderer;
mod wgpu_helper;

use crate::renderer::Renderer;

const WIDTH: u32 = 1024;
const HEIGHT: u32 = 768;

#[derive(Default)]
pub struct App {
  name: String,
  renderer: Option<Renderer>,
  window: Option<Arc<Window>>,
}

impl App {
  pub fn new(name: &str) -> Self {
    Self {
      name: name.to_string(),
      window: None,
      renderer: None,
    }
  }

  pub fn is_initialized(&self) -> bool {
    self.renderer.is_some()
  }
}

impl ApplicationHandler for App {
  // Despite the name this is called when the application is first created
  fn resumed(&mut self, event_loop: &ActiveEventLoop) {
    if self.window.is_none() {
      let size = LogicalSize::new(WIDTH, HEIGHT);

      let win = event_loop.create_window(Window::default_attributes().with_inner_size(size).with_title(&self.name)).unwrap();
      self.window = Some(Arc::new(win));
      println!("Window created with size: {:?}", size);

      let window_clone = self.window.as_ref().unwrap().clone();
      let renderer = pollster::block_on(Renderer::new(window_clone)).unwrap();
      self.renderer = Some(renderer);

      // Kick off the first frame. Each RedrawRequested re-arms the next via
      // request_redraw(), so this single call bootstraps the continuous loop.
      self.window.as_ref().unwrap().request_redraw();
    }
  }

  fn window_event(&mut self, event_loop: &ActiveEventLoop, _window_id: WindowId, event: winit::event::WindowEvent) {
    match event {
      WindowEvent::Resized(phys_size) => {
        if self.renderer.is_some() {
          self.renderer.as_mut().unwrap().resize(phys_size);
        }
      }

      WindowEvent::CloseRequested => {
        event_loop.exit();
      }

      WindowEvent::RedrawRequested => {
        if let Some(state) = self.renderer.as_mut() {
          state.update();
          state.render().expect("render completed")
        }

        if let Some(window) = self.window.as_ref() {
          window.request_redraw();
        }
      }

      WindowEvent::KeyboardInput { event, .. } => {
        // Exit the application if the Escape key is pressed
        if event.logical_key == winit::keyboard::Key::Named(winit::keyboard::NamedKey::Escape) {
          event_loop.exit();
        }
      }

      _ => (),
    }
  }
}
