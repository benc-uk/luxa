// ===================================================================================================
// Luxa web viewer: minimal WebAssembly entry point.
// For now this is just a "hello world" that proves the wasm toolchain, logging and DOM access work.
// ===================================================================================================
mod js_helpers;

use glam::vec3;
use js_helpers::{add_listener, fetch_bytes};
use luxa::Engine;
use std::cell::{Cell, RefCell};
use wasm_bindgen::prelude::*;
use web_sys::{PointerEvent, WheelEvent};

// One tracked pointer (mouse button held, finger, or pen) and its last-seen canvas position.
struct Pointer {
  id: i32,
  x: f64,
  y: f64,
}

struct CamState {
  yaw: f32,
  pitch: f32,
  radius: f32,
  // Pointers currently pressed on the canvas. 1 => orbit drag, 2 => pinch zoom.
  pointers: Vec<Pointer>,
  // Distance between the two fingers on the previous pinch frame, for computing the delta.
  pinch_dist: Option<f64>,
}

thread_local! {
  static ENGINE: RefCell<Option<Engine>> = RefCell::new(None);
  static SCENE: Cell<Option<luxa::SceneHandle>> = Cell::new(None);
  static CAMERA: Cell<Option<luxa::Node3DHandle>> = Cell::new(None);
  static CAM_STATE: RefCell<CamState> = RefCell::new(CamState {
    yaw: 0.0,
    pitch: 0.0,
    radius: 2.0,
    pointers: Vec::new(),
    pinch_dist: None,
  });
}

const DEFAULT_MODEL: &str = "DamagedHelmet.glb";

// Marks this as the module's entry point
#[wasm_bindgen(start)]
pub fn start() -> Result<(), JsValue> {
  // Route Rust panics to the browser console with a readable stack trace instead of an opaque "unreachable".
  console_error_panic_hook::set_once();
  console_log::init_with_level(log::Level::Info).ok();

  // need to get the canvas element from the DOM and pass it to luxa::Engine::new() as a surface target
  let document = web_sys::window().and_then(|window| window.document()).ok_or_else(|| JsValue::from_str("no document"))?;
  let canvas = document
    .get_element_by_id("canvas")
    .ok_or_else(|| JsValue::from_str("no canvas element with id 'canvas'"))?;
  let canvas: web_sys::HtmlCanvasElement = canvas.dyn_into().map_err(|_| JsValue::from_str("element with id 'canvas' is not a canvas"))?;

  // Everything that is async must be inside this block, because of reasons
  wasm_bindgen_futures::spawn_local(async move {
    let size = (canvas.width(), canvas.height());
    setup_input(&canvas);

    match Engine::new_from_canvas(canvas, size).await {
      Ok(mut engine) => {
        ENGINE.with(|cell| *cell.borrow_mut() = Some(engine));
      }

      Err(e) => log::error!("engine init failed: {e:#}"),
    }

    let model_bytes = fetch_bytes(model_from_hash().as_str()).await.expect("failed to fetch model");
    let hdr_bytes = fetch_bytes("/assets/ibl/colorful_studio_4k.hdr").await.expect("failed to fetch HDR");

    build_scene(model_bytes, hdr_bytes);

    start_render_loop();
  });

  Ok(())
}

// Pick the model to load from the URL fragment, e.g. `.../index.html#water_bottle.glb`.
// The hash is returned including its leading '#', so we strip it; an empty hash falls back to the default.
fn model_from_hash() -> String {
  let hash = web_sys::window().and_then(|w| w.location().hash().ok()).unwrap_or_default();
  let name = hash.trim_start_matches('#');
  if name.is_empty() { DEFAULT_MODEL.to_string() } else { name.to_string() }
}

// Build the scene with the given model & HDR environment, and create a camera node.
// The camera node is stored in a thread-local so it can be updated each frame.
fn build_scene(model: Vec<u8>, hdr: Vec<u8>) {
  ENGINE.with(|cell| {
    if let Some(engine) = cell.borrow_mut().as_mut() {
      let (scene, root) = engine.create_scene();

      engine.create_light_node(root, vec3(5.3, 3.2, -3.5), vec3(1.0, 1.0, 1.0), 15.0);
      engine.create_light_node(root, vec3(-7.0, 2.0, 1.0), vec3(1.0, 0.3, 0.1), 15.0);
      engine.create_light_node(root, vec3(3.0, 5.0, 4.0), vec3(0.1, 0.8, 0.3), 15.0);

      engine.set_environment(&hdr);
      let n = engine.load_gltf_bytes(&model, root).unwrap();

      // Get the node AABB size and use that to scale the model to 1,1,1
      let aabb = engine.node(n).aabb().unwrap();
      let size = aabb.size();
      let size_avg = (size.x + size.y + size.z) / 3.0;
      let center = aabb.center();
      let scale = glam::vec3(1.0 / size_avg, 1.0 / size_avg, 1.0 / size_avg);

      // Move the model so that its center is at the origin, and scale it to fit in a 1x1x1 cube
      engine.node_mut(n).set_scale(scale);
      engine.node_mut(n).set_position(-scale * center);

      let camera = engine.create_camera_node(root, vec3(0.0, 1.0, 4.0), vec3(0.0, 0.0, 0.0), glam::Vec3::ONE, 70.0, 0.1, 200.0);
      SCENE.with(|cell| cell.set(Some(scene)));
      CAMERA.with(|cell| cell.set(Some(camera)));
    }
  });

  // remove #loading  div from the DOM, so the user can see the canvas
  let document = web_sys::window()
    .and_then(|window| window.document())
    .ok_or_else(|| JsValue::from_str("no document"))
    .unwrap();
  if let Some(loading) = document.get_element_by_id("loading") {
    loading.remove();
  }
}

fn setup_input(canvas: &web_sys::HtmlCanvasElement) {
  let target: &web_sys::EventTarget = canvas.as_ref();

  // A pointer went down: start tracking it and grab pointer capture so we keep
  // receiving its move/up events even if it strays outside the canvas mid-drag.
  // `canvas` is a JS handle, so cloning it is cheap (it just bumps a reference).
  let canvas_dn = canvas.clone();
  add_listener::<PointerEvent, _>(target, "pointerdown", move |e| {
    e.prevent_default();
    let id = e.pointer_id();
    let _ = canvas_dn.set_pointer_capture(id);
    CAM_STATE.with(|c| {
      let mut c = c.borrow_mut();
      let (x, y) = (e.client_x() as f64, e.client_y() as f64);
      if let Some(p) = c.pointers.iter_mut().find(|p| p.id == id) {
        p.x = x;
        p.y = y;
      } else {
        c.pointers.push(Pointer { id, x, y });
      }
      // Reset the pinch baseline; it's re-measured on the next two-finger move.
      c.pinch_dist = None;
    });
  });

  // A pointer moved: one pointer orbits, two pointers pinch-zoom.
  add_listener::<PointerEvent, _>(target, "pointermove", |e| {
    CAM_STATE.with(|c| {
      let mut c = c.borrow_mut();
      let id = e.pointer_id();
      let (x, y) = (e.client_x() as f64, e.client_y() as f64);

      // Ignore moves for pointers we're not tracking (e.g. hover with no button down).
      let Some(idx) = c.pointers.iter().position(|p| p.id == id) else {
        return;
      };
      let (px, py) = (c.pointers[idx].x, c.pointers[idx].y);
      c.pointers[idx].x = x;
      c.pointers[idx].y = y;

      match c.pointers.len() {
        1 => {
          // Single finger / mouse drag => orbit.
          let dx = (x - px) as f32;
          let dy = (y - py) as f32;
          c.yaw -= dx * 0.01;
          c.pitch = (c.pitch - dy * 0.01).clamp(-1.5, 1.5); // avoid flipping at the poles
        }
        2 => {
          // Two fingers => pinch: compare the current finger spread to the last
          // one and feed the change into the orbit radius (spread apart = zoom in).
          let ax = c.pointers[0].x;
          let ay = c.pointers[0].y;
          let bx = c.pointers[1].x;
          let by = c.pointers[1].y;
          let dist = ((ax - bx).powi(2) + (ay - by).powi(2)).sqrt();
          if let Some(prev) = c.pinch_dist {
            let delta = (prev - dist) as f32;
            c.radius = (c.radius + delta * 0.01).clamp(0.75, 50.0);
          }
          c.pinch_dist = Some(dist);
        }
        _ => {}
      }
    });
  });

  // A pointer was released or cancelled (finger lifted, gesture aborted): stop
  // tracking it. `drop_pointer` is a plain fn, so it can be reused for both events.
  add_listener::<PointerEvent, _>(target, "pointerup", drop_pointer);
  add_listener::<PointerEvent, _>(target, "pointercancel", drop_pointer);

  // Desktop mouse wheel still zooms directly.
  add_listener::<WheelEvent, _>(target, "wheel", |e| {
    e.prevent_default(); // stop the page scrolling
    CAM_STATE.with(|c| {
      let mut c = c.borrow_mut();
      c.radius = (c.radius + e.delta_y() as f32 * 0.0006).clamp(0.75, 50.0);
    });
  });
}

// Remove a finished pointer from the tracked set and clear the pinch baseline so the
// next two-finger gesture starts fresh. Used for both `pointerup` and `pointercancel`.
fn drop_pointer(e: PointerEvent) {
  CAM_STATE.with(|c| {
    let mut c = c.borrow_mut();
    c.pointers.retain(|p| p.id != e.pointer_id());
    c.pinch_dist = None;
  });
}

fn start_render_loop() {
  ENGINE.with(|cell| {
    if let Some(engine) = cell.borrow_mut().as_mut() {
      engine.update(); // advance the time uniform so animation progresses

      let scene = SCENE.with(|cell| cell.get());
      let camera = CAMERA.with(|cell| cell.get());
      if let (Some(scene), Some(camera)) = (scene, camera) {
        // Get the camera position from the orbit camera state and update the camera node.
        let (yaw, pitch, radius) = CAM_STATE.with(|c| {
          let c = c.borrow();
          (c.yaw, c.pitch, c.radius)
        });
        let dir = glam::Quat::from_euler(glam::EulerRot::YXZ, yaw, pitch, 0.0) * glam::Vec3::Z;
        engine.node_mut(camera).set_position(dir * radius);

        if let Err(e) = engine.render(scene, camera) {
          log::error!("render failed: {e:#}");
        }
      }
    }
  });

  // Schedule the next frame. `once_into_js` hands the browser a one-shot JS callback (freed after it fires),
  // and each invocation re-schedules the next, so the loop runs until the page closes.
  let callback = Closure::once_into_js(start_render_loop);
  web_sys::window()
    .expect("no window")
    .request_animation_frame(callback.unchecked_ref())
    .expect("failed to request animation frame");
}
