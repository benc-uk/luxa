use crate::Transform;
use crate::nodes::{Node, NodeHandle, NodeKind};
use glam::{Quat, Vec3};

pub(crate) struct LightData {
  pub color: Vec3,
  pub intensity: f32,
}

#[derive(Debug, Clone)]
pub struct LightDescriptor {
  pub parent: Option<NodeHandle>,
  pub transform: Transform,
  pub color: Vec3,
  pub intensity: f32,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct LightHandle(NodeHandle);

impl From<LightHandle> for NodeHandle {
  fn from(handle: LightHandle) -> Self {
    handle.0
  }
}

impl Node {
  pub(crate) fn new_light(device: &wgpu::Device, bind_group_layout: &wgpu::BindGroupLayout, desc: LightDescriptor) -> Self {
    let mut node = Self::new(device, bind_group_layout, desc.transform);

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
