use std::sync::Arc;

use glam::{Quat, Vec3, vec3};
use wgpu_engine::{Engine, Size};
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
  thing: Option<wgpu_engine::Node3DHandle>,
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

    let (scene, root) = engine.create_scene();
    engine.create_light_node(root, vec3(15.0, 2.0, 3.0), vec3(1.0, 1.0, 1.0), 0.7);
    engine.create_light_node(root, vec3(-5.0, 6.0, 7.0), vec3(1.0, 0.3, 0.1), 0.4);
    self.camera = Some(engine.create_camera_node(root, vec3(0.0, 0.0, 2.0), vec3(0.0, 0.0, 0.0), Vec3::ONE, 45.0, 0.1, 100.0));
    let model_hdl = engine.load_gltf("./assets/water_bottle.glb", root).expect("failed to load gltf");
    engine.node_mut(model_hdl).set_scale(vec3(0.5, 0.5, 0.5));
    self.thing = Some(model_hdl);

    window.request_redraw();
    self.window = Some(window);
    self.engine = Some(engine);
    self.scene = Some(scene);
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

          let rotation = Quat::from_rotation_y(engine.t());
          engine.node_mut(self.thing.unwrap()).set_rotation(rotation);

          // Loan for the camera: nothing else touches `engine` while it's alive, so it's fine.
          let camera = engine.node_mut(self.camera.unwrap());
          camera.set_position(vec3(0.0, 0.0, 0.2));
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
