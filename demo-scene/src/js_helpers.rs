use gloo_net::http::Request;
use wasm_bindgen::{JsCast, JsValue, prelude::Closure};
use web_sys::HtmlCanvasElement;

pub fn init_browser_logging() {
  console_error_panic_hook::set_once();
  console_log::init_with_level(log::Level::Info).ok();
}

pub fn canvas_by_id(id: &str) -> Result<HtmlCanvasElement, JsValue> {
  let document = web_sys::window().and_then(|window| window.document()).ok_or_else(|| JsValue::from_str("no document"))?;
  let element = document.get_element_by_id(id).ok_or_else(|| JsValue::from_str(&format!("no element with id '{id}'")))?;
  element.dyn_into().map_err(|_| JsValue::from_str(&format!("element with id '{id}' is not a canvas")))
}

pub fn set_element_html(id: &str, html: &str) -> Result<(), JsValue> {
  let document = web_sys::window().and_then(|window| window.document()).ok_or_else(|| JsValue::from_str("no document"))?;
  if let Some(element) = document.get_element_by_id(id) {
    element.set_inner_html(html);
  }
  Ok(())
}

pub fn request_animation_frame(callback: impl FnOnce() + 'static) -> Result<i32, JsValue> {
  // A one-shot JS callback is freed after it fires. The callback can schedule the next frame itself.
  let callback = Closure::once_into_js(callback);
  web_sys::window()
    .ok_or_else(|| JsValue::from_str("no window"))?
    .request_animation_frame(callback.unchecked_ref())
}

pub async fn fetch_bytes(url: &str) -> Result<Vec<u8>, gloo_net::Error> {
  log::info!("Fetching file from {url}");
  Request::get(url).send().await?.binary().await
}

// F is the handler; E is the concrete event type (MouseEvent, WheelEvent...).
// `'static` because the browser may call it at any future point, so it can't
// borrow anything with a shorter lifetime.
pub fn add_listener<E, F>(target: &web_sys::EventTarget, event: &str, mut handler: F)
where
  E: JsCast,             // so we can turn the generic JS event into our concrete type
  F: FnMut(E) + 'static, // FnMut: it mutates captured state (CAM_STATE) across calls
{
  // Wrap so JS receives a web_sys::Event, then downcast to the concrete type.
  let closure = Closure::<dyn FnMut(web_sys::Event)>::new(move |e: web_sys::Event| {
    handler(e.dyn_into::<E>().unwrap());
  });
  target
    .add_event_listener_with_callback(event, closure.as_ref().unchecked_ref())
    .expect("failed to add listener");
  closure.forget(); // leak it: lives for the page lifetime
}
