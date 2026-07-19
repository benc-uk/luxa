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
  pub(crate) count: u32,
  _padding: [u32; 3], // push the array to offset 16 (array align = 16)
  lights: [LightUniform; MAX_LIGHTS],
}

impl LightsUniform {
  pub(crate) fn new() -> Self {
    Self {
      count: 0,
      _padding: [0; 3],
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
