// ===================================================================================================
// Browser pointer and wheel controls for an orbit camera.
// ===================================================================================================
use crate::js_helpers::add_listener;
use std::cell::RefCell;
use web_sys::{HtmlCanvasElement, PointerEvent, WheelEvent};

// One tracked pointer (mouse button held, finger, or pen) and its last-seen canvas position.
struct Pointer {
  id: i32,
  x: f64,
  y: f64,
}

struct OrbitState {
  yaw: f32,
  pitch: f32,
  radius: f32,
  // Pointers currently pressed on the canvas. 1 => orbit drag, 2 => pinch zoom.
  pointers: Vec<Pointer>,
  // Distance between the two fingers on the previous pinch frame, for computing the delta.
  pinch_dist: Option<f64>,
}

thread_local! {
  static STATE: RefCell<OrbitState> = RefCell::new(OrbitState {
    yaw: 0.0,
    pitch: 0.0,
    radius: 1.6,
    pointers: Vec::new(),
    pinch_dist: None,
  });
}

pub fn state() -> (f32, f32, f32) {
  STATE.with(|state| {
    let state = state.borrow();
    (state.yaw, state.pitch, state.radius)
  })
}

pub fn attach(canvas: &HtmlCanvasElement) {
  let target: &web_sys::EventTarget = canvas.as_ref();

  // A pointer went down: start tracking it and grab pointer capture so we keep
  // receiving its move/up events even if it strays outside the canvas mid-drag.
  // `canvas` is a JS handle, so cloning it is cheap (it just bumps a reference).
  let canvas_down = canvas.clone();
  add_listener::<PointerEvent, _>(target, "pointerdown", move |event| {
    event.prevent_default();
    let id = event.pointer_id();
    let _ = canvas_down.set_pointer_capture(id);
    STATE.with(|state| {
      let mut state = state.borrow_mut();
      let (x, y) = (event.client_x() as f64, event.client_y() as f64);
      if let Some(pointer) = state.pointers.iter_mut().find(|pointer| pointer.id == id) {
        pointer.x = x;
        pointer.y = y;
      } else {
        state.pointers.push(Pointer { id, x, y });
      }
      // Reset the pinch baseline; it's re-measured on the next two-finger move.
      state.pinch_dist = None;
    });
  });

  // A pointer moved: one pointer orbits, two pointers pinch-zoom.
  add_listener::<PointerEvent, _>(target, "pointermove", |event| {
    STATE.with(|state| {
      let mut state = state.borrow_mut();
      let id = event.pointer_id();
      let (x, y) = (event.client_x() as f64, event.client_y() as f64);

      // Ignore moves for pointers we're not tracking (e.g. hover with no button down).
      let Some(index) = state.pointers.iter().position(|pointer| pointer.id == id) else {
        return;
      };
      let (previous_x, previous_y) = (state.pointers[index].x, state.pointers[index].y);
      state.pointers[index].x = x;
      state.pointers[index].y = y;

      match state.pointers.len() {
        1 => {
          // Single finger / mouse drag => orbit.
          let dx = (x - previous_x) as f32;
          let dy = (y - previous_y) as f32;
          state.yaw -= dx * 0.01;
          state.pitch = (state.pitch - dy * 0.01).clamp(-1.5, 1.5); // avoid flipping at the poles
        }
        2 => {
          // Two fingers => pinch: compare the current finger spread to the last
          // one and feed the change into the orbit radius (spread apart = zoom in).
          let ax = state.pointers[0].x;
          let ay = state.pointers[0].y;
          let bx = state.pointers[1].x;
          let by = state.pointers[1].y;
          let distance = ((ax - bx).powi(2) + (ay - by).powi(2)).sqrt();
          if let Some(previous_distance) = state.pinch_dist {
            let delta = (previous_distance - distance) as f32;
            state.radius = (state.radius + delta * 0.01).clamp(0.75, 50.0);
          }
          state.pinch_dist = Some(distance);
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
  add_listener::<WheelEvent, _>(target, "wheel", |event| {
    event.prevent_default(); // stop the page scrolling
    STATE.with(|state| {
      let mut state = state.borrow_mut();
      state.radius = (state.radius + event.delta_y() as f32 * 0.0006).clamp(0.75, 50.0);
    });
  });
}

// Remove a finished pointer from the tracked set and clear the pinch baseline so the
// next two-finger gesture starts fresh. Used for both `pointerup` and `pointercancel`.
fn drop_pointer(event: PointerEvent) {
  STATE.with(|state| {
    let mut state = state.borrow_mut();
    state.pointers.retain(|pointer| pointer.id != event.pointer_id());
    state.pinch_dist = None;
  });
}
