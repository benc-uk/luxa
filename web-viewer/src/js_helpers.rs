use gloo_net::http::Request;
use wasm_bindgen::{JsCast, prelude::Closure};

pub async fn fetch_bytes(url: &str) -> Result<Vec<u8>, gloo_net::Error> {
  log::info!("Fetching file from {}", url);
  Request::get(url).send().await.expect("unable to load file").binary().await
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
