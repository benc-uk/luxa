use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use wasm_bindgen::{JsCast, JsValue};
use web_sys::HtmlCanvasElement;
use winit::dpi::LogicalSize;
use winit::event_loop::EventLoop;
use winit::platform::web::{EventLoopExtWebSys, WindowAttributesExtWebSys};
use winit::window::{Window, WindowAttributes};

use crate::app::App;
use crate::renderer::Renderer;

pub use web_time::Instant;

pub struct Runtime {
  canvas: HtmlCanvasElement,
  renderer: Rc<RefCell<Option<Renderer>>>,
}

impl Runtime {
  pub fn new(canvas: HtmlCanvasElement) -> Self {
    Self {
      canvas,
      renderer: Rc::new(RefCell::new(None)),
    }
  }

  pub fn window_attributes(&self, name: &str) -> WindowAttributes {
    Window::default_attributes()
      .with_title(name)
      .with_inner_size(LogicalSize::new(10, 10)) // Size is irrelevant, will immediately be updated by the renderer
      .with_canvas(Some(self.canvas.clone()))
      .with_prevent_default(false)
  }

  // This whole function is weird and littered with async WASM specific stuff. I don't like it, but there's no better way
  pub fn start_renderer(&mut self, window: Arc<Window>) -> anyhow::Result<()> {
    let renderer_state = Rc::clone(&self.renderer);

    wasm_bindgen_futures::spawn_local(async move {
      match Renderer::new(Arc::clone(&window), window.inner_size()).await {
        Ok(mut renderer) => {
          // Resize events received during async initialisation were ignored while
          // no renderer existed, so catch up to the window's current size now.
          renderer.resize(window.inner_size());
          *renderer_state.borrow_mut() = Some(renderer);
          window.request_redraw();
        }
        Err(error) => show_error(&format!("WebGPU initialisation failed: {error:#}")),
      }
    });

    Ok(())
  }

  pub fn with_renderer_mut<T>(&mut self, callback: impl FnOnce(&mut Renderer) -> T) -> Option<T> {
    self.renderer.borrow_mut().as_mut().map(callback)
  }

  pub fn is_ready(&self) -> bool {
    self.renderer.borrow().is_some()
  }

  pub fn report_error(&self, message: &str) {
    show_error(message);
  }
}

// Web/WASM entry point for the application. This is called from the HTML page to start the app.
pub fn launch() -> Result<(), JsValue> {
  console_error_panic_hook::set_once();
  console_log::init_with_level(log::Level::Info).map_err(|error| JsValue::from_str(&error.to_string()))?;

  let document = web_sys::window()
    .and_then(|window| window.document())
    .ok_or_else(|| JsValue::from_str("Browser document is unavailable"))?;

  let canvas = document
    .get_element_by_id("cube-canvas")
    .ok_or_else(|| JsValue::from_str("Canvas element #cube-canvas was not found"))?
    .dyn_into::<HtmlCanvasElement>()?;

  let event_loop = EventLoop::new().map_err(|error| JsValue::from_str(&error.to_string()))?;
  event_loop.spawn_app(App::new("wgpu cube", Runtime::new(canvas)));

  Ok(())
}

fn show_error(message: &str) {
  log::error!("{message}");
  if let Some(error_element) = web_sys::window()
    .and_then(|window| window.document())
    .and_then(|document| document.get_element_by_id("error"))
  {
    error_element.set_text_content(Some(message));
    let _ = error_element.set_attribute("data-visible", "true");
  }
}
