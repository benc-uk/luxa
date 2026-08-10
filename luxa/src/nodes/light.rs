use crate::Transform;
use crate::nodes::{Node, NodeHandle, NodeKind};
use glam::Vec3;

pub(crate) struct LightData {
  pub color: Vec3,
  pub intensity: f32,
}

#[derive(Debug, Clone)]
pub struct LightDescriptor {
  pub parent: Option<NodeHandle>,
  pub position: Vec3,
  pub rotation: glam::Quat,
  pub scale: Vec3,
  pub color: Vec3,
  pub intensity: f32,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct LightHandle(pub(crate) NodeHandle);

impl From<LightHandle> for NodeHandle {
  fn from(handle: LightHandle) -> Self {
    handle.0
  }
}

impl Node {
  pub(crate) fn new_light(device: &wgpu::Device, bind_group_layout: &wgpu::BindGroupLayout, desc: LightDescriptor) -> Self {
    let mut node = Self::new(
      device,
      bind_group_layout,
      Transform {
        position: desc.position,
        rotation: desc.rotation,
        scale: desc.scale,
      },
    );

    node.kind = NodeKind::Light(LightData {
      color: desc.color,
      intensity: desc.intensity,
    });

    node
  }

  pub(crate) fn light_data(&self) -> Option<&LightData> {
    match &self.kind {
      NodeKind::Light(data) => Some(data),
      _ => None,
    }
  }
}
