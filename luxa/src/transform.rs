use glam::{Quat, Vec3};

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct Transform {
  pub position: Vec3,
  pub rotation: Quat,
  pub scale: Vec3,
}

impl Default for Transform {
  fn default() -> Self {
    Self {
      position: Vec3::ZERO,
      rotation: Quat::IDENTITY,
      scale: Vec3::ONE,
    }
  }
}

impl Transform {
  pub fn from_translation(position: Vec3) -> Self {
    Self {
      position,
      rotation: Quat::IDENTITY,
      scale: Vec3::ONE,
    }
  }

  pub fn p(x: f32, y: f32, z: f32) -> Self {
    Self {
      position: Vec3::new(x, y, z),
      rotation: Quat::IDENTITY,
      scale: Vec3::ONE,
    }
  }

  pub fn from_rotation(rotation: Quat) -> Self {
    Self {
      position: Vec3::ZERO,
      rotation,
      scale: Vec3::ONE,
    }
  }

  pub fn r(x: f32, y: f32, z: f32) -> Self {
    Self {
      position: Vec3::ZERO,
      rotation: Quat::from_euler(glam::EulerRot::XYZ, x, y, z),
      scale: Vec3::ONE,
    }
  }

  pub fn from_scale(scale: Vec3) -> Self {
    Self {
      position: Vec3::ZERO,
      rotation: Quat::IDENTITY,
      scale,
    }
  }

  pub fn s(x: f32, y: f32, z: f32) -> Self {
    Self {
      position: Vec3::ZERO,
      rotation: Quat::IDENTITY,
      scale: Vec3::new(x, y, z),
    }
  }

  pub fn looking_at(eye: Vec3, target: Vec3) -> Self {
    let rotation = Quat::from_rotation_arc(Vec3::Z, (target - eye).normalize());
    Self {
      position: eye,
      rotation,
      scale: Vec3::ONE,
    }
  }
}
