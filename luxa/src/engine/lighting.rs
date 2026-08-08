use bytemuck::Zeroable;
use glam::Vec3;

const MAX_LIGHTS: usize = 16;

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct LightUniform {
  position: [f32; 3],
  intensity: f32, // fills the w slot so position+intensity = one 16-byte vec4
  color: [f32; 3],
  _padding: f32, // pads color up to a full 16-byte vec4
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct LightsUniform {
  pub(crate) ambient_color: [f32; 3],
  pub(crate) ambient_intensity: f32,

  pub(crate) count: u32,
  pub(crate) ibl_enabled: u32,
  _padding: [u32; 2],

  lights: [LightUniform; MAX_LIGHTS],
}

impl LightsUniform {
  pub(crate) fn new() -> Self {
    Self {
      ambient_color: [0.0; 3],
      ambient_intensity: 0.0,
      count: 0,
      ibl_enabled: 0,
      _padding: [0; 2],
      lights: [LightUniform::zeroed(); MAX_LIGHTS],
    }
  }

  pub(crate) fn add_light(&mut self, light_data: &crate::nodes::LightData, world_pos: Vec3) {
    if self.count as usize >= MAX_LIGHTS {
      log::warn!("Maximum number of lights ({}) exceeded, ignoring additional lights", MAX_LIGHTS);
      return;
    }

    let idx = self.count as usize;
    self.lights[idx] = LightUniform {
      position: [world_pos.x, world_pos.y, world_pos.z],
      intensity: light_data.intensity,
      color: [light_data.color.x, light_data.color.y, light_data.color.z],
      _padding: 0.0,
    };
    self.count += 1;
  }
}
