#![cfg(target_arch = "wasm32")]

// ===================================================================================================
// Luxa web viewer: minimal WebAssembly entry point.
// For now this is just a "hello world" that proves the wasm toolchain, logging and DOM access work.
// ===================================================================================================
mod js_helpers;
mod orbit_controls;

use js_helpers::{canvas_by_id, fetch_bytes, init_browser_logging, request_animation_frame, set_element_html};
use luxa::Engine;
use luxa::ModelDescriptor;
use luxa::SceneDescriptor;

use luxa::glam::{EulerRot, Quat, Vec3, vec3};
use std::cell::{Cell, RefCell};
use wasm_bindgen::prelude::*;

thread_local! {
  static ENGINE: RefCell<Option<Engine>> = RefCell::new(None);
  static SCENE: Cell<Option<luxa::SceneHandle>> = Cell::new(None);
  static CAMERA: Cell<Option<luxa::CameraHandle>> = Cell::new(None);
  static MODEL_NODE : Cell<Option<luxa::NodeHandle>> = Cell::new(None);
}

const DEFAULT_MODEL: &str = "assets/models/khronos/DamagedHelmet.glb";
const DEFAULT_ENVIRONMENT: &str = "assets/ibl/colorful_studio_4k.hdr";

// Marks this as the module's entry point
#[wasm_bindgen(start)]
pub fn start() -> Result<(), JsValue> {
  init_browser_logging();
  let canvas = canvas_by_id("canvas")?;

  // Everything that is async must be inside this block, because of reasons
  wasm_bindgen_futures::spawn_local(async move {
    let size = (canvas.width(), canvas.height());
    orbit_controls::attach(&canvas);

    match Engine::new_from_canvas(canvas, size).await {
      Ok(engine) => {
        ENGINE.with(|cell| *cell.borrow_mut() = Some(engine));
      }

      Err(e) => log::error!("engine init failed: {e:#}"),
    }

    build_scene();
    load_model(DEFAULT_MODEL, "").await;
    change_environment(DEFAULT_ENVIRONMENT).await;

    set_message("");
    render_loop();
  });

  Ok(())
}

// Build the scene with the given model & HDR environment, and create a camera node.
// The camera node is stored in a thread-local so it can be updated each frame.
fn build_scene() {
  ENGINE.with(|cell| {
    if let Some(engine) = cell.borrow_mut().as_mut() {
      let scene_hdl = engine.create_scene(SceneDescriptor::default());
      let scene = engine.scene_mut(scene_hdl).unwrap();
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

      engine.skybox_set_mode(luxa::SkyboxMode::PrefilteredMap, 1.6);

      SCENE.with(|cell| cell.set(Some(scene_hdl)));
      CAMERA.with(|cell| cell.set(Some(camera)));
    }
  });

  set_message("");
}

#[wasm_bindgen]
pub async fn load_model(path: &str, material: &str) {
  set_message("🗿 Loading model...");
  let model_bytes = fetch_bytes(path).await.expect("failed to fetch model");
  let use_metal = material == "metal";

  ENGINE.with(|cell| {
    if let Some(engine) = cell.borrow_mut().as_mut() {
      let scene = SCENE.with(|cell| cell.get()).unwrap();

      if let Some(model) = MODEL_NODE.with(|cell| cell.get()) {
        let _ = engine.remove_node(model);
      }

      // Optionally skin every mesh in the model with the shared metal material.
      let material_override = use_metal.then(|| metal_material(engine));

      let model = engine
        .load_model_bytes(
          scene,
          &model_bytes,
          ModelDescriptor {
            material_override,
            ..Default::default()
          },
        )
        .unwrap();

      MODEL_NODE.with(|cell| cell.set(Some(model)));

      // Get the node AABB size and use that to scale the model to 1,1,1
      let aabb = engine.node(model).unwrap().aabb().unwrap();
      let size = aabb.size();
      let size_avg = (size.x + size.y + size.z) / 3.0;
      let center = aabb.center();
      let scale = vec3(1.0 / size_avg, 1.0 / size_avg, 1.0 / size_avg);

      // Move the model so that its center is at the origin, and scale it to fit in a 1x1x1 cube
      engine.node_mut(model).unwrap().set_scale(scale);
      engine.node_mut(model).unwrap().set_position(-scale * center);
    }
  });

  set_message("");
}

// The render loop. This function schedules itself to be called on each animation frame.
fn render_loop() {
  ENGINE.with(|cell| {
    if let Some(engine) = cell.borrow_mut().as_mut() {
      let camera = CAMERA.with(|cell| cell.get());
      if let Some(camera) = camera {
        let camera_node = camera;

        // Get the camera position from the orbit camera state and update the camera node.
        let (yaw, pitch, radius) = orbit_controls::state();

        let dir = Quat::from_euler(EulerRot::YXZ, yaw, pitch, 0.0) * Vec3::Z;
        if let Ok(node) = engine.node_mut(camera_node) {
          node.set_position(dir * radius);
          node.look_at(vec3(0.0, 0.0, 0.0), Vec3::Y);
        }

        // Actually rendering the scene happens here
        if let Err(e) = engine.render(camera_node) {
          log::error!("render failed: {e:#}");
        }
      }
    }
  });

  // Each invocation schedules the next, so the loop runs until the page closes.
  request_animation_frame(render_loop).expect("failed to request animation frame");
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
        engine.scene_mut(scene_handle).unwrap().set_ibl_enabled(true);
      }

      None => {
        engine.clear_environment();
        let scene = engine.scene_mut(scene_handle).unwrap();
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

// Creates the shared polished-metal material (aluminium/steel-like, fully metallic, low roughness).
fn metal_material(engine: &mut Engine) -> luxa::MaterialHandle {
  engine
    .create_material(luxa::MaterialDescriptor {
      base_color_factor: [0.52, 0.53, 0.55, 1.0],
      metallic_factor: 1.0,
      roughness_factor: 0.35,
      ..Default::default()
    })
    .expect("failed to create metal material")
}

// Update the loading message in the DOM. This is called from async functions, so it must be a separate function.
fn set_message(message: &str) {
  set_element_html("message", message).expect("failed to update message");
}
