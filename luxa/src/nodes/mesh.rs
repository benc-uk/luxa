use crate::Transform;
use crate::common::Aabb;
use crate::engine::MeshHandle;
use crate::models::Mesh;
use crate::nodes::{Node, NodeHandle, NodeKind};
use glam::Vec3;
use slotmap::SlotMap;

#[derive(Debug, Clone)]
pub struct MeshNodeDescriptor {
  pub parent: Option<NodeHandle>,
  pub position: Vec3,
  pub rotation: glam::Quat,
  pub scale: Vec3,
  pub meshes: Vec<MeshHandle>,
}

impl Default for MeshNodeDescriptor {
  fn default() -> Self {
    Self {
      parent: None,
      position: Vec3::ZERO,
      rotation: glam::Quat::IDENTITY,
      scale: Vec3::ONE,
      meshes: Vec::new(),
    }
  }
}

pub(crate) struct MeshData {
  pub meshes: Vec<MeshHandle>,
}

impl Node {
  pub(crate) fn new_mesh(device: &wgpu::Device, bind_group_layout: &wgpu::BindGroupLayout, mesh_arena: &SlotMap<MeshHandle, Mesh>, desc: MeshNodeDescriptor) -> Self {
    let mut node = Self::new(
      device,
      bind_group_layout,
      Transform {
        position: desc.position,
        rotation: desc.rotation,
        scale: desc.scale,
      },
    );

    // Compute the AABB and center of the node based on its meshes. This is useful for culling, camera framing, etc.
    // The handles are just keys, so we look each mesh up in the engine's mesh arena to read its local AABB.
    let mut aabb: Option<Aabb> = None;
    for handle in &desc.meshes {
      if let Some(mesh) = mesh_arena.get(*handle) {
        let mesh_aabb = mesh.aabb();
        aabb = Some(match aabb {
          Some(current_aabb) => current_aabb.union(&mesh_aabb),
          None => mesh_aabb,
        });
      }
    }

    node.aabb = aabb;
    node.center = aabb.map(|aabb| aabb.center());

    node.kind = NodeKind::Mesh(MeshData { meshes: desc.meshes });
    node
  }
}
