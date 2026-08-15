#![cfg(target_arch = "wasm32")]

// ===================================================================================================
// Luxa web viewer: minimal WebAssembly entry point.
// For now this is just a "hello world" that proves the wasm toolchain, logging and DOM access work.
// ===================================================================================================
mod js_helpers;
mod orbit_controls;

use js_helpers::{canvas_by_id, fetch_bytes, init_browser_logging, request_animation_frame, set_element_html};
use luxa::{AlphaMode, Engine, MaterialDescriptor, MeshBuilder, MeshNodeDescriptor, ModelDescriptor, SceneDescriptor};

use luxa::glam::{EulerRot, Quat, Vec3, vec3};
use std::cell::{Cell, RefCell};
use wasm_bindgen::prelude::*;

thread_local! {
  static ENGINE: RefCell<Option<Engine>> = RefCell::new(None);
  static SCENE: Cell<Option<luxa::SceneHandle>> = Cell::new(None);
  static CAMERA: Cell<Option<luxa::CameraHandle>> = Cell::new(None);
  static ORBIT_TARGET: Cell<Vec3> = Cell::new(Vec3::ZERO);
}

const TABLE_MODEL: &str = "assets/models/polyhaven/wooden_table_02_2k.glb";
const APPLE_MODEL: &str = "assets/models/polyhaven/food_apple_01_2k.glb";
const VASE_MODEL: &str = "assets/models/polyhaven/brass_vase_03_2k.glb";
const SEARCHLIGHT_MODEL: &str = "assets/models/polyhaven/portable_searchlight_2k.glb";
const FLOOR_MATERIAL: &str = "assets/models/polyhaven/floor_tiles_08_1k.glb";
const ENVIRONMENT: &str = "assets/ibl/photo_studio_loft_hall_2k.hdr";
const TABLE_WIDTH: f32 = 2.6;
// Prop heights in world units, and their placement on the tabletop as a fraction of the table half-size.
const VASE_HEIGHT: f32 = 0.792;
const APPLE_HEIGHT: f32 = 0.14;
const SEARCHLIGHT_HEIGHT: f32 = 0.315;
const VASE_OFFSET: (f32, f32) = (-0.12, -0.08);
const APPLE_OFFSET: (f32, f32) = (0.22, -0.08);
const APPLE2_OFFSET: (f32, f32) = (0.34, 0.12);
const SEARCHLIGHT_OFFSET: (f32, f32) = (-0.4, 0.28);
// Yaw applied to the apples so the pair don't look identical or regimented.
const APPLE1_YAW: f32 = 0.9;
const APPLE2_YAW: f32 = 2.1;
// Yaw so the searchlight faces into the scene rather than square-on.
const SEARCHLIGHT_YAW: f32 = 0.7;
const FLOOR_SIZE: f32 = 40.0;
const FLOOR_UV_TILING: f32 = 10.0;
const FLOOR_NORMAL_SCALE: f32 = 2.0;
const SHADOW_SEGMENTS: u32 = 48;
const FOOT_X_OFFSET: f32 = 0.4271;
const FOOT_Z_OFFSET: f32 = 0.3822;

// Marks this as the module's entry point
#[wasm_bindgen(start)]
pub fn start() -> Result<(), JsValue> {
  init_browser_logging();
  let canvas = canvas_by_id("canvas")?;

  // Everything that is async must be inside this block, because of reasons
  wasm_bindgen_futures::spawn_local(async move {
    let size = (canvas.width(), canvas.height());
    orbit_controls::attach(&canvas);

    let engine = match Engine::new_from_canvas(canvas, size).await {
      Ok(engine) => engine,
      Err(error) => {
        log::error!("engine init failed: {error:#}");
        set_message("Unable to initialise WebGPU");
        return;
      }
    };
    ENGINE.with(|cell| *cell.borrow_mut() = Some(engine));

    set_message("Loading table...");
    let table_bytes = match fetch_bytes(TABLE_MODEL).await {
      Ok(bytes) => bytes,
      Err(error) => {
        log::error!("failed to load table: {error}");
        set_message("Unable to load table model");
        return;
      }
    };

    set_message("Loading apple...");
    let apple_bytes = match fetch_bytes(APPLE_MODEL).await {
      Ok(bytes) => bytes,
      Err(error) => {
        log::error!("failed to load apple: {error}");
        set_message("Unable to load apple model");
        return;
      }
    };

    set_message("Loading vase...");
    let vase_bytes = match fetch_bytes(VASE_MODEL).await {
      Ok(bytes) => bytes,
      Err(error) => {
        log::error!("failed to load vase: {error}");
        set_message("Unable to load vase model");
        return;
      }
    };

    set_message("Loading searchlight...");
    let searchlight_bytes = match fetch_bytes(SEARCHLIGHT_MODEL).await {
      Ok(bytes) => bytes,
      Err(error) => {
        log::error!("failed to load searchlight: {error}");
        set_message("Unable to load searchlight model");
        return;
      }
    };

    set_message("Loading floor material...");
    let floor_material_bytes = match fetch_bytes(FLOOR_MATERIAL).await {
      Ok(bytes) => bytes,
      Err(error) => {
        log::error!("failed to load floor material: {error}");
        set_message("Unable to load floor material");
        return;
      }
    };

    set_message("Preparing studio lighting...");
    let environment_bytes = match fetch_bytes(ENVIRONMENT).await {
      Ok(bytes) => bytes,
      Err(error) => {
        log::error!("failed to load environment: {error}");
        set_message("Unable to load studio environment");
        return;
      }
    };

    build_scene(&table_bytes, &apple_bytes, &vase_bytes, &searchlight_bytes, &floor_material_bytes, &environment_bytes);

    set_message("");
    render_loop();
  });

  Ok(())
}

// Build the grounded table scene and create the orbit camera.
fn build_scene(table_bytes: &[u8], apple_bytes: &[u8], vase_bytes: &[u8], searchlight_bytes: &[u8], floor_material_bytes: &[u8], environment_bytes: &[u8]) {
  ENGINE.with(|cell| {
    if let Some(engine) = cell.borrow_mut().as_mut() {
      let scene_handle = engine.create_scene(SceneDescriptor {
        background_color: [0.12, 0.13, 0.14],
        ibl_enabled: true,
        ..Default::default()
      });

      engine.set_environment(environment_bytes).expect("failed to set studio environment");
      engine.skybox_set_mode(luxa::SkyboxMode::PrefilteredMap, 1.6);

      let floor_material = engine
        .import_gltf_materials_bytes(floor_material_bytes)
        .expect("failed to import floor material")
        .into_iter()
        .find(|material| material.index == 0)
        .expect("floor material GLB contains no material")
        .handle;
      engine.material_mut(floor_material).expect("missing floor material").set_normal_scale(FLOOR_NORMAL_SCALE);
      let floor_mesh = engine
        .create_mesh(MeshBuilder::new().plane().scale_uvs([FLOOR_UV_TILING, FLOOR_UV_TILING]).material(floor_material))
        .expect("failed to create floor mesh");
      engine
        .create_mesh_node(
          scene_handle,
          MeshNodeDescriptor {
            scale: vec3(FLOOR_SIZE, 1.0, FLOOR_SIZE),
            meshes: vec![floor_mesh],
            ..Default::default()
          },
        )
        .expect("failed to create floor node");

      let table = engine
        .load_model_bytes(scene_handle, table_bytes, ModelDescriptor::default())
        .expect("failed to load table model");
      let bounds = engine.node(table).expect("missing table node").aabb().expect("table model has no bounds");
      let size = bounds.size();
      let scale = TABLE_WIDTH / size.x.max(size.z);
      let table_size = size * scale;
      let table_height = size.y * scale;

      // Centre the table in XZ and put its lowest point exactly on the floor.
      engine.node_mut(table).expect("missing table node").set_scale(Vec3::splat(scale)).set_position(vec3(
        -bounds.center().x * scale,
        -bounds.min.y * scale,
        -bounds.center().z * scale,
      ));

      add_contact_shadows(engine, scene_handle, table_size);

      // Sit the props on the tabletop. Half the table size gives the usable radius for placement.
      let half = table_size * 0.5;
      place_prop(
        engine,
        scene_handle,
        vase_bytes,
        VASE_HEIGHT,
        half.x * VASE_OFFSET.0,
        half.z * VASE_OFFSET.1,
        table_height,
        0.0,
      );
      place_prop(
        engine,
        scene_handle,
        apple_bytes,
        APPLE_HEIGHT,
        half.x * APPLE_OFFSET.0,
        half.z * APPLE_OFFSET.1,
        table_height,
        APPLE1_YAW,
      );
      place_prop(
        engine,
        scene_handle,
        apple_bytes,
        APPLE_HEIGHT,
        half.x * APPLE2_OFFSET.0,
        half.z * APPLE2_OFFSET.1,
        table_height,
        APPLE2_YAW,
      );
      place_prop(
        engine,
        scene_handle,
        searchlight_bytes,
        SEARCHLIGHT_HEIGHT,
        half.x * SEARCHLIGHT_OFFSET.0,
        half.z * SEARCHLIGHT_OFFSET.1,
        table_height,
        SEARCHLIGHT_YAW,
      );

      let orbit_target = vec3(0.0, table_height, 0.0);
      let orbit_radius = TABLE_WIDTH * 0.85;
      orbit_controls::set_view(0.65, -0.28, orbit_radius);
      ORBIT_TARGET.with(|cell| cell.set(orbit_target));

      let (yaw, pitch, radius) = orbit_controls::state();
      let direction = Quat::from_euler(EulerRot::YXZ, yaw, pitch, 0.0) * Vec3::Z;

      let camera = engine
        .create_camera(
          scene_handle,
          luxa::CameraDescriptor {
            position: orbit_target + direction * radius,
            fov_degrees: 45.0,
            near_plane: 0.05,
            ..Default::default()
          },
        )
        .unwrap();

      SCENE.with(|cell| cell.set(Some(scene_handle)));
      CAMERA.with(|cell| cell.set(Some(camera)));
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
        let target = ORBIT_TARGET.with(|cell| cell.get());

        let dir = Quat::from_euler(EulerRot::YXZ, yaw, pitch, 0.0) * Vec3::Z;
        if let Ok(node) = engine.node_mut(camera_node) {
          node.set_position(target + dir * radius);
          node.look_at(target, Vec3::Y);
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

// Swap the HDR environment (and its baked IBL) at runtime, driven by the HTML picker.
#[wasm_bindgen]
pub async fn change_environment(path: &str) {
  set_message("Loading environment & baking IBL...");
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
        engine.skybox_set_mode(luxa::SkyboxMode::PrefilteredMap, 1.6);
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

// Update the loading message in the DOM. This is called from async functions, so it must be a separate function.
fn set_message(message: &str) {
  set_element_html("message", message).expect("failed to update message");
}

fn add_contact_shadows(engine: &mut Engine, scene: luxa::SceneHandle, table_size: Vec3) {
  // Split the broad shadow into faint nested layers. Their combined alpha gives a soft edge
  // without requiring a texture API or pretending that this is a general shadow solution.
  for (index, (coverage, opacity)) in [(0.95, 0.08), (0.78, 0.11), (0.58, 0.16)].into_iter().enumerate() {
    let mesh = create_shadow_disc(engine, opacity);
    engine
      .create_mesh_node(
        scene,
        MeshNodeDescriptor {
          position: vec3(0.0, 0.001 + index as f32 * 0.0005, 0.0),
          scale: vec3(table_size.x * coverage, 1.0, table_size.z * coverage),
          meshes: vec![mesh],
          ..Default::default()
        },
      )
      .expect("failed to create table contact shadow");
  }

  let foot_shadow = create_shadow_disc(engine, 0.4);
  let foot_offset = vec3(table_size.x * FOOT_X_OFFSET, 0.003, table_size.z * FOOT_Z_OFFSET);
  let foot_diameter = table_size.x.min(table_size.z) * 0.13;
  for x in [-foot_offset.x, foot_offset.x] {
    for z in [-foot_offset.z, foot_offset.z] {
      engine
        .create_mesh_node(
          scene,
          MeshNodeDescriptor {
            position: vec3(x, foot_offset.y, z),
            scale: vec3(foot_diameter, 1.0, foot_diameter),
            meshes: vec![foot_shadow],
            ..Default::default()
          },
        )
        .expect("failed to create foot contact shadow");
    }
  }
}

// Load a model, scale it to `target_height` world units and rest its base on the tabletop at (x, z).
fn place_prop(engine: &mut Engine, scene: luxa::SceneHandle, bytes: &[u8], target_height: f32, x: f32, z: f32, table_height: f32, yaw: f32) {
  let node = engine.load_model_bytes(scene, bytes, ModelDescriptor::default()).expect("failed to load prop model");
  let bounds = engine.node(node).expect("missing prop node").aabb().expect("prop model has no bounds");
  let scale = target_height / bounds.size().y;
  engine
    .node_mut(node)
    .expect("missing prop node")
    .set_scale(Vec3::splat(scale))
    .set_rotation(Quat::from_rotation_y(yaw))
    .set_position(vec3(x - bounds.center().x * scale, table_height - bounds.min.y * scale, z - bounds.center().z * scale));
}

fn create_shadow_disc(engine: &mut Engine, opacity: f32) -> luxa::MeshHandle {
  let material = engine
    .create_material(MaterialDescriptor {
      base_color_factor: [0.005, 0.005, 0.005, opacity],
      roughness_factor: 1.0,
      alpha_mode: AlphaMode::Blend,
      double_sided: true,
      ..Default::default()
    })
    .expect("failed to create contact shadow material");

  engine
    .create_mesh(MeshBuilder::new().disc(SHADOW_SEGMENTS).material(material))
    .expect("failed to create contact shadow mesh")
}
