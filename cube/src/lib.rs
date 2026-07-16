mod app;
mod camera;
mod models;
mod platform;
mod renderer;
mod wgpu_helper;

// Desktop entry point. This is called from the main function to start the app.
#[cfg(not(target_arch = "wasm32"))]
pub fn start() -> anyhow::Result<()> {
  platform::launch()
}

// Web/WASM entry point. This is called automatically when the WASM module is loaded in the browser.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn start() -> Result<(), wasm_bindgen::JsValue> {
  platform::launch()
}
