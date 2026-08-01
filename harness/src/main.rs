use std::sync::Arc;

use glam::{Quat, Vec3, vec3};
use luxa::{Engine, Node3DHandle, SceneHandle};
use winit::dpi::LogicalSize;
use winit::{
  application::ApplicationHandler,
  event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent},
  event_loop::{ActiveEventLoop, EventLoop},
  window::{Window, WindowId},
};

// How fast the camera orbits per pixel of mouse movement (radians).
const ORBIT_SENSITIVITY: f32 = 0.005;
// How fast the camera zooms per unit of scroll (world units).
const ZOOM_SENSITIVITY: f32 = 0.3;
// Keep the camera from crossing the origin or drifting too far away.
const MIN_RADIUS: f32 = 0.5;
const MAX_RADIUS: f32 = 50.0;

#[derive(Default)]
struct App {
  engine: Option<Engine>,
  window: Option<Arc<Window>>,
  scene: Option<SceneHandle>,
  camera: Option<Node3DHandle>,

  // Orbit camera state: the camera always looks at the origin and sits on a
  // sphere of `radius` around it, positioned by `yaw` (around Y) and `pitch`.
  yaw: f32,
  pitch: f32,
  radius: f32,
  dragging: bool,
  last_cursor: Option<(f64, f64)>,
}

impl App {
  // Convert the current yaw/pitch/radius into a world position and point the
  // camera back at the origin.
  fn update_orbit_camera(&mut self) {
    let (Some(engine), Some(camera)) = (self.engine.as_mut(), self.camera) else {
      return;
    };

    let x = self.radius * self.pitch.cos() * self.yaw.sin();
    let y = self.radius * self.pitch.sin();
    let z = self.radius * self.pitch.cos() * self.yaw.cos();

    let node = engine.node_mut(camera);
    node.set_position(vec3(x, y, z));
    node.look_at(Vec3::ZERO);
  }
}

impl ApplicationHandler for App {
  fn resumed(&mut self, event_loop: &ActiveEventLoop) {
    if self.window.is_some() {
      return;
    }

    let window = Arc::new(
      event_loop
        .create_window(Window::default_attributes().with_title("Luxa Test Harness").with_inner_size(LogicalSize::new(1024, 1024)))
        .expect("failed to create window"),
    );

    let window_size = window.inner_size();

    let mut engine = pollster::block_on(Engine::new(Arc::clone(&window), (window_size.width, window_size.height))).expect("failed to create engine");

    let (scene, root) = engine.create_scene();
    // engine.create_light_node(root, vec3(5.3, 3.2, -3.5), vec3(1.0, 1.0, 1.0), 15.0);
    // engine.create_light_node(root, vec3(-7.0, 2.0, 1.0), vec3(1.0, 0.3, 0.1), 15.0);
    // engine.create_light_node(root, vec3(3.0, 5.0, 4.0), vec3(0.1, 0.8, 0.3), 15.0);

    // Initial camera position is not important, since we will immediately update it
    self.camera = Some(engine.create_camera_node(root, vec3(0.0, 0.0, 3.0), vec3(0.0, 0.0, 0.0), Vec3::ONE, 45.0, 0.1, 100.0));

    self.yaw = 0.0;
    self.pitch = 0.6;
    self.radius = 3.0;

    let node = engine.create_node(root, vec3(0.0, 0.0, 0.0), Quat::IDENTITY, vec3(1.0, 1.0, 1.0));
    let pot = engine.load_gltf("../assets/models/khronos/PotOfCoals.glb", node).expect("failed to load gltf");
    engine.node_mut(pot).set_position(vec3(0.0, 0.0, 0.0));
    engine.node_mut(pot).set_scale(vec3(20.0, 20.0, 20.0));
    let cube = engine.load_gltf("../assets/models/khronos/Cube.glb", node).expect("failed to load gltf");
    engine.node_mut(cube).set_scale(vec3(1.0, 1.0, 1.0));
    engine.node_mut(cube).set_position(vec3(0.0, -1.0, 0.0));

    window.request_redraw();
    self.window = Some(window);
    self.engine = Some(engine);
    self.scene = Some(scene);

    self.update_orbit_camera();
  }

  fn window_event(&mut self, event_loop: &ActiveEventLoop, window_id: WindowId, event: WindowEvent) {
    // Clone the Arc so we hold an owned handle rather than a borrow of `self`,
    let Some(window) = self.window.clone().filter(|window| window.id() == window_id) else {
      return;
    };

    match event {
      WindowEvent::CloseRequested => event_loop.exit(),

      WindowEvent::Resized(size) => {
        if let Some(engine) = self.engine.as_mut() {
          engine.resize((size.width, size.height));
        }
      }

      WindowEvent::RedrawRequested => {
        if let Some(engine) = self.engine.as_mut() {
          if let Some(scene) = self.scene {
            engine.render(scene, self.camera.unwrap()).expect("Rendering failed");
          }
        }

        window.request_redraw();
      }

      WindowEvent::MouseInput { state, button, .. } => {
        if button == MouseButton::Left {
          self.dragging = state == ElementState::Pressed;
          // Reset the drag anchor so the next move starts a fresh delta.
          self.last_cursor = None;
        }
      }

      WindowEvent::CursorMoved { position, .. } => {
        if self.dragging {
          if let Some((last_x, last_y)) = self.last_cursor {
            let dx = (position.x - last_x) as f32;
            let dy = (position.y - last_y) as f32;

            self.yaw -= dx * ORBIT_SENSITIVITY;
            self.pitch += dy * ORBIT_SENSITIVITY;

            // Clamp pitch just short of the poles to avoid the view flipping.
            let limit = std::f32::consts::FRAC_PI_2 - 0.01;
            self.pitch = self.pitch.clamp(-limit, limit);

            self.update_orbit_camera();
          }
          self.last_cursor = Some((position.x, position.y));
        }
      }

      WindowEvent::MouseWheel { delta, .. } => {
        // Line-based wheels report whole notches; touchpads report pixels.
        let scroll = match delta {
          MouseScrollDelta::LineDelta(_, y) => y,
          MouseScrollDelta::PixelDelta(pos) => pos.y as f32 / 20.0,
        };

        // Scroll up (positive) zooms in by shrinking the radius.
        self.radius = (self.radius - scroll * ZOOM_SENSITIVITY).clamp(MIN_RADIUS, MAX_RADIUS);
        self.update_orbit_camera();
      }

      _ => {}
    }
  }
}

fn main() {
  env_logger::init();
  let event_loop = EventLoop::new().expect("failed to create event loop");
  event_loop.run_app(&mut App::default()).expect("event loop failed");
}
