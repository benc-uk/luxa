use std::sync::Arc;

use glam::{Quat, Vec3, vec3};
use wgpu_engine::{Engine, MeshBuilder, Size};
use winit::dpi::LogicalSize;
use winit::{
  application::ApplicationHandler,
  event::WindowEvent,
  event_loop::{ActiveEventLoop, EventLoop},
  window::{Window, WindowId},
};

#[derive(Default)]
struct App {
  engine: Option<Engine>,
  window: Option<Arc<Window>>,
  scene: Option<wgpu_engine::SceneHandle>,
  camera: Option<wgpu_engine::Node3DHandle>,
  c1: Option<wgpu_engine::Node3DHandle>,
  c2: Option<wgpu_engine::Node3DHandle>,
}

impl ApplicationHandler for App {
  fn resumed(&mut self, event_loop: &ActiveEventLoop) {
    if self.window.is_some() {
      return;
    }

    let window = Arc::new(
      event_loop
        .create_window(Window::default_attributes().with_title("WGPU-Engine Harness").with_inner_size(LogicalSize::new(1280, 720)))
        .expect("failed to create window"),
    );

    let window_size = window.inner_size();

    let mut engine = pollster::block_on(Engine::new(
      Arc::clone(&window),
      Size {
        width: window_size.width,
        height: window_size.height,
      },
    ))
    .expect("failed to create engine");

    let crate_tex = engine.create_texture("assets/crate_wood.jpg").expect("failed to load texture");
    let ball_tex = engine.create_texture("assets/ball.jpg").expect("failed to load texture");
    let crate_mat1 = engine.create_material(Some(crate_tex));
    let ball_mat = engine.create_material(Some(ball_tex));
    // engine.material_mut(ball_mat).set_base_color([0.1, 0.5, 1.0, 1.0]);

    let meshbox = MeshBuilder::new(&engine).add_primitive_cube().set_material(crate_mat1).build(&mut engine);
    let meshball = MeshBuilder::new(&engine).add_primitive_sphere(24, 24).set_material(ball_mat).build(&mut engine);

    let (scene, root) = engine.create_scene();
    let n1 = engine.create_node(root, vec3(0.8, 0.0, 0.0), Quat::IDENTITY, Vec3::ONE); // blue
    let n2 = engine.create_node(root, vec3(-0.8, 0.0, 0.0), Quat::IDENTITY, Vec3::ONE); // not blue
    let camera = engine.create_camera_node(root, vec3(0.0, 0.0, 2.0), vec3(0.0, 0.0, 0.0), Vec3::ONE, 45.0, 0.1, 100.0);
    self.c1 = Some(engine.create_mesh_node(n1, vec![meshball], vec3(0.0, 0.0, 0.0), Quat::IDENTITY, Vec3::ONE));
    self.c2 = Some(engine.create_mesh_node(n2, vec![meshbox], vec3(0.0, 0.0, 0.0), Quat::IDENTITY, Vec3::ONE));

    let light1 = engine.create_light_node(root, vec3(15.0, 2.0, 3.0), vec3(1.0, 1.0, 1.0), 0.7);
    let light2 = engine.create_light_node(root, vec3(-5.0, 6.0, 7.0), vec3(1.0, 1.0, 0.0), 0.4);

    window.request_redraw();
    self.window = Some(window);
    self.engine = Some(engine);
    self.scene = Some(scene);
    self.camera = Some(camera);
  }

  fn window_event(&mut self, event_loop: &ActiveEventLoop, window_id: WindowId, event: WindowEvent) {
    let Some(window) = self.window.as_ref().filter(|window| window.id() == window_id) else {
      return;
    };

    match event {
      WindowEvent::CloseRequested => event_loop.exit(),

      WindowEvent::Resized(size) => {
        if let Some(engine) = self.engine.as_mut() {
          engine.resize(Size {
            width: size.width,
            height: size.height,
          });
        }
      }

      WindowEvent::RedrawRequested => {
        if let Some(engine) = self.engine.as_mut() {
          // Update the engine by one frame
          engine.update();

          let h = (engine.t() * 2.5).sin() * 0.3;

          // Loan for c1: opens and closes on this one line.
          engine.node_mut(self.c1.unwrap()).set_position(vec3(-h, 0.0, 0.0));
          let rotation = Quat::from_rotation_y(engine.t());
          engine.node_mut(self.c2.unwrap()).set_rotation(rotation);
          engine.node_mut(self.c1.unwrap()).set_rotation(-rotation);

          // Loan for the camera: nothing else touches `engine` while it's alive, so it's fine.
          let camera = engine.node_mut(self.camera.unwrap());
          camera.set_position(vec3(0.0, 1.5, 2.8));
          camera.look_at(vec3(0.0, 0.0, 0.0));

          // Render happens here!
          if let Some(scene) = self.scene {
            engine.render(scene, self.camera.unwrap()).expect("Rendering failed");
          }
        }

        window.request_redraw();
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
