// ======================================================================================
// Holds scene with a root node and other configuration
// ======================================================================================

use crate::engine::NodeHandle;

#[derive(Debug)]
pub struct Scene {
  root: NodeHandle,
  background_color: [f32; 3],

  ambient_color: [f32; 3],
  ambient_intensity: f32,

  ibl_enabled: bool,
}

#[derive(Debug, Clone)]
pub struct SceneDescriptor {
  pub background_color: [f32; 3],
  pub ambient_color: [f32; 3],
  pub ambient_intensity: f32,
  pub ibl_enabled: bool,
}

// implement default for SceneDescriptor
impl Default for SceneDescriptor {
  fn default() -> Self {
    Self {
      background_color: [0.0, 0.0, 0.0],
      ambient_color: [1.0, 1.0, 1.0],
      ambient_intensity: 0.01,
      ibl_enabled: false,
    }
  }
}

impl Scene {
  pub(crate) fn new(root: NodeHandle, desc: SceneDescriptor) -> Self {
    Self {
      root,
      background_color: desc.background_color,
      ambient_color: desc.ambient_color,
      ambient_intensity: desc.ambient_intensity,
      ibl_enabled: desc.ibl_enabled,
    }
  }

  /// Get the root node of the scene.
  pub fn root(&self) -> NodeHandle {
    self.root
  }

  /// Get the background color of the scene as an RGB array.
  pub fn background_color(&self) -> [f32; 3] {
    self.background_color
  }

  /// Set the background color of the scene as an RGB array.
  pub fn set_background_color(&mut self, color: [f32; 3]) {
    self.background_color = color;
  }

  /// Get the ambient light of the scene as an RGB array. Only used when IBL is disabled.
  pub fn ambient_color(&self) -> [f32; 3] {
    self.ambient_color
  }

  /// Set the ambient light of the scene as an RGB array. Only used when IBL is disabled.
  pub fn set_ambient_color(&mut self, color: [f32; 3]) {
    self.ambient_color = color;
  }

  /// Get the ambient light intensity of the scene. Only used when IBL is disabled.
  pub fn ambient_intensity(&self) -> f32 {
    self.ambient_intensity
  }

  /// Set the ambient light intensity of the scene. Only used when IBL is disabled.
  pub fn set_ambient_intensity(&mut self, intensity: f32) {
    self.ambient_intensity = intensity;
  }

  /// Check if image-based lighting (IBL) is enabled for the scene.
  pub fn ibl_enabled(&self) -> bool {
    self.ibl_enabled
  }

  /// Enable or disable image-based lighting (IBL) for the scene.
  pub fn set_ibl_enabled(&mut self, enabled: bool) {
    self.ibl_enabled = enabled;
  }
}
