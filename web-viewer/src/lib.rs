#![cfg(target_arch = "wasm32")]

// ===================================================================================================
// Luxa web viewer: minimal WebAssembly entry point.
// For now this is just a "hello world" that proves the wasm toolchain, logging and DOM access work.
// ===================================================================================================
mod js_helpers;

use js_helpers::{add_listener, fetch_bytes};
use luxa::Engine;
use luxa::ModelDescriptor;
use luxa::SceneDescriptor;

use luxa::glam::{EulerRot, Quat, Vec3, vec3};
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
  static CAMERA: Cell<Option<luxa::NodeHandle>> = Cell::new(None);
  static MODEL_NODE : Cell<Option<luxa::NodeHandle>> = Cell::new(None);
  static CAM_STATE: RefCell<CamState> = RefCell::new(CamState {
    yaw: 0.0,
    pitch: 0.0,
    radius: 1.6,
    pointers: Vec::new(),
    pinch_dist: None,
  });
}

const DEFAULT_MODEL: &str = "assets/models/khronos/PotOfCoals.glb";
const DEFAULT_ENVIRONMENT: &str = "assets/ibl/colorful_studio_4k.hdr";

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
      Ok(engine) => {
        ENGINE.with(|cell| *cell.borrow_mut() = Some(engine));
      }

      Err(e) => log::error!("engine init failed: {e:#}"),
    }

    build_scene();
    load_model(DEFAULT_MODEL).await;
    change_environment(DEFAULT_ENVIRONMENT).await;

    set_message("");
    render_loop();
  });

  Ok(())
}

#[wasm_bindgen]
pub async fn load_model(path: &str) {
  set_message("🗿 Loading model...");
  let model_bytes = fetch_bytes(path).await.expect("failed to fetch model");

  ENGINE.with(|cell| {
    if let Some(engine) = cell.borrow_mut().as_mut() {
      let scene = SCENE.with(|cell| cell.get()).unwrap();

      if let Some(model) = MODEL_NODE.with(|cell| cell.get()) {
        engine.remove_node(model);
      }

      let model = engine.load_gltf_bytes(scene, &model_bytes, ModelDescriptor::default()).unwrap();
      MODEL_NODE.with(|cell| cell.set(Some(model)));

      // Get the node AABB size and use that to scale the model to 1,1,1
      let aabb = engine.node(model).aabb().unwrap();
      let size = aabb.size();
      let size_avg = (size.x + size.y + size.z) / 3.0;
      let center = aabb.center();
      let scale = vec3(1.0 / size_avg, 1.0 / size_avg, 1.0 / size_avg);

      // Move the model so that its center is at the origin, and scale it to fit in a 1x1x1 cube
      engine.node_mut(model).set_scale(scale);
      engine.node_mut(model).set_position(-scale * center);
    }
  });

  set_message("");
}

#[wasm_bindgen]
pub async fn change_environment(path: &str) {
  set_message("🌅 Loading environment & baking IBL...");
  let hdr_bytes = if path == "disabled" {
    None
  } else {
    Some(fetch_bytes(path).await.expect("failed to fetch HDR"))
  };

  let Some(scene_handle) = SCENE.with(|cell| cell.get()) else {
    return;
  };

  ENGINE.with(|cell| {
    let mut engine = cell.borrow_mut();
    let Some(engine) = engine.as_mut() else { return };

    match hdr_bytes.as_deref() {
      Some(hdr_bytes) => {
        engine.set_environment(hdr_bytes).expect("failed to set environment");
        engine.scene_mut(scene_handle).set_ibl_enabled(true);
      }
      None => {
        engine.clear_environment();
        let scene = engine.scene_mut(scene_handle);
        scene.set_ibl_enabled(false);
        scene.set_ambient_intensity(0.2);
      }
    }
  });

  set_message("");
}

#[wasm_bindgen]
pub async fn set_skybox_mode(mode: &str) {
  ENGINE.with(|cell| {
    if let Some(engine) = cell.borrow_mut().as_mut() {
      match mode {
        "env" => engine.skybox_set_mode(luxa::SkyboxMode::EnvironmentMap, 0.0),
        "env_blurred" => engine.skybox_set_mode(luxa::SkyboxMode::EnvironmentMap, 5.0),
        "prefiltered" => engine.skybox_set_mode(luxa::SkyboxMode::PrefilteredMap, 1.6),
        "none" => engine.skybox_set_mode(luxa::SkyboxMode::None, 0.0),
        _ => log::warn!("unknown skybox mode: {mode}"),
      }
    }
  });
}

// Build the scene with the given model & HDR environment, and create a camera node.
// The camera node is stored in a thread-local so it can be updated each frame.
fn build_scene() {
  ENGINE.with(|cell| {
    if let Some(engine) = cell.borrow_mut().as_mut() {
      let scene_hdl = engine.create_scene(SceneDescriptor::default());
      let scene = engine.scene_mut(scene_hdl);
      scene.set_background_color([0.1, 0.1, 0.1]);
      scene.set_ambient_intensity(0.3);

      let camera = engine
        .create_camera(
          scene_hdl,
          luxa::CameraDescriptor {
            position: vec3(0.0, 0.0, 1.6),
            ..Default::default()
          },
        )
        .unwrap();
      engine.skybox_set_mode(luxa::SkyboxMode::EnvironmentMap, 0.0);

      SCENE.with(|cell| cell.set(Some(scene_hdl)));
      CAMERA.with(|cell| cell.set(Some(camera)));
    }
  });

  set_message("");
}

// Update the loading message in the DOM. This is called from async functions, so it must be a separate function.
fn set_message(message: &str) {
  let document = web_sys::window()
    .and_then(|window| window.document())
    .ok_or_else(|| JsValue::from_str("no document"))
    .unwrap();
  if let Some(message_div) = document.get_element_by_id("message") {
    message_div.set_inner_html(message);
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

// The render loop. This function schedules itself to be called on each animation frame.
fn render_loop() {
  ENGINE.with(|cell| {
    if let Some(engine) = cell.borrow_mut().as_mut() {
      engine.update(); // advance the time uniform so animation progresses

      let scene = SCENE.with(|cell| cell.get());
      let camera = CAMERA.with(|cell| cell.get());
      if let (Some(scene), Some(camera)) = (scene, camera) {
        // let rotation = Quat::from_rotation_y(engine.t() * 2.0);
        // engine.node_mut(camera).set_rotation(rotation);

        // Get the camera position from the orbit camera state and update the camera node.
        let (yaw, pitch, radius) = CAM_STATE.with(|c| {
          let c = c.borrow();
          (c.yaw, c.pitch, c.radius)
        });
        let dir = Quat::from_euler(EulerRot::YXZ, yaw, pitch, 0.0) * Vec3::Z;
        engine.node_mut(camera).set_position(dir * radius);
        engine.node_mut(camera).look_at(vec3(0.0, 0.0, 0.0), Vec3::Y);

        // Actually rendering the scene happens here
        if let Err(e) = engine.render(scene, camera) {
          log::error!("render failed: {e:#}");
        }
      }
    }
  });

  // Schedule the next frame. `once_into_js` hands the browser a one-shot JS callback (freed after it fires),
  // and each invocation re-schedules the next, so the loop runs until the page closes.
  let callback = Closure::once_into_js(render_loop);
  web_sys::window()
    .expect("no window")
    .request_animation_frame(callback.unchecked_ref())
    .expect("failed to request animation frame");
}
