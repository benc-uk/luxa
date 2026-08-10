mod builder;
mod cubemap;
mod material;
mod mesh_vert;
mod texture;

pub use builder::MeshBuilder;
pub(crate) use cubemap::Cubemap;
pub(crate) use material::MaterialFallbacks;
pub use material::{AlphaMode, Material};
pub use mesh_vert::{Mesh, Vertex};
pub(crate) use texture::Texture;

use crate::NodeHandle;
use glam::{Quat, Vec3};

#[derive(Debug, Clone)]
pub struct ModelDescriptor {
  pub parent: Option<NodeHandle>,
  pub position: Vec3,
  pub rotation: Quat,
  pub scale: Vec3,
}

impl Default for ModelDescriptor {
  fn default() -> Self {
    Self {
      parent: None,
      position: Vec3::ZERO,
      rotation: Quat::IDENTITY,
      scale: Vec3::ONE,
    }
  }
}
