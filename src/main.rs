use wgpu_learning::App;
use winit::event_loop::{ControlFlow, EventLoop};

fn main() {
  let event_loop = EventLoop::new().unwrap();
  event_loop.set_control_flow(ControlFlow::Poll);
  event_loop.set_control_flow(ControlFlow::Wait);

  env_logger::init();

  let mut app = App::new("WGPU Learning App");

  event_loop.run_app(&mut app).unwrap();
}
